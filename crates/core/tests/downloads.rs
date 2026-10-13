use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, Response, StatusCode},
    routing::get,
    Router,
};
use flux_core::{DownloadRequest, Engine, Status};
use sha2::{Digest, Sha256};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tempfile::TempDir;

#[tokio::test]
async fn global_limit_is_shared_between_tasks_and_changes_wake_waiting_workers() {
    let f = Fixture::new("normal", 64 * 1024).await;
    let mut settings = f.engine.settings().await.unwrap();
    settings.speed_limit = 64 * 1024;
    f.engine.save_settings(settings.clone()).await.unwrap();
    let mut one = f.input();
    one.filename = "global-one.bin".into();
    let mut two = one.clone();
    two.filename = "global-two.bin".into();
    let tasks = f.engine.enqueue_batch(vec![one, two]).await.unwrap();
    let started = tokio::time::Instant::now();
    for task in &tasks {
        f.engine.start(&task.id).await.unwrap();
    }
    for task in &tasks {
        f.assert_file(&f.engine.wait(&task.id).await.unwrap()).await;
    }
    assert!(started.elapsed() >= Duration::from_millis(1800));
    settings.speed_limit = 1;
    f.engine.save_settings(settings.clone()).await.unwrap();
    let mut input = f.input();
    input.filename = "changed-limit.bin".into();
    let task = f.engine.enqueue(input).await.unwrap();
    f.engine.start(&task.id).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    settings.speed_limit = 0;
    f.engine.save_settings(settings).await.unwrap();
    let task = tokio::time::timeout(Duration::from_secs(3), f.engine.wait(&task.id))
        .await
        .unwrap()
        .unwrap();
    f.assert_file(&task).await;
}
#[tokio::test]
async fn wildcard_listing_is_scoped_and_ipv6_is_not_a_numeric_range() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener,Router::new().route("/files/",get(||async{r#"<a href="a.zip">a</a><a href="a.zip">duplicate</a><a href="b.txt">b</a><a href="../escape.zip">escape</a><a href="https://example.invalid/foreign.zip">foreign</a>"#}))).await.unwrap();
    });
    assert_eq!(
        flux_core::expand_batch(&format!("{base}/files/*.zip"))
            .await
            .unwrap(),
        vec![format!("{base}/files/a.zip")]
    );
    assert_eq!(
        flux_core::expand_batch("http://[::1]:8080/file.zip")
            .await
            .unwrap(),
        vec!["http://[::1]:8080/file.zip"]
    );
    server.abort();
}

