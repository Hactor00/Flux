use crate::transport::Client;
use crate::{
    http::{self, Retryable},
    limiter::Limiter,
    model::{self, Part},
    store::Store,
    DownloadRequest, Progress, Settings, Status, Task,
};
use anyhow::{bail, ensure, Context, Result};
use fs2::FileExt;
use futures_util::{future::BoxFuture, stream::FuturesUnordered, FutureExt, StreamExt};
use reqwest::{header, StatusCode};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{broadcast, watch, Mutex, Semaphore},
};
use tokio_util::sync::CancellationToken;

const CHUNK: u64 = 4 * 1024 * 1024;
const MAX_PARTS: u64 = 65536;

fn effective_connections(requested: u8, ranged: bool, total: Option<u64>) -> u8 {
    if !ranged {
        return 1;
    }
    if requested != 0 {
        return requested;
    }
    // Keep at least four MiB of useful work per connection; large transfers can
    // use 32 workers while small or non-range sources avoid needless requests.
    total.unwrap_or(CHUNK).div_ceil(CHUNK).clamp(1, 32) as u8
}

#[cfg(test)]
mod connection_tests {
    use super::*;
    #[test]
    fn automatic_connections_follow_source_size_and_range_capability() {
        assert_eq!(effective_connections(0, false, Some(1_000_000_000)), 1);
        assert_eq!(effective_connections(0, true, Some(1_000_000)), 1);
        assert_eq!(effective_connections(0, true, Some(32 * 1024 * 1024)), 8);
        assert_eq!(effective_connections(0, true, Some(256 * 1024 * 1024)), 32);
        assert_eq!(effective_connections(16, true, Some(1_000_000)), 16);
    }
}

#[derive(Clone)]
pub struct Engine(Arc<Inner>);
struct Inner {
    store: Store,
    client: Client,
    data: PathBuf,
    downloads: PathBuf,
    category_root: PathBuf,
    active: Mutex<HashMap<String, Job>>,
    connections: Arc<Semaphore>,
    events: broadcast::Sender<Progress>,
    closing: AtomicBool,
    global_limiter: Arc<Limiter>,
    refresh: Arc<crate::healing::RefreshBroker>,
    torrents: crate::torrent::TorrentManager,
    completion: Mutex<crate::preferences::CompletionWatch>,
    _lock: std::fs::File,
}
#[derive(Clone)]
struct Job {
    cancel: CancellationToken,
    finished: watch::Receiver<bool>,
    torrent: bool,
}
struct WorkerControl {
    part: Part,
    cancel: CancellationToken,
    done: Arc<AtomicU64>,
    streaming: Arc<AtomicBool>,
}
#[derive(Clone)]
struct TransferContext {
    cancel: CancellationToken,
    limiter: Arc<Limiter>,
    progress: Arc<AtomicU64>,
    local: Arc<AtomicU64>,
    streaming: Arc<AtomicBool>,
}

