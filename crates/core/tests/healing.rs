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

#[derive(Clone)]
struct MidTransfer {
    bytes: Arc<Vec<u8>>,
    expired: Arc<std::sync::atomic::AtomicBool>,
    refreshed_offsets: Arc<std::sync::Mutex<Vec<usize>>>,
    status: u16,
}
async fn mid_transfer(
    State(state): State<MidTransfer>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response<Body> {
    let fresh = query.get("token").is_some_and(|token| token == "fresh");
    if !fresh && state.expired.load(Ordering::SeqCst) {
        return Response::builder()
            .status(state.status)
            .body(Body::empty())
            .unwrap();
    }
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
    if fresh {
        state.refreshed_offsets.lock().unwrap().push(start);
    }
    let body = if start == end {
        Body::from(state.bytes[start..=end].to_vec())
    } else {
        Body::from_stream(futures_util::stream::unfold(
            (state.clone(), start),
            move |(state, cursor)| async move {
                if cursor > end {
                    return None;
                }
                if !fresh && state.expired.load(Ordering::SeqCst) {
                    return Some((
                        Err::<Vec<u8>, _>(std::io::Error::new(
                            std::io::ErrorKind::ConnectionReset,
                            "Expired fixture connection",
                        )),
                        (state, end + 1),
                    ));
                }
                tokio::time::sleep(Duration::from_millis(4)).await;
                let next = (cursor + 16384).min(end + 1);
                Some((Ok(state.bytes[cursor..next].to_vec()), (state, next)))
            },
        ))
    };
    Response::builder()
        .status(206)
        .header("content-length", end - start + 1)
        .header(
            "content-range",
            format!("bytes {start}-{end}/{}", state.bytes.len()),
        )
        .header("etag", "\"stable-content\"")
        .body(body)
        .unwrap()
}

#[tokio::test]
async fn session_expiry_mid_transfer_resumes_offsets_after_403_and_410() {
    for status in [403, 410] {
        let state = MidTransfer {
            bytes: Arc::new(
                (0..9 * 1024 * 1024)
                    .map(|index| (index % 251) as u8)
                    .collect(),
            ),
            expired: Default::default(),
            refreshed_offsets: Default::default(),
            status,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let fixture = state.clone();
        let server = tokio::spawn(
            axum::serve(
                listener,
                Router::new()
                    .route("/file", get(mid_transfer))
                    .with_state(fixture),
            )
            .into_future(),
        );
        let root = tempfile::tempdir().unwrap();
        let engine = Engine::open(root.path().join("data"), root.path().join("out"))
            .await
            .unwrap();
        let task = engine
            .enqueue(DownloadRequest {
                url: format!("{origin}/file?token=old&id=stable"),
                filename: "renewed.bin".into(),
                connections: 4,
                browser_source: Some(BrowserSource {
                    page_url: format!("{origin}/watch"),
                    frame_url: None,
                }),
                expected_sha256: Some(hex::encode(Sha256::digest(state.bytes.as_ref()))),
                ..Default::default()
            })
            .await
            .unwrap();
        let watcher = engine.clone();
        let refresh = tokio::spawn(async move {
            loop {
                for ticket in watcher.refresh_poll().await {
                    watcher
                        .refresh_submit(
                            ticket.ticket,
                            ticket.page_url,
                            ticket.url.replace("token=old", "token=fresh"),
                            None,
                        )
                        .await
                        .unwrap();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        let mut events = engine.subscribe();
        engine.start(&task.id).await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let progress = events.recv().await.unwrap();
                if progress.id == task.id && progress.completed > 65536 {
                    break;
                }
            }
        })
        .await
        .unwrap();
        state.expired.store(true, Ordering::SeqCst);
        let complete = tokio::time::timeout(Duration::from_secs(20), engine.wait(&task.id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(complete.status, Status::Complete, "{:?}", complete.error);
        assert_eq!(
            tokio::fs::read(&complete.destination).await.unwrap(),
            *state.bytes
        );
        assert!(
            state
                .refreshed_offsets
                .lock()
                .unwrap()
                .iter()
                .any(|offset| *offset > 0 && offset % (4 * 1024 * 1024) != 0),
            "Renewed ranges must retain an already written prefix"
        );
        engine.shutdown().await.unwrap();
        refresh.abort();
        server.abort();
    }
}
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
