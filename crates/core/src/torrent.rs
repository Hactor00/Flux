use crate::{
    http, model, store::Store, transport::Client, DownloadRequest, Progress, Status, Task,
    TorrentFile, TorrentOptions, TorrentSnapshot,
};
use anyhow::{bail, ensure, Context, Result};
use bytes::Bytes;
use futures_util::StreamExt;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, DhtSessionConfig, ListenerOptions, Session,
    SessionOptions,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU32,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{broadcast, watch, Mutex},
};
use tokio_util::sync::CancellationToken;

const MAX_METAINFO: usize = 8 * 1024 * 1024;
const MAX_FILES: usize = 10000;

pub(crate) fn is_torrent(request: &DownloadRequest) -> bool {
    request.torrent.is_some()
        || request.url.starts_with("magnet:")
        || request.url.starts_with("torrent:")
        || request.content_type.as_deref().is_some_and(|mime| {
            mime.split(';').next().is_some_and(|value| {
                value
                    .trim()
                    .eq_ignore_ascii_case("application/x-bittorrent")
            })
        })
        || url::Url::parse(&request.url)
            .ok()
            .is_some_and(|url| url.path().to_ascii_lowercase().ends_with(".torrent"))
}

pub(crate) fn validate_request(request: &DownloadRequest) -> Result<()> {
    ensure!(
        request.media.is_none(),
        "Torrent and media options are mutually exclusive"
    );
    ensure!(
        request.url.len() <= 16384 && !request.url.chars().any(char::is_control),
        "Invalid torrent source"
    );
    if request.url.starts_with("magnet:") {
        let url = url::Url::parse(&request.url)?;
        ensure!(
            url.query_pairs().count() <= 128,
            "Too many magnet parameters"
        );
        // Bound file-index expansions before passing them to the native parser.
        for (key, value) in url.query_pairs() {
            if key == "so" {
                for range in value.split(',') {
                    let (start, end) = range.split_once('-').unwrap_or((range, range));
                    let start: usize = start.parse().context("Invalid magnet file selection")?;
                    let end: usize = end.parse().context("Invalid magnet file selection")?;
                    ensure!(
                        start <= end && end < MAX_FILES,
                        "Magnet file selection exceeds the supported limit"
                    );
                }
            }
            if key == "tr" {
                let tracker = url::Url::parse(&value)?;
                ensure!(
                    matches!(tracker.scheme(), "http" | "https" | "udp")
                        && tracker.host_str().is_some()
                        && tracker.username().is_empty()
                        && tracker.password().is_none(),
                    "Invalid tracker URL"
                );
            }
        }
        let magnet = librqbit::Magnet::parse(&request.url)?;
        ensure!(
            magnet.as_id20().is_some() || magnet.as_id32().is_some(),
            "Magnet must contain a valid v1 or v2 info hash"
        );
    } else if let Some(id) = request.url.strip_prefix("torrent:") {
        uuid::Uuid::parse_str(id).context("Invalid imported torrent identifier")?;
    } else {
        model::validate_url(&request.url)?;
    }
    validate_options(
        request
            .torrent
            .as_ref()
            .unwrap_or(&TorrentOptions::default()),
        request.speed_limit,
    )
}

pub(crate) fn validate_options(options: &TorrentOptions, download_limit: u64) -> Result<()> {
    if let Some(rule) = &options.seeding {
        rule.validate()?;
    }
    ensure!(
        (1..=500).contains(&options.peer_limit),
        "Peer limit must be between 1 and 500"
    );
    ensure!(
        options.only_files.len() <= MAX_FILES
            && options.only_files.iter().all(|index| *index < MAX_FILES),
        "Invalid torrent file selection"
    );
    ensure!(
        options.only_files.iter().collect::<HashSet<_>>().len() == options.only_files.len(),
        "Duplicate torrent file selection"
    );
    ensure!(
        options.initial_peers.len() <= 128
            && options.initial_peers.iter().all(|peer| peer.port() != 0
                && !peer.ip().is_unspecified()
                && !peer.ip().is_multicast()),
        "Invalid initial torrent peers"
    );
    for rate in [download_limit, options.upload_limit] {
        ensure!(rate == 0 || (16384..=u32::MAX as u64).contains(&rate), "Torrent bandwidth limits must be unlimited or between 16384 and 4294967295 bytes per second");
    }
    Ok(())
}

pub(crate) struct TorrentManager {
    data: PathBuf,
    downloads: PathBuf,
    session: Arc<Mutex<Option<Arc<Session>>>>,
    download_budget: Arc<AtomicU64>,
    upload_budget: Arc<AtomicU64>,
    seeds: Arc<Mutex<HashMap<String, SeedJob>>>,
    native: Arc<Mutex<Option<Arc<crate::torrent_native::Session>>>>,
}
impl Drop for TorrentManager {
    fn drop(&mut self) {
        if let Ok(seeds) = self.seeds.try_lock() {
            for job in seeds.values() {
                job.preserve.store(true, Ordering::Release);
                job.cancel.cancel();
            }
        }
    }
}

struct SeedJob {
    cancel: CancellationToken,
    preserve: Arc<AtomicBool>,
    finished: watch::Receiver<bool>,
}