impl Engine {
    pub async fn open(data: impl AsRef<Path>, downloads: impl AsRef<Path>) -> Result<Self> {
        let downloads = downloads.as_ref().to_path_buf();
        Self::open_with_category_root(data, &downloads, &downloads).await
    }
    pub async fn open_with_category_root(
        data: impl AsRef<Path>,
        downloads: impl AsRef<Path>,
        category_root: impl AsRef<Path>,
    ) -> Result<Self> {
        fs::create_dir_all(data.as_ref()).await?;
        fs::create_dir_all(downloads.as_ref()).await?;
        let data = fs::canonicalize(data.as_ref()).await?;
        let downloads = fs::canonicalize(downloads.as_ref()).await?;
        fs::create_dir_all(category_root.as_ref()).await?;
        let category_root = fs::canonicalize(category_root.as_ref()).await?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(data.join("engine.lock"))?;
        lock.try_lock_exclusive()
            .context("Another engine owns this data directory")?;
        let db_path = data.join("tasks.sqlite");
        let store = tokio::task::spawn_blocking(move || Store::open(&db_path)).await??;
        store.recover().await?;
        let global_rate = store.settings().await?.speed_limit;
        let (events, _) = broadcast::channel(256);
        // TLS/QUIC configuration has a larger stack footprint than a GUI callback.
        // Construct it on a blocking worker, outside the native event-loop stack.
        let client = tokio::task::spawn_blocking(http::client).await??;
        Ok(Self(Arc::new(Inner {
            store,
            client,
            torrents: crate::torrent::TorrentManager::new(data.clone(), downloads.clone()),
            data,
            downloads,
            category_root,
            active: Mutex::new(HashMap::new()),
            connections: Arc::new(Semaphore::new(64)),
            events,
            closing: AtomicBool::new(false),
            global_limiter: Arc::new(Limiter::new(global_rate)),
            refresh: Arc::new(crate::healing::RefreshBroker::default()),
            completion: Mutex::new(crate::preferences::CompletionWatch::default()),
            _lock: lock,
        })))
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Progress> {
        self.0.events.subscribe()
    }
    pub async fn poll_completion_action(&self) -> Result<Option<crate::ShutdownAction>> {
        let mut completion = self.0.completion.lock().await;
        let settings = self.settings().await?;
        let tasks = self.list().await?;
        Ok(completion.observe(settings.completion_action, &tasks))
    }
    async fn observe_completion_tasks(&self, tasks: &[Task]) -> Result<()> {
        let mut completion = self.0.completion.lock().await;
        let action = self.settings().await?.completion_action;
        completion.observe(action, tasks);
        Ok(())
    }
    pub fn default_category_root(&self) -> PathBuf {
        self.0.category_root.clone()
    }
    pub async fn routed_directory(
        &self,
        request: &DownloadRequest,
        explicit: Option<PathBuf>,
    ) -> Result<PathBuf> {
        if let Some(directory) = explicit {
            return Ok(directory);
        }
        let settings = self.settings().await?;
        if settings.auto_categorize {
            if let Some(category) =
                crate::preferences::category_for(&request.filename, request.content_type.as_deref())
            {
                return Ok(settings
                    .category_folders
                    .resolve(category, &self.default_category_root()));
            }
        }
        Ok(settings
            .download_directory
            .unwrap_or_else(|| self.0.downloads.clone()))
    }
    pub async fn refresh_poll(&self) -> Vec<crate::RefreshRequest> {
        self.0.refresh.poll().await
    }
    pub async fn refresh_submit(
        &self,
        ticket: String,
        page_url: String,
        url: String,
        protected_credentials: Option<String>,
    ) -> Result<()> {
        let credentials = match protected_credentials {
            Some(value) => self.0.store.unprotect_credentials(&value)?,
            None => Vec::new(),
        };
        self.0
            .refresh
            .submit(&ticket, &page_url, &url, credentials)
            .await
    }
    pub async fn list(&self) -> Result<Vec<Task>> {
        self.0.store.list().await
    }
    pub async fn list_page(&self, offset: usize, limit: usize) -> Result<(usize, Vec<Task>)> {
        ensure!(
            offset <= i64::MAX as usize && (1..=50).contains(&limit),
            "Invalid task page"
        );
        self.0.store.list_page(offset, limit).await
    }
    pub async fn inspect_media(
        &self,
        url: String,
        referer: Option<String>,
        protected_credentials: Option<String>,
    ) -> Result<crate::MediaInfo> {
        let mut request = DownloadRequest {
            url,
            referer,
            filename: "inspection.mp4".into(),
            protected_credentials,
            ..DownloadRequest::default()
        };
        self.decode_credentials(&mut request)?;
        self.inspect_request(request).await
    }
    pub async fn resolve_media_authenticated(
        &self,
        url: &str,
        protected_credentials: Option<String>,
    ) -> Result<crate::extractor::ResolvedMedia> {
        let mut request = DownloadRequest {
            url: url.into(),
            filename: "inspection.mp4".into(),
            protected_credentials,
            ..Default::default()
        };
        self.decode_credentials(&mut request)?;
        model::validate(&request)?;
        crate::extractor::resolve_with_session(&request.url, &request.credentials).await
    }
    pub async fn resolve_collection_authenticated(
        &self,
        url: &str,
        offset: usize,
        limit: usize,
        protected_credentials: Option<String>,
    ) -> Result<crate::extractor::ResolvedCollection> {
        let mut request = DownloadRequest {
            url: url.into(),
            filename: "collection.mp4".into(),
            protected_credentials,
            ..Default::default()
        };
        self.decode_credentials(&mut request)?;
        model::validate(&request)?;
        crate::extractor::resolve_collection_with_session(
            &request.url,
            offset,
            limit,
            &request.credentials,
        )
        .await
    }
    pub async fn inspect_task_media(&self, id: &str) -> Result<crate::MediaInfo> {
        let task = self.task(id).await?;
        self.inspect_request(task.request).await
    }
    async fn inspect_request(&self, request: DownloadRequest) -> Result<crate::MediaInfo> {
        model::validate(&request)?;
        let directory = self
            .0
            .data
            .join("inspections")
            .join(uuid::Uuid::new_v4().to_string());
        let gateway = crate::media_cache::Gateway::open(
            &request,
            &directory,
            self.0.connections.clone(),
            self.0.global_limiter.clone(),
            CancellationToken::new(),
            None,
        )
        .await?;
        let result = async {
            let url = gateway.url(&request.url).await?;
            let mut info = crate::inspect_media(&url, None).await?;
            if info.format.contains("hls") || info.format.contains("dash") {
                let (tracks, chapters) = gateway.discover(&request.url).await?;
                // Prefer the manifest source for HLS text so callers get a stable language list.
                if !tracks.is_empty() {
                    info.subtitles = tracks;
                }
                if info.chapters.is_empty() {
                    info.chapters = chapters;
                }
            }
            anyhow::Ok(info)
        }
        .await;
        gateway.shutdown().await?;
        let _ = fs::remove_dir_all(directory).await;
        result
    }
    pub async fn task(&self, id: &str) -> Result<Task> {
        self.0.store.get(id).await
    }
    pub async fn settings(&self) -> Result<Settings> {
        self.0.store.settings().await
    }
    pub fn default_download_directory(&self) -> PathBuf {
        self.0.downloads.clone()
    }
    pub async fn save_settings(&self, mut settings: Settings) -> Result<Settings> {
        settings.validate()?;
        ensure!(
            settings.speed_limit == 0
                || settings.speed_limit >= 32768
                || !self.0.active.lock().await.values().any(|job| job.torrent),
            "The global limit must be at least 32768 bytes per second while torrents are active"
        );
        if let Some(directory) = &settings.download_directory {
            fs::create_dir_all(directory).await?;
            settings.download_directory = Some(fs::canonicalize(directory).await?);
        }
        {
            let mut completion = self.0.completion.lock().await;
            self.0.store.save_settings(settings.clone()).await?;
            completion.configure(settings.completion_action);
        }
        self.apply_global_budget(&*self.0.active.lock().await, settings.speed_limit)
            .await?;
        Ok(settings)
    }
    pub async fn queue(&self, id: &str) -> Result<Task> {
        let active = self.0.active.lock().await;
        let mut task = self.task(id).await?;
        ensure!(
            !matches!(task.status, Status::AwaitingApproval | Status::Cancelled),
            "Download requires user approval"
        );
        if active.contains_key(id) || task.status == Status::Complete {
            return Ok(task);
        }
        task.status = Status::Queued;
        task.error = None;
        if task
            .request
            .stop_at
            .is_some_and(|time| time <= epoch_seconds())
        {
            task.request.stop_at = None;
        }
        self.0.store.save(task.clone()).await?;
        Ok(task)
    }
    pub async fn set_priority(&self, id: &str, priority: i8) -> Result<Task> {
        ensure!(
            (-10..=10).contains(&priority),
            "Priority must be between -10 and 10"
        );
        let _gate = self.0.active.lock().await;
        ensure!(
            !_gate.contains_key(id),
            "Pause the task before changing its priority"
        );
        let mut task = self.task(id).await?;
        task.request.priority = priority;
        self.0.store.save(task.clone()).await?;
        Ok(task)
    }
    pub async fn remove(&self, id: &str) -> Result<()> {
        self.pause(id).await?;
        let active = self.0.active.lock().await;
        ensure!(!active.contains_key(id), "Task is still active");
        let task = self.task(id).await?;
        if crate::torrent::is_torrent(&task.request) {
            self.0.torrents.remove_metadata(&task).await?;
        }
        self.0.store.remove(id.to_owned()).await?;
        let dir = self.directory(id);
        if fs::try_exists(&dir).await? {
            fs::remove_dir_all(dir).await?;
        }
        if let Some(parent) = task.destination.parent() {
            let _ = fs::remove_file(parent.join(format!(".flux-{id}.staging"))).await;
        }
        Ok(())
    }
    pub async fn pause_all(&self) -> Result<()> {
        let mut settings = self.settings().await?;
        settings.queue_enabled = false;
        self.save_settings(settings).await?;
        for task in self.list().await? {
            if matches!(
                task.status,
                Status::Running | Status::Verifying | Status::Queued
            ) {
                self.pause(&task.id).await?;
            }
        }
        Ok(())
    }
    pub async fn run_scheduler(&self, cancel: CancellationToken) -> Result<()> {
        let mut interval = tokio::time::interval(Duration::from_millis(300));
        loop {
            tokio::select! { _ = cancel.cancelled() => return Ok(()), _ = interval.tick() => {} }
            if self.0.closing.load(Ordering::SeqCst) {
                return Ok(());
            }
            let settings = self.settings().await?;
            let (expired, ready) = self
                .0
                .store
                .schedule(epoch_seconds(), settings.concurrent_tasks)
                .await?;
            for id in expired {
                if let Err(error) = self.pause(&id).await {
                    if error.to_string() != "Task not found" {
                        eprintln!("Cannot pause scheduled task {id}: {error:#}");
                    }
                }
            }
            if !settings.queue_enabled {
                continue;
            }
            for id in ready {
                if self.0.active.lock().await.len() >= settings.concurrent_tasks as usize {
                    break;
                }
                if let Err(error) = self.start_impl(&id, true).await {
                    eprintln!("Cannot start queued task {id}: {error:#}");
                }
            }
        }
    }
    fn decode_credentials(&self, request: &mut DownloadRequest) -> Result<()> {
        if let Some(protected) = request.protected_credentials.take() {
            let credentials = self.0.store.unprotect_credentials(&protected)?;
            ensure!(
                request.credentials.is_empty() || request.credentials == credentials,
                "Conflicting authentication details"
            );
            request.credentials = credentials;
        }
        Ok(())
    }
    pub async fn enqueue(&self, request: DownloadRequest) -> Result<Task> {
        self.enqueue_id(uuid::Uuid::new_v4().to_string(), request)
            .await
    }
    pub async fn import_torrent_file(
        &self,
        path: PathBuf,
        filename: Option<String>,
        directory: Option<PathBuf>,
        start: bool,
        options: Option<crate::TorrentOptions>,
    ) -> Result<Task> {
        let name = filename.unwrap_or_else(|| {
            crate::preferences::safe_filename(
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("Torrent"),
            )
        });
        let request = DownloadRequest {
            filename: name,
            torrent: Some(options.unwrap_or_default()),
            auto_rename: true,
            ..Default::default()
        };
        self.import_torrent_request(path, request, directory, start)
            .await
    }
    pub async fn import_torrent_request(
        &self,
        path: PathBuf,
        mut request: DownloadRequest,
        directory: Option<PathBuf>,
        start: bool,
    ) -> Result<Task> {
        request.url = "https://example.invalid/import.torrent".into();
        model::validate(&request)?;
        let source = self.0.torrents.import_file(&path).await?;
        let id = source
            .strip_prefix("torrent:")
            .context("Missing imported torrent ID")?
            .to_owned();
        request.url = source;
        self.insert_request(
            id,
            &mut request,
            if start {
                Status::Queued
            } else {
                Status::Paused
            },
            directory,
        )
        .await
    }
    pub async fn set_torrent_options(
        &self,
        id: &str,
        options: crate::TorrentOptions,
        speed_limit: Option<u64>,
    ) -> Result<Task> {
        let previous = self.task(id).await?;
        ensure!(
            crate::torrent::is_torrent(&previous.request),
            "Task is not a torrent"
        );
        ensure!(
            !matches!(
                previous.status,
                Status::Complete | Status::Cancelled | Status::AwaitingApproval
            ),
            "Torrent options cannot change after completion or before approval"
        );
        let speed_limit = speed_limit.unwrap_or(previous.request.speed_limit);
        crate::torrent::validate_options(&options, speed_limit)?;
        if let Some(snapshot) = &previous.torrent {
            ensure!(
                options
                    .only_files
                    .iter()
                    .all(|index| *index < snapshot.files.len()),
                "Torrent file index is out of range"
            );
        }
        let active = matches!(previous.status, Status::Running | Status::Verifying);
        self.pause(id).await?;
        let mut task = self.task(id).await?;
        task.request.torrent = Some(options);
        task.request.speed_limit = speed_limit;
        self.0.store.save(task).await?;
        if active {
            self.start(id).await?;
        }
        self.task(id).await
    }
    pub async fn enqueue_clipboard(&self, url: &str) -> Result<Task> {
        let mut request =
            crate::preferences::clipboard_request(url, self.settings().await?.connections)?;
        if crate::extractor::is_provider_url(url) {
            let resolved = crate::extractor::resolve(url).await?;
            let format = resolved
                .formats
                .first()
                .context("No supported video format")?;
            request.filename =
                format!("{}.mp4", crate::preferences::safe_filename(&resolved.title));
            request.url.clone_from(&format.url);
            request.media = Some(crate::MediaOptions {
                provider: Some(crate::MediaProvider {
                    url: url.into(),
                    format_id: Some(format.id.clone()),
                    height: format.height,
                    audio_format_id: None,
                }),
                audio_url: format.audio_url.clone(),
                ..Default::default()
            });
        }
        self.enqueue(request).await
    }
    pub async fn enqueue_batch(&self, requests: Vec<DownloadRequest>) -> Result<Vec<Task>> {
        self.insert_batch(requests, None, true).await
    }
    pub async fn enqueue_batch_to(
        &self,
        requests: Vec<DownloadRequest>,
        directory: PathBuf,
        start: bool,
    ) -> Result<Vec<Task>> {
        self.insert_batch(requests, Some(directory), start).await
    }
    async fn insert_batch(
        &self,
        requests: Vec<DownloadRequest>,
        directory: Option<PathBuf>,
        start: bool,
    ) -> Result<Vec<Task>> {
        ensure!(
            !requests.is_empty() && requests.len() <= 500,
            "Batch must contain 1 to 500 downloads"
        );
        ensure!(
            serde_json::to_vec(&requests)?.len() <= 2 * 1024 * 1024,
            "Batch input exceeds two MiB"
        );
        let _gate = self.0.active.lock().await;
        ensure!(
            !self.0.closing.load(Ordering::SeqCst),
            "Engine is shutting down"
        );
        let automatic_destination = directory.is_none();
        let directory = match directory {
            Some(directory) => {
                ensure!(
                    directory.is_absolute(),
                    "Download directory must be absolute"
                );
                fs::create_dir_all(&directory).await?;
                fs::canonicalize(directory).await?
            }
            None => self
                .settings()
                .await?
                .download_directory
                .unwrap_or_else(|| self.0.downloads.clone()),
        };
        let mut reserved: HashSet<String> = self
            .0
            .store
            .destinations()
            .await?
            .into_iter()
            .map(|path| destination_key(&path))
            .collect();
        let mut tasks = Vec::new();
        for mut request in requests {
            self.decode_credentials(&mut request)?;
            model::validate(&request)?;
            let id = uuid::Uuid::new_v4().to_string();
            let directory = if automatic_destination {
                self.routed_directory(&request, None).await?
            } else {
                directory.clone()
            };
            fs::create_dir_all(&directory).await?;
            let directory = fs::canonicalize(directory).await?;
            let destination = self
                .reserve_destination(&directory, &id, &request, &mut reserved)
                .await?;
            tasks.push(Task {
                id,
                created_at: Some(epoch_seconds()),
                started_at: None,
                finished_at: None,
                timestamp_estimated: false,
                request,
                destination,
                automatic_destination,
                status: if start {
                    Status::Queued
                } else {
                    Status::Paused
                },
                total: None,
                total_estimated: false,
                media_progress: None,
                torrent: None,
                completed: 0,
                etag: None,
                ranged: false,
                source_validated: false,
                capture_url: None,
                media_outputs: Vec::new(),
                direct_write: false,
                sha256: None,
                error: None,
            });
        }
        self.0.store.insert_many(tasks.clone()).await?;
        self.observe_completion_tasks(&tasks).await?;
        Ok(tasks)
    }
    async fn reserve_destination(
        &self,
        directory: &Path,
        id: &str,
        request: &DownloadRequest,
        reserved: &mut HashSet<String>,
    ) -> Result<PathBuf> {
        let mut destination = directory.join(&request.filename);
        if reserved.contains(&destination_key(&destination)) || fs::try_exists(&destination).await?
        {
            ensure!(
                request.auto_rename,
                "Destination already exists or is reserved"
            );
            let path = Path::new(&request.filename);
            let stem = path
                .file_stem()
                .context("Missing filename")?
                .to_string_lossy();
            let extension = path
                .extension()
                .map(|value| format!(".{}", value.to_string_lossy()))
                .unwrap_or_default();
            destination = directory.join(format!("{stem}-{}{extension}", &id[..12]));
        }
        ensure!(
            !fs::try_exists(&destination).await? && reserved.insert(destination_key(&destination)),
            "Destination already exists or is reserved"
        );
        Ok(destination)
    }
    pub async fn enqueue_id(&self, id: String, mut request: DownloadRequest) -> Result<Task> {
        self.insert_request(id, &mut request, Status::Queued, None)
            .await
    }
    pub async fn propose_id(&self, id: String, mut request: DownloadRequest) -> Result<Task> {
        self.insert_request(id, &mut request, Status::AwaitingApproval, None)
            .await
    }
    pub async fn enqueue_to(
        &self,
        request: DownloadRequest,
        directory: PathBuf,
        start: bool,
    ) -> Result<Task> {
        self.enqueue_with_directory(request, Some(directory), start)
            .await
    }
    pub async fn enqueue_with_directory(
        &self,
        mut request: DownloadRequest,
        directory: Option<PathBuf>,
        start: bool,
    ) -> Result<Task> {
        self.insert_request(
            uuid::Uuid::new_v4().to_string(),
            &mut request,
            if start {
                Status::Queued
            } else {
                Status::Paused
            },
            directory,
        )
        .await
    }
    async fn insert_request(
        &self,
        id: String,
        request: &mut DownloadRequest,
        status: Status,
        directory: Option<PathBuf>,
    ) -> Result<Task> {
        self.decode_credentials(request)?;
        model::validate(request)?;
        uuid::Uuid::parse_str(&id).context("Task ID must be a UUID")?;
        // Serializes deduplication and destination reservation with start/pause commands.
        let _gate = self.0.active.lock().await;
        ensure!(
            !self.0.closing.load(Ordering::SeqCst),
            "Engine is shutting down"
        );
        if let Ok(existing) = self.task(&id).await {
            ensure!(
                existing.request == *request
                    || (status == Status::AwaitingApproval
                        && existing.status != Status::AwaitingApproval
                        && existing
                            .capture_url
                            .as_deref()
                            .unwrap_or(&existing.request.url)
                            == request.url),
                "Request ID was reused with different data"
            );
            return Ok(existing);
        }
        let automatic_destination = directory.is_none();
        let directory = self.routed_directory(request, directory).await?;
        ensure!(
            directory.is_absolute(),
            "Download directory must be absolute"
        );
        fs::create_dir_all(&directory).await?;
        let directory = fs::canonicalize(directory).await?;
        let mut reserved = self
            .list()
            .await?
            .into_iter()
            .map(|task| destination_key(&task.destination))
            .collect();
        let destination = self
            .reserve_destination(&directory, &id, request, &mut reserved)
            .await?;
        let task = Task {
            media_outputs: Vec::new(),
            direct_write: false,
            id,
            created_at: Some(epoch_seconds()),
            started_at: None,
            finished_at: None,
            timestamp_estimated: false,
            capture_url: if status == Status::AwaitingApproval {
                Some(request.url.clone())
            } else {
                None
            },
            request: request.clone(),
            status,
            total: None,
            total_estimated: false,
            media_progress: None,
            torrent: None,
            completed: 0,
            etag: None,
            ranged: false,
            source_validated: false,
            sha256: None,
            error: None,
            destination,
            automatic_destination,
        };
        self.0
            .store
            .insert(task.clone())
            .await
            .context("Destination is already reserved by another task")?;
        self.observe_completion_tasks(std::slice::from_ref(&task))
            .await?;
        Ok(task)
    }
    pub async fn approve(
        &self,
        id: &str,
        mut input: DownloadRequest,
        directory: PathBuf,
        start: bool,
    ) -> Result<Task> {
        let _gate = self.0.active.lock().await;
        let mut task = self.task(id).await?;
        ensure!(
            task.status == Status::AwaitingApproval,
            "Download is not awaiting approval"
        );
        let provider_selection = crate::extractor::is_provider_url(&task.request.url)
            && input.media.is_some()
            && url::Url::parse(&input.url)?
                .host_str()
                .is_some_and(|host| host.ends_with(".googlevideo.com"));
        ensure!(
            input.url == task.request.url || provider_selection,
            "Approval cannot change the captured source"
        );
        input.credentials = task.request.credentials.clone();
        input.protected_credentials = None;
        model::validate(&input)?;
        ensure!(
            directory.is_absolute(),
            "Download directory must be absolute"
        );
        fs::create_dir_all(&directory).await?;
        let directory = fs::canonicalize(directory).await?;
        let mut reserved = self
            .list()
            .await?
            .into_iter()
            .filter(|item| item.id != id)
            .map(|item| destination_key(&item.destination))
            .collect();
        task.destination = self
            .reserve_destination(&directory, id, &input, &mut reserved)
            .await?;
        task.request = input;
        task.automatic_destination = false;
        task.status = if start {
            Status::Queued
        } else {
            Status::Paused
        };
        self.0.store.save(task.clone()).await?;
        Ok(task)
    }
    pub async fn reject(&self, id: &str) -> Result<Task> {
        let _gate = self.0.active.lock().await;
        let mut task = self.task(id).await?;
        ensure!(
            task.status == Status::AwaitingApproval,
            "Download is not awaiting approval"
        );
        task.status = Status::Cancelled;
        task.destination = self
            .0
            .data
            .join("rejected")
            .join(id)
            .join(&task.request.filename);
        task.request.credentials.clear();
        self.0.store.save(task.clone()).await?;
        Ok(task)
    }
    pub async fn start(&self, id: &str) -> Result<()> {
        self.start_impl(id, false).await
    }
    async fn start_impl(&self, id: &str, queued_only: bool) -> Result<()> {
        let mut active = self.0.active.lock().await;
        ensure!(
            !self.0.closing.load(Ordering::SeqCst),
            "Engine is shutting down"
        );
        if active.contains_key(id) {
            return Ok(());
        }
        let mut task = match self.task(id).await {
            Ok(task) => task,
            Err(error) if queued_only => {
                if error.to_string() == "Task not found" {
                    return Ok(());
                }
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        if queued_only
            && (task.status != Status::Queued
                || task
                    .request
                    .start_at
                    .is_some_and(|time| time > epoch_seconds())
                || task
                    .request
                    .stop_at
                    .is_some_and(|time| time <= epoch_seconds()))
        {
            return Ok(());
        }
        if active.len() >= self.settings().await?.concurrent_tasks as usize {
            if queued_only {
                return Ok(());
            }
            bail!("Concurrent task limit reached");
        }
        if task.status == Status::Complete {
            return Ok(());
        }
        ensure!(
            !matches!(task.status, Status::AwaitingApproval | Status::Cancelled),
            "Download requires user approval"
        );
        let settings = self.settings().await?;
        ensure!(
            settings.speed_limit == 0
                || settings.speed_limit >= 32768
                || !(crate::torrent::is_torrent(&task.request)
                    || active.values().any(|job| job.torrent)),
            "The global limit must be at least 32768 bytes per second while torrents are active"
        );
        task.status = Status::Running;
        task.started_at.get_or_insert_with(epoch_seconds);
        task.source_validated = false;
        task.error = None;
        self.0.store.save(task.clone()).await?;
        self.observe_completion_tasks(std::slice::from_ref(&task))
            .await?;
        let cancel = CancellationToken::new();
        let (finished, receiver) = watch::channel(false);
        active.insert(
            id.to_owned(),
            Job {
                cancel: cancel.clone(),
                finished: receiver,
                torrent: crate::torrent::is_torrent(&task.request),
            },
        );
        self.apply_global_budget(&active, settings.speed_limit)
            .await?;
        let engine = self.clone();
        tokio::spawn(async move {
            let result = engine.run(task.clone(), cancel.clone()).await;
            if let Err(error) = result {
                task = engine.task(&task.id).await.unwrap_or(task);
                if !crate::media::is_media(&task.request)
                    && !crate::torrent::is_torrent(&task.request)
                {
                    task.completed = engine
                        .0
                        .store
                        .parts(&task.id)
                        .await
                        .unwrap_or_default()
                        .iter()
                        .map(|p| p.done)
                        .sum();
                }
                task.status = if cancel.is_cancelled() {
                    Status::Paused
                } else {
                    Status::Failed
                };
                task.source_validated = false;
                task.error = if cancel.is_cancelled() {
                    None
                } else {
                    Some(format!("{error:#}").chars().take(8192).collect())
                };
                if let Err(e) = engine.0.store.save(task.clone()).await {
                    eprintln!("Cannot persist task state: {e}");
                }
            }
            {
                let mut active = engine.0.active.lock().await;
                active.remove(&task.id);
                if let Ok(settings) = engine.settings().await {
                    if let Err(error) = engine
                        .apply_global_budget(&active, settings.speed_limit)
                        .await
                    {
                        eprintln!("Cannot update transfer bandwidth allocation: {error:#}");
                    }
                }
            }
            let _ = finished.send(true);
        });
        Ok(())
    }
    pub async fn pause(&self, id: &str) -> Result<Task> {
        let job = {
            let active = self.0.active.lock().await;
            let job = active.get(id).cloned();
            if job.is_none() {
                let mut task = self.task(id).await?;
                if task.status == Status::Queued {
                    task.status = Status::Paused;
                    self.0.store.save(task).await?;
                }
            } else if let Some(job) = &job {
                job.cancel.cancel();
            }
            job
        };
        if let Some(mut job) = job {
            job.cancel.cancel();
            while !*job.finished.borrow() {
                job.finished.changed().await?;
            }
        }
        self.task(id).await
    }
    pub async fn wait(&self, id: &str) -> Result<Task> {
        let job = self.0.active.lock().await.get(id).cloned();
        if let Some(mut job) = job {
            while !*job.finished.borrow() {
                job.finished.changed().await?;
            }
        }
        self.task(id).await
    }
    pub async fn shutdown(&self) -> Result<()> {
        let ids: Vec<_> = {
            let active = self.0.active.lock().await;
            self.0.closing.store(true, Ordering::SeqCst);
            active.keys().cloned().collect()
        };
        for id in ids {
            self.pause(&id).await?;
        }
        self.0.torrents.shutdown().await;
        Ok(())
    }
    async fn apply_global_budget(&self, active: &HashMap<String, Job>, limit: u64) -> Result<()> {
        let torrents = active.values().filter(|job| job.torrent).count() as u64;
        let http = active.len() as u64 - torrents;
        let torrent_budget = if limit == 0 || torrents == 0 {
            0
        } else if http == 0 {
            limit.min(u32::MAX as u64)
        } else {
            limit
                .saturating_mul(torrents)
                .checked_div(torrents + http)
                .unwrap_or(0)
                .clamp(16384, limit.saturating_sub(1))
                .min(u32::MAX as u64)
        };
        self.0.torrents.set_global_limit(torrent_budget).await?;
        self.0
            .global_limiter
            .set_rate(if torrents == 0 {
                limit
            } else {
                limit.saturating_sub(torrent_budget)
            })
            .await;
        Ok(())
    }
    fn directory(&self, id: &str) -> PathBuf {
        self.0.data.join("parts").join(id)
    }

    async fn refresh_provider(&self, task: &mut Task, cancel: &CancellationToken) -> Result<bool> {
        let Some(media) = &task.request.media else {
            return Ok(false);
        };
        let provider = media.provider.clone().or_else(|| {
            // Upgrade saved 1.1 tasks using their original browser page, never a
            // random Referer on a non-provider download.
            let source = url::Url::parse(&task.request.url).ok()?;
            let host = source.host_str()?;
            if host != "googlevideo.com" && !host.ends_with(".googlevideo.com") {
                return None;
            }
            let page = task.request.referer.as_ref()?;
            if !crate::extractor::is_provider_url(page) {
                return None;
            }
            let path: Vec<_> = source.path_segments()?.collect();
            let format_id = source
                .query_pairs()
                .find(|(key, _)| key == "itag")
                .map(|(_, value)| value.into_owned())
                .or_else(|| {
                    path.windows(2)
                        .find(|pair| pair[0] == "itag")
                        .map(|pair| pair[1].to_owned())
                });
            Some(crate::MediaProvider {
                url: page.clone(),
                format_id,
                audio_format_id: None,
                height: None,
            })
        });
        let Some(mut provider) = provider else {
            return Ok(false);
        };
        let mut result = tokio::select! {
            _ = cancel.cancelled() => bail!("Cancelled"),
            result = crate::extractor::resolve_with_session(&provider.url, &task.request.credentials) => result,
        };
        for _ in 0..2 {
            let expired = result.as_ref().err().is_some_and(|error| {
                let message = error.to_string().to_ascii_lowercase();
                message.contains("expired")
                    || [
                        "http error 401",
                        "http error 403",
                        "http error 410",
                        "401 unauthorized",
                        "403 forbidden",
                        "410 gone",
                    ]
                    .iter()
                    .any(|status| message.contains(status))
            });
            if !expired {
                break;
            }
            let Some(source) = &task.request.browser_source else {
                break;
            };
            let fresh = self
                .0
                .refresh
                .request(&task.id, source, &provider.url, cancel)
                .await?;
            provider.url = fresh.url;
            for credential in fresh.credentials {
                task.request
                    .credentials
                    .retain(|old| old.origin != credential.origin);
                task.request.credentials.insert(0, credential);
            }
            task.request.credentials.truncate(8);
            result = tokio::select! {
                _ = cancel.cancelled() => bail!("Cancelled"),
                result = crate::extractor::resolve_with_session(&provider.url, &task.request.credentials) => result,
            };
        }
        let resolved = result?;
        let format = resolved
            .formats
            .iter()
            .find(|format| {
                provider
                    .format_id
                    .as_ref()
                    .is_some_and(|id| &format.id == id || format.alternative_ids.contains(id))
            })
            .or_else(|| {
                resolved
                    .formats
                    .iter()
                    .find(|format| provider.height.is_some() && format.height == provider.height)
            })
            .or_else(|| {
                if provider.format_id.is_none() && provider.height.is_none() {
                    resolved.formats.first()
                } else {
                    None
                }
            })
            .context("Selected video quality is unavailable; choose a new quality")?;
        task.request.url.clone_from(&format.url);
        let media = task
            .request
            .media
            .as_mut()
            .context("Missing media options")?;
        if let Some(id) = &provider.audio_format_id {
            let audio = resolved
                .audio_tracks
                .iter()
                .find(|audio| &audio.id == id)
                .context("Selected audio track is unavailable; choose a new audio track")?;
            media.audio_url = Some(audio.url.clone());
        } else {
            media.audio_url.clone_from(&format.audio_url);
        }
        media.video_stream = None;
        media.audio_stream = None;
        media.provider = Some(crate::MediaProvider {
            url: provider.url,
            format_id: Some(format.id.clone()),
            audio_format_id: provider.audio_format_id,
            height: format.height,
        });
        task.total = if matches!(media.container.as_str(), "mp3" | "mka") {
            None
        } else {
            format.size
        };
        task.completed = 0;
        self.0.store.save(task.clone()).await?;
        Ok(true)
    }

    async fn run(&self, mut task: Task, cancel: CancellationToken) -> Result<()> {
        if crate::torrent::is_torrent(&task.request) {
            let client = self.0.client.clone().with_healing(
                self.0.refresh.clone(),
                task.request.browser_source.as_ref(),
                &task.id,
                &cancel,
            );
            return self
                .0
                .torrents
                .run(&mut task, &client, &self.0.store, &self.0.events, &cancel)
                .await;
        }
        let dir = self.directory(&task.id);
        fs::create_dir_all(&dir).await?;
        // Recover a crash between publication and the final database commit.
        if let Some(hash) = &task.sha256 {
            if fs::try_exists(&task.destination).await? {
                ensure!(
                    hash_file(&task.destination).await? == *hash,
                    "Published file was modified"
                );
                for output in &task.media_outputs {
                    ensure!(
                        hash_file(&output.destination).await? == output.sha256,
                        "Published subtitle was modified or lost"
                    );
                }
                task.status = Status::Complete;
                task.finished_at.get_or_insert_with(epoch_seconds);
                self.0.store.save(task.clone()).await?;
                let publication = task
                    .destination
                    .parent()
                    .context("Missing destination directory")?
                    .join(format!(
                        ".flux-{id}.staging",
                        id = dir
                            .file_name()
                            .context("Missing task directory")?
                            .to_string_lossy()
                    ));
                let _ = fs::remove_file(publication).await;
                let _ = fs::remove_dir_all(dir).await;
                return Ok(());
            }
        }
        if crate::media::is_media(&task.request) {
            let renewable = self.refresh_provider(&mut task, &cancel).await?;
            for attempt in 0..2 {
                let stage = dir.join("media-output");
                if fs::try_exists(&stage).await? {
                    fs::remove_file(&stage).await?;
                }
                let gateway = crate::media_cache::Gateway::open(
                    &task.request,
                    &dir.join("media-cache"),
                    self.0.connections.clone(),
                    self.0.global_limiter.clone(),
                    cancel.clone(),
                    Some((self.0.refresh.clone(), task.id.clone())),
                )
                .await?;
                let mut input = task.request.clone();
                if let Some(media) = &mut input.media {
                    if matches!(media.container.as_str(), "mp3" | "mka")
                        && !(media.subtitle_mode == crate::SubtitleMode::External
                            && media.subtitles.iter().any(|track| track.index.is_some()))
                    {
                        if let Some(audio) = media.audio_url.take() {
                            input.url = audio;
                            media.kind = None;
                            media.video_stream = None;
                            media.audio_stream = None;
                        }
                    }
                }
                input.url = gateway.url(&input.url).await?;
                if let Some(media) = &mut input.media {
                    if let Some(audio) = &media.audio_url {
                        media.audio_url = Some(gateway.url(audio).await?);
                    }
                    if media.subtitle_mode != crate::SubtitleMode::None {
                        for (index, track) in media.subtitles.iter_mut().enumerate() {
                            if let Some(url) = &track.url {
                                track.url = Some(gateway.subtitle_input(url, index).await?);
                            }
                        }
                    } else {
                        media.subtitles.clear();
                    }
                }
                let network_bytes = gateway.bytes();
                let output_bytes = Arc::new(AtomicU64::new(0));
                let progress_bytes = output_bytes.clone();
                let sender = self.0.events.clone();
                let id = task.id.clone();
                let network_total = if task.total_estimated {
                    None
                } else {
                    task.total
                };
                let tracker = gateway.tracker();
                let output_time = Arc::new(AtomicU64::new(0));
                let timeline = output_time.clone();
                let ticker_cancel = CancellationToken::new();
                let ticker_stop = ticker_cancel.clone();
                let _ticker_guard = ticker_cancel.clone().drop_guard();
                let validation_store = self.0.store.clone();
                let ticker = tokio::spawn(async move {
                    let mut validated = false;
                    let mut interval = tokio::time::interval(Duration::from_millis(250));
                    let mut previous = 0;
                    let mut estimator = crate::media_progress::Estimator::default();
                    let mut checkpoint = tokio::time::Instant::now();
                    let mut time = tokio::time::Instant::now();
                    loop {
                        tokio::select! { _ = ticker_stop.cancelled() => break, _ = interval.tick() => {} }
                        let received = network_bytes.load(Ordering::Relaxed);
                        if received > 0 && progress_bytes.load(Ordering::Relaxed) > 0 && !validated
                        {
                            validated = validation_store.source_validated(id.clone()).await.is_ok();
                        }
                        let speed = (received.saturating_sub(previous) as f64
                            / time.elapsed().as_secs_f64().max(0.001))
                            as u64;
                        let stats = tracker.snapshot()?;
                        let (total, total_estimated, media) = estimator.update(
                            &stats,
                            timeline.load(Ordering::Relaxed),
                            time.elapsed().as_secs_f64(),
                            network_total,
                            received,
                        );
                        let progress = Progress {
                            id: id.clone(),
                            completed: stats.completed,
                            total,
                            total_estimated,
                            media: Some(media),
                            bytes_per_second: speed,
                        };
                        if checkpoint.elapsed() >= Duration::from_secs(1) {
                            validation_store
                                .media_progress(id.clone(), progress.clone())
                                .await?;
                            checkpoint = tokio::time::Instant::now();
                        }
                        let _ = sender.send(progress);
                        previous = received;
                        time = tokio::time::Instant::now();
                    }
                    let stats = tracker.snapshot()?;
                    let (total, total_estimated, media) = estimator.update(
                        &stats,
                        timeline.load(Ordering::Relaxed),
                        time.elapsed().as_secs_f64(),
                        network_total,
                        network_bytes.load(Ordering::Relaxed),
                    );
                    validation_store
                        .media_progress(
                            id.clone(),
                            Progress {
                                id,
                                completed: stats.completed,
                                total,
                                total_estimated,
                                media: Some(media),
                                bytes_per_second: 0,
                            },
                        )
                        .await?;
                    anyhow::Ok(())
                });
                let result = async {
                    let concat = if let Some(media) = &input.media {
                        if media.segment_urls.is_empty() {
                            None
                        } else {
                            Some(gateway.concat(&media.segment_urls).await?)
                        }
                    } else {
                        None
                    };
                    crate::media::assemble(
                        &input,
                        &stage,
                        cancel.clone(),
                        output_bytes,
                        output_time,
                        concat.as_deref(),
                    )
                    .await
                }
                .await;
                ticker_cancel.cancel();
                ticker.await??;
                let source_error = gateway.last_error().await;
                gateway.shutdown().await?;
                if result.is_err()
                    && renewable
                    && attempt == 0
                    && !cancel.is_cancelled()
                    && source_error.as_ref().is_some_and(|error| {
                        error.contains("403") || error.contains("401") || error.contains("410")
                    })
                {
                    self.refresh_provider(&mut task, &cancel).await?;
                    continue;
                }
                if let Some(error) = source_error {
                    bail!("Media source failed: {error}");
                }
                result?;
                ensure!(!cancel.is_cancelled(), "Cancelled");
                task.completed = fs::metadata(&stage).await?.len();
                task.total = Some(task.completed);
                task.total_estimated = false;
                task.media_progress = self.task(&task.id).await?.media_progress;
                task.status = Status::Verifying;
                task.source_validated = true;
                self.0.store.save(task.clone()).await?;
                let digest = hash_file(&stage).await?;
                if let Some(expected) = &task.request.expected_sha256 {
                    ensure!(
                        digest.eq_ignore_ascii_case(expected),
                        "SHA-256 mismatch; file was not published"
                    );
                }
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&stage)
                    .await?
                    .sync_all()
                    .await?;
                task.sha256 = Some(digest);
                self.0.store.save(task.clone()).await?;
                self.publish_subtitles(&mut task, &dir, &cancel).await?;
                return self.publish(task, &stage, &dir, &cancel).await;
            }
            bail!("Media source could not be renewed");
        }
        let metadata = self.probe(&task.request, &cancel).await?;
        if task.automatic_destination && task.completed == 0 && task.sha256.is_none() {
            task.request.content_type = Some(metadata.content_type.clone());
            let directory = self.routed_directory(&task.request, None).await?;
            fs::create_dir_all(&directory).await?;
            let directory = fs::canonicalize(directory).await?;
            if task.destination.parent() != Some(directory.as_path()) {
                let _gate = self.0.active.lock().await;
                let mut reserved = self
                    .list()
                    .await?
                    .into_iter()
                    .filter(|t| t.id != task.id)
                    .map(|t| destination_key(&t.destination))
                    .collect();
                task.destination = self
                    .reserve_destination(&directory, &task.id, &task.request, &mut reserved)
                    .await?;
                self.0.store.save(task.clone()).await?;
            }
        }
        let worker_limit =
            effective_connections(task.request.connections, metadata.ranged, metadata.total);
        let mut parts = self.0.store.parts(&task.id).await?;
        let reusable = metadata.ranged
            && task.ranged
            && task.etag == metadata.etag
            && task.total == metadata.total
            && !parts.is_empty();
        let stage = crate::disk::staging_path(&task.destination, &task.id)?;
        let legacy = reusable && !task.direct_write;
        let stage_exists = fs::try_exists(&stage).await?;
        crate::disk::prepare(&stage, metadata.total, !reusable || legacy || !stage_exists).await?;
        if !reusable {
            parts = if metadata.ranged {
                let size = metadata.total.context("Missing range size")?;
                // Many bounded pieces let free workers take work from lagging connections.
                let piece_size = CHUNK.max(size.div_ceil(MAX_PARTS));
                (0..size.div_ceil(piece_size))
                    .map(|index| Part {
                        index: index as u32,
                        start: index * piece_size,
                        end: ((index + 1) * piece_size).min(size) - 1,
                        done: 0,
                        digest: None,
                    })
                    .collect()
            } else if metadata.total == Some(0) {
                Vec::new()
            } else {
                vec![Part {
                    index: 0,
                    start: 0,
                    end: metadata.total.unwrap_or(i64::MAX as u64) - 1,
                    done: 0,
                    digest: None,
                }]
            };
            task.etag = metadata.etag;
            task.total = metadata.total;
            task.ranged = metadata.ranged;
            task.completed = 0;
            task.sha256 = None;
            task.direct_write = true;
            self.0.store.plan(task.clone(), parts.clone()).await?;
        }
        for part in &mut parts {
            let path = if legacy {
                dir.join(format!("{}.part", part.index))
            } else {
                stage.clone()
            };
            let offset = if legacy { 0 } else { part.start };
            let valid = part.done > 0
                && fs::metadata(&path)
                    .await
                    .is_ok_and(|m| m.is_file() && m.len() >= offset + part.done);
            let verified = if valid {
                let hash = crate::disk::range_hasher(&path, offset, part.done).await?;
                part.digest.as_deref() == Some(hex::encode(hash.finalize()).as_str())
            } else {
                false
            };
            if verified && legacy {
                let mut input = File::open(&path).await?.take(part.done);
                let mut output = crate::disk::writer(&stage, part.start).await?;
                tokio::io::copy(&mut input, &mut output).await?;
                output.sync_data().await?;
            }
            if !verified {
                part.done = 0;
                part.digest = None;
            }
        }
        task.direct_write = true;
        self.0.store.plan(task.clone(), parts.clone()).await?;
        let progress = Arc::new(AtomicU64::new(parts.iter().map(|p| p.done).sum()));
        let ticker_cancel = CancellationToken::new();
        let _ticker_guard = ticker_cancel.clone().drop_guard();
        let ticker = {
            let events = self.0.events.clone();
            let progress = progress.clone();
            let ticker_cancel = ticker_cancel.clone();
            let id = task.id.clone();
            let total = task.total;
            let validation_store = self.0.store.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_millis(250));
                let mut last = progress.load(Ordering::Relaxed);
                let mut validated = false;
                let mut at = tokio::time::Instant::now();
                loop {
                    tokio::select! { _ = ticker_cancel.cancelled() => break, _ = interval.tick() => {
                        let now = tokio::time::Instant::now();
                        let completed = progress.load(Ordering::Relaxed);
                        if completed > 0 && !validated { validated = validation_store.source_validated(id.clone()).await.is_ok(); }
                        let speed = (completed.saturating_sub(last) as f64 / now.duration_since(at).as_secs_f64().max(0.001)) as u64;
                        let _ = events.send(Progress { id: id.clone(), total, completed, bytes_per_second: speed, total_estimated: false, media: None });
                        last = completed; at = now;
                    }}
                }
            })
        };
        let limiter = Arc::new(Limiter::new(task.request.speed_limit));
        let stop = cancel.child_token();
        let mut next_index = parts.iter().map(|p| p.index).max().unwrap_or(0) + 1;
        let mut pending: VecDeque<_> = parts
            .into_iter()
            .filter(|p| !task.ranged || p.done != p.len())
            .collect();
        let mut workers = FuturesUnordered::new();
        let mut controls: HashMap<u32, WorkerControl> = HashMap::new();
        let mut rescuing = HashSet::new();
        for _ in 0..worker_limit {
            if let Some(part) = pending.pop_front() {
                let local = Arc::new(AtomicU64::new(part.done));
                let streaming = Arc::new(AtomicBool::new(false));
                let worker_cancel = stop.child_token();
                controls.insert(
                    part.index,
                    WorkerControl {
                        part: part.clone(),
                        cancel: worker_cancel.clone(),
                        done: local.clone(),
                        streaming: streaming.clone(),
                    },
                );
                workers.push(self.launch(
                    &task,
                    part,
                    TransferContext {
                        cancel: worker_cancel,
                        limiter: limiter.clone(),
                        progress: progress.clone(),
                        local,
                        streaming,
                    },
                ));
            }
        }
        let mut failure = None;
        let mut balance = tokio::time::interval(Duration::from_millis(500));
        balance.tick().await;
        while !workers.is_empty() {
            let (index, result) = tokio::select! {
                result = workers.next() => result.context("Worker queue disappeared")?,
                _ = balance.tick(), if task.ranged && pending.is_empty() && rescuing.is_empty() && workers.len() < worker_limit as usize && next_index < MAX_PARTS as u32 && !stop.is_cancelled() => {
                    // Stop one lagging connection before changing its range. No overlapping writes.
                    if let Some((&index, control)) = controls.iter().filter(|(_, c)| c.streaming.load(Ordering::Relaxed) && c.part.len().saturating_sub(c.done.load(Ordering::Relaxed)) > 1024 * 1024).max_by_key(|(_, c)| c.part.len().saturating_sub(c.done.load(Ordering::Relaxed))) {
                        rescuing.insert(index);
                        control.cancel.cancel();
                    }
                    continue;
                }
            };
            controls.remove(&index);
            if rescuing.remove(&index) && !stop.is_cancelled() {
                let checkpoint = self
                    .0
                    .store
                    .parts(&task.id)
                    .await?
                    .into_iter()
                    .find(|p| p.index == index)
                    .context("Missing rescue checkpoint")?;
                // A disk failure cannot be mistaken for a successful rescue cancellation.
                if result
                    .as_ref()
                    .err()
                    .is_some_and(|e| e.to_string() != "Cancelled")
                {
                    failure = result.err();
                    stop.cancel();
                } else if checkpoint.done < checkpoint.len() {
                    let tail_start = checkpoint.start
                        + checkpoint.done
                        + (checkpoint.len() - checkpoint.done).div_ceil(2);
                    let tail = Part {
                        index: next_index,
                        start: tail_start,
                        end: checkpoint.end,
                        done: 0,
                        digest: None,
                    };
                    let mut prefix = checkpoint;
                    prefix.end = tail_start - 1;
                    self.0
                        .store
                        .split(task.id.clone(), prefix.clone(), tail.clone())
                        .await?;
                    next_index += 1;
                    pending.push_back(prefix);
                    pending.push_back(tail);
                }
            } else if let Err(error) = result {
                if failure.is_none() {
                    failure = Some(error);
                }
                stop.cancel();
            }
            if !stop.is_cancelled() {
                while workers.len() < worker_limit as usize {
                    let Some(part) = pending.pop_front() else {
                        break;
                    };
                    let local = Arc::new(AtomicU64::new(part.done));
                    let streaming = Arc::new(AtomicBool::new(false));
                    let worker_cancel = stop.child_token();
                    controls.insert(
                        part.index,
                        WorkerControl {
                            part: part.clone(),
                            cancel: worker_cancel.clone(),
                            done: local.clone(),
                            streaming: streaming.clone(),
                        },
                    );
                    workers.push(self.launch(
                        &task,
                        part,
                        TransferContext {
                            cancel: worker_cancel,
                            limiter: limiter.clone(),
                            progress: progress.clone(),
                            local,
                            streaming,
                        },
                    ));
                }
            }
        }
        drop(workers);
        ticker_cancel.cancel();
        ticker.await?;
        if let Some(error) = failure {
            return Err(error);
        }
        ensure!(!cancel.is_cancelled(), "Cancelled");
        task.completed = progress.load(Ordering::Relaxed);
        if let Some(total) = task.total {
            ensure!(
                task.completed == total,
                "Download size does not match server metadata"
            );
        }
        task.total = Some(task.completed);
        task.status = Status::Verifying;
        task.source_validated = true;
        self.0.store.save(task.clone()).await?;
        let output = OpenOptions::new().write(true).open(&stage).await?;
        output.set_len(task.completed).await?;
        output.sync_all().await?;
        drop(output);
        let checkpoints = self.0.store.parts(&task.id).await?;
        let digest = crate::disk::verify(&stage, &checkpoints, task.completed, &cancel).await?;
        if let Some(expected) = &task.request.expected_sha256 {
            ensure!(
                digest.eq_ignore_ascii_case(expected),
                "SHA-256 mismatch; file was not published"
            );
        }
        task.sha256 = Some(digest);
        self.0.store.save(task.clone()).await?;
        self.publish(task, &stage, &dir, &cancel).await
    }
    async fn publish_subtitles(
        &self,
        task: &mut Task,
        directory: &Path,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let Some(media) = task
            .request
            .media
            .as_ref()
            .filter(|m| m.subtitle_mode == crate::SubtitleMode::External)
        else {
            return Ok(());
        };
        let stem = task
            .destination
            .file_stem()
            .context("Missing output filename")?
            .to_string_lossy();
        let mut staged_outputs = Vec::new();
        for (index, track) in media.subtitles.iter().enumerate() {
            ensure!(!cancel.is_cancelled(), "Cancelled");
            let staged = directory.join(format!("media-subtitle-{index}.srt"));
            let destination = task
                .destination
                .with_file_name(format!("{stem}.{}.{index}.srt", track.language));
            let digest = hash_file(&staged).await?;
            if fs::try_exists(&destination).await? {
                ensure!(
                    hash_file(&destination).await? == digest,
                    "Subtitle output already exists with different content"
                );
            }
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&staged)
                .await?
                .sync_all()
                .await?;
            staged_outputs.push((staged, destination, digest));
        }
        task.media_outputs = staged_outputs
            .iter()
            .map(|(_, destination, sha256)| crate::model::MediaOutput {
                destination: destination.clone(),
                sha256: sha256.clone(),
            })
            .collect();
        self.0.store.save(task.clone()).await?;
        for (staged, destination, _) in staged_outputs {
            ensure!(!cancel.is_cancelled(), "Cancelled");
            if !fs::try_exists(&destination).await? {
                publish_exclusive(&staged, &destination).await?;
            }
        }
        Ok(())
    }

    async fn publish(
        &self,
        mut task: Task,
        stage: &Path,
        dir: &Path,
        cancel: &CancellationToken,
    ) -> Result<()> {
        ensure!(!cancel.is_cancelled(), "Cancelled");
        // An exclusive hard link publishes complete bytes without overwriting an existing file.
        // Parts are inside downloads' volume only if the caller selects matching volumes.
        // Cross-volume publication uses a destination-side staging file and the same exclusive link.
        let parent = task
            .destination
            .parent()
            .context("Missing destination directory")?;
        let publication = parent.join(format!(".flux-{}.staging", task.id));
        if stage == publication {
            publish_exclusive(stage, &task.destination).await?;
        } else if let Err(error) = fs::hard_link(stage, &task.destination).await {
            ensure!(
                error.kind() != std::io::ErrorKind::AlreadyExists
                    && !fs::try_exists(&task.destination).await?,
                "Destination already exists"
            );
            let mut source = File::open(stage).await?;
            // The filename is task-owned. Discard an incomplete cross-volume copy left by a crash.
            if fs::try_exists(&publication).await? {
                fs::remove_file(&publication).await?;
            }
            let mut destination = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&publication)
                .await?;
            tokio::io::copy(&mut source, &mut destination).await?;
            destination.sync_all().await?;
            drop(destination);
            ensure!(!cancel.is_cancelled(), "Cancelled");
            publish_exclusive(&publication, &task.destination).await?;
        }
        #[cfg(unix)]
        {
            let directory = parent.to_owned();
            tokio::task::spawn_blocking(move || std::fs::File::open(directory)?.sync_all())
                .await??;
        }
        task.status = Status::Complete;
        task.finished_at = Some(epoch_seconds());
        self.0.store.save(task.clone()).await?;
        let _ = fs::remove_file(publication).await;
        if let Err(error) = fs::remove_dir_all(dir).await {
            eprintln!("Cannot clean completed parts: {error}");
        }
        let _ = self.0.events.send(Progress {
            id: task.id,
            completed: task.completed,
            total: task.total,
            bytes_per_second: 0,
            total_estimated: false,
            media: None,
        });
        Ok(())
    }

    fn launch<'a>(
        &'a self,
        task: &'a Task,
        part: Part,
        context: TransferContext,
    ) -> BoxFuture<'a, (u32, Result<()>)> {
        async move { (part.index, self.worker(task, part, context).await) }.boxed()
    }
    async fn worker(&self, task: &Task, mut part: Part, context: TransferContext) -> Result<()> {
        let TransferContext {
            ref cancel,
            ref progress,
            ..
        } = context;
        let path = crate::disk::staging_path(&task.destination, &task.id)?;
        let mut hash = crate::disk::range_hasher(&path, part.start, part.done).await?;
        let mut file = crate::disk::writer(&path, part.start + part.done).await?;
        use tokio::io::AsyncSeekExt;
        let mut result = Ok(());
        for attempt in 0..=http::MAX_RETRIES {
            result = self
                .transfer(task, &mut part, &mut file, &mut hash, &context)
                .await;
            context.streaming.store(false, Ordering::Relaxed);
            if result.is_ok() || cancel.is_cancelled() {
                break;
            }
            // Semantic errors (bad ranges, changed objects, disk failures) never retry.
            if !result.as_ref().err().is_some_and(|e| e.is::<Retryable>())
                || attempt == http::MAX_RETRIES
            {
                break;
            }
            if !task.ranged {
                progress.fetch_sub(part.done, Ordering::Relaxed);
                part.done = 0;
                context.local.store(0, Ordering::Relaxed);
                hash = Sha256::new();
                file.set_len(0).await?;
                file.seek(std::io::SeekFrom::Start(0)).await?;
            }
            let seconds = result
                .as_ref()
                .err()
                .and_then(|e| e.downcast_ref::<Retryable>())
                .map(|r| r.delay)
                .unwrap_or(0)
                .max(1 << attempt);
            if seconds > 300 {
                break;
            }
            tokio::select! { _ = cancel.cancelled() => break, _ = tokio::time::sleep(Duration::from_secs(seconds)) => {} }
        }
        file.sync_all().await?;
        drop(file);
        part.digest = Some(hex::encode(hash.finalize()));
        self.0.store.checkpoint(task.id.clone(), part).await?;
        ensure!(!cancel.is_cancelled(), "Cancelled");
        result
    }
    async fn transfer(
        &self,
        task: &Task,
        part: &mut Part,
        file: &mut File,
        hash: &mut Sha256,
        context: &TransferContext,
    ) -> Result<()> {
        let TransferContext {
            cancel,
            limiter,
            progress,
            local,
            streaming,
        } = context;
        if task.ranged && part.done == part.len() {
            return Ok(());
        }
        let _permit = tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), p = self.0.connections.acquire() => p? };
        let mut request = http::authenticate(
            self.0
                .client
                .clone()
                .with_healing(
                    self.0.refresh.clone(),
                    task.request.browser_source.as_ref(),
                    &task.id,
                    cancel,
                )
                .with_policy(task.request.transport)
                .get(&task.request.url)
                .header(header::ACCEPT_ENCODING, "identity"),
            &task.request.url,
            &task.request.credentials,
        )?;
        if let Some(referer) = &task.request.referer {
            request = request.header(header::REFERER, referer);
        }
        if let Some(etag) = &task.etag {
            request = request.header(header::IF_MATCH, etag);
        }
        if task.ranged {
            request = request.header(
                header::RANGE,
                format!("bytes={}-{}", part.start + part.done, part.end),
            );
        }
        let response = tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), r = request.send() => r.map_err(|e| anyhow::Error::new(Retryable { message: e.to_string(), delay: 0 }))? };
        if let Some(error) = http::transient(&response) {
            return Err(error.into());
        }
        response.error_for_status_ref()?;
        http::identity(&response)?;
        if let Some(tag) = &task.etag {
            ensure!(
                http::etag(&response).as_ref() == Some(tag),
                "Server object changed or validator is missing"
            );
        }
        if task.ranged {
            ensure!(
                response.status() == StatusCode::PARTIAL_CONTENT,
                "Server ignored Range; refusing unsafe merge"
            );
            let range = http::content_range(
                response
                    .headers()
                    .get(header::CONTENT_RANGE)
                    .context("Missing Content-Range")?
                    .to_str()?,
            )?;
            ensure!(
                range
                    == (
                        part.start + part.done,
                        part.end,
                        task.total.context("Missing total")?
                    ),
                "Incorrect Content-Range"
            );
        } else {
            ensure!(
                response.status() == StatusCode::OK,
                "Unexpected stream status"
            );
        }
        streaming.store(true, Ordering::Relaxed);
        let mut stream = response.bytes_stream();
        loop {
            let next = tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), n = stream.next() => n };
            let Some(bytes) = next else {
                break;
            };
            let bytes = bytes.map_err(|e| {
                anyhow::Error::new(Retryable {
                    message: e.to_string(),
                    delay: 0,
                })
            })?;
            let remaining = if task.ranged {
                Some(part.len() - part.done)
            } else {
                task.total.map(|t| t.saturating_sub(part.done))
            };
            ensure!(
                remaining.is_none_or(|n| bytes.len() as u64 <= n),
                "Server sent too many bytes"
            );
            for slice in bytes.chunks(65536) {
                limiter.acquire(slice.len(), cancel).await?;
                self.0.global_limiter.acquire(slice.len(), cancel).await?;
                file.write_all(slice).await?;
                hash.update(slice);
                part.done += slice.len() as u64;
                local.store(part.done, Ordering::Relaxed);
                progress.fetch_add(slice.len() as u64, Ordering::Relaxed);
            }
        }
        let expected = if task.ranged {
            Some(part.len())
        } else {
            task.total
        };
        if expected.is_some_and(|n| n != part.done) {
            return Err(Retryable {
                message: "Response ended before all bytes arrived".into(),
                delay: 0,
            }
            .into());
        }
        Ok(())
    }
    async fn probe(
        &self,
        request: &DownloadRequest,
        cancel: &CancellationToken,
    ) -> Result<http::Metadata> {
        let client = self
            .0
            .client
            .clone()
            .with_healing(
                self.0.refresh.clone(),
                request.browser_source.as_ref(),
                "probe",
                cancel,
            )
            .with_policy(request.transport);
        for attempt in 0..=http::MAX_RETRIES {
            let result = {
                let _permit = tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), permit = self.0.connections.acquire() => permit? };
                tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), result = http::probe(&client, &request.url, request.referer.as_deref(), &request.credentials) => result }
            };
            match result {
                Ok(metadata) => return Ok(metadata),
                Err(error) => {
                    let transient = error.is::<Retryable>()
                        || error.downcast_ref::<reqwest::Error>().is_some_and(|e| {
                            e.is_timeout() || e.is_connect() || e.is_body() || e.is_request()
                        });
                    let seconds = error
                        .downcast_ref::<Retryable>()
                        .map(|e| e.delay)
                        .unwrap_or(0)
                        .max(1 << attempt);
                    if !transient || attempt == http::MAX_RETRIES || seconds > 300 {
                        return Err(error);
                    }
                    tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), _ = tokio::time::sleep(Duration::from_secs(seconds)) => {} }
                }
            }
        }
        bail!("Probe retry budget exhausted")
    }
}

async fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).await?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 65536];
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}

fn epoch_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn destination_key(path: &Path) -> String {
    let value = path.to_string_lossy().to_string();
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}

async fn publish_exclusive(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        tokio::task::spawn_blocking(move || {
            // No REPLACE_EXISTING flag: FAT/exFAT are supported without ever overwriting a file.
            let result = unsafe {
                windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                    source.as_ptr(),
                    destination.as_ptr(),
                    windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
                )
            };
            if result == 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        })
        .await??;
    }
    #[cfg(not(windows))]
    fs::hard_link(source, destination)
        .await
        .context("Cannot publish file exclusively")?;
    Ok(())
}