#[derive(Clone)]
struct Server {
    bytes: Arc<Vec<u8>>,
    mode: &'static str,
    requests: Arc<Mutex<Vec<String>>>,
    changed: Arc<AtomicBool>,
    failures: Arc<AtomicUsize>,
}
async fn handler(State(server): State<Server>, headers: HeaderMap) -> Response<Body> {
    if server.mode == "forbidden-range" && headers.contains_key("range") {
        return Response::builder().status(403).body(Body::empty()).unwrap();
    }
    let tag = if server.changed.load(Ordering::SeqCst) {
        "\"version-2\""
    } else {
        "\"version-1\""
    };
    if headers.get("if-match").is_some_and(|value| value != tag) {
        return Response::builder().status(412).body(Body::empty()).unwrap();
    }
    let range = headers
        .get("range")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_owned();
    server.requests.lock().unwrap().push(range.clone());
    if server.bytes.is_empty() {
        return Response::builder()
            .status(416)
            .header("content-range", "bytes */0")
            .body(Body::empty())
            .unwrap();
    }
    let ranged = !range.is_empty() && server.mode != "no-range" && server.mode != "unknown";
    let (start, end) = if ranged {
        let (start, end) = range
            .strip_prefix("bytes=")
            .unwrap()
            .split_once('-')
            .unwrap();
        (
            start.parse::<usize>().unwrap(),
            end.parse::<usize>().unwrap(),
        )
    } else {
        (0, server.bytes.len() - 1)
    };
    let probe = range == "bytes=0-0";
    if ((!probe && server.mode == "rate-limit") || (probe && server.mode == "probe-rate-limit"))
        && server.failures.fetch_add(1, Ordering::SeqCst) == 0
    {
        return Response::builder()
            .status(429)
            .header("retry-after", "1")
            .body(Body::empty())
            .unwrap();
    }
    if !probe && server.mode == "ignore-range" {
        return Response::builder()
            .status(200)
            .header("etag", tag)
            .body(Body::from(server.bytes.as_ref().clone()))
            .unwrap();
    }
    let mut response = Response::builder().status(if ranged {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    });
    if server.mode != "no-etag" {
        response = response.header("etag", tag);
    }
    if server.mode != "unknown" {
        response = response.header("content-length", end - start + 1);
    }
    if ranged {
        response = response.header(
            "content-range",
            format!(
                "bytes {}-{}/{}",
                if server.mode == "bad-range" && !probe {
                    start + 1
                } else {
                    start
                },
                end,
                server.bytes.len()
            ),
        );
    }
    if probe && ranged {
        return response.body(Body::from(vec![server.bytes[0]])).unwrap();
    }
    let bytes = server.bytes.clone();
    let mode = server.mode;
    let fail_stream = mode == "disconnect" && server.failures.fetch_add(1, Ordering::SeqCst) == 0;
    let stream = futures_util::stream::unfold((start, false), move |(offset, failed)| {
        let bytes = bytes.clone();
        async move {
            if offset > end || failed {
                return None;
            }
            if mode == "slow" || (mode == "tail-slow" && start >= 4 * 1024 * 1024) {
                tokio::time::sleep(Duration::from_millis(3)).await;
            }
            if fail_stream && offset >= start + 65536 {
                return Some((
                    Err(std::io::Error::other("Injected disconnect")),
                    (offset, true),
                ));
            }
            let next = (offset + 16384).min(end + 1);
            Some((
                Ok(Bytes::copy_from_slice(&bytes[offset..next])),
                (next, false),
            ))
        }
    });
    response.body(Body::from_stream(stream)).unwrap()
}
struct Fixture {
    root: TempDir,
    engine: Engine,
    server: Server,
    url: String,
    handle: tokio::task::JoinHandle<()>,
}
impl Fixture {
    async fn new(mode: &'static str, size: usize) -> Self {
        let server = Server {
            bytes: Arc::new((0..size).map(|i| (i % 251) as u8).collect()),
            mode,
            requests: Default::default(),
            changed: Default::default(),
            failures: Default::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        let router = Router::new()
            .route("/file", get(handler))
            .with_state(server.clone());
        let handle = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let root = tempfile::tempdir().unwrap();
        let engine = Engine::open(root.path().join("data"), root.path().join("output"))
            .await
            .unwrap();
        Self {
            root,
            engine,
            server,
            url,
            handle,
        }
    }
    fn input(&self) -> DownloadRequest {
        DownloadRequest {
            url: self.url.clone(),
            filename: "payload.bin".into(),
            connections: 4,
            speed_limit: 0,
            expected_sha256: Some(hex::encode(Sha256::digest(self.server.bytes.as_slice()))),
            ..DownloadRequest::default()
        }
    }
    async fn download(&self) -> flux_core::Task {
        let task = self.engine.enqueue(self.input()).await.unwrap();
        self.engine.start(&task.id).await.unwrap();
        tokio::time::timeout(Duration::from_secs(15), self.engine.wait(&task.id))
            .await
            .unwrap()
            .unwrap()
    }
    async fn assert_file(&self, task: &flux_core::Task) {
        assert_eq!(task.status, Status::Complete, "{:?}", task.error);
        assert_eq!(
            tokio::fs::read(&task.destination).await.unwrap(),
            *self.server.bytes
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn wait_for_partial_bytes(engine: &Engine, id: &str) {
    let mut events = engine.subscribe();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match events.recv().await {
                Ok(event) if event.id == id && event.completed > 0 => break,
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(error) => panic!("Cannot observe partial download: {error}"),
            }
        }
    })
    .await
    .expect("The fixture did not write partial data before the deadline");
}

#[tokio::test]
async fn scheduler_obeys_priority_and_does_not_restart_paused_tasks() {
    let fixture = Fixture::new("slow", 4 * 1024 * 1024).await;
    let mut settings = fixture.engine.settings().await.unwrap();
    settings.concurrent_tasks = 1;
    fixture.engine.save_settings(settings).await.unwrap();
    let mut input = fixture.input();
    input.filename = "low.bin".into();
    input.priority = -5;
    let low = fixture.engine.enqueue(input.clone()).await.unwrap();
    input.filename = "high.bin".into();
    input.priority = 5;
    let high = fixture.engine.enqueue(input).await.unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    let engine = fixture.engine.clone();
    let stop = cancel.clone();
    let scheduler = tokio::spawn(async move { engine.run_scheduler(stop).await });
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        fixture.engine.task(&high.id).await.unwrap().status,
        Status::Running
    );
    assert_eq!(
        fixture.engine.task(&low.id).await.unwrap().status,
        Status::Queued
    );
    fixture.engine.pause(&high.id).await.unwrap();
    tokio::time::sleep(Duration::from_millis(450)).await;
    assert_eq!(
        fixture.engine.task(&high.id).await.unwrap().status,
        Status::Paused
    );
    assert_eq!(
        fixture.engine.task(&low.id).await.unwrap().status,
        Status::Running
    );
    cancel.cancel();
    scheduler.await.unwrap().unwrap();
    fixture.engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn scheduled_start_and_stop_and_settings_survive_reopen() {
    let fixture = Fixture::new("slow", 8 * 1024 * 1024).await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut input = fixture.input();
    input.start_at = Some(now + 2);
    input.stop_at = Some(now + 3);
    input.connections = 1;
    input.speed_limit = 65536;
    let task = fixture.engine.enqueue(input).await.unwrap();
    let mut settings = fixture.engine.settings().await.unwrap();
    settings.locale = "tr-TR".into();
    settings.concurrent_tasks = 1;
    fixture.engine.save_settings(settings).await.unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    let engine = fixture.engine.clone();
    let stop = cancel.clone();
    let scheduler = tokio::spawn(async move { engine.run_scheduler(stop).await });
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        fixture.engine.task(&task.id).await.unwrap().status,
        Status::Queued
    );
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert_eq!(
        fixture.engine.task(&task.id).await.unwrap().status,
        Status::Paused
    );
    assert!(fixture.engine.task(&task.id).await.unwrap().completed > 0);
    cancel.cancel();
    scheduler.await.unwrap().unwrap();
    assert_eq!(fixture.engine.settings().await.unwrap().locale, "tr-TR");
}