pub(crate) enum SeedHandle {
    Rqbit {
        session: Arc<Session>,
        id: usize,
        handle: Arc<librqbit::ManagedTorrent>,
    },
    Native {
        session: Arc<crate::torrent_native::Session>,
        id: u64,
    },
}
impl SeedHandle {
    fn stats(&self) -> Result<(u64, u64, u32)> {
        match self {
            Self::Rqbit { handle, .. } => {
                let stats = handle.stats();
                ensure!(stats.error.is_none(), "Torrent seeding failed");
                let speed = stats
                    .live
                    .as_ref()
                    .map_or(0, |live| live.upload_speed.as_bytes());
                let peers = stats
                    .live
                    .as_ref()
                    .map_or(0, |live| live.snapshot.peer_stats.live);
                Ok((stats.uploaded_bytes, speed, peers))
            }
            Self::Native { session, id } => {
                let stats = session.stats(*id)?;
                Ok((stats.uploaded, stats.upload_rate, stats.peers as u32))
            }
        }
    }
    async fn resume(&self) -> Result<()> {
        match self {
            Self::Rqbit {
                session, handle, ..
            } => session.unpause(handle).await,
            Self::Native { session, id } => session.pause(*id, false),
        }
    }
    async fn remove(&self) -> Result<()> {
        match self {
            Self::Rqbit { session, id, .. } => session.delete((*id).into(), false).await,
            Self::Native { session, id } => {
                session.pause(*id, true)?;
                session.remove(*id)
            }
        }
    }
}

