use axum::{
    body::{Body, Bytes},
    extract::{Path, State},
    http::{HeaderMap, Response},
    routing::get,
    Router,
};
use flux_core::{BrowserCredentials, DownloadRequest, Engine, MediaOptions, Status};
use std::{
    collections::HashMap,
    future::IntoFuture,
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};
use tempfile::TempDir;
#[derive(Clone)]
struct Files {
    files: Arc<HashMap<String, Vec<u8>>>,
    ranges: Arc<Mutex<Vec<(String, String)>>>,
    auth: bool,
    chunked: Arc<std::sync::atomic::AtomicBool>,
}
async fn serve(
    State(files): State<Files>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Response<Body> {
    if files.auth
        && headers.get("cookie").and_then(|v| v.to_str().ok()) != Some("session=media-test")
    {
        return Response::builder().status(401).body(Body::empty()).unwrap();
    }
    let Some(bytes) = files.files.get(&name).cloned() else {
        return Response::builder().status(404).body(Body::empty()).unwrap();
    };
    let range = headers
        .get("range")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let chunked = files.chunked.load(std::sync::atomic::Ordering::Relaxed);
    files
        .ranges
        .lock()
        .unwrap()
        .push((name.clone(), range.clone()));
    let range = if chunked { "" } else { range.as_str() };
    let (start, end) = range
        .strip_prefix("bytes=")
        .and_then(|v| v.split_once('-'))
        .map(|(a, b)| {
            (
                a.parse::<usize>().unwrap(),
                b.parse::<usize>().unwrap_or(bytes.len() - 1),
            )
        })
        .unwrap_or((0, bytes.len() - 1));
    let mut response = Response::builder()
        .status(if range.is_empty() { 200 } else { 206 })
        .header(
            "content-type",
            if !chunked && (name.ends_with("m3u8") || name == "playlist") {
                "application/vnd.apple.mpegurl"
            } else {
                "application/octet-stream"
            },
        );
    if !chunked {
        response = response
            .header("content-length", end - start + 1)
            .header("etag", "\"media-v1\"");
    }
    if !range.is_empty() {
        response = response.header(
            "content-range",
            format!("bytes {start}-{end}/{}", bytes.len()),
        );
    }
    response
        .body(Body::from_stream(futures_util::stream::iter(
            bytes[start..=end]
                .chunks(4096)
                .map(|part| Ok::<_, std::io::Error>(Bytes::copy_from_slice(part)))
                .collect::<Vec<_>>(),
        )))
        .unwrap()
}
struct Fixture {
    _root: TempDir,
    data: PathBuf,
    engine: Engine,
    url: String,
    files: Files,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}
impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let fixtures = root.path().join("fixtures");
        std::fs::create_dir(&fixtures).unwrap();
        let result = Command::new(
            std::env::var_os("FLUX_FIXTURE_FFMPEG")
                .or_else(|| std::env::var_os("FLUX_FFMPEG"))
                .unwrap_or_else(|| "ffmpeg".into()),
        )
        .current_dir(&fixtures)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=24",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000",
            "-t",
            "4",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-g",
            "24",
            "-c:a",
            "aac",
            "-f",
            "hls",
            "-hls_time",
            "1",
            "-hls_list_size",
            "0",
            "stream.m3u8",
        ])
        .output()
        .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result = Command::new(
            std::env::var_os("FLUX_FIXTURE_FFMPEG")
                .or_else(|| std::env::var_os("FLUX_FFMPEG"))
                .unwrap_or_else(|| "ffmpeg".into()),
        )
        .current_dir(&fixtures)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            "stream.m3u8",
            "-map",
            "0:v:0",
            "-c",
            "copy",
            "video.mp4",
            "-map",
            "0:a:0",
            "-c",
            "copy",
            "audio.m4a",
            "-map",
            "0:a:0",
            "-c",
            "copy",
            "-f",
            "hls",
            "-hls_time",
            "1",
            "-hls_list_size",
            "0",
            "-hls_segment_filename",
            "audio%02d.png",
            "soundtrack.m3u8",
        ])
        .output()
        .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let mut files: HashMap<_, _> = std::fs::read_dir(&fixtures)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().to_string_lossy().to_string(),
                    std::fs::read(entry.path()).unwrap(),
                )
            })
            .collect();
        files.insert("unsafe.m3u8".into(),b"#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\nfile:///C:/Windows/win.ini\n#EXT-X-ENDLIST\n".to_vec());
        let disguised = String::from_utf8(files["stream.m3u8"].clone())
            .unwrap()
            .replace(".ts", ".jpg")
            .into_bytes();
        let segments: Vec<_> = files
            .iter()
            .filter(|(name, _)| name.ends_with(".ts"))
            .map(|(name, bytes)| (name.replace(".ts", ".jpg"), bytes.clone()))
            .collect();
        files.extend(segments);
        files.insert("disguised.m3u8".into(), disguised.clone());
        files.insert("playlist".into(), disguised);
        files.insert("soundtrack".into(), files["soundtrack.m3u8"].clone());
        let files = Files {
            files: Arc::new(files),
            ranges: Default::default(),
            auth: true,
            chunked: Default::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(
            axum::serve(
                listener,
                Router::new()
                    .route("/{name}", get(serve))
                    .with_state(files.clone()),
            )
            .into_future(),
        );
        let data = root.path().join("data");
        let engine = Engine::open(&data, root.path().join("output"))
            .await
            .unwrap();
        Self {
            _root: root,
            data,
            engine,
            url,
            files,
            server,
        }
    }
    fn request(&self, name: &str, container: &str) -> DownloadRequest {
        DownloadRequest {
            url: format!("{}/stream.m3u8", self.url),
            filename: format!("{name}.{container}"),
            credentials: vec![BrowserCredentials {
                origin: self.url.clone(),
                cookie: Some("session=media-test".into()),
                authorization: None,
                user_agent: None,
            }],
            media: Some(MediaOptions {
                kind: None,
                provider: None,
                segment_urls: Vec::new(),
                audio_url: None,
                video_stream: None,
                audio_stream: None,
                duration_seconds: None,
                container: container.into(),
                ..MediaOptions::default()
            }),
            ..DownloadRequest::default()
        }
    }
    async fn finish(&self, request: DownloadRequest) -> flux_core::Task {
        let task = self.engine.enqueue(request).await.unwrap();
        self.engine.start(&task.id).await.unwrap();
        let task = tokio::time::timeout(Duration::from_secs(40), self.engine.wait(&task.id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.status, Status::Complete, "{:?}", task.error);
        assert!(task.source_validated);
        task
    }
    fn streams(task: &flux_core::Task) {
        let result =
            Command::new(std::env::var_os("FLUX_FFPROBE").unwrap_or_else(|| "ffprobe".into()))
                .args([
                    "-v",
                    "error",
                    "-show_entries",
                    "stream=codec_type",
                    "-of",
                    "json",
                ])
                .arg(&task.destination)
                .output()
                .unwrap();
        assert!(result.status.success());
        let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        let streams = value["streams"].as_array().unwrap();
        assert!(streams.iter().any(|stream| stream["codec_type"] == "video"));
        assert!(streams.iter().any(|stream| stream["codec_type"] == "audio"));
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
#[tokio::test]
async fn authenticated_hls_inspection_and_explicit_ts_concat() {
    let f = Fixture::new().await;
    let mut input = f.request("concat", "mp4");
    let sealed = flux_core::protect_credentials(&f.data, &input.credentials).unwrap();
    let info = f
        .engine
        .inspect_media(input.url.clone(), None, Some(sealed))
        .await
        .unwrap();
    assert!(info.streams.iter().any(|s| s.codec_type == "video"));
    let mut names: Vec<_> = f
        .files
        .files
        .keys()
        .filter(|name| name.ends_with(".ts"))
        .cloned()
        .collect();
    names.sort();
    input.media.as_mut().unwrap().segment_urls = names
        .iter()
        .map(|name| format!("{}/{name}", f.url))
        .collect();
    Fixture::streams(&f.finish(input).await);
}
#[tokio::test]
async fn separate_audio_is_muxed_into_mkv() {
    let f = Fixture::new().await;
    let mut input = f.request("separate", "mkv");
    input.url = format!("{}/video.mp4", f.url);
    input.media.as_mut().unwrap().audio_url = Some(format!("{}/audio.m4a", f.url));
    Fixture::streams(&f.finish(input).await);
}
#[tokio::test]
async fn media_pause_resumes_verified_cache_ranges_and_obeys_speed_limit() {
    let f = Fixture::new().await;
    let mut input = f.request("resumed", "mp4");
    input.url = format!("{}/video.mp4", f.url);
    input.media.as_mut().unwrap().audio_url = Some(format!("{}/audio.m4a", f.url));
    input.speed_limit = 32768;
    let task = f.engine.enqueue(input).await.unwrap();
    let started = tokio::time::Instant::now();
    f.engine.start(&task.id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let cache = f.data.join("parts").join(&task.id).join("media-cache");
            let received = std::fs::read_dir(cache).is_ok_and(|entries| {
                entries.filter_map(Result::ok).any(|entry| {
                    entry.path().extension().is_some_and(|ext| ext == "json")
                        && std::fs::read(entry.path()).is_ok_and(|bytes| {
                            serde_json::from_slice::<serde_json::Value>(&bytes).is_ok_and(
                                |checkpoint| {
                                    checkpoint["done"].as_u64().is_some_and(|done| done > 4096)
                                        && checkpoint["complete"].as_bool() == Some(false)
                                        && checkpoint["digest"]
                                            .as_str()
                                            .is_some_and(|digest| digest.len() == 64)
                                },
                            )
                        })
                })
            });
            if received {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    let paused = f.engine.pause(&task.id).await.unwrap();
    assert_eq!(paused.status, Status::Paused);
    assert!(!paused.destination.exists());
    let dir = f.data.join("parts").join(&task.id).join("media-cache");
    assert!(std::fs::read_dir(dir).unwrap().any(|entry| entry
        .unwrap()
        .path()
        .extension()
        .is_some_and(|value| value == "json")));
    f.engine.start(&task.id).await.unwrap();
    let task = tokio::time::timeout(Duration::from_secs(40), f.engine.wait(&task.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.status, Status::Complete, "{:?}", task.error);
    Fixture::streams(&task);
    let bytes = f.files.files["video.mp4"].len() + f.files.files["audio.m4a"].len();
    assert!(started.elapsed().as_secs_f64() >= bytes as f64 / 32768.0 * 0.8);
    assert!(f
        .files
        .ranges
        .lock()
        .unwrap()
        .iter()
        .any(|(_, range)| range.starts_with("bytes=")
            && range != "bytes=0-0"
            && !range.starts_with("bytes=0-")));
}
#[tokio::test]
async fn remote_manifest_cannot_read_local_files() {
    let f = Fixture::new().await;
    let mut input = f.request("unsafe", "mp4");
    input.url = format!("{}/unsafe.m3u8", f.url);
    let task = f.engine.enqueue(input).await.unwrap();
    f.engine.start(&task.id).await.unwrap();
    let task = f.engine.wait(&task.id).await.unwrap();
    assert_eq!(task.status, Status::Failed);
    assert!(!task.destination.exists());
}
#[tokio::test]
async fn audio_export_contains_the_selected_soundtrack_and_no_video() {
    let f = Fixture::new().await;
    let mut request = f.request("master.m3u8", "mp3");
    request.filename = "soundtrack.mp3".into();
    let task = f.finish(request).await;
    let result = Command::new(std::env::var_os("FLUX_FFPROBE").unwrap_or_else(|| "ffprobe".into()))
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(task.destination)
        .output()
        .unwrap();
    assert!(result.status.success());
    let info: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let streams = info["streams"].as_array().unwrap();
    assert_eq!(streams.len(), 1);
    assert_eq!(streams[0]["codec_type"], "audio");
    assert_eq!(streams[0]["codec_name"], "mp3");
}
#[tokio::test]
async fn hls_with_jpeg_segment_names_can_be_inspected_and_downloaded() {
    let f = Fixture::new().await;
    let mut input = f.request("disguised", "mp4");
    input.url = format!("{}/disguised.m3u8", f.url);
    let info = f
        .engine
        .inspect_media(
            input.url.clone(),
            None,
            Some(flux_core::protect_credentials(&f.data, &input.credentials).unwrap()),
        )
        .await
        .unwrap();
    assert!(info
        .streams
        .iter()
        .any(|stream| stream.codec_type == "video" && stream.height == Some(180)));
    Fixture::streams(&f.finish(input).await);
}
#[tokio::test]
async fn extensionless_hls_with_jpeg_segment_names_uses_captured_media_kind() {
    let f = Fixture::new().await;
    let mut input = f.request("extensionless", "mp4");
    input.url = format!("{}/playlist", f.url);
    input.media.as_mut().unwrap().kind = Some("hls".into());
    Fixture::streams(&f.finish(input).await);
}
#[tokio::test]
async fn extensionless_hls_audio_with_png_segments_is_muxed_with_video() {
    let f = Fixture::new().await;
    let mut input = f.request("separate-hls", "mp4");
    input.url = format!("{}/playlist", f.url);
    let media = input.media.as_mut().unwrap();
    media.kind = Some("hls".into());
    media.audio_url = Some(format!("{}/soundtrack", f.url));
    Fixture::streams(&f.finish(input).await);
}
#[tokio::test]
async fn extensionless_hls_audio_export_preserves_the_selected_soundtrack() {
    let f = Fixture::new().await;
    let mut input = f.request("selected-hls-audio", "mka");
    input.url = format!("{}/playlist", f.url);
    let media = input.media.as_mut().unwrap();
    media.kind = Some("hls".into());
    media.audio_url = Some(format!("{}/soundtrack", f.url));
    let task = f.finish(input).await;
    let output = Command::new(std::env::var_os("FLUX_FFPROBE").unwrap_or_else(|| "ffprobe".into()))
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(task.destination)
        .output()
        .unwrap();
    assert!(output.status.success());
    let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let streams = info["streams"].as_array().unwrap();
    assert_eq!(streams.len(), 1);
    assert_eq!(streams[0]["codec_type"], "audio");
    assert_eq!(streams[0]["codec_name"], "aac");
}
#[tokio::test]
async fn extensionless_hls_is_detected_without_a_captured_kind() {
    let f = Fixture::new().await;
    let mut input = f.request("untyped-hls", "mp4");
    input.url = format!("{}/playlist", f.url);
    Fixture::streams(&f.finish(input).await);
}
#[tokio::test]
async fn hls_video_with_a_regular_audio_file_keeps_both_streams() {
    let f = Fixture::new().await;
    let mut input = f.request("mixed-inputs", "mp4");
    input.url = format!("{}/playlist", f.url);
    let media = input.media.as_mut().unwrap();
    media.kind = Some("hls".into());
    media.audio_url = Some(format!("{}/audio.m4a", f.url));
    Fixture::streams(&f.finish(input).await);
}
#[tokio::test]
async fn chunked_hls_reports_progress_eta_and_preserves_snapshot_on_pause() {
    let f = Fixture::new().await;
    f.files
        .chunked
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let mut input = f.request("unknown-size", "mp4");
    input.url = format!("{}/playlist", f.url);
    input.media.as_mut().unwrap().kind = Some("hls".into());
    input.speed_limit = 32768;
    let mut events = f.engine.subscribe();
    let task = f.engine.enqueue(input).await.unwrap();
    f.engine.start(&task.id).await.unwrap();
    let observed = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let progress = events.recv().await.unwrap();
            if progress.id == task.id
                && progress.media.as_ref().is_some_and(|media| {
                    media
                        .percent
                        .is_some_and(|percent| percent > 0.0 && percent < 99.0)
                        && media.eta_seconds.is_some()
                })
            {
                break progress;
            }
        }
    })
    .await
    .unwrap();
    assert!(observed.total_estimated && observed.total.is_some());
    assert!(observed.completed > 0);
    assert_eq!(observed.media.unwrap().duration_ms, Some(4000));
    let paused = f.engine.pause(&task.id).await.unwrap();
    assert!(paused.completed > 0);
    assert!(paused.media_progress.as_ref().unwrap().percent.unwrap() > 0.0);
    assert!(paused.total_estimated);
    f.engine.start(&task.id).await.unwrap();
    let complete = tokio::time::timeout(Duration::from_secs(40), f.engine.wait(&task.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(complete.status, Status::Complete, "{:?}", complete.error);
    assert!(!complete.total_estimated);
    assert_eq!(
        complete.total,
        Some(std::fs::metadata(&complete.destination).unwrap().len())
    );
    Fixture::streams(&complete);
}
