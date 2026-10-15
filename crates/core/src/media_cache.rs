//! A private HTTP gateway keeps FFmpeg's network reads bounded, cancellable and recoverable.
use crate::{http, limiter::Limiter, model, DownloadRequest};
use anyhow::{ensure, Context, Result};
use axum::{
    body::Body,
    extract::{OriginalUri, State},
    http::{HeaderMap, Response, StatusCode},
    routing::get,
    Router,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::{Mutex, Semaphore},
};
use tokio_util::{io::ReaderStream, sync::CancellationToken};
use url::Url;

#[derive(Clone)]
struct GatewayState(Arc<Shared>);
struct Shared {
    client: crate::transport::Client,
    directory: PathBuf,
    origins: Mutex<HashMap<String, Url>>,
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    token: String,
    base: String,
    session: String,
    referer: Option<String>,
    credentials: Vec<crate::BrowserCredentials>,
    manifest_video_id: Option<String>,
    manifest_audio_id: Option<String>,
    manifest_snapshot: Option<crate::model::ManifestSnapshot>,
    cancel: CancellationToken,
    limiter: Limiter,
    global_limiter: Arc<Limiter>,
    connections: Arc<Semaphore>,
    file_connections: Semaphore,
    source_connections: u8,
    immutable_keys: HashMap<String, String>,
    pub bytes: Arc<AtomicU64>,
    cached_bytes: Arc<AtomicU64>,
    last_error: Mutex<HashMap<String, String>>,
    tracker: Arc<crate::media_progress::Tracker>,
}
pub(crate) struct Gateway {
    state: GatewayState,
    server: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
}
#[derive(Default, Serialize, Deserialize)]
struct Cached {
    etag: Option<String>,
    total: Option<u64>,
    done: u64,
    digest: String,
    complete: bool,
    session: String,
    content_type: String,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ManifestKind {
    Hls,
    Dash,
}
async fn manifest_kind(remote: &Url, path: &Path, cache: &Cached) -> Result<Option<ManifestKind>> {
    let path_suffix = remote.path().to_ascii_lowercase();
    let content_type = cache.content_type.to_ascii_lowercase();
    if path_suffix.ends_with(".m3u8") || content_type.contains("mpegurl") {
        return Ok(Some(ManifestKind::Hls));
    }
    if path_suffix.ends_with(".mpd") || content_type.contains("dash+xml") {
        return Ok(Some(ManifestKind::Dash));
    }
    if cache.done <= 2 * 1024 * 1024 {
        let mut prefix = [0; 1024];
        let mut file = File::open(path).await?;
        let count = file.read(&mut prefix).await?;
        let text = String::from_utf8_lossy(&prefix[..count]);
        let text = text.trim_start_matches('\u{feff}').trim_start();
        if text.starts_with("#EXTM3U") {
            return Ok(Some(ManifestKind::Hls));
        }
        if text.starts_with('<') && text.contains("<MPD") {
            return Ok(Some(ManifestKind::Dash));
        }
    }
    Ok(None)
}
struct RangePlan<'a> {
    remote: &'a Url,
    key: &'a str,
    revision: &'a str,
    workers: usize,
}

fn google_revision(remote: &Url) -> Option<(u64, String)> {
    let host = remote.host_str()?;
    if remote.scheme() != "https" || !host.ends_with(".googlevideo.com") {
        return None;
    }
    let query: HashMap<_, _> = remote.query_pairs().collect();
    let length: u64 = query.get("clen")?.parse().ok()?;
    let modified: u64 = query.get("lmt")?.parse().ok()?;
    let format: u32 = query.get("itag")?.parse().ok()?;
    if length == 0 || modified == 0 {
        return None;
    }
    Some((length, format!("{format}:{modified}:{length}")))
}
async fn file_digest(path: &Path) -> Result<String> {
    let mut file = File::open(path).await?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let size = file.read(&mut buffer).await?;
        if size == 0 {
            break;
        }
        digest.update(&buffer[..size]);
    }
    Ok(hex::encode(digest.finalize()))
}