impl TorrentManager {
    pub fn new(data: PathBuf, downloads: PathBuf, upload: u64) -> Self {
        Self {
            data,
            downloads,
            session: Arc::new(Mutex::new(None)),
            download_budget: Arc::new(AtomicU64::new(0)),
            upload_budget: Arc::new(AtomicU64::new(upload)),
            seeds: Arc::new(Mutex::new(HashMap::new())),
            native: Arc::new(Mutex::new(None)),
        }
    }
    async fn session(&self) -> Result<Arc<Session>> {
        let mut current = self.session.lock().await;
        if let Some(session) = current.as_ref() {
            return Ok(session.clone());
        }
        fs::create_dir_all(self.data.join("torrents")).await?;
        let session = Session::new_with_opts(
            self.downloads.clone(),
            SessionOptions {
                dht: Some(DhtSessionConfig {
                    persistence: Some(librqbit::dht::DhtPersistenceConfig {
                        config_filename: Some(self.data.join("torrents/dht.json")),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                listen: Some(ListenerOptions::default()),
                fastresume: false,
                persistence: None,
                peer_limit: Some(80),
                concurrent_init_limit: Some(2),
                ratelimits: librqbit::limits::LimitsConfig {
                    download_bps: rate(self.download_budget.load(Ordering::Acquire))?,
                    upload_bps: rate(self.upload_budget.load(Ordering::Acquire))?,
                },
                client_name_and_version: Some(concat!("Flux/", env!("CARGO_PKG_VERSION")).into()),
                ..Default::default()
            },
        )
        .await?;
        session
            .ratelimits
            .set_download_bps(rate(self.download_budget.load(Ordering::Acquire))?);
        *current = Some(session.clone());
        Ok(session)
    }
    pub async fn set_global_limit(&self, limit: u64) -> Result<()> {
        self.download_budget.store(limit, Ordering::Release);
        self.rebalance().await
    }
    pub async fn set_upload_limit(&self, limit: u64) -> Result<()> {
        self.upload_budget.store(limit, Ordering::Release);
        self.rebalance().await
    }
    async fn rebalance(&self) -> Result<()> {
        balance_sessions(
            &self.session,
            &self.native,
            self.download_budget.load(Ordering::Acquire),
            self.upload_budget.load(Ordering::Acquire),
        )
        .await
    }
    pub async fn shutdown(&self) {
        let ids: Vec<_> = self.seeds.lock().await.keys().cloned().collect();
        for id in ids {
            let _ = self.stop_seed(&id, true).await;
        }
        if let Some(session) = self.session.lock().await.take() {
            session.stop().await;
        }
        self.native.lock().await.take();
    }
    pub async fn stop_seed(&self, id: &str, preserve: bool) -> Result<bool> {
        let job = self.seeds.lock().await.remove(id);
        if let Some(mut job) = job {
            job.preserve.store(preserve, Ordering::Release);
            job.cancel.cancel();
            while !*job.finished.borrow() {
                job.finished.changed().await?;
            }
            return Ok(true);
        }
        Ok(false)
    }
    pub(crate) async fn start_seed(
        &self,
        mut task: Task,
        backend: SeedHandle,
        store: Store,
        events: broadcast::Sender<Progress>,
    ) -> Result<bool> {
        let rule = task
            .request
            .torrent
            .as_ref()
            .and_then(|options| options.seeding.clone())
            .unwrap_or(store.settings().await?.seeding);
        let snapshot = task.torrent.as_ref().context("Missing torrent snapshot")?;
        if rule.reached(snapshot.uploaded, task.completed, snapshot.seeded_seconds) {
            return Ok(false);
        }
        let base_uploaded = snapshot.uploaded.saturating_sub(backend.stats()?.0);
        let base_seconds = snapshot.seeded_seconds;
        backend.resume().await?;
        task.torrent
            .as_mut()
            .context("Missing torrent snapshot")?
            .state = "seeding".into();
        store.save(task.clone()).await?;
        let cancel = CancellationToken::new();
        let preserve = Arc::new(AtomicBool::new(false));
        let (finished, receiver) = watch::channel(false);
        let seeds = self.seeds.clone();
        let rq_pool = self.session.clone();
        let native_pool = self.native.clone();
        let download_budget = self.download_budget.clone();
        let upload_budget = self.upload_budget.clone();
        let mut entries = seeds.lock().await;
        ensure!(
            !entries.contains_key(&task.id),
            "Torrent is already seeding"
        );
        entries.insert(
            task.id.clone(),
            SeedJob {
                cancel: cancel.clone(),
                preserve: preserve.clone(),
                finished: receiver,
            },
        );
        drop(entries);
        tokio::spawn(async move {
            let id = task.id.clone();
            let mut last_tick = tokio::time::Instant::now();
            let mut elapsed = Duration::ZERO;
            let result: Result<()> = async {
                let mut interval = tokio::time::interval(Duration::from_secs(1));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! { _ = cancel.cancelled() => {}, _ = interval.tick() => {} }
                    elapsed += last_tick.elapsed().min(Duration::from_secs(3));
                    last_tick = tokio::time::Instant::now();
                    if download_budget.load(Ordering::Acquire) != 0
                        || upload_budget.load(Ordering::Acquire) != 0
                    {
                        balance_sessions(
                            &rq_pool,
                            &native_pool,
                            download_budget.load(Ordering::Acquire),
                            upload_budget.load(Ordering::Acquire),
                        )
                        .await?;
                    }
                    task = store.get(&id).await?;
                    let (uploaded, speed, peers) = backend.stats()?;
                    let rule = task
                        .request
                        .torrent
                        .as_ref()
                        .and_then(|options| options.seeding.clone())
                        .unwrap_or(store.settings().await?.seeding);
                    let torrent = task.torrent.as_mut().context("Missing torrent snapshot")?;
                    torrent.uploaded = base_uploaded.saturating_add(uploaded);
                    torrent.seeded_seconds = base_seconds.saturating_add(elapsed.as_secs());
                    torrent.upload_bps = speed;
                    torrent.peers = peers;
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_secs();
                    let reached =
                        rule.reached(torrent.uploaded, task.completed, torrent.seeded_seconds)
                            || task.request.stop_at.is_some_and(|stop| stop <= now);
                    let stopping = cancel.is_cancelled() || reached;
                    if stopping {
                        torrent.state = if reached {
                            "complete"
                        } else if preserve.load(Ordering::Acquire) {
                            "seeding"
                        } else {
                            "seed_paused"
                        }
                        .into();
                        torrent.upload_bps = 0;
                        torrent.peers = 0;
                    }
                    store.save(task.clone()).await?;
                    let _ = events.send(Progress {
                        id: id.clone(),
                        completed: task.completed,
                        total: task.total,
                        bytes_per_second: 0,
                        total_estimated: false,
                        media: None,
                    });
                    if stopping {
                        break;
                    }
                }
                Ok(())
            }
            .await;
            if let Err(error) = result {
                if let Ok(mut task) = store.get(&id).await {
                    if let Some(torrent) = &mut task.torrent {
                        torrent.state = "seed_failed".into();
                        torrent.upload_bps = 0;
                        torrent.peers = 0;
                    }
                    task.error = Some(error.to_string());
                    let _ = store.save(task).await;
                }
            }
            let _ = backend.remove().await;
            seeds.lock().await.remove(&id);
            let _ = finished.send(true);
        });
        Ok(true)
    }
    fn metainfo_path(&self, id: &str) -> Result<PathBuf> {
        let id = uuid::Uuid::parse_str(id)?;
        Ok(self.data.join("torrents").join(format!("{id}.torrent")))
    }
    pub async fn import_file(&self, path: &Path) -> Result<String> {
        ensure!(
            path.is_absolute()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("torrent")),
            "An absolute .torrent filename is required"
        );
        let bytes = read_bounded(path).await?;
        validate_metainfo(&bytes)?;
        let id = uuid::Uuid::new_v4().to_string();
        self.save_metainfo(&id, &bytes).await?;
        Ok(format!("torrent:{id}"))
    }
    async fn save_metainfo(&self, id: &str, bytes: &[u8]) -> Result<()> {
        ensure!(
            bytes.len() <= MAX_METAINFO,
            "Torrent metadata exceeds eight MiB"
        );
        fs::create_dir_all(self.data.join("torrents")).await?;
        let path = self.metainfo_path(id)?;
        if fs::try_exists(&path).await? {
            ensure!(
                read_bounded(&path).await?.as_ref() == bytes,
                "Persisted torrent metadata changed"
            );
            return Ok(());
        }
        let stage = path.with_extension("staging");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage)
            .await?;
        file.write_all(bytes).await?;
        file.sync_all().await?;
        drop(file);
        if let Err(error) = fs::rename(&stage, &path).await {
            let _ = fs::remove_file(&stage).await;
            return Err(error.into());
        }
        Ok(())
    }
    pub async fn remove_metadata(&self, task: &Task) -> Result<()> {
        for id in [
            Some(task.id.as_str()),
            task.request.url.strip_prefix("torrent:"),
        ]
        .into_iter()
        .flatten()
        {
            let path = self.metainfo_path(id)?;
            if fs::try_exists(&path).await? {
                fs::remove_file(path).await?;
            }
        }
        let ownership = self
            .data
            .join("torrents")
            .join(format!("{}.destination.json", task.id));
        if fs::try_exists(&ownership).await? {
            fs::remove_file(ownership).await?;
        }
        Ok(())
    }
    async fn source(&self, task: &Task, client: &Client) -> Result<AddTorrent<'static>> {
        let cached = self.metainfo_path(&task.id)?;
        if fs::try_exists(&cached).await? {
            return Ok(AddTorrent::from_bytes(read_bounded(&cached).await?));
        }
        if let Some(id) = task.request.url.strip_prefix("torrent:") {
            return Ok(AddTorrent::from_bytes(
                read_bounded(&self.metainfo_path(id)?).await?,
            ));
        }
        if task.request.url.starts_with("magnet:") {
            return Ok(AddTorrent::from_url(task.request.url.clone()));
        }
        let mut request = http::authenticate(
            client.get(&task.request.url),
            &task.request.url,
            &task.request.credentials,
        )?;
        if let Some(referer) = &task.request.referer {
            request = request.header(reqwest::header::REFERER, referer);
        }
        let response = request.send().await?.error_for_status()?;
        ensure!(
            response
                .content_length()
                .is_none_or(|size| size <= MAX_METAINFO as u64),
            "Torrent metadata exceeds eight MiB"
        );
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            ensure!(
                bytes.len().saturating_add(chunk.len()) <= MAX_METAINFO,
                "Torrent metadata exceeds eight MiB"
            );
            bytes.extend_from_slice(&chunk);
        }
        validate_metainfo(&bytes)?;
        Ok(AddTorrent::from_bytes(bytes))
    }
    pub async fn run(
        &self,
        task: &mut Task,
        client: &Client,
        store: &Store,
        events: &broadcast::Sender<Progress>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        validate_request(&task.request)?;
        let mut options = task.request.torrent.clone().unwrap_or_default();
        if task.request.torrent.is_none() && task.request.url.starts_with("magnet:") {
            options.only_files = librqbit::Magnet::parse(&task.request.url)?
                .get_select_only()
                .unwrap_or_default();
        }
        task.torrent = Some(TorrentSnapshot {
            seeded_seconds: task
                .torrent
                .as_ref()
                .map_or(0, |torrent| torrent.seeded_seconds),
            info_hash: String::new(),
            state: "resolving".into(),
            peers: 0,
            uploaded: task.torrent.as_ref().map_or(0, |torrent| torrent.uploaded),
            upload_bps: 0,
            files: Vec::new(),
            file_count: 0,
        });
        store.save(task.clone()).await?;
        let source = tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), result = self.source(task, client) => result? };
        let native = match &source {
            AddTorrent::Url(url) => librqbit::Magnet::parse(url)?.as_id32().is_some(),
            AddTorrent::TorrentFileBytes(bytes) => metainfo_version(bytes)?.is_some(),
        };
        if native {
            return self
                .run_native(task, source, options, store, events, cancel)
                .await;
        }
        let session = tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), result = self.session() => result? };
        let inspected = tokio::select! {
            _ = cancel.cancelled() => bail!("Cancelled"),
            result = tokio::time::timeout(Duration::from_secs(120), session.add_torrent(source, Some(AddTorrentOptions { list_only: true, initial_peers: Some(options.initial_peers.clone()), ..Default::default() }))) => result.context("Torrent metadata discovery timed out")??,
        };
        let AddTorrentResponse::ListOnly(inspected) = inspected else {
            bail!("This torrent is already active in another task");
        };
        ensure!(
            inspected.torrent_bytes.len() <= MAX_METAINFO,
            "Torrent metadata exceeds eight MiB"
        );
        let files = files_from_info(&inspected.info, &options)?;
        if task.request.expected_sha256.is_some() {
            ensure!(
                files.iter().filter(|file| file.selected).count() == 1,
                "Expected SHA-256 requires exactly one selected torrent file"
            );
        }
        validate_output(
            &task.destination,
            &files,
            &self
                .data
                .join("torrents")
                .join(format!("{}.destination.json", task.id)),
        )
        .await?;
        self.save_metainfo(&task.id, &inspected.torrent_bytes)
            .await?;
        task.torrent = Some(TorrentSnapshot {
            seeded_seconds: task
                .torrent
                .as_ref()
                .map_or(0, |torrent| torrent.seeded_seconds),
            info_hash: inspected.info_hash.as_string(),
            state: "initializing".into(),
            peers: 0,
            uploaded: task.torrent.as_ref().map_or(0, |torrent| torrent.uploaded),
            upload_bps: 0,
            file_count: files.len(),
            files,
        });
        task.total = Some(
            task.torrent
                .as_ref()
                .context("Missing torrent snapshot")?
                .files
                .iter()
                .filter(|file| file.selected)
                .map(|file| file.length)
                .sum(),
        );
        task.total_estimated = false;
        store.save(task.clone()).await?;
        let selected = (!options.only_files.is_empty()).then_some(options.only_files.clone());
        let added = session
            .add_torrent(
                AddTorrent::from_bytes(inspected.torrent_bytes),
                Some(AddTorrentOptions {
                    paused: true,
                    overwrite: true,
                    output_folder: Some(
                        task.destination
                            .to_str()
                            .context("Torrent destination must use valid Unicode")?
                            .into(),
                    ),
                    only_files: selected,
                    initial_peers: Some(options.initial_peers),
                    peer_limit: Some(options.peer_limit as usize),
                    ratelimits: librqbit::limits::LimitsConfig {
                        download_bps: rate(task.request.speed_limit)?,
                        upload_bps: rate(options.upload_limit)?,
                    },
                    ..Default::default()
                }),
            )
            .await?;
        let AddTorrentResponse::Added(id, handle) = added else {
            bail!("This torrent is already active in another task");
        };
        let result = self
            .transfer(task, &session, &handle, store, events, cancel)
            .await;
        if result.is_ok()
            && self
                .start_seed(
                    task.clone(),
                    SeedHandle::Rqbit {
                        session: session.clone(),
                        id,
                        handle: handle.clone(),
                    },
                    store.clone(),
                    events.clone(),
                )
                .await?
        {
            return Ok(());
        }
        let cleanup = session.delete(id.into(), false).await;
        result?;
        cleanup?;
        Ok(())
    }
    async fn run_native(
        &self,
        task: &mut Task,
        source: AddTorrent<'static>,
        options: TorrentOptions,
        store: &Store,
        events: &broadcast::Sender<Progress>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let session = {
            let mut native = self.native.lock().await;
            if let Some(session) = &*native {
                session.clone()
            } else {
                let rate = self
                    .download_budget
                    .load(Ordering::Acquire)
                    .min(i32::MAX as u64);
                let upload = store.settings().await?.torrent_upload_limit;
                let session = tokio::task::spawn_blocking(move || {
                    crate::torrent_native::Session::new(rate, upload, true)
                })
                .await??;
                *native = Some(session.clone());
                session
            }
        };
        let (bytes, magnet) = match source {
            AddTorrent::Url(url) => (url.into_owned().into_bytes(), true),
            AddTorrent::TorrentFileBytes(bytes) => (bytes.to_vec(), false),
        };
        let scratch = self
            .data
            .join("torrents")
            .join(format!("{}.metadata", task.id));
        fs::create_dir_all(&scratch).await?;
        let id = session.add(&bytes, magnet, &scratch)?;
        let result: Result<()> = async {
            for peer in &options.initial_peers { session.peer(id, *peer)?; }
            let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
            loop {
                ensure!(tokio::time::Instant::now() < deadline, "Torrent metadata discovery timed out");
                tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), _ = tokio::time::sleep(Duration::from_millis(200)) => {} }
                if session.stats(id)?.metadata != 0 { break; }
            }
            let info = session.metadata(id)?;
            let info_hash = info.info_hash.clone();
            let files = files_from_native(info, &options)?;
            ensure!(task.request.expected_sha256.is_none() || files.iter().filter(|file| file.selected).count() == 1, "Expected SHA-256 requires exactly one selected torrent file");
            validate_output(&task.destination, &files, &self.data.join("torrents").join(format!("{}.destination.json", task.id))).await?;
            if !magnet { self.save_metainfo(&task.id, &bytes).await?; }
            let previous = task.torrent.as_ref().context("Missing torrent snapshot")?;
            let uploaded_before = previous.uploaded;
            task.total = Some(files.iter().filter(|file| file.selected).map(|file| file.length).sum());
            task.torrent = Some(TorrentSnapshot { seeded_seconds: previous.seeded_seconds, info_hash, state: "initializing".into(), peers: 0, uploaded: uploaded_before, upload_bps: 0, file_count: files.len(), files });
            store.save(task.clone()).await?;
            session.start(id, &task.destination, &options, task.request.speed_limit)?;
            self.rebalance().await?;
            for peer in &options.initial_peers { session.peer(id, *peer)?; }
            let mut interval = tokio::time::interval(Duration::from_millis(300));
            loop {
                tokio::select! { _ = cancel.cancelled() => { session.pause_and_flush(id).await?; bail!("Cancelled"); }, _ = interval.tick() => {} }
                let stats = session.stats(id)?;
                task.completed = stats.completed.min(task.total.unwrap_or(0));
                task.source_validated |= task.completed > 0;
                let torrent = task.torrent.as_mut().context("Missing torrent snapshot")?;
                torrent.state = "downloading".into();
                torrent.peers = stats.peers as u32;
                torrent.upload_bps = stats.upload_rate;
                torrent.uploaded = uploaded_before.saturating_add(stats.uploaded);
                store.save(task.clone()).await?;
                let _ = events.send(Progress { id: task.id.clone(), completed: task.completed, total: task.total, bytes_per_second: stats.download_rate, total_estimated: false, media: None });
                if stats.finished != 0 && task.completed == task.total.unwrap_or(0) { break; }
            }
            session.pause_and_flush(id).await?;
            verify_outputs(task, store, events, cancel).await
        }.await;
        if result.is_ok()
            && self
                .start_seed(
                    task.clone(),
                    SeedHandle::Native {
                        session: session.clone(),
                        id,
                    },
                    store.clone(),
                    events.clone(),
                )
                .await?
        {
            return Ok(());
        }
        session.pause(id, true)?;
        session.remove(id)?;
        result
    }
    async fn transfer(
        &self,
        task: &mut Task,
        session: &Arc<Session>,
        handle: &Arc<librqbit::ManagedTorrent>,
        store: &Store,
        events: &broadcast::Sender<Progress>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), result = handle.wait_until_initialized() => result? };
        session.unpause(handle).await?;
        self.rebalance().await?;
        task.source_validated = true;
        let uploaded_before = task.torrent.as_ref().map_or(0, |torrent| torrent.uploaded);
        let mut interval = tokio::time::interval(Duration::from_millis(300));
        loop {
            tokio::select! { _ = cancel.cancelled() => {
                let stats = handle.stats();
                session.pause(handle).await?;
                update_stats(task, &stats, uploaded_before);
                if let Some(torrent) = &mut task.torrent { torrent.state = "paused".into(); }
                store.save(task.clone()).await?;
                bail!("Cancelled");
            }, _ = interval.tick() => {} }
            let stats = handle.stats();
            if let Some(error) = &stats.error {
                bail!("Torrent engine failed: {error}");
            }
            update_stats(task, &stats, uploaded_before);
            store.save(task.clone()).await?;
            let _ = events.send(Progress {
                id: task.id.clone(),
                completed: task.completed,
                total: task.total,
                bytes_per_second: stats
                    .live
                    .as_ref()
                    .map_or(0, |live| live.download_speed.as_bytes()),
                total_estimated: false,
                media: None,
            });
            if stats.finished {
                break;
            }
        }
        session.pause(handle).await?;
        verify_outputs(task, store, events, cancel).await
    }
}

