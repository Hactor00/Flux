use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Response, StatusCode},
    routing::get,
    Router,
};
use flux_core::{BrowserCredentials, DownloadRequest, Engine, Status};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
async fn serve(
    State(log): State<Arc<Mutex<Vec<Option<String>>>>>,
    headers: HeaderMap,
) -> Response<Body> {
    let cookie = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    log.lock().unwrap().push(cookie.clone());
    if cookie.as_deref() != Some("session=fixture-secret") {
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::empty())
            .unwrap();
    }
    let bytes = vec![42u8; 128 * 1024];
    let range = headers
        .get("range")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
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
        .header("etag", "\"auth-v1\"")
        .header("content-length", end - start + 1);
    if !range.is_empty() {
        response = response.header(
            "content-range",
            format!("bytes {start}-{end}/{}", bytes.len()),
        );
    }
    response
        .body(Body::from(bytes[start..=end].to_vec()))
        .unwrap()
}
#[tokio::test]
async fn authenticated_ranges_survive_restart_without_plaintext_persistence() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/file", get(serve)).with_state(log),
        )
        .await
        .unwrap();
    });
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let output = root.path().join("output");
    let engine = Engine::open(&data, &output).await.unwrap();
    let credentials = vec![BrowserCredentials {
        origin: origin.clone(),
        cookie: Some("session=fixture-secret".into()),
        authorization: Some("Bearer fixture-auth".into()),
        user_agent: Some("Flux-test".into()),
    }];
    let sealed = flux_core::protect_credentials(&data, &credentials).unwrap();
    let input = DownloadRequest {
        url: format!("{origin}/file"),
        filename: "authenticated.bin".into(),
        protected_credentials: Some(sealed),
        expected_sha256: Some(hex::encode(Sha256::digest(vec![42u8; 128 * 1024]))),
        ..DownloadRequest::default()
    };
    let task = engine.enqueue(input).await.unwrap();
    let public = serde_json::to_string(&task).unwrap();
    assert!(
        !public.contains("fixture-secret")
            && !public.contains("fixture-auth")
            && !public.contains("protected_credentials")
    );
    for filename in ["tasks.sqlite", "tasks.sqlite-wal"] {
        if let Ok(bytes) = std::fs::read(data.join(filename)) {
            assert!(!bytes
                .windows(b"fixture-secret".len())
                .any(|window| window == b"fixture-secret"));
            assert!(!bytes
                .windows(b"fixture-auth".len())
                .any(|window| window == b"fixture-auth"));
        }
    }
    drop(engine);
    let engine = Engine::open(&data, &output).await.unwrap();
    engine.start(&task.id).await.unwrap();
    let task = engine.wait(&task.id).await.unwrap();
    assert_eq!(task.status, Status::Complete, "{:?}", task.error);
    assert!(task.source_validated);
    assert_eq!(
        std::fs::read(task.destination).unwrap(),
        vec![42u8; 128 * 1024]
    );
    server.abort();
}
#[tokio::test]
async fn credentials_are_origin_scoped_and_header_injection_is_rejected() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = log.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/file", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/file", get(serve)).with_state(log),
        )
        .await
        .unwrap();
    });
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::open(root.path().join("data"), root.path().join("out"))
        .await
        .unwrap();
    let mut input = DownloadRequest {
        url,
        filename: "wrong-origin.bin".into(),
        credentials: vec![BrowserCredentials {
            origin: "https://example.invalid".into(),
            cookie: Some("session=fixture-secret".into()),
            authorization: None,
            user_agent: None,
        }],
        ..DownloadRequest::default()
    };
    let task = engine.enqueue(input.clone()).await.unwrap();
    engine.start(&task.id).await.unwrap();
    let task = engine.wait(&task.id).await.unwrap();
    assert_eq!(task.status, Status::Failed);
    assert!(!task.source_validated);
    assert!(seen.lock().unwrap().iter().all(Option::is_none));
    assert!(!task.destination.exists());
    input.filename = "injection.bin".into();
    input.credentials[0].cookie = Some("session=x\r\nX-Injected: yes".into());
    assert!(engine.enqueue(input).await.is_err());
    server.abort();
}
