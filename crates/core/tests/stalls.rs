use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, Response},
    routing::get,
    Router,
};
use flux_core::{DownloadRequest, Engine, Status};
use sha2::{Digest, Sha256};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

#[tokio::test]
async fn stalled_range_reconnects_from_written_offset_without_waiting_for_server() {
    #[derive(Clone)]
    struct Fixture {
        bytes: Arc<Vec<u8>>,
        stalled: Arc<AtomicBool>,
        ranges: Arc<Mutex<Vec<String>>>,
    }
    async fn serve(State(fixture): State<Fixture>, headers: HeaderMap) -> Response<Body> {
        let range = headers.get("range").unwrap().to_str().unwrap().to_owned();
        fixture.ranges.lock().unwrap().push(range.clone());
        let (start, end) = range
            .strip_prefix("bytes=")
            .unwrap()
            .split_once('-')
            .unwrap();
        let start: usize = start.parse().unwrap();
        let end: usize = end.parse().unwrap();
        let stall = end > 0 && !fixture.stalled.swap(true, Ordering::SeqCst);
        let body = if stall {
            let bytes = fixture.bytes.clone();
            Body::from_stream(futures_util::stream::unfold(0, move |step| {
                let bytes = bytes.clone();
                async move {
                    if step == 0 {
                        Some((
                            Ok::<_, std::io::Error>(Bytes::copy_from_slice(
                                &bytes[start..start + 65536],
                            )),
                            1,
                        ))
                    } else if step == 1 {
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        Some((Ok(Bytes::copy_from_slice(&bytes[start + 65536..=end])), 2))
                    } else {
                        None
                    }
                }
            }))
        } else {
            Body::from(fixture.bytes[start..=end].to_vec())
        };
        Response::builder()
            .status(206)
            .header("etag", "\"fixture-v1\"")
            .header(
                "content-range",
                format!("bytes {start}-{end}/{}", fixture.bytes.len()),
            )
            .header("content-length", end - start + 1)
            .body(body)
            .unwrap()
    }
    let fixture = Fixture {
        bytes: Arc::new((0..256 * 1024).map(|offset| (offset % 251) as u8).collect()),
        stalled: Arc::new(AtomicBool::new(false)),
        ranges: Arc::new(Mutex::new(Vec::new())),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let application = Router::new()
        .route("/file", get(serve))
        .with_state(fixture.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, application).await.unwrap();
    });
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::open(root.path().join("data"), root.path().join("downloads"))
        .await
        .unwrap();
    let task = engine
        .enqueue(DownloadRequest {
            url: format!("http://{address}/file"),
            filename: "file.bin".into(),
            connections: 1,
            expected_sha256: Some(hex::encode(Sha256::digest(fixture.bytes.as_slice()))),
            ..Default::default()
        })
        .await
        .unwrap();
    let started = tokio::time::Instant::now();
    engine.start(&task.id).await.unwrap();
    let task = tokio::time::timeout(Duration::from_secs(40), engine.wait(&task.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.status, Status::Complete, "{:?}", task.error);
    assert!(started.elapsed() >= Duration::from_secs(30));
    assert_eq!(
        tokio::fs::read(&task.destination).await.unwrap(),
        *fixture.bytes
    );
    assert!(fixture
        .ranges
        .lock()
        .unwrap()
        .iter()
        .any(|range| range == "bytes=65536-262143"));
    engine.shutdown().await.unwrap();
    server.abort();
}
