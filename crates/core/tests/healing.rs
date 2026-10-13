use axum::{
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, Response},
    routing::get,
    Router,
};
use flux_core::{BrowserSource, DownloadRequest, Engine, Status};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    future::IntoFuture,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
async fn serve(
    State(bytes): State<Arc<Vec<u8>>>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response<Body> {
    if query.get("token").map(String::as_str) != Some("fresh") {
        return Response::builder().status(403).body(Body::empty()).unwrap();
    }
    let range = headers.get("range").and_then(|v| v.to_str().ok()).unwrap();
    let (start, end) = range
        .strip_prefix("bytes=")
        .unwrap()
        .split_once('-')
        .unwrap();
    let start: usize = start.parse().unwrap();
    let end: usize = end.parse().unwrap();
    Response::builder()
        .status(206)
        .header("content-length", end - start + 1)
        .header(
            "content-range",
            format!("bytes {start}-{end}/{}", bytes.len()),
        )
        .header("etag", "\"same-object\"")
        .body(Body::from(bytes[start..=end].to_vec()))
        .unwrap()
}
#[tokio::test]
async fn expired_download_is_renewed_in_flight_and_still_verified() {
    let bytes: Arc<Vec<u8>> = Arc::new((0..9 * 1024 * 1024).map(|v| (v % 251) as u8).collect());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(
        axum::serve(
            listener,
            Router::new()
                .route("/file", get(serve))
                .with_state(bytes.clone()),
        )
        .into_future(),
    );
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::open(root.path().join("data"), root.path().join("out"))
        .await
        .unwrap();
    let task = engine
        .enqueue(DownloadRequest {
            url: format!("{origin}/file?token=expired&id=content"),
            filename: "healed.bin".into(),
            browser_source: Some(BrowserSource {
                page_url: format!("{origin}/watch"),
                frame_url: None,
            }),
            connections: 4,
            expected_sha256: Some(hex::encode(Sha256::digest(bytes.as_ref()))),
            ..Default::default()
        })
        .await
        .unwrap();
    engine.start(&task.id).await.unwrap();
    let watcher = engine.clone();
    let hits = Arc::new(AtomicUsize::new(0));
    let count = hits.clone();
    let refresh = tokio::spawn(async move {
        loop {
            for request in watcher.refresh_poll().await {
                watcher
                    .refresh_submit(
                        request.ticket,
                        request.page_url,
                        request.url.replace("expired", "fresh"),
                        None,
                    )
                    .await
                    .unwrap();
                count.fetch_add(1, Ordering::SeqCst);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    let task = tokio::time::timeout(Duration::from_secs(20), engine.wait(&task.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.status, Status::Complete, "{:?}", task.error);
    assert_eq!(std::fs::read(task.destination).unwrap(), *bytes);
    assert!(hits.load(Ordering::SeqCst) > 0);
    refresh.abort();
    server.abort();
}
