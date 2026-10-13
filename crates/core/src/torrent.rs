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
    collections::HashSet,
    num::NonZeroU32,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{broadcast, Mutex},
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
            magnet.as_id20().is_some(),
            "BitTorrent v2-only magnets are not supported by this engine"
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
    session: Mutex<Option<Arc<Session>>>,
    download_budget: AtomicU64,
}

impl TorrentManager {
    pub fn new(data: PathBuf, downloads: PathBuf) -> Self {
        Self {
            data,
            downloads,
            session: Mutex::new(None),
            download_budget: AtomicU64::new(0),
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
                    upload_bps: None,
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
        if let Some(session) = self.session.lock().await.as_ref() {
            session.ratelimits.set_download_bps(rate(limit)?);
        }
        Ok(())
    }
    pub async fn shutdown(&self) {
        if let Some(session) = self.session.lock().await.take() {
            session.stop().await;
        }
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
            info_hash: String::new(),
            state: "resolving".into(),
            peers: 0,
            uploaded: task.torrent.as_ref().map_or(0, |torrent| torrent.uploaded),
            upload_bps: 0,
            files: Vec::new(),
            file_count: 0,
        });
        store.save(task.clone()).await?;
        let session = tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), result = self.session() => result? };
        let source = tokio::select! { _ = cancel.cancelled() => bail!("Cancelled"), result = self.source(task, client) => result? };
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
        let cleanup = session.delete(id.into(), false).await;
        result?;
        cleanup?;
        Ok(())
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
                .get_mut(output.index)
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
        task.finished_at = Some(
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
}

fn rate(value: u64) -> Result<Option<NonZeroU32>> {
    if value == 0 {
        return Ok(None);
    }
    Ok(NonZeroU32::new(
        u32::try_from(value)
            .context("Torrent bandwidth limit exceeds four GiB per second")?
            .max(16384),
    ))
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

fn validate_metainfo(bytes: &[u8]) -> Result<()> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_METAINFO,
        "Torrent metadata must contain between one byte and eight MiB"
    );
    let parsed = librqbit::torrent_from_bytes(bytes)?;
    let info = parsed.info.data.validate()?;
    files_from_info(&info, &TorrentOptions::default())?;
    Ok(())
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