async fn verify_outputs(
    task: &mut Task,
    store: &Store,
    events: &broadcast::Sender<Progress>,
    cancel: &CancellationToken,
) -> Result<()> {
    task.status = Status::Verifying;
    task.torrent
        .as_mut()
        .context("Missing torrent snapshot")?
        .state = "verifying".into();
    store.save(task.clone()).await?;
    let outputs = task
        .torrent
        .as_ref()
        .context("Missing torrent snapshot")?
        .files
        .clone();
    let mut aggregate = Sha256::new();
    for output in outputs.into_iter().filter(|file| file.selected) {
        let path = task.destination.join(&output.path);
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .await?;
        ensure!(
            file.metadata().await?.len() == output.length,
            "Torrent output has an incorrect length"
        );
        let mut digest = Sha256::new();
        let mut buffer = vec![0u8; 256 * 1024];
        loop {
            let count = tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), result = file.read(&mut buffer) => result? };
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
            aggregate.update(&buffer[..count]);
        }
        file.sync_all().await?;
        let hash = hex::encode(digest.finalize());
        if let Some(expected) = &task.request.expected_sha256 {
            ensure!(
                hash.eq_ignore_ascii_case(expected),
                "Torrent SHA-256 verification failed"
            );
        }
        task.torrent
            .as_mut()
            .context("Missing torrent snapshot")?
            .files
            .iter_mut()
            .find(|file| file.index == output.index)
            .context("Torrent file index disappeared during verification")?
            .sha256 = Some(hash);
    }
    let torrent = task.torrent.as_mut().context("Missing torrent snapshot")?;
    torrent.state = "complete".into();
    torrent.peers = 0;
    torrent.upload_bps = 0;
    task.sha256 = Some(hex::encode(aggregate.finalize()));
    task.completed = task.total.unwrap_or(0);
    task.status = Status::Complete;
    task.finished_at.get_or_insert(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
    );
    task.error = None;
    store.save(task.clone()).await?;
    let _ = events.send(Progress {
        id: task.id.clone(),
        completed: task.completed,
        total: task.total,
        bytes_per_second: 0,
        total_estimated: false,
        media: None,
    });
    Ok(())
}

