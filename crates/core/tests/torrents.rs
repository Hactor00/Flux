use flux_core::{DownloadRequest, Engine, Status, TorrentOptions};
use librqbit::{
    AddTorrent, AddTorrentOptions, CreateTorrentOptions, ListenerOptions, Session, SessionOptions,
};
use sha2::{Digest, Sha256};
use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tempfile::TempDir;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

struct Seeder {
    _root: TempDir,
    session: Arc<Session>,
    torrent: bytes::Bytes,
    magnet: String,
    peer: SocketAddr,
    expected: String,
}
impl Seeder {
    async fn new(length: usize) -> Self {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("fixture.bin");
        let content: Vec<u8> = (0..length)
            .map(|index| ((index.wrapping_mul(31) ^ (index >> 12)) % 251) as u8)
            .collect();
        tokio::fs::write(&file, &content).await.unwrap();
        let torrent = librqbit::create_torrent(
            &file,
            CreateTorrentOptions {
                piece_length: Some(16384),
                ..Default::default()
            },
            &librqbit::spawn_utils::BlockingSpawner::new(2),
        )
        .await
        .unwrap();
        let magnet = torrent.as_magnet().to_string();
        let torrent = torrent.as_bytes().unwrap();
        let session = Session::new_with_opts(
            root.path().into(),
            SessionOptions {
                dht: None,
                disable_trackers: true,
                disable_local_service_discovery: true,
                listen: Some(ListenerOptions {
                    listen_addr: "127.0.0.1:0".parse().unwrap(),
                    ipv4_only: true,
                    ..Default::default()
                }),
                ratelimits: librqbit::limits::LimitsConfig {
                    upload_bps: std::num::NonZeroU32::new(512 * 1024),
                    download_bps: None,
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let handle = session
            .add_torrent(
                AddTorrent::from_bytes(torrent.clone()),
                Some(AddTorrentOptions {
                    overwrite: true,
                    output_folder: Some(root.path().to_str().unwrap().into()),
                    ..Default::default()
                }),
            )
            .await
            .unwrap()
            .into_handle()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), handle.wait_until_initialized())
            .await
            .unwrap()
            .unwrap();
        assert!(handle.stats().finished);
        let peer = session.listen_addr().unwrap();
        Self {
            _root: root,
            session,
            torrent,
            magnet,
            peer,
            expected: hex::encode(Sha256::digest(&content)),
        }
    }
    fn options(&self) -> TorrentOptions {
        TorrentOptions {
            initial_peers: vec![self.peer],
            peer_limit: 4,
            ..Default::default()
        }
    }
    async fn metainfo_file(&self, directory: &std::path::Path) -> PathBuf {
        let path = directory.join("fixture.torrent");
        tokio::fs::write(&path, &self.torrent).await.unwrap();
        path
    }
}

async fn finish(engine: &Engine, id: &str) -> flux_core::Task {
    tokio::time::timeout(Duration::from_secs(40), engine.wait(id))
        .await
        .expect("Torrent completion timed out")
        .unwrap()
}

#[tokio::test]
async fn imported_torrent_resumes_after_restart_and_repairs_corrupt_piece() {
    let seeder = Seeder::new(4 * 1024 * 1024).await;
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let downloads = root.path().join("downloads");
    let engine = Engine::open(&data, &downloads).await.unwrap();
    let metainfo = seeder.metainfo_file(root.path()).await;
    let task = engine
        .import_torrent_file(
            metainfo,
            Some("Imported".into()),
            None,
            false,
            Some(seeder.options()),
        )
        .await
        .unwrap();
    assert_eq!(task.status, Status::Paused);
    assert!(task.request.url.starts_with("torrent:"));
    engine.start(&task.id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let current = engine.task(&task.id).await.unwrap();
            if current.completed >= 64 * 1024 {
                break;
            }
            assert_ne!(current.status, Status::Failed, "{:?}", current.error);
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let paused = tokio::time::timeout(Duration::from_secs(5), engine.pause(&task.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(paused.status, Status::Paused);
    assert!(paused.completed >= 64 * 1024 && paused.completed < 4 * 1024 * 1024);
    engine.shutdown().await.unwrap();
    drop(engine);
    let output = paused.destination.join("fixture.bin");
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(&output)
        .await
        .unwrap();
    file.seek(std::io::SeekFrom::Start(0)).await.unwrap();
    file.write_all(&vec![0xff; 64 * 1024]).await.unwrap();
    file.sync_all().await.unwrap();
    drop(file);
    let engine = Engine::open(&data, &downloads).await.unwrap();
    assert_eq!(engine.task(&task.id).await.unwrap().status, Status::Paused);
    engine.start(&task.id).await.unwrap();
    let complete = finish(&engine, &task.id).await;
    assert_eq!(complete.status, Status::Complete, "{:?}", complete.error);
    assert_eq!(complete.completed, 4 * 1024 * 1024);
    assert_eq!(complete.sha256.as_deref(), Some(seeder.expected.as_str()));
    assert_eq!(
        complete.torrent.as_ref().unwrap().files[0]
            .sha256
            .as_deref(),
        Some(seeder.expected.as_str())
    );
    assert_eq!(
        hex::encode(Sha256::digest(tokio::fs::read(&output).await.unwrap())),
        seeder.expected
    );
    engine.remove(&task.id).await.unwrap();
    assert!(
        output.exists(),
        "Removing a task must preserve downloaded user data"
    );
    engine.shutdown().await.unwrap();
    seeder.session.stop().await;
}

#[tokio::test]
async fn magnet_resolves_metadata_from_embedded_peer_and_verifies_file() {
    let seeder = Seeder::new(256 * 1024).await;
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::open(root.path().join("data"), root.path().join("downloads"))
        .await
        .unwrap();
    let task = engine
        .enqueue(DownloadRequest {
            url: seeder.magnet.clone(),
            filename: "Magnet".into(),
            torrent: Some(seeder.options()),
            expected_sha256: Some(seeder.expected.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    engine.start(&task.id).await.unwrap();
    let task = finish(&engine, &task.id).await;
    assert_eq!(task.status, Status::Complete, "{:?}", task.error);
    assert_eq!(task.sha256.as_deref(), Some(seeder.expected.as_str()));
    assert_eq!(task.torrent.as_ref().unwrap().info_hash.len(), 40);
    assert_eq!(task.torrent.as_ref().unwrap().state, "complete");
    engine.shutdown().await.unwrap();
    seeder.session.stop().await;
}

#[tokio::test]
async fn http_metainfo_uses_native_engine_and_rejects_oversized_input() {
    let seeder = Seeder::new(128 * 1024).await;
    let root = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/fixture.torrent", listener.local_addr().unwrap());
    let bytes = seeder.torrent.clone();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new().route(
                "/fixture.torrent",
                axum::routing::get(move || {
                    let bytes = bytes.clone();
                    async move { ([("content-type", "application/x-bittorrent")], bytes) }
                }),
            ),
        )
        .await
        .unwrap();
    });
    let engine = Engine::open(root.path().join("data"), root.path().join("downloads"))
        .await
        .unwrap();
    let task = engine
        .enqueue(DownloadRequest {
            url,
            filename: "HttpTorrent".into(),
            torrent: Some(seeder.options()),
            ..Default::default()
        })
        .await
        .unwrap();
    engine.start(&task.id).await.unwrap();
    let task = finish(&engine, &task.id).await;
    assert_eq!(task.status, Status::Complete, "{:?}", task.error);
    assert_eq!(task.sha256.as_deref(), Some(seeder.expected.as_str()));
    let oversized = root.path().join("oversized.torrent");
    let file = tokio::fs::File::create(&oversized).await.unwrap();
    file.set_len(8 * 1024 * 1024 + 1).await.unwrap();
    drop(file);
    assert!(engine
        .import_torrent_file(oversized, None, None, false, None)
        .await
        .is_err());
    assert_eq!(engine.list().await.unwrap().len(), 1);
    engine.shutdown().await.unwrap();
    seeder.session.stop().await;
    server.abort();
}

#[tokio::test]
async fn torrent_cannot_overwrite_unowned_destination() {
    let seeder = Seeder::new(128 * 1024).await;
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::open(root.path().join("data"), root.path().join("downloads"))
        .await
        .unwrap();
    let task = engine
        .enqueue(DownloadRequest {
            url: seeder.magnet.clone(),
            filename: "Occupied".into(),
            torrent: Some(seeder.options()),
            ..Default::default()
        })
        .await
        .unwrap();
    tokio::fs::create_dir(&task.destination).await.unwrap();
    let original = task.destination.join("fixture.bin");
    tokio::fs::write(&original, b"user data").await.unwrap();
    engine.start(&task.id).await.unwrap();
    let task = finish(&engine, &task.id).await;
    assert_eq!(task.status, Status::Failed);
    assert!(task.error.unwrap().contains("without task ownership"));
    assert_eq!(tokio::fs::read(original).await.unwrap(), b"user data");
    engine.shutdown().await.unwrap();
    seeder.session.stop().await;
}

#[tokio::test]
async fn http_and_torrent_share_the_configured_global_bandwidth_budget() {
    let seeder = Seeder::new(256 * 1024).await;
    let root = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/plain.bin", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new().route(
                "/plain.bin",
                axum::routing::get(|| async { vec![7u8; 256 * 1024] }),
            ),
        )
        .await
        .unwrap();
    });
    let engine = Engine::open(root.path().join("data"), root.path().join("downloads"))
        .await
        .unwrap();
    let mut settings = engine.settings().await.unwrap();
    settings.speed_limit = 128 * 1024;
    engine.save_settings(settings).await.unwrap();
    let http = engine
        .enqueue(DownloadRequest {
            url,
            filename: "plain.bin".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let torrent = engine
        .enqueue(DownloadRequest {
            url: seeder.magnet.clone(),
            filename: "Shaped".into(),
            torrent: Some(seeder.options()),
            ..Default::default()
        })
        .await
        .unwrap();
    let started = tokio::time::Instant::now();
    engine.start(&http.id).await.unwrap();
    engine.start(&torrent.id).await.unwrap();
    let (http, torrent) = tokio::join!(finish(&engine, &http.id), finish(&engine, &torrent.id));
    assert_eq!(http.status, Status::Complete, "{:?}", http.error);
    assert_eq!(torrent.status, Status::Complete, "{:?}", torrent.error);
    assert!(
        started.elapsed() >= Duration::from_millis(2500),
        "Independent full-sized protocol budgets would bypass the configured aggregate limit"
    );
    assert_eq!(torrent.sha256.as_deref(), Some(seeder.expected.as_str()));
    engine.shutdown().await.unwrap();
    seeder.session.stop().await;
    server.abort();
}

#[tokio::test]
async fn multi_file_selection_verifies_only_requested_outputs() {
    let seed_root = tempfile::tempdir().unwrap();
    let alpha = vec![19u8; 128 * 1024];
    let beta = vec![23u8; 128 * 1024];
    tokio::fs::write(seed_root.path().join("alpha.bin"), &alpha)
        .await
        .unwrap();
    tokio::fs::write(seed_root.path().join("beta.bin"), &beta)
        .await
        .unwrap();
    let torrent = librqbit::create_torrent(
        seed_root.path(),
        CreateTorrentOptions {
            piece_length: Some(16384),
            ..Default::default()
        },
        &librqbit::spawn_utils::BlockingSpawner::new(2),
    )
    .await
    .unwrap()
    .as_bytes()
    .unwrap();
    let parsed = librqbit::torrent_from_bytes(&torrent).unwrap();
    let info = parsed.info.data.validate().unwrap();
    let alpha_index = info
        .iter_file_details()
        .position(|file| file.filename.to_pathbuf() == std::path::Path::new("alpha.bin"))
        .unwrap();
    let seeder = Session::new_with_opts(
        seed_root.path().into(),
        SessionOptions {
            dht: None,
            disable_trackers: true,
            disable_local_service_discovery: true,
            listen: Some(ListenerOptions {
                listen_addr: "127.0.0.1:0".parse().unwrap(),
                ipv4_only: true,
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let handle = seeder
        .add_torrent(
            AddTorrent::from_bytes(torrent.clone()),
            Some(AddTorrentOptions {
                overwrite: true,
                output_folder: Some(seed_root.path().to_str().unwrap().into()),
                ..Default::default()
            }),
        )
        .await
        .unwrap()
        .into_handle()
        .unwrap();
    handle.wait_until_initialized().await.unwrap();
    assert!(handle.stats().finished);
    let root = tempfile::tempdir().unwrap();
    let metainfo = root.path().join("multi.torrent");
    tokio::fs::write(&metainfo, &torrent).await.unwrap();
    let engine = Engine::open(root.path().join("data"), root.path().join("downloads"))
        .await
        .unwrap();
    let task = engine
        .import_torrent_file(
            metainfo,
            None,
            None,
            false,
            Some(TorrentOptions {
                only_files: vec![alpha_index],
                initial_peers: vec![seeder.listen_addr().unwrap()],
                ..Default::default()
            }),
        )
        .await
        .unwrap();
    engine.start(&task.id).await.unwrap();
    let task = finish(&engine, &task.id).await;
    assert_eq!(task.status, Status::Complete, "{:?}", task.error);
    assert_eq!(task.total, Some(alpha.len() as u64));
    assert_eq!(task.sha256, Some(hex::encode(Sha256::digest(&alpha))));
    let files = &task.torrent.as_ref().unwrap().files;
    assert_eq!(
        files
            .iter()
            .filter(|file| file.selected && file.sha256.is_some())
            .count(),
        1
    );
    assert!(files
        .iter()
        .filter(|file| !file.selected)
        .all(|file| file.sha256.is_none()));
    assert_eq!(
        tokio::fs::read(task.destination.join("alpha.bin"))
            .await
            .unwrap(),
        alpha
    );
    engine.shutdown().await.unwrap();
    seeder.stop().await;
}

#[tokio::test]
async fn local_metainfo_rejects_path_traversal_before_task_creation() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("seed.bin");
    tokio::fs::write(&source, b"test").await.unwrap();
    let torrent = librqbit::create_torrent(
        &source,
        CreateTorrentOptions {
            name: Some("../escape.bin"),
            ..Default::default()
        },
        &librqbit::spawn_utils::BlockingSpawner::new(2),
    )
    .await
    .unwrap();
    let metainfo = root.path().join("unsafe.torrent");
    tokio::fs::write(&metainfo, torrent.as_bytes().unwrap())
        .await
        .unwrap();
    let engine = Engine::open(root.path().join("data"), root.path().join("downloads"))
        .await
        .unwrap();
    assert!(engine
        .import_torrent_file(metainfo, None, None, false, None)
        .await
        .is_err());
    assert!(engine.list().await.unwrap().is_empty());
    assert!(!root.path().join("escape.bin").exists());
    engine.shutdown().await.unwrap();
}