impl Gateway {
    pub async fn open(
        request: &DownloadRequest,
        directory: &Path,
        connections: Arc<Semaphore>,
        global_limiter: Arc<Limiter>,
        cancel: CancellationToken,
        healing: Option<(Arc<crate::healing::RefreshBroker>, String)>,
    ) -> Result<Self> {
        fs::create_dir_all(directory).await?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mut immutable_keys = HashMap::new();
        if let Some(provider) = request
            .media
            .as_ref()
            .and_then(|media| media.provider.as_ref())
        {
            for (track, source) in [
                ("video", Some(&request.url)),
                (
                    "audio",
                    request
                        .media
                        .as_ref()
                        .and_then(|media| media.audio_url.as_ref()),
                ),
            ] {
                if let Some(source) = source {
                    if let Ok(remote) = Url::parse(source) {
                        if let Some((_, revision)) = google_revision(&remote) {
                            immutable_keys.insert(
                                source.clone(),
                                format!(
                                    "{}:{}:{track}:{revision}",
                                    provider.url,
                                    provider.format_id.as_deref().unwrap_or("")
                                ),
                            );
                        }
                    }
                }
            }
        }
        let state = GatewayState(Arc::new(Shared {
            client: match healing {
                Some((broker, id)) => http::client()?.with_healing(
                    broker,
                    request.browser_source.as_ref(),
                    &id,
                    &cancel,
                ),
                None => http::client()?,
            }
            .with_policy(request.transport),
            directory: directory.to_owned(),
            origins: Mutex::new(HashMap::new()),
            locks: Mutex::new(HashMap::new()),
            token: uuid::Uuid::new_v4().simple().to_string(),
            base: format!("http://{}", listener.local_addr()?),
            session: uuid::Uuid::new_v4().to_string(),
            referer: request.referer.clone(),
            credentials: request.credentials.clone(),
            manifest_video_id: request
                .media
                .as_ref()
                .and_then(|media| media.manifest_video_id.clone()),
            manifest_audio_id: request
                .media
                .as_ref()
                .and_then(|media| media.manifest_audio_id.clone()),
            manifest_snapshot: request
                .media
                .as_ref()
                .and_then(|media| media.manifest_snapshot.clone()),
            cancel: cancel.child_token(),
            limiter: Limiter::new(request.speed_limit),
            global_limiter,
            connections,
            file_connections: Semaphore::new(if request.connections == 0 {
                8
            } else {
                request.connections as usize
            }),
            source_connections: request.connections,
            immutable_keys,
            bytes: Arc::new(AtomicU64::new(0)),
            cached_bytes: Arc::new(AtomicU64::new(0)),
            last_error: Mutex::new(HashMap::new()),
            tracker: Arc::new(crate::media_progress::Tracker::new(request)),
        }));
        let router = Router::new()
            .route("/{*path}", get(handle))
            .with_state(state.clone());
        let stop = state.0.cancel.clone();
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(stop.cancelled_owned())
                .await
        });
        Ok(Self {
            state,
            server: Some(server),
        })
    }
    pub async fn url(&self, url: &str) -> Result<String> {
        self.state.proxy_url(url).await
    }
    pub async fn is_hls_input(&self, url: &str) -> Result<bool> {
        let url = Url::parse(url)?;
        ensure!(
            url.origin().ascii_serialization() == self.state.0.base,
            "Media input must use its task gateway"
        );
        let uri = url[url::Position::BeforePath..].parse()?;
        let remote = self.state.remote_url(&uri).await?;
        if let Some(snapshot) = &self.state.0.manifest_snapshot {
            if remote.as_str() == snapshot.url {
                return Ok(snapshot.text.trim_start().starts_with("#EXTM3U"));
            }
        }
        let (path, cache) = self.state.cached(&remote).await?;
        Ok(manifest_kind(&remote, &path, &cache).await? == Some(ManifestKind::Hls))
    }
    pub async fn discover(
        &self,
        url: &str,
    ) -> Result<(Vec<crate::SubtitleTrack>, Vec<crate::MediaChapter>)> {
        let base = Url::parse(url)?;
        let (path, saved) = self.state.cached(&base).await?;
        ensure!(
            saved.done <= crate::media_assets::MAX_ASSET as u64,
            "Manifest exceeds discovery limit"
        );
        crate::media_assets::discover(&base, &fs::read_to_string(path).await?)
    }
    pub async fn subtitle_input(&self, url: &str, index: usize) -> Result<String> {
        let remote = Url::parse(url)?;
        let text = self.asset_text(&remote).await?;
        let output = self
            .state
            .0
            .directory
            .parent()
            .context("Missing media task directory")?
            .join(format!("subtitle-input-{index}.vtt"));
        let merged = if text.trim_start_matches('\u{feff}').starts_with("#EXTM3U") {
            ensure!(
                text.contains("#EXT-X-ENDLIST"),
                "External subtitles require a finite playlist"
            );
            ensure!(
                !text.contains("#EXT-X-KEY:"),
                "Encrypted subtitle playlists are not supported"
            );
            let mut segments = Vec::new();
            let mut duration = None;
            let mut size = 0;
            for line in text.lines() {
                if let Some(value) = line.strip_prefix("#EXTINF:") {
                    duration = Some(
                        value
                            .split(',')
                            .next()
                            .context("Missing subtitle duration")?
                            .parse::<f64>()?,
                    );
                } else if !line.trim().is_empty() && !line.starts_with('#') {
                    ensure!(segments.len() < 2048, "Too many subtitle segments");
                    let source = remote.join(line.trim())?;
                    model::validate_url(source.as_str())?;
                    let text = self.asset_text(&source).await?;
                    size += text.len();
                    ensure!(
                        size <= crate::media_assets::MAX_ASSET,
                        "Subtitles exceed size limit"
                    );
                    segments.push((
                        duration
                            .take()
                            .context("Subtitle segment has no duration")?,
                        text,
                    ));
                }
            }
            crate::media_assets::merge_webvtt(&segments)?
        } else {
            text
        };
        fs::write(&output, merged).await?;
        Ok(output.to_string_lossy().into())
    }
    async fn asset_text(&self, remote: &Url) -> Result<String> {
        model::validate_url(remote.as_str())?;
        let mut request = http::authenticate(
            self.state
                .0
                .client
                .get(remote.clone())
                .header(reqwest::header::ACCEPT_ENCODING, "identity"),
            remote.as_str(),
            &self.state.0.credentials,
        )?;
        if let Some(referer) = &self.state.0.referer {
            request = request.header(reqwest::header::REFERER, referer);
        }
        let response = tokio::select! { _ = self.state.0.cancel.cancelled() => anyhow::bail!("Cancelled"), response = request.send() => response? };
        response.error_for_status_ref()?;
        let mut stream = response.bytes_stream();
        let mut text = Vec::new();
        loop {
            let next = tokio::select! { _ = self.state.0.cancel.cancelled() => anyhow::bail!("Cancelled"), next = stream.next() => next };
            let Some(chunk) = next else {
                break;
            };
            let chunk = chunk?;
            ensure!(
                text.len() + chunk.len() <= crate::media_assets::MAX_ASSET,
                "Subtitle input exceeds size limit"
            );
            for bytes in chunk.chunks(65536) {
                self.state
                    .0
                    .limiter
                    .acquire(bytes.len(), &self.state.0.cancel)
                    .await?;
                self.state
                    .0
                    .global_limiter
                    .acquire(bytes.len(), &self.state.0.cancel)
                    .await?;
            }
            self.state
                .0
                .bytes
                .fetch_add(chunk.len() as u64, Ordering::Relaxed);
            text.extend_from_slice(&chunk);
        }
        Ok(String::from_utf8(text)?)
    }
    pub fn bytes(&self) -> Arc<AtomicU64> {
        self.state.0.bytes.clone()
    }
    pub fn tracker(&self) -> Arc<crate::media_progress::Tracker> {
        self.state.0.tracker.clone()
    }
    pub async fn last_error(&self) -> Option<String> {
        self.state
            .0
            .last_error
            .lock()
            .await
            .values()
            .next()
            .cloned()
    }
    pub async fn concat(&self, urls: &[String]) -> Result<PathBuf> {
        let mut resources =
            futures_util::stream::iter(urls.iter().cloned().enumerate().map(|(index, url)| {
                let state = self.state.clone();
                async move {
                    let (path, _) = state.cached(&Url::parse(&url)?).await?;
                    Ok::<_, anyhow::Error>((index, path))
                }
            }))
            .buffer_unordered(self.state.0.file_connections.available_permits().max(1));
        let mut paths = Vec::new();
        while let Some(resource) = resources.next().await {
            paths.push(resource?);
        }
        paths.sort_by_key(|(index, _)| *index);
        let mut descriptor = String::from("ffconcat version 1.0\n");
        for (_, path) in paths {
            descriptor.push_str(&format!(
                "file '{}'\n",
                path.file_name()
                    .context("Missing cache filename")?
                    .to_string_lossy()
            ));
        }
        let path = self.state.0.directory.join("segments.ffconcat");
        fs::write(&path, descriptor).await?;
        Ok(path)
    }
    pub async fn shutdown(mut self) -> Result<()> {
        self.state.0.cancel.cancel();
        if let Some(server) = self.server.take() {
            server.await??;
        }
        Ok(())
    }
}
impl Drop for Gateway {
    fn drop(&mut self) {
        self.state.0.cancel.cancel();
    }
}
impl GatewayState {
    async fn proxy_url(&self, value: &str) -> Result<String> {
        model::validate_url(value)?;
        let remote = Url::parse(value)?;
        let origin = remote.origin().ascii_serialization();
        let id = hex::encode(&Sha256::digest(origin.as_bytes())[..12]);
        self.0
            .origins
            .lock()
            .await
            .insert(id.clone(), Url::parse(&format!("{origin}/"))?);
        Ok(format!(
            "{}/{}/{id}{}{}",
            self.0.base,
            self.0.token,
            remote.path(),
            remote
                .query()
                .map(|query| format!("?{query}"))
                .unwrap_or_default()
        ))
    }
    async fn remote_url(&self, uri: &axum::http::Uri) -> Result<Url> {
        let mut segments = uri.path().trim_start_matches('/').splitn(3, '/');
        ensure!(
            segments.next() == Some(&self.0.token),
            "Invalid media gateway token"
        );
        let origin = segments.next().context("Missing media origin")?;
        let path = segments.next().unwrap_or("");
        let base = self
            .0
            .origins
            .lock()
            .await
            .get(origin)
            .cloned()
            .context("Unknown media origin")?;
        let mut url = base.join(&format!("/{path}"))?;
        url.set_query(uri.query());
        model::validate_url(url.as_str())?;
        Ok(url)
    }
    async fn cached(&self, remote: &Url) -> Result<(PathBuf, Cached)> {
        let progress = self.0.tracker.resource(remote)?;
        let identity = self.0.immutable_keys.get(remote.as_str());
        let key = hex::encode(Sha256::digest(
            identity
                .map(String::as_str)
                .unwrap_or(remote.as_str())
                .as_bytes(),
        ));
        let lock = {
            let mut locks = self.0.locks.lock().await;
            ensure!(
                locks.contains_key(&key) || locks.len() < 65536,
                "Media contains too many resources"
            );
            locks
                .entry(key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _guard = tokio::select! { _ = self.0.cancel.cancelled() => anyhow::bail!("Cancelled"), guard = lock.lock() => guard };
        let _file_permit = tokio::select! { _ = self.0.cancel.cancelled() => anyhow::bail!("Cancelled"), permit = self.0.file_connections.acquire() => permit? };
        let path = self.0.directory.join(format!("{key}.cache"));
        let journal = self.0.directory.join(format!("{key}.json"));
        let mut saved: Cached = match fs::read(&journal).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Cached::default(),
        };
        let playlist = matches!(remote.path().rsplit('.').next(), Some("m3u8" | "mpd"))
            || saved.content_type.contains("mpegurl")
            || saved.content_type.contains("dash+xml");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .await?;
        let mut hash = Sha256::new();
        if file.metadata().await?.len() >= saved.done && saved.done > 0 {
            file.set_len(saved.done).await?;
            let mut buffer = [0; 65536];
            let mut remaining = saved.done;
            while remaining > 0 {
                let n = file
                    .read(&mut buffer[..remaining.min(65536) as usize])
                    .await?;
                ensure!(n > 0, "Media cache is truncated");
                hash.update(&buffer[..n]);
                remaining -= n as u64;
            }
            if hex::encode(hash.clone().finalize()) != saved.digest {
                saved = Cached::default();
                hash = Sha256::new();
            }
        } else {
            saved = Cached::default();
        }
        if !playlist && saved.complete && saved.session == self.0.session {
            progress.metadata(saved.total, &saved.content_type);
            progress.finish(saved.done);
            return Ok((path, saved));
        }
        let mut attempt = 0;
        loop {
            let result = async {
                let metadata = {
                    let _permit = tokio::select! { _ = self.0.cancel.cancelled() => anyhow::bail!("Cancelled"), permit = self.0.connections.acquire() => permit? };
                    tokio::select! { _ = self.0.cancel.cancelled() => anyhow::bail!("Cancelled"), value = http::probe(&self.0.client, remote.as_str(), self.0.referer.as_deref(), &self.0.credentials) => value? }
                };
                progress.metadata(metadata.total, &metadata.content_type);
                let immutable = identity.is_some() && google_revision(remote).is_some_and(|(total, _)| Some(total) == metadata.total);
                let safe_ranges = metadata.ranged || immutable && metadata.range_supported;
                let reusable = !playlist && safe_ranges && saved.etag == metadata.etag && saved.total == metadata.total;
                if reusable && saved.complete { progress.finish(saved.done); saved.session = self.0.session.clone(); self.0.cached_bytes.fetch_add(saved.done, Ordering::Relaxed); return Ok(()); }
                if !reusable { saved = Cached { etag: metadata.etag, total: metadata.total, ..Cached::default() }; hash = Sha256::new(); }
                saved.content_type = metadata.content_type;
                progress.reset(saved.done);
                saved.complete = false;
                let workers = if self.0.source_connections == 0 { saved.total.unwrap_or(0).div_ceil(4 * 1024 * 1024).clamp(1, 32) as usize } else { self.0.source_connections as usize };
                if !playlist && !saved.content_type.contains("mpegurl") && !saved.content_type.contains("dash+xml") && safe_ranges && workers > 1 && saved.total.is_some_and(|total| total > 4 * 1024 * 1024) {
                    let revision = identity.cloned().unwrap_or_else(|| format!("{}:{}", saved.etag.as_deref().unwrap_or(""), saved.total.unwrap_or(0)));
                    self.parallel(RangePlan { remote, key: &key, revision: &revision, workers }, &mut file, &mut saved, &mut hash).await?;
                    saved.complete = true; saved.session = self.0.session.clone(); progress.finish(saved.done);
                    return Ok(());
                }
                let _permit = tokio::select! { _ = self.0.cancel.cancelled() => anyhow::bail!("Cancelled"), permit = self.0.connections.acquire() => permit? };
                file.set_len(saved.done).await?; file.seek(std::io::SeekFrom::Start(saved.done)).await?;
                let mut request = http::authenticate(self.0.client.get(remote.clone()).header(reqwest::header::ACCEPT_ENCODING,"identity"), remote.as_str(), &self.0.credentials)?;
                if let Some(referer) = &self.0.referer { request = request.header(reqwest::header::REFERER, referer); }
                if saved.done > 0 { request = request.header(reqwest::header::RANGE,format!("bytes={}-",saved.done)); if let Some(etag) = &saved.etag { request = request.header(reqwest::header::IF_MATCH, etag); } else { ensure!(immutable,"Resume requires a validator"); } }
                else if metadata.range_supported && metadata.total.is_some_and(|total| total > 0) { request = request.header(reqwest::header::RANGE,format!("bytes=0-{}",metadata.total.unwrap_or(1)-1)); }
                let response = tokio::select! { _ = self.0.cancel.cancelled() => anyhow::bail!("Cancelled"), response = request.send() => response? };
                if let Some(error) = http::transient(&response) { return Err(error.into()); }
                response.error_for_status_ref().map_err(reqwest::Error::without_url)?; http::identity(&response)?;
                if saved.done > 0 {
                    ensure!(response.status() == reqwest::StatusCode::PARTIAL_CONTENT,"Media server ignored resume range");
                    let range = http::content_range(response.headers().get(reqwest::header::CONTENT_RANGE).context("Missing media Content-Range")?.to_str()?)?;
                    ensure!(range.0 == saved.done && Some(range.2) == saved.total && range.1 + 1 == range.2,"Invalid media resume range");
                } else if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                    let range = http::content_range(response.headers().get(reqwest::header::CONTENT_RANGE).context("Missing media Content-Range")?.to_str()?)?;
                    ensure!(range.0 == 0 && Some(range.2) == saved.total && range.1 + 1 == range.2,"Invalid initial media range");
                } else { ensure!(response.status() == reqwest::StatusCode::OK,"Unexpected media response status"); }
                saved.content_type = response.headers().get(reqwest::header::CONTENT_TYPE).and_then(|value|value.to_str().ok()).unwrap_or("application/octet-stream").to_owned();
                let mut stream = response.bytes_stream(); let mut checkpoint = saved.done; let mut checkpoint_time = tokio::time::Instant::now();
                loop {
                    let chunk = tokio::select! { _ = self.0.cancel.cancelled() => anyhow::bail!("Cancelled"), chunk = stream.next() => chunk };
                    let Some(chunk) = chunk else { break }; let chunk = chunk?;
                    for bytes in chunk.chunks(65536) {
                        self.0.limiter.acquire(bytes.len(), &self.0.cancel).await?;
                        self.0.global_limiter.acquire(bytes.len(), &self.0.cancel).await?;
                        ensure!(saved.total.is_none_or(|total| saved.done + bytes.len() as u64 <= total),"Media response exceeds declared length");
                        if playlist || saved.content_type.contains("mpegurl") || saved.content_type.contains("dash+xml") { ensure!(saved.done + bytes.len() as u64 <= 2 * 1024 * 1024,"Media manifest exceeds two MiB"); }
                        file.write_all(bytes).await?; hash.update(bytes); saved.done += bytes.len() as u64; progress.done.store(saved.done, Ordering::Relaxed); self.0.bytes.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                    }
                    if saved.done - checkpoint >= 4 * 1024 * 1024 || checkpoint_time.elapsed() >= Duration::from_secs(1) {
                        self.checkpoint(&mut file,&journal,&mut saved,&hash).await?; checkpoint = saved.done; checkpoint_time = tokio::time::Instant::now();
                    }
                }
                if saved.total.is_some_and(|total| total != saved.done) { return Err(http::Retryable { delay: 0, message: "Media stream ended before its declared length".into() }.into()); }
                saved.total = Some(saved.done); saved.complete = true; saved.session = self.0.session.clone(); progress.finish(saved.done);
                anyhow::Ok(())
            }.await;
            self.checkpoint(&mut file, &journal, &mut saved, &hash)
                .await?;
            match result {
                Ok(()) => return Ok((path, saved)),
                Err(error) => {
                    if self.0.cancel.is_cancelled() {
                        return Err(error);
                    }
                    let retry = error.downcast_ref::<http::Retryable>();
                    let transient = retry.is_some()
                        || error.downcast_ref::<reqwest::Error>().is_some_and(|error| {
                            error.is_body()
                                || error.is_connect()
                                || error.is_timeout()
                                || error.is_request()
                        });
                    let seconds = retry
                        .map(|retry| retry.delay)
                        .unwrap_or(0)
                        .max(1 << attempt);
                    if !transient || attempt >= http::MAX_RETRIES || seconds > 300 {
                        return Err(error);
                    }
                    attempt += 1;
                    tokio::select! { _ = self.0.cancel.cancelled() => return Err(error), _ = tokio::time::sleep(Duration::from_secs(seconds)) => {} }
                }
            }
        }
    }
    async fn parallel(
        &self,
        plan: RangePlan<'_>,
        file: &mut File,
        saved: &mut Cached,
        hash: &mut Sha256,
    ) -> Result<()> {
        const CHUNK: u64 = 4 * 1024 * 1024;
        let total = saved.total.context("Missing media length")?;
        ensure!(
            total.div_ceil(CHUNK) <= 65536,
            "Media contains too many ranges"
        );
        let revision = hex::encode(Sha256::digest(plan.revision.as_bytes()));
        let directory = self.0.directory.join(format!("{}-{}", plan.key, revision));
        fs::create_dir_all(&directory).await?;
        let etag = saved.etag.clone();
        let progress = self.0.tracker.resource(plan.remote)?;
        progress.reset(0);
        let mut pending = futures_util::stream::iter((0..total.div_ceil(CHUNK)).map(|index| {
            let state = self.clone(); let progress = progress.clone(); let remote = plan.remote.clone(); let directory = directory.clone(); let etag = etag.clone();
            async move {
                let start = index * CHUNK; let end = ((index + 1) * CHUNK).min(total) - 1;
                let path = directory.join(format!("{index}.part"));
                let digest_path = directory.join(format!("{index}.sha256"));
                if fs::metadata(&path).await.is_ok_and(|meta| meta.len() == end - start + 1) {
                    if let Ok(expected) = fs::read_to_string(&digest_path).await {
                        if expected == file_digest(&path).await? {
                            state.0.cached_bytes.fetch_add(end - start + 1, Ordering::Relaxed);
                            progress.done.fetch_add(end - start + 1, Ordering::Relaxed);
                            return Ok::<_, anyhow::Error>((index, path));
                        }
                    }
                }
                for attempt in 0..=http::MAX_RETRIES {
                    let mut received = 0u64;
                    let result = async {
                        let _permit = tokio::select! { _ = state.0.cancel.cancelled() => anyhow::bail!("Cancelled"), permit = state.0.connections.acquire() => permit? };
                        let mut request = http::authenticate(state.0.client.get(remote.clone()).header(reqwest::header::RANGE, format!("bytes={start}-{end}")).header(reqwest::header::ACCEPT_ENCODING, "identity"), remote.as_str(), &state.0.credentials)?;
                        if let Some(etag) = &etag { request = request.header(reqwest::header::IF_MATCH, etag); }
                        if let Some(referer) = &state.0.referer { request = request.header(reqwest::header::REFERER, referer); }
                        let response = tokio::select! { _ = state.0.cancel.cancelled() => anyhow::bail!("Cancelled"), response = request.send() => response? };
                        if let Some(error) = http::transient(&response) { return Err(error.into()); }
                        response.error_for_status_ref().map_err(reqwest::Error::without_url)?;
                        http::identity(&response)?;
                        ensure!(response.status() == reqwest::StatusCode::PARTIAL_CONTENT, "Media server ignored range");
                        let range = http::content_range(response.headers().get(reqwest::header::CONTENT_RANGE).context("Missing media Content-Range")?.to_str()?)?;
                        ensure!(range == (start, end, total), "Incorrect media range");
                        if etag.is_some() { ensure!(http::etag(&response) == etag, "Media source changed during transfer"); }
                        let mut output = File::create(&path).await?;
                        let mut digest = Sha256::new(); let mut done = 0;
                        let mut stream = response.bytes_stream();
                        loop {
                            let next = tokio::select! { _ = state.0.cancel.cancelled() => anyhow::bail!("Cancelled"), next = stream.next() => next };
                            let Some(bytes) = next else { break; }; let bytes = bytes?;
                            for bytes in bytes.chunks(65536) {
                                ensure!(done + bytes.len() as u64 <= end - start + 1, "Media range exceeded its length");
                                state.0.limiter.acquire(bytes.len(), &state.0.cancel).await?;
                                state.0.global_limiter.acquire(bytes.len(), &state.0.cancel).await?;
                                output.write_all(bytes).await?; digest.update(bytes); done += bytes.len() as u64; received += bytes.len() as u64; progress.done.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                                state.0.bytes.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                            }
                        }
                        if done != end - start + 1 { return Err(http::Retryable { message: "Truncated media range".into(), delay: 0 }.into()); }
                        output.sync_all().await?; drop(output);
                        let temporary = digest_path.with_extension("sha256.tmp");
                        let mut journal = File::create(&temporary).await?;
                        journal.write_all(hex::encode(digest.finalize()).as_bytes()).await?;
                        journal.sync_all().await?; drop(journal);
                        fs::rename(temporary, &digest_path).await?;
                        anyhow::Ok(())
                    }.await;
                    match result {
                        Ok(()) => return Ok((index, path)),
                        Err(error) => {
                            progress.done.fetch_sub(received, Ordering::Relaxed);
                            let retry = error.downcast_ref::<http::Retryable>();
                            let transient = retry.is_some() || error.downcast_ref::<reqwest::Error>().is_some_and(|error| error.is_timeout() || error.is_connect() || error.is_body() || error.is_request());
                            let delay = retry.map(|error| error.delay).unwrap_or(0).max(1 << attempt);
                            if !transient || attempt == http::MAX_RETRIES || delay > 300 { anyhow::bail!("Media range failed: {error:#}"); }
                            tokio::select! { _ = state.0.cancel.cancelled() => return Err(error), _ = tokio::time::sleep(Duration::from_secs(delay)) => {} }
                        }
                    }
                }
                anyhow::bail!("Media retries exhausted")
            }
        })).buffer_unordered(plan.workers);
        let mut parts = Vec::new();
        while let Some(part) = pending.next().await {
            parts.push(part?);
        }
        parts.sort_by_key(|(index, _)| *index);
        file.set_len(0).await?;
        file.seek(std::io::SeekFrom::Start(0)).await?;
        *hash = Sha256::new();
        saved.done = 0;
        let mut buffer = [0; 65536];
        for (_, path) in parts {
            let mut input = File::open(path).await?;
            loop {
                let read = tokio::select! { _ = self.0.cancel.cancelled() => anyhow::bail!("Cancelled"), read = input.read(&mut buffer) => read? };
                if read == 0 {
                    break;
                }
                file.write_all(&buffer[..read]).await?;
                hash.update(&buffer[..read]);
                saved.done += read as u64;
            }
        }
        ensure!(saved.done == total, "Incomplete media assembly");
        Ok(())
    }

