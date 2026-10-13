use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, Response},
    routing::get,
    Router,
};
use flux_core::{
    DownloadRequest, Engine, MediaChapter, MediaOptions, Status, SubtitleMode, SubtitleTrack,
};
use std::{
    collections::HashMap, future::IntoFuture, path::PathBuf, process::Command, sync::Arc,
    time::Duration,
};

struct Fixture {
    root: tempfile::TempDir,
    engine: Engine,
    url: String,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}
async fn serve(
    State(files): State<Arc<HashMap<String, Vec<u8>>>>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Response<Body> {
    let Some(bytes) = files.get(&name) else {
        return Response::builder().status(404).body(Body::empty()).unwrap();
    };
    let range = headers
        .get("range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes="));
    let (start, end) = range
        .and_then(|v| v.split_once('-'))
        .map(|(a, b)| {
            (
                a.parse::<usize>().unwrap(),
                b.parse::<usize>().unwrap_or(bytes.len() - 1),
            )
        })
        .unwrap_or((0, bytes.len() - 1));
    let mut response = Response::builder()
        .status(if range.is_some() { 206 } else { 200 })
        .header("etag", "\"asset-v1\"")
        .header("content-length", end - start + 1);
    if range.is_some() {
        response = response.header(
            "content-range",
            format!("bytes {start}-{end}/{}", bytes.len()),
        );
    }
    response
        .body(Body::from(bytes[start..=end].to_vec()))
        .unwrap()
}
impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let mut files = HashMap::new();
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/media");
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            files.insert(
                entry.file_name().to_string_lossy().into_owned(),
                std::fs::read(entry.path()).unwrap(),
            );
        }
        files.insert("master.m3u8".into(), b"#EXTM3U\n#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",LANGUAGE=\"tr\",NAME=\"Turkish\",URI=\"subtitles.m3u8\"\n#EXT-X-STREAM-INF:BANDWIDTH=500000,SUBTITLES=\"subs\"\nfilm.m3u8\n".to_vec());
        files.insert("subtitles.m3u8".into(), b"#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4,\nfirst.vtt\n#EXTINF:4,\nsecond.vtt\n#EXT-X-ENDLIST\n".to_vec());
        files.insert("first.vtt".into(), b"WEBVTT\nX-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:900000\n\n00:00:00.200 --> 00:00:02.000\nFirst subtitle\n".to_vec());
        files.insert("second.vtt".into(), b"WEBVTT\nX-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:1260000\n\n00:00:00.200 --> 00:00:02.000\nSecond subtitle\n".to_vec());
        files.insert(
            "english.srt".into(),
            b"1\n00:00:00,200 --> 00:00:02,000\nEnglish subtitle\n".to_vec(),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(
            axum::serve(
                listener,
                Router::new()
                    .route("/{name}", get(serve))
                    .with_state(Arc::new(files)),
            )
            .into_future(),
        );
        let engine = Engine::open(root.path().join("data"), root.path().join("output"))
            .await
            .unwrap();
        Self {
            root,
            engine,
            url,
            server,
        }
    }
    fn request(&self, container: &str, mode: SubtitleMode, normalize: bool) -> DownloadRequest {
        DownloadRequest {
            url: format!("{}/film.m3u8", self.url),
            filename: format!("suite.{container}"),
            media: Some(MediaOptions {
                kind: Some("hls".into()),
                container: container.into(),
                subtitle_mode: mode,
                normalize_audio: normalize,
                subtitles: vec![
                    SubtitleTrack {
                        url: Some(format!("{}/subtitles.m3u8", self.url)),
                        index: None,
                        language: "tur".into(),
                        title: "Turkish".into(),
                    },
                    SubtitleTrack {
                        url: Some(format!("{}/english.srt", self.url)),
                        index: None,
                        language: "eng".into(),
                        title: "English".into(),
                    },
                ],
                chapters: vec![
                    MediaChapter {
                        start_ms: 0,
                        end_ms: 4000,
                        title: "First; chapter".into(),
                    },
                    MediaChapter {
                        start_ms: 4000,
                        end_ms: 8000,
                        title: "Second chapter".into(),
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        }
    }
    async fn download(&self, input: DownloadRequest) -> flux_core::Task {
        let task = self.engine.enqueue(input).await.unwrap();
        self.engine.start(&task.id).await.unwrap();
        let task = tokio::time::timeout(Duration::from_secs(45), self.engine.wait(&task.id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.status, Status::Complete, "{:?}", task.error);
        task
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
fn probe(path: &std::path::Path) -> serde_json::Value {
    let output = Command::new(std::env::var_os("FLUX_FFPROBE").unwrap_or_else(|| "ffprobe".into()))
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_chapters",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[tokio::test]
async fn hls_subtitle_discovery_and_mp4_multi_subtitle_chapters() {
    let fixture = Fixture::new().await;
    let info = fixture
        .engine
        .inspect_media(format!("{}/master.m3u8", fixture.url), None, None)
        .await
        .unwrap();
    assert_eq!(info.subtitles.len(), 1);
    assert_eq!(info.subtitles[0].language, "tr");
    let task = fixture
        .download(fixture.request("mp4", SubtitleMode::Embed, false))
        .await;
    let info = probe(&task.destination);
    let subs: Vec<_> = info["streams"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|stream| stream["codec_type"] == "subtitle")
        .collect();
    assert_eq!(subs.len(), 2);
    assert!(subs.iter().all(|stream| stream["codec_name"] == "mov_text"));
    assert_eq!(info["chapters"].as_array().unwrap().len(), 2);
    assert_eq!(info["chapters"][0]["tags"]["title"], "First; chapter");
}
#[tokio::test]
async fn mkv_external_srt_contains_both_hls_segments() {
    let fixture = Fixture::new().await;
    let task = fixture
        .download(fixture.request("mkv", SubtitleMode::External, false))
        .await;
    let first =
        std::fs::read_to_string(task.destination.with_file_name("suite.tur.0.srt")).unwrap();
    assert!(first.contains("First subtitle") && first.contains("Second subtitle"));
    assert!(first.contains("00:00:04,200"));
    assert!(
        std::fs::read_to_string(task.destination.with_file_name("suite.eng.1.srt"))
            .unwrap()
            .contains("English subtitle")
    );
    assert!(!probe(&task.destination)["streams"]
        .as_array()
        .unwrap()
        .iter()
        .any(|stream| stream["codec_type"] == "subtitle"));
}
#[tokio::test]
async fn mkv_embeds_multiple_text_tracks_with_language_metadata() {
    let fixture = Fixture::new().await;
    let task = fixture
        .download(fixture.request("mkv", SubtitleMode::Embed, false))
        .await;
    let info = probe(&task.destination);
    let tracks: Vec<_> = info["streams"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|stream| stream["codec_type"] == "subtitle")
        .collect();
    assert_eq!(tracks.len(), 2);
    assert!(tracks.iter().all(|track| track["codec_name"] == "subrip"));
    assert!(tracks
        .iter()
        .any(|track| track["tags"]["language"] == "tur"));
    assert!(tracks
        .iter()
        .any(|track| track["tags"]["language"] == "eng"));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lossless_audio_keeps_original_codec_and_packets() {
    let fixture = Fixture::new().await;
    let task = fixture
        .download(fixture.request("mka", SubtitleMode::None, false))
        .await;
    let info = probe(&task.destination);
    let streams = info["streams"].as_array().unwrap();
    assert_eq!(streams.len(), 1);
    assert_eq!(streams[0]["codec_name"], "aac");
    let reference = fixture.root.path().join("reference.mka");
    let tool = std::env::var_os("FLUX_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
    let help = Command::new(&tool)
        .args(["-hide_banner", "-h", "demuxer=hls"])
        .output()
        .unwrap();
    let mut command = Command::new(tool);
    for option in ["allowed_segment_extensions", "extension_picky"] {
        if String::from_utf8_lossy(&help.stdout).contains(&format!("-{option} ")) {
            command.args([
                format!("-{option}").as_str(),
                if option == "extension_picky" {
                    "0"
                } else {
                    "ALL"
                },
            ]);
        }
    }
    let output = command
        .args([
            "-v",
            "error",
            "-allowed_extensions",
            "ALL",
            "-i",
            &format!("{}/film.m3u8", fixture.url),
            "-vn",
            "-c:a",
            "copy",
            "-f",
            "matroska",
        ])
        .arg(&reference)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let hashes = |path: &std::path::Path| {
        let output =
            Command::new(std::env::var_os("FLUX_FFPROBE").unwrap_or_else(|| "ffprobe".into()))
                .args([
                    "-v",
                    "error",
                    "-select_streams",
                    "a",
                    "-show_packets",
                    "-show_data_hash",
                    "sha256",
                    "-of",
                    "json",
                ])
                .arg(path)
                .output()
                .unwrap();
        let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        info["packets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|packet| packet["data_hash"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(hashes(&task.destination), hashes(&reference));
}
#[tokio::test]
async fn two_pass_r128_normalization_measures_near_target_loudness() {
    let fixture = Fixture::new().await;
    let task = fixture
        .download(fixture.request("mp4", SubtitleMode::None, true))
        .await;
    let output = Command::new(std::env::var_os("FLUX_FFMPEG").unwrap_or_else(|| "ffmpeg".into()))
        .args(["-hide_banner", "-i"])
        .arg(&task.destination)
        .args([
            "-vn",
            "-af",
            "loudnorm=I=-23:TP=-1:LRA=7:print_format=json",
            "-f",
            "null",
            "-",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stderr);
    let start = text.rfind('{').unwrap();
    let end = start + text[start..].find('}').unwrap();
    let value: serde_json::Value = serde_json::from_str(&text[start..=end]).unwrap();
    let loudness: f64 = value["input_i"].as_str().unwrap().parse().unwrap();
    assert!(
        (loudness + 23.0).abs() < 1.0,
        "Measured loudness: {loudness}"
    );
}