#[tokio::test]
async fn batch_expands_ranges_deduplicates_and_rejects_credentials() {
    let urls = flux_core::expand_batch(
        "https://example.com/file[001-003].zip\nhttps://example.com/file001.zip",
    )
    .await
    .unwrap();
    assert_eq!(
        urls,
        [
            "https://example.com/file001.zip",
            "https://example.com/file002.zip",
            "https://example.com/file003.zip"
        ]
    );
    assert!(flux_core::expand_batch("https://example.com/[1-600].bin")
        .await
        .is_err());
    assert!(
        flux_core::expand_batch("https://name:secret@example.com/file.bin")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn parallel_ranges_write_and_verify_exact_bytes() {
    let f = Fixture::new("normal", 10 * 1024 * 1024 + 17).await;
    let task = f.download().await;
    f.assert_file(&task).await;
    let requests = f.server.requests.lock().unwrap();
    assert!(requests.iter().any(|r| r == "bytes=0-4194303"));
    assert!(requests.iter().any(|r| r == "bytes=4194304-8388607"));
}

#[tokio::test]
async fn direct_staging_is_preallocated_and_never_creates_chunk_files() {
    let f = Fixture::new("slow", 64 * 1024 * 1024).await;
    let mut input = f.input();
    input.connections = 32;
    let task = f.engine.enqueue(input).await.unwrap();
    f.engine.start(&task.id).await.unwrap();
    wait_for_partial_bytes(&f.engine, &task.id).await;
    let paused = f.engine.pause(&task.id).await.unwrap();
    assert!(paused.completed > 0 && paused.completed < 64 * 1024 * 1024);
    assert!(paused.direct_write);
    let stage = task
        .destination
        .parent()
        .unwrap()
        .join(format!(".flux-{}.staging", task.id));
    assert_eq!(
        tokio::fs::metadata(&stage).await.unwrap().len(),
        64 * 1024 * 1024
    );
    let mut entries = tokio::fs::read_dir(f.root.path().join("data/parts").join(&task.id))
        .await
        .unwrap();
    assert!(entries.next_entry().await.unwrap().is_none());
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        assert_ne!(
            std::fs::metadata(&stage).unwrap().file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_SPARSE_FILE,
            0
        );
    }
    f.engine.start(&task.id).await.unwrap();
    f.assert_file(&f.engine.wait(&task.id).await.unwrap()).await;
    assert!(!stage.exists());
}

#[tokio::test]
async fn legacy_chunk_checkpoints_migrate_without_losing_verified_prefixes() {
    let f = Fixture::new("slow", 10 * 1024 * 1024).await;
    let task = f.engine.enqueue(f.input()).await.unwrap();
    f.engine.start(&task.id).await.unwrap();
    wait_for_partial_bytes(&f.engine, &task.id).await;
    let paused = f.engine.pause(&task.id).await.unwrap();
    assert!(paused.completed > 0);
    let stage = task
        .destination
        .parent()
        .unwrap()
        .join(format!(".flux-{}.staging", task.id));
    let staged = tokio::fs::read(&stage).await.unwrap();
    let db = rusqlite::Connection::open(f.root.path().join("data/tasks.sqlite")).unwrap();
    let checkpoint_bodies = db
        .prepare("SELECT body FROM parts WHERE task_id=?1 ORDER BY idx")
        .unwrap()
        .query_map([&task.id], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let mut resumed_offsets = Vec::new();
    for body in checkpoint_bodies {
        let part: serde_json::Value = serde_json::from_str(&body).unwrap();
        let start = part["start"].as_u64().unwrap() as usize;
        let done = part["done"].as_u64().unwrap() as usize;
        let path = f
            .root
            .path()
            .join("data/parts")
            .join(&task.id)
            .join(format!("{}.part", part["index"].as_u64().unwrap()));
        tokio::fs::write(path, &staged[start..start + done])
            .await
            .unwrap();
        if done > 0 && done < part["end"].as_u64().unwrap() as usize - start + 1 {
            resumed_offsets.push(start + done);
        }
    }
    assert!(!resumed_offsets.is_empty());
    db.execute(
        "UPDATE tasks SET body=json_set(body,'$.direct_write',json('false')) WHERE id=?1",
        [&task.id],
    )
    .unwrap();
    drop(db);
    tokio::fs::remove_file(&stage).await.unwrap();
    f.server.requests.lock().unwrap().clear();
    f.engine.start(&task.id).await.unwrap();
    let complete = f.engine.wait(&task.id).await.unwrap();
    f.assert_file(&complete).await;
    assert!(complete.direct_write);
    let requests = f.server.requests.lock().unwrap();
    assert!(resumed_offsets.iter().any(|offset| requests
        .iter()
        .any(|request| request.starts_with(&format!("bytes={offset}-")))));
}
#[tokio::test]
async fn no_range_and_no_validator_fall_back_to_safe_single_stream() {
    for mode in ["no-range", "no-etag", "unknown"] {
        let f = Fixture::new(mode, 1024 * 1024 + 3).await;
        let task = f.download().await;
        f.assert_file(&task).await;
        assert!(!task.ranged);
        assert!(f
            .server
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(String::is_empty));
    }
}
#[tokio::test]
async fn empty_file_is_published_and_hashed() {
    let f = Fixture::new("normal", 0).await;
    let task = f.download().await;
    f.assert_file(&task).await;
    assert_eq!(task.total, Some(0));
}
#[tokio::test]
async fn ignored_or_incorrect_ranges_are_never_published() {
    for mode in ["ignore-range", "bad-range"] {
        let f = Fixture::new(mode, 1024 * 1024).await;
        let task = f.download().await;
        assert_eq!(task.status, Status::Failed);
        assert!(!task.destination.exists());
    }
}
#[tokio::test]
async fn digest_mismatch_never_publishes_output() {
    let f = Fixture::new("normal", 65536).await;
    let mut input = f.input();
    input.expected_sha256 = Some("00".repeat(32));
    let task = f.engine.enqueue(input).await.unwrap();
    f.engine.start(&task.id).await.unwrap();
    let task = f.engine.wait(&task.id).await.unwrap();
    assert_eq!(task.status, Status::Failed);
    assert!(!task.destination.exists());
}
#[tokio::test]
async fn pause_checkpoints_then_resume_uses_byte_offsets() {
    let f = Fixture::new("slow", 10 * 1024 * 1024).await;
    let task = f.engine.enqueue(f.input()).await.unwrap();
    f.engine.start(&task.id).await.unwrap();
    wait_for_partial_bytes(&f.engine, &task.id).await;
    let paused = f.engine.pause(&task.id).await.unwrap();
    assert_eq!(paused.status, Status::Paused);
    assert!(paused.completed > 0);
    f.server.requests.lock().unwrap().clear();
    f.engine.start(&task.id).await.unwrap();
    f.assert_file(&f.engine.wait(&task.id).await.unwrap()).await;
    assert!(f
        .server
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|r| r.starts_with("bytes=") && r != "bytes=0-0" && !r.starts_with("bytes=0-")));
}
#[tokio::test]
async fn corrupt_checkpoint_is_re_downloaded() {
    let f = Fixture::new("slow", 5 * 1024 * 1024).await;
    let task = f.engine.enqueue(f.input()).await.unwrap();
    f.engine.start(&task.id).await.unwrap();
    wait_for_partial_bytes(&f.engine, &task.id).await;
    f.engine.pause(&task.id).await.unwrap();
    let part = task
        .destination
        .parent()
        .unwrap()
        .join(format!(".flux-{}.staging", task.id));
    let mut bytes = tokio::fs::read(&part).await.unwrap();
    assert!(!bytes.is_empty());
    bytes[0] ^= 0xff;
    tokio::fs::write(part, bytes).await.unwrap();
    f.server.requests.lock().unwrap().clear();
    f.engine.start(&task.id).await.unwrap();
    f.assert_file(&f.engine.wait(&task.id).await.unwrap()).await;
    assert!(f
        .server
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|r| r == "bytes=0-4194303"));
}
#[tokio::test]
async fn new_etag_discards_old_checkpoints() {
    let f = Fixture::new("slow", 5 * 1024 * 1024).await;
    let task = f.engine.enqueue(f.input()).await.unwrap();
    f.engine.start(&task.id).await.unwrap();
    wait_for_partial_bytes(&f.engine, &task.id).await;
    f.engine.pause(&task.id).await.unwrap();
    f.server.changed.store(true, Ordering::SeqCst);
    f.server.requests.lock().unwrap().clear();
    f.engine.start(&task.id).await.unwrap();
    f.assert_file(&f.engine.wait(&task.id).await.unwrap()).await;
    assert!(f
        .server
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|r| r == "bytes=0-4194303"));
}
#[tokio::test]
async fn rate_limit_and_disconnection_retry_safely() {
    for mode in ["rate-limit", "probe-rate-limit", "disconnect"] {
        let f = Fixture::new(mode, 256 * 1024).await;
        let task = f.download().await;
        f.assert_file(&task).await;
        assert!(f.server.failures.load(Ordering::SeqCst) >= 2);
    }
}
#[tokio::test]
async fn existing_destination_and_duplicate_names_are_rejected() {
    let f = Fixture::new("normal", 512).await;
    let first = f.engine.enqueue(f.input()).await.unwrap();
    assert!(f.engine.enqueue(f.input()).await.is_err());
    let sentinel = b"existing user data";
    tokio::fs::write(&first.destination, sentinel)
        .await
        .unwrap();
    f.engine.start(&first.id).await.unwrap();
    let task = f.engine.wait(&first.id).await.unwrap();
    assert_eq!(task.status, Status::Failed);
    assert_eq!(tokio::fs::read(task.destination).await.unwrap(), sentinel);
}
#[tokio::test]
async fn idempotent_enqueue_and_single_owner_lock() {
    let f = Fixture::new("normal", 512).await;
    let id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        f.engine.enqueue_id(id.clone(), f.input()).await.unwrap().id,
        id
    );
    assert_eq!(
        f.engine.enqueue_id(id.clone(), f.input()).await.unwrap().id,
        id
    );
    assert_eq!(f.engine.list().await.unwrap().len(), 1);
    assert!(
        Engine::open(f.root.path().join("data"), f.root.path().join("output"))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn rejects_unsafe_filename_and_connection_count() {
    let f = Fixture::new("normal", 512).await;
    for name in ["../escape", "CON.txt", "name.", "foo:bar", "LPT1"] {
        let mut input = f.input();
        input.filename = name.into();
        assert!(f.engine.enqueue(input).await.is_err());
    }
    let mut input = f.input();
    input.connections = 33;
    assert!(f.engine.enqueue(input).await.is_err());
}

#[tokio::test]
async fn idle_workers_split_a_lagging_range_without_overlapping_bytes() {
    let f = Fixture::new("tail-slow", 8 * 1024 * 1024).await;
    let task = f.download().await;
    f.assert_file(&task).await;
    let requests = f.server.requests.lock().unwrap();
    assert!(
        requests.len() > 3,
        "Expected active range rescue: {requests:?}"
    );
    assert!(requests
        .iter()
        .filter(|r| r.as_str() != "bytes=0-0")
        .any(|r| r.ends_with("-8388607") && !r.starts_with("bytes=4194304-")));
}

#[tokio::test]
async fn persisted_task_resumes_after_engine_reopens() {
    let f = Fixture::new("slow", 5 * 1024 * 1024).await;
    let data = f.root.path().join("restart-data");
    let output = f.root.path().join("restart-output");
    let engine = Engine::open(&data, &output).await.unwrap();
    let task = engine.enqueue(f.input()).await.unwrap();
    assert!(task.created_at.is_some());
    assert!(task.started_at.is_none() && task.finished_at.is_none());
    engine.start(&task.id).await.unwrap();
    wait_for_partial_bytes(&engine, &task.id).await;
    let paused = engine.pause(&task.id).await.unwrap();
    assert!(paused.completed > 0);
    let first_start = paused.started_at.unwrap();
    assert!(first_start >= task.created_at.unwrap());
    engine.shutdown().await.unwrap();
    drop(engine);
    tokio::task::yield_now().await;
    let reopened = Engine::open(data, output).await.unwrap();
    assert_eq!(
        reopened.task(&task.id).await.unwrap().status,
        Status::Paused
    );
    reopened.start(&task.id).await.unwrap();
    let completed = reopened.wait(&task.id).await.unwrap();
    f.assert_file(&completed).await;
    assert_eq!(completed.started_at, Some(first_start));
    assert!(completed
        .finished_at
        .is_some_and(|time| time >= first_start));
    assert!(!completed.timestamp_estimated);
}

#[tokio::test]
async fn legacy_completed_tasks_recover_file_time_without_inventing_start_time() {
    let f = Fixture::new("normal", 65536).await;
    let data = f.root.path().join("legacy-data");
    let output = f.root.path().join("legacy-output");
    let engine = Engine::open(&data, &output).await.unwrap();
    let task = engine.enqueue(f.input()).await.unwrap();
    engine.start(&task.id).await.unwrap();
    let completed = engine.wait(&task.id).await.unwrap();
    f.assert_file(&completed).await;
    let expected = std::fs::metadata(&completed.destination)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    engine.shutdown().await.unwrap();
    drop(engine);
    {
        let database = rusqlite::Connection::open(data.join("tasks.sqlite")).unwrap();
        database.execute("UPDATE tasks SET body=json_remove(body,'$.created_at','$.started_at','$.finished_at','$.timestamp_estimated') WHERE id=?1", [&task.id]).unwrap();
    }
    let engine = Engine::open(&data, &output).await.unwrap();
    let historical = engine.task(&task.id).await.unwrap();
    assert_eq!(historical.finished_at, Some(expected));
    assert!(historical.timestamp_estimated);
    assert!(historical.created_at.is_none() && historical.started_at.is_none());
}

#[tokio::test]
async fn token_bucket_limits_the_aggregate_file_rate() {
    let f = Fixture::new("normal", 64 * 1024).await;
    let mut input = f.input();
    input.speed_limit = 64 * 1024;
    let task = f.engine.enqueue(input).await.unwrap();
    let at = tokio::time::Instant::now();
    f.engine.start(&task.id).await.unwrap();
    f.assert_file(&f.engine.wait(&task.id).await.unwrap()).await;
    assert!(at.elapsed() >= Duration::from_millis(900));
}
#[tokio::test]
async fn valid_plain_get_survives_servers_that_forbid_range_probes() {
    let f = Fixture::new("forbidden-range", 1024 * 128).await;
    let task = f.engine.enqueue(f.input()).await.unwrap();
    f.engine.start(&task.id).await.unwrap();
    let task = f.engine.wait(&task.id).await.unwrap();
    f.assert_file(&task).await;
    assert!(!task.ranged);
}