type RqPool = Arc<Mutex<Option<Arc<Session>>>>;
type NativePool = Arc<Mutex<Option<Arc<crate::torrent_native::Session>>>>;

async fn balance_sessions(
    rq: &RqPool,
    native: &NativePool,
    download: u64,
    upload: u64,
) -> Result<()> {
    let rq = rq.lock().await.clone();
    let native = native.lock().await.clone();
    let (rq_down, rq_active) = rq.as_ref().map_or((0, 0), |session| {
        session.with_torrents(|torrents| {
            torrents.fold((0u64, 0u64), |(down, active), (_, handle)| {
                let stats = handle.stats();
                let live = matches!(stats.state, librqbit::TorrentStatsState::Live);
                (
                    down + u64::from(live && !stats.finished),
                    active + u64::from(live),
                )
            })
        })
    });
    let (native_down, native_active) = native
        .as_ref()
        .map(|session| session.counts())
        .transpose()?
        .unwrap_or_default();
    let share = |limit: u64, count: u64, total: u64| {
        if limit == 0 || total == 0 || count == 0 {
            limit
        } else {
            (u128::from(limit) * u128::from(count) / u128::from(total)).max(1) as u64
        }
    };
    if let Some(session) = rq {
        session
            .ratelimits
            .set_download_bps(rate(share(download, rq_down, rq_down + native_down))?);
        session.ratelimits.set_upload_bps(rate(share(
            upload,
            rq_active,
            rq_active + native_active,
        ))?);
    }
    if let Some(session) = native {
        session.limits(
            share(download, native_down, rq_down + native_down),
            share(upload, native_active, rq_active + native_active),
        )?;
    }
    Ok(())
}

