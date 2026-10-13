use axum::{
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Router,
};
use flux_core::{DownloadRequest, Engine, MediaOptions, Status};
use std::{collections::HashMap, process::Command, sync::Arc, time::Duration};
use tempfile::TempDir;

async fn serve(
    State(files): State<Arc<HashMap<String, Vec<u8>>>>,
    Path(name): Path<String>,
) -> Result<Body, StatusCode> {
    files
        .get(&name)
        .cloned()
        .map(Body::from)
        .ok_or(StatusCode::NOT_FOUND)
}
fn generate(directory: &std::path::Path, format: &str) {
    let mut command = Command::new(
        std::env::var_os("FLUX_FIXTURE_FFMPEG")
            .or_else(|| std::env::var_os("FLUX_FFMPEG"))
            .unwrap_or_else(|| "ffmpeg".into()),
    );
    command.current_dir(directory).args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=160x90:rate=12",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000",
        "-t",
        "2",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-g",
        "12",
        "-c:a",
        "aac",
    ]);
    if format == "hls" {
        command.args([
            "-f",
            "hls",
            "-hls_time",
            "1",
            "-hls_list_size",
            "0",
            "stream.m3u8",
        ]);
    } else {
        command.args(["-f", "dash", "-seg_duration", "1", "stream.mpd"]);
    }
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
async fn exercise(format: &str) {
    let available =
        Command::new(std::env::var_os("FLUX_FFMPEG").unwrap_or_else(|| "ffmpeg".into()))
            .arg("-version")
            .output()
            .is_ok();
    if !available {
        assert!(
            std::env::var_os("FLUX_REQUIRE_MEDIA_TESTS").is_none(),
            "FFmpeg is required for media tests"
        );
        eprintln!("FFmpeg is unavailable; media test was skipped");
        return;
    }
    let fixtures = TempDir::new().unwrap();
    generate(fixtures.path(), format);
    let files: HashMap<_, _> = std::fs::read_dir(fixtures.path())
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().to_string(),
                std::fs::read(entry.path()).unwrap(),
            )
        })
        .collect();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(
        axum::serve(
            listener,
            Router::new()
                .route("/{name}", get(serve))
                .with_state(Arc::new(files)),
        )
        .into_future(),
    );
    let url = format!(
        "http://{address}/stream.{}",
        if format == "hls" { "m3u8" } else { "mpd" }
    );
    let info = flux_core::inspect_media(&url, None).await.unwrap();
    assert!(info
        .streams
        .iter()
        .any(|s| s.codec_type == "video" && s.width == Some(160)));
    assert!(info.streams.iter().any(|s| s.codec_type == "audio"));
    let data = TempDir::new().unwrap();
    let downloads = TempDir::new().unwrap();
    let engine = Engine::open(data.path(), downloads.path()).await.unwrap();
    let task = engine
        .enqueue(DownloadRequest {
            url,
            filename: "assembled.mp4".into(),
            media: Some(MediaOptions {
                kind: None,
                provider: None,
                segment_urls: Vec::new(),
                container: "mp4".into(),
                audio_url: None,
                video_stream: None,
                audio_stream: None,
                duration_seconds: None,
                ..MediaOptions::default()
            }),
            ..DownloadRequest::default()
        })
        .await
        .unwrap();
    engine.start(&task.id).await.unwrap();
    let task = tokio::time::timeout(Duration::from_secs(30), engine.wait(&task.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.status, Status::Complete, "{:?}", task.error);
    assert!(task.completed > 1000 && task.sha256.as_ref().is_some_and(|hash| hash.len() == 64));
    let output = Command::new(std::env::var_os("FLUX_FFPROBE").unwrap_or_else(|| "ffprobe".into()))
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
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let streams = value["streams"].as_array().unwrap();
    assert!(streams.iter().any(|stream| stream["codec_type"] == "video"));
    assert!(streams.iter().any(|stream| stream["codec_type"] == "audio"));
    server.abort();
}
use std::future::IntoFuture;
#[tokio::test]
async fn hls_segments_are_muxed_with_audio() {
    exercise("hls").await;
}
#[tokio::test]
async fn dash_segments_are_muxed_with_audio() {
    exercise("dash").await;
}