    async fn checkpoint(
        &self,
        file: &mut File,
        journal: &Path,
        saved: &mut Cached,
        hash: &Sha256,
    ) -> Result<()> {
        file.sync_all().await?;
        saved.digest = hex::encode(hash.clone().finalize());
        let temporary = journal.with_extension("json.new");
        let mut output = File::create(&temporary).await?;
        output.write_all(&serde_json::to_vec(saved)?).await?;
        output.sync_all().await?;
        drop(output);
        fs::rename(temporary, journal).await?;
        Ok(())
    }
    async fn rewrite_hls(&self, remote: &Url, bytes: &[u8]) -> Result<Vec<u8>> {
        self.0.tracker.hls(remote, bytes)?;
        let source = std::str::from_utf8(bytes)?;
        ensure!(
            source.trim_start_matches('\u{feff}').starts_with("#EXTM3U"),
            "Invalid HLS manifest"
        );
        let uri_pattern = regex::Regex::new("URI=\"([^\"]+)\"")?;
        let subtitle_pattern = regex::Regex::new(r#",?SUBTITLES="[^"]*""#)?;
        let mut output = String::new();
        for line in source.lines() {
            // Text renditions are prepared independently. FFmpeg's HLS demuxer
            // cannot ingest WebVTT playlists as ordinary audio/video segments.
            if line.starts_with("#EXT-X-MEDIA:") && line.contains("TYPE=SUBTITLES") {
                continue;
            }
            if line.starts_with('#') {
                let mut offset = 0;
                for capture in uri_pattern.captures_iter(line) {
                    let value = capture.get(1).context("Missing HLS URI")?;
                    output.push_str(&line[offset..value.start()]);
                    output.push_str(
                        &self
                            .proxy_url(remote.join(value.as_str())?.as_str())
                            .await?,
                    );
                    offset = value.end();
                }
                let tail = subtitle_pattern.replace_all(&line[offset..], "");
                output.push_str(&tail);
            } else if !line.trim().is_empty() {
                output.push_str(&self.proxy_url(remote.join(line.trim())?.as_str()).await?);
            }
            output.push('\n');
        }
        Ok(output.into_bytes())
    }
    async fn rewrite_dash(&self, remote: &Url, bytes: &[u8]) -> Result<Vec<u8>> {
        let mut document = crate::manifest::xml(bytes)?;
        if self.0.manifest_video_id.is_some() || self.0.manifest_audio_id.is_some() {
            crate::manifest::select_dash(
                &mut document,
                self.0.manifest_video_id.as_deref(),
                self.0.manifest_audio_id.as_deref(),
            )?;
        }
        self.0.tracker.dash(remote, &document)?;
        ensure!(document.name == "MPD", "Invalid DASH manifest");
        self.rewrite_element(&mut document, remote, None).await?;
        let mut output = Vec::new();
        document.write(&mut output)?;
        Ok(output)
    }
    fn rewrite_element<'a>(
        &'a self,
        element: &'a mut xmltree::Element,
        parent: &'a Url,
        inherited: Option<xmltree::Element>,
    ) -> futures_util::future::BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut effective = parent.clone();
            if let Some(base) = element
                .get_child("BaseURL")
                .and_then(|base| base.get_text())
            {
                effective = parent.join(base.trim())?;
            }
            model::validate_url(effective.as_str())?;
            if element.name == "Representation" && element.get_child("SegmentTemplate").is_none() {
                if let Some(template) = inherited.clone() {
                    element.children.push(xmltree::XMLNode::Element(template));
                }
            }
            let template = element.get_child("SegmentTemplate").cloned().or(inherited);
            if element.name == "BaseURL" {
                let original = element.get_text().context("Empty DASH BaseURL")?;
                let value = self
                    .proxy_url(parent.join(original.trim())?.as_str())
                    .await?;
                element.children = vec![xmltree::XMLNode::Text(value)];
                return Ok(());
            }
            for key in ["media", "initialization", "sourceURL", "index"] {
                if let Some(value) = element.attributes.get(key).cloned() {
                    // Templates keep $RepresentationID$ and $Number%05d$ visible for FFmpeg substitution.
                    let value = self.proxy_url(effective.join(&value)?.as_str()).await?;
                    element.attributes.insert(key.into(), value);
                }
            }
            for child in &mut element.children {
                if let xmltree::XMLNode::Element(child) = child {
                    // A BaseURL is resolved against its parent's inherited base, not itself.
                    let base = if child.name == "BaseURL" {
                        parent
                    } else {
                        &effective
                    };
                    self.rewrite_element(child, base, template.clone()).await?;
                }
            }
            Ok(())
        })
    }
}
async fn handle(
    State(state): State<GatewayState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Result<Response<Body>, (StatusCode, String)> {
    let result = async {
        let remote = state.remote_url(&uri).await?;
        if let Some(snapshot) = &state.0.manifest_snapshot {
            if remote.as_str() == snapshot.url {
                let hls = snapshot.text.trim_start().starts_with("#EXTM3U");
                let rewritten = if hls {
                    state.rewrite_hls(&remote, snapshot.text.as_bytes()).await?
                } else {
                    state
                        .rewrite_dash(&remote, snapshot.text.as_bytes())
                        .await?
                };
                return Ok(Response::builder()
                    .header(
                        "content-type",
                        if hls {
                            "application/vnd.apple.mpegurl"
                        } else {
                            "application/dash+xml"
                        },
                    )
                    .header("content-length", rewritten.len())
                    .body(Body::from(rewritten))?);
            }
        }
        let (path, cache) = state.cached(&remote).await?;
        if let Some(kind) = manifest_kind(&remote, &path, &cache).await? {
            ensure!(
                cache.done <= 2 * 1024 * 1024,
                "Media manifest exceeds two MiB"
            );
            let bytes = fs::read(path).await?;
            let rewritten = if kind == ManifestKind::Hls {
                state.rewrite_hls(&remote, &bytes).await?
            } else {
                state.rewrite_dash(&remote, &bytes).await?
            };
            return Ok(Response::builder()
                .header(
                    "content-type",
                    if kind == ManifestKind::Hls {
                        "application/vnd.apple.mpegurl"
                    } else {
                        "application/dash+xml"
                    },
                )
                .header("content-length", rewritten.len())
                .body(Body::from(rewritten))?);
        }
        let length = cache.done;
        let (start, end, ranged) = if let Some(range) = headers.get("range") {
            let range = range
                .to_str()?
                .strip_prefix("bytes=")
                .context("Invalid media range")?;
            ensure!(!range.contains(','), "Multiple ranges are unsupported");
            let (start, end) = range.split_once('-').context("Invalid media range")?;
            if start.is_empty() {
                let suffix: u64 = end.parse()?;
                ensure!(suffix > 0 && length > 0, "Invalid suffix range");
                (length.saturating_sub(suffix), length - 1, true)
            } else {
                let start: u64 = start.parse()?;
                let end = if end.is_empty() {
                    length.saturating_sub(1)
                } else {
                    end.parse::<u64>()?.min(length.saturating_sub(1))
                };
                ensure!(start <= end && end < length, "Media range is out of bounds");
                (start, end, true)
            }
        } else {
            (0, length.saturating_sub(1), false)
        };
        let size = if length == 0 { 0 } else { end - start + 1 };
        let mut file = File::open(path).await?;
        file.seek(std::io::SeekFrom::Start(start)).await?;
        let mut response = Response::builder()
            .status(if ranged {
                StatusCode::PARTIAL_CONTENT
            } else {
                StatusCode::OK
            })
            .header("content-type", cache.content_type)
            .header("accept-ranges", "bytes")
            .header("content-length", size);
        if ranged {
            response = response.header("content-range", format!("bytes {start}-{end}/{length}"));
        }
        Ok::<_, anyhow::Error>(
            response.body(Body::from_stream(ReaderStream::new(file.take(size))))?,
        )
    }
    .await;
    match result {
        Ok(response) => {
            state.0.last_error.lock().await.remove(&uri.to_string());
            Ok(response)
        }
        Err(error) => {
            let message = format!("{error:#}");
            state
                .0
                .last_error
                .lock()
                .await
                .insert(uri.to_string(), message.clone());
            Err((StatusCode::BAD_GATEWAY, message))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use std::sync::atomic::{AtomicBool, AtomicUsize};
    #[derive(Clone)]
    struct Fixture {
        bytes: Arc<Vec<u8>>,
        revision: Arc<AtomicUsize>,
        requests: Arc<AtomicUsize>,
        active: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
        invalid: Arc<AtomicBool>,
        cut: Arc<AtomicBool>,
    }
    async fn serve(State(f): State<Fixture>, headers: HeaderMap) -> Response<Body> {
        let range = headers
            .get("range")
            .unwrap()
            .to_str()
            .unwrap()
            .strip_prefix("bytes=")
            .unwrap();
        let (start, end) = range.split_once('-').unwrap();
        let start: usize = start.parse().unwrap();
        let end: usize = end.parse().unwrap();
        let revision = format!("\"media-{}\"", f.revision.load(Ordering::Relaxed));
        if headers
            .get("if-match")
            .is_some_and(|value| value.to_str().unwrap() != revision)
        {
            return Response::builder().status(412).body(Body::empty()).unwrap();
        }
        if end > start {
            f.requests.fetch_add(1, Ordering::Relaxed);
            let active = f.active.fetch_add(1, Ordering::Relaxed) + 1;
            f.peak.fetch_max(active, Ordering::Relaxed);
            tokio::time::sleep(Duration::from_millis(30)).await;
            f.active.fetch_sub(1, Ordering::Relaxed);
        }
        let cut = end > start && f.cut.swap(false, Ordering::Relaxed);
        Response::builder()
            .status(206)
            .header("etag", revision)
            .header("content-type", "video/mp4")
            .header("content-length", end - start + 1)
            .header(
                "content-range",
                format!(
                    "bytes {}-{end}/{}",
                    if f.invalid.load(Ordering::Relaxed) && end > start {
                        start + 1
                    } else {
                        start
                    },
                    f.bytes.len()
                ),
            )
            .body(if cut {
                Body::from_stream(futures_util::stream::iter(vec![
                    Ok(axum::body::Bytes::copy_from_slice(
                        &f.bytes[start..start + 1024],
                    )),
                    Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionReset,
                        "Interrupted fixture connection",
                    )),
                ]))
            } else {
                Body::from(f.bytes[start..=end].to_vec())
            })
            .unwrap()
    }
    async fn fixture() -> (
        Fixture,
        String,
        tokio::task::JoinHandle<std::io::Result<()>>,
    ) {
        let fixture = Fixture {
            bytes: Arc::new(
                (0..12 * 1024 * 1024)
                    .map(|index| (index % 251) as u8)
                    .collect(),
            ),
            revision: Arc::new(AtomicUsize::new(1)),
            requests: Default::default(),
            active: Default::default(),
            peak: Default::default(),
            invalid: Default::default(),
            cut: Default::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/video", listener.local_addr().unwrap());
        let state = fixture.clone();
        let server = tokio::spawn(async {
            axum::serve(
                listener,
                Router::new().route("/video", get(serve)).with_state(state),
            )
            .await
        });
        (fixture, url, server)
    }
    async fn gateway(url: &str, directory: &Path) -> Gateway {
        Gateway::open(
            &DownloadRequest {
                url: url.into(),
                filename: "video.mp4".into(),
                ..DownloadRequest::default()
            },
            directory,
            Arc::new(Semaphore::new(64)),
            Arc::new(Limiter::new(0)),
            CancellationToken::new(),
            None,
        )
        .await
        .unwrap()
    }
    #[tokio::test]
    async fn parallel_media_reuses_verified_chunks_and_discards_changed_revisions() {
        let root = tempfile::tempdir().unwrap();
        let (fixture, url, server) = fixture().await;
        let remote = Url::parse(&url).unwrap();
        let first = gateway(&url, root.path()).await;
        let (path, _) = first.state.cached(&remote).await.unwrap();
        assert_eq!(fs::read(&path).await.unwrap(), *fixture.bytes);
        assert!(fixture.peak.load(Ordering::Relaxed) >= 2);
        assert_eq!(fixture.requests.load(Ordering::Relaxed), 3);
        first.shutdown().await.unwrap();
        // A truncated assembly and a corrupt cached chunk must not be trusted.
        File::create(&path).await.unwrap();
        let mut entries = fs::read_dir(root.path()).await.unwrap();
        let mut parts = None;
        while let Some(entry) = entries.next_entry().await.unwrap() {
            if entry.file_type().await.unwrap().is_dir() {
                parts = Some(entry.path());
                break;
            }
        }
        let corrupt = parts.unwrap().join("0.part");
        let mut bytes = fs::read(&corrupt).await.unwrap();
        bytes[0] ^= 1;
        fs::write(corrupt, bytes).await.unwrap();
        let second = gateway(&url, root.path()).await;
        let (path, _) = second.state.cached(&remote).await.unwrap();
        assert_eq!(fs::read(path).await.unwrap(), *fixture.bytes);
        assert_eq!(fixture.requests.load(Ordering::Relaxed), 4);
        assert_eq!(second.bytes().load(Ordering::Relaxed), 4 * 1024 * 1024);
        second.shutdown().await.unwrap();
        fixture.revision.store(2, Ordering::Relaxed);
        let third = gateway(&url, root.path()).await;
        let (path, _) = third.state.cached(&remote).await.unwrap();
        assert_eq!(fs::read(path).await.unwrap(), *fixture.bytes);
        assert_eq!(fixture.requests.load(Ordering::Relaxed), 7);
        third.shutdown().await.unwrap();
        server.abort();
    }
    #[tokio::test]
    async fn parallel_media_rejects_incorrect_content_ranges() {
        let root = tempfile::tempdir().unwrap();
        let (fixture, url, server) = fixture().await;
        fixture.invalid.store(true, Ordering::Relaxed);
        let gateway = gateway(&url, root.path()).await;
        let error = gateway
            .state
            .cached(&Url::parse(&url).unwrap())
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("Incorrect media range"));
        gateway.shutdown().await.unwrap();
        server.abort();
    }
    #[tokio::test]
    async fn interrupted_parallel_media_retries_without_losing_verified_ranges() {
        let root = tempfile::tempdir().unwrap();
        let (fixture, url, server) = fixture().await;
        fixture.cut.store(true, Ordering::Relaxed);
        let gateway = gateway(&url, root.path()).await;
        let (path, _) = gateway
            .state
            .cached(&Url::parse(&url).unwrap())
            .await
            .unwrap();
        assert_eq!(fs::read(path).await.unwrap(), *fixture.bytes);
        assert_eq!(fixture.requests.load(Ordering::Relaxed), 4);
        gateway.shutdown().await.unwrap();
        server.abort();
    }
}