fn rate(value: u64) -> Result<Option<NonZeroU32>> {
    if value == 0 {
        return Ok(None);
    }
    Ok(NonZeroU32::new(u32::try_from(value).context(
        "Torrent bandwidth limit exceeds four GiB per second",
    )?))
}

fn update_stats(task: &mut Task, stats: &librqbit::TorrentStats, uploaded_before: u64) {
    task.completed = stats
        .progress_bytes
        .min(task.total.unwrap_or(stats.total_bytes));
    if let Some(torrent) = &mut task.torrent {
        torrent.state = match stats.state {
            librqbit::TorrentStatsState::Live => "downloading",
            librqbit::TorrentStatsState::Paused => "paused",
            librqbit::TorrentStatsState::Error => "failed",
            _ => "initializing",
        }
        .into();
        torrent.uploaded = uploaded_before.saturating_add(stats.uploaded_bytes);
        torrent.peers = stats
            .live
            .as_ref()
            .map_or(0, |live| live.snapshot.peer_stats.live);
        torrent.upload_bps = stats
            .live
            .as_ref()
            .map_or(0, |live| live.upload_speed.as_bytes());
    }
}

fn metainfo_version(bytes: &[u8]) -> Result<Option<u32>> {
    crate::bencode_guard::validate(bytes)?;
    #[derive(serde::Deserialize)]
    struct Metainfo {
        info: Info,
    }
    #[derive(serde::Deserialize)]
    struct Info {
        #[serde(rename = "meta version")]
        version: Option<u32>,
    }
    let info: Metainfo =
        librqbit_bencode::from_bytes(bytes).map_err(|error| anyhow::anyhow!("{error}"))?;
    Ok(info.info.version)
}

