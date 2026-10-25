use axum::{body::Body, http::Response, routing::get, Router};
use flux_core::{DownloadRequest, Engine, Status};
use std::{future::IntoFuture, time::Duration};

#[tokio::test]
async fn preflight_reports_size_and_can_save_torrent_metadata_as_an_ordinary_file() {
    let bytes = b"d4:infod6:lengthi5e4:name8:demo.txt12:piece lengthi16384e6:pieces20:12345678901234567890ee".to_vec();
    let served = bytes.clone();
    let app = Router::new().route(
        "/sample.torrent",
        get(move || {
            let body = served.clone();
            async move {
                Response::builder()
                    .header("Content-Type", "application/x-bittorrent")
                    .body(Body::from(body))
                    .unwrap()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/sample.torrent", listener.local_addr().unwrap());
    let server = tokio::spawn(axum::serve(listener, app).into_future());
    let data = tempfile::tempdir().unwrap();
    let downloads = tempfile::tempdir().unwrap();
    let engine = Engine::open(data.path(), downloads.path()).await.unwrap();
    let info = engine.source_info(url.clone(), None, None).await.unwrap();
    assert_eq!(info.size, Some(bytes.len() as u64));
    assert_eq!(info.files.len(), 1);
    assert_eq!(info.files[0].length, 5);
    let task = engine
        .enqueue(DownloadRequest {
            url,
            filename: "sample.torrent".into(),
            save_torrent_file: true,
            ..Default::default()
        })
        .await
        .unwrap();
    engine.start(&task.id).await.unwrap();
    let task = tokio::time::timeout(Duration::from_secs(10), engine.wait(&task.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.status, Status::Complete, "{:?}", task.error);
    assert!(task.torrent.is_none());
    assert_eq!(tokio::fs::read(task.destination).await.unwrap(), bytes);
    let malformed = data.path().join("invalid.torrent");
    tokio::fs::write(&malformed, b"not a torrent")
        .await
        .unwrap();
    assert!(engine
        .source_info(String::new(), Some(malformed), None)
        .await
        .is_err());
    server.abort();
}