fn validate_metainfo(bytes: &[u8]) -> Result<()> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_METAINFO,
        "Torrent metadata must contain between one byte and eight MiB"
    );
    if let Some(version) = metainfo_version(bytes)? {
        ensure!(version == 2, "Unsupported torrent metadata version");
        let info = crate::torrent_native::Session::describe(bytes)?;
        ensure!(
            info.v2,
            "Torrent declares v2 without a valid SHA-256 info hash"
        );
        files_from_native(info, &TorrentOptions::default())?;
        return Ok(());
    }
    let parsed = librqbit::torrent_from_bytes(bytes)?;
    let info = parsed.info.data.validate()?;
    files_from_info(&info, &TorrentOptions::default())?;
    Ok(())
}

fn files_from_native(
    info: crate::torrent_native::MetadataInfo,
    options: &TorrentOptions,
) -> Result<Vec<TorrentFile>> {
    ensure!(
        !info.files.is_empty() && info.files.len() <= MAX_FILES,
        "Invalid torrent file count"
    );
    let mut seen = HashSet::new();
    let mut indexes = HashSet::new();
    let mut total = 0u64;
    let mut files = Vec::new();
    for file in info.files {
        let path = Path::new(&file.path);
        ensure!(
            !path.is_absolute()
                && path
                    .components()
                    .all(|component| matches!(component, Component::Normal(_))),
            "Unsafe torrent path"
        );
        for component in path.components() {
            model::validate(&DownloadRequest {
                url: "https://example.invalid/file".into(),
                filename: component
                    .as_os_str()
                    .to_str()
                    .context("Invalid torrent filename")?
                    .into(),
                ..Default::default()
            })?;
        }
        ensure!(
            !file.path.is_empty()
                && file.index < MAX_FILES
                && seen.insert(file.path.to_lowercase())
                && indexes.insert(file.index),
            "Duplicate or invalid torrent file"
        );
        total = total
            .checked_add(file.length)
            .context("Torrent size overflow")?;
        ensure!(total <= i64::MAX as u64, "Torrent exceeds supported size");
        files.push(TorrentFile {
            index: file.index,
            path: file.path,
            length: file.length,
            selected: options.only_files.is_empty() || options.only_files.contains(&file.index),
            sha256: None,
        });
    }
    ensure!(
        options
            .only_files
            .iter()
            .all(|index| indexes.contains(index)),
        "Torrent file index is out of range"
    );
    Ok(files)
}

fn files_from_info<B: AsRef<[u8]>>(
    info: &librqbit::ValidatedTorrentMetaV1Info<B>,
    options: &TorrentOptions,
) -> Result<Vec<TorrentFile>> {
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    let mut total = 0u64;
    for (index, file) in info.iter_file_details().enumerate() {
        ensure!(index < MAX_FILES, "Torrent exceeds ten thousand files");
        ensure!(
            !file.attrs().symlink && file.symlink_path.is_none(),
            "Torrent symbolic-link entries are forbidden"
        );
        let path = file.filename.to_pathbuf();
        let path_string = path
            .to_str()
            .context("Torrent filenames must use valid Unicode")?
            .to_owned();
        ensure!(
            path.components()
                .all(|component| matches!(component, Component::Normal(_)))
                && !path_string.is_empty(),
            "Unsafe torrent path"
        );
        for component in path.components() {
            let name = component
                .as_os_str()
                .to_str()
                .context("Invalid torrent filename")?;
            let request = DownloadRequest {
                url: "https://example.invalid/file".into(),
                filename: name.into(),
                ..Default::default()
            };
            model::validate(&request).context("Unsafe torrent filename")?;
        }
        ensure!(
            seen.insert(path_string.to_lowercase()),
            "Duplicate torrent path"
        );
        total = total
            .checked_add(file.len)
            .context("Torrent size overflow")?;
        ensure!(total <= i64::MAX as u64, "Torrent is too large");
        files.push(TorrentFile {
            index,
            path: path_string,
            length: file.len,
            selected: options.only_files.is_empty() || options.only_files.contains(&index),
            sha256: None,
        });
    }
    ensure!(!files.is_empty(), "Torrent contains no files");
    ensure!(
        options.only_files.iter().all(|index| *index < files.len()),
        "Torrent file index is out of range"
    );
    Ok(files)
}

async fn validate_output(root: &Path, files: &[TorrentFile], ownership: &Path) -> Result<()> {
    if fs::try_exists(ownership).await? {
        let stored: PathBuf = serde_json::from_slice(&fs::read(ownership).await?)?;
        ensure!(stored == root, "Torrent destination ownership changed");
    } else {
        ensure!(
            !fs::try_exists(root).await?,
            "Torrent destination already exists without task ownership"
        );
        let stage = ownership.with_extension("staging");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&stage)
            .await?;
        file.write_all(&serde_json::to_vec(root)?).await?;
        file.sync_all().await?;
        drop(file);
        fs::rename(&stage, ownership).await?;
    }
    if !fs::try_exists(root).await? {
        fs::create_dir(root).await?;
    }
    ensure!(
        fs::symlink_metadata(root).await?.file_type().is_dir(),
        "Torrent destination must be an ordinary directory"
    );
    for file in files {
        let mut path = root.to_path_buf();
        for component in Path::new(&file.path).components() {
            path.push(component);
            match fs::symlink_metadata(&path).await {
                Ok(metadata) => ensure!(
                    !metadata.file_type().is_symlink(),
                    "Torrent output cannot contain symbolic links"
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}

async fn read_bounded(path: &Path) -> Result<Bytes> {
    let file = fs::File::open(path).await?;
    let metadata = file.metadata().await?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_METAINFO as u64,
        "Torrent metadata exceeds eight MiB or is not a regular file"
    );
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_METAINFO as u64 + 1)
        .read_to_end(&mut bytes)
        .await?;
    ensure!(
        bytes.len() <= MAX_METAINFO,
        "Torrent metadata exceeds eight MiB"
    );
    validate_metainfo(&bytes)?;
    Ok(bytes.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extensionless_torrent_sources_use_case_insensitive_mime_routing() {
        let mut request = DownloadRequest {
            url: "https://example.test/download?id=7".into(),
            filename: "Linux".into(),
            content_type: Some(" Application/X-BitTorrent ; charset=binary".into()),
            ..Default::default()
        };
        assert!(is_torrent(&request));
        assert!(model::validate(&request).is_ok());
        request.content_type = Some("application/octet-stream".into());
        assert!(!is_torrent(&request));
    }
    #[test]
    fn magnet_validation_bounds_selection_and_protocol() {
        let mut request = DownloadRequest {
            url: "magnet:?xt=urn:btih:cab507494d02ebb1178b38f2e9d7be299c86b862&dn=Linux".into(),
            filename: "Linux".into(),
            ..Default::default()
        };
        assert!(model::validate(&request).is_ok());
        request.url.push_str("&so=0-999999999999");
        assert!(model::validate(&request).is_err());
        request.url = "torrent:../../private".into();
        assert!(model::validate(&request).is_err());
        request.url = "magnet:?xt=urn:btmh:12200000000000000000000000000000000000000000000000000000000000000000".into();
        assert!(model::validate(&request).is_ok());
        request.url = "magnet:?xt=urn:btmh:122000".into();
        assert!(model::validate(&request).is_err());
    }
    #[test]
    fn limits_reject_unusable_piece_quotas() {
        assert!(validate_options(&TorrentOptions::default(), 1024).is_err());
        assert!(validate_options(&TorrentOptions::default(), 65536).is_ok());
        assert!(validate_options(
            &TorrentOptions {
                peer_limit: 0,
                ..Default::default()
            },
            0
        )
        .is_err());
    }
}
