use flux_core::{Engine, Status, TorrentWatch, WatchMode};
use std::{path::Path, time::Duration};

async fn fixture(root: &Path) -> Vec<u8> {
    let source = root.join("example.bin");
    tokio::fs::write(&source, b"watch folder transfer fixture")
        .await
        .unwrap();
    librqbit::create_torrent(
        &source,
        librqbit::CreateTorrentOptions::default(),
        &librqbit::spawn_utils::BlockingSpawner::new(1),
    )
    .await
    .unwrap()
    .as_bytes()
    .unwrap()
    .to_vec()
}

#[tokio::test]
async fn watch_imports_stable_files_once_and_keeps_deduplication_after_restart_and_removal() {
    let root = tempfile::tempdir().unwrap();
    let watch = root.path().join("watch");
    tokio::fs::create_dir(&watch).await.unwrap();
    let bytes = fixture(root.path()).await;
    let path = watch.join("Example.TORRENT");
    tokio::fs::write(&path, &bytes).await.unwrap();
    let data = root.path().join("data");
    let downloads = root.path().join("downloads");
    let engine = Engine::open(&data, &downloads).await.unwrap();
    let mut settings = engine.settings().await.unwrap();
    settings.torrent_watch = TorrentWatch {
        enabled: true,
        folders: vec![watch.clone()],
        ..Default::default()
    };
    engine.save_settings(settings).await.unwrap();
    assert_eq!(engine.scan_torrent_folders().await.unwrap(), 0);
    assert_eq!(engine.scan_torrent_folders().await.unwrap(), 1);
    let task = engine.list().await.unwrap().pop().unwrap();
    assert_eq!(task.status, Status::AwaitingApproval);
    assert_eq!(engine.scan_torrent_folders().await.unwrap(), 0);
    tokio::fs::write(watch.join("Duplicate.torrent"), &bytes)
        .await
        .unwrap();
    engine.scan_torrent_folders().await.unwrap();
    assert_eq!(engine.scan_torrent_folders().await.unwrap(), 0);
    engine.remove(&task.id).await.unwrap();
    engine.shutdown().await.unwrap();
    drop(engine);
    let engine = Engine::open(&data, &downloads).await.unwrap();
    engine.scan_torrent_folders().await.unwrap();
    assert_eq!(engine.scan_torrent_folders().await.unwrap(), 0);
    assert!(engine.list().await.unwrap().is_empty());
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn partial_and_invalid_files_do_not_block_valid_torrents_or_trigger_early_import() {
    let root = tempfile::tempdir().unwrap();
    let bytes = fixture(root.path()).await;
    let watch = root.path().join("watch");
    tokio::fs::create_dir(&watch).await.unwrap();
    let engine = Engine::open(root.path().join("data"), root.path().join("downloads"))
        .await
        .unwrap();
    let mut settings = engine.settings().await.unwrap();
    settings.queue_enabled = false;
    settings.torrent_watch = TorrentWatch {
        enabled: true,
        folders: vec![watch.clone()],
        mode: WatchMode::Automatic,
        destination: Some(root.path().join("torrent-downloads")),
    };
    engine.save_settings(settings).await.unwrap();
    tokio::fs::write(watch.join("Broken.torrent"), b"not torrent data")
        .await
        .unwrap();
    let partial = watch.join("Partial.torrent");
    tokio::fs::write(&partial, &bytes[..bytes.len() / 2])
        .await
        .unwrap();
    assert_eq!(engine.scan_torrent_folders().await.unwrap(), 0);
    tokio::time::sleep(Duration::from_millis(10)).await;
    tokio::fs::write(&partial, &bytes).await.unwrap();
    assert_eq!(engine.scan_torrent_folders().await.unwrap(), 0);
    assert_eq!(engine.scan_torrent_folders().await.unwrap(), 1);
    let tasks = engine.list().await.unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, Status::Queued);
    assert!(tasks[0].destination.starts_with(
        root.path()
            .join("torrent-downloads")
            .canonicalize()
            .unwrap()
    ));
    let mut settings = engine.settings().await.unwrap();
    settings.torrent_watch.enabled = false;
    engine.save_settings(settings).await.unwrap();
    tokio::fs::write(
        watch.join("WhileDisabled.torrent"),
        fixture(root.path()).await,
    )
    .await
    .unwrap();
    assert_eq!(engine.scan_torrent_folders().await.unwrap(), 0);
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn missing_folder_recovers_when_it_becomes_available_and_settings_are_backward_compatible() {
    let root = tempfile::tempdir().unwrap();
    let watch = root.path().join("later");
    let engine = Engine::open(root.path().join("data"), root.path().join("downloads"))
        .await
        .unwrap();
    let mut settings: flux_core::Settings = serde_json::from_str("{\"connections\":8}").unwrap();
    assert!(!settings.torrent_watch.enabled);
    settings.torrent_watch = TorrentWatch {
        enabled: true,
        folders: vec![watch.clone()],
        ..Default::default()
    };
    engine.save_settings(settings).await.unwrap();
    assert_eq!(engine.scan_torrent_folders().await.unwrap(), 0);
    assert!(engine.torrent_watch_report().await.error.is_some());
    tokio::fs::create_dir(&watch).await.unwrap();
    tokio::fs::write(
        watch.join("NowAvailable.torrent"),
        fixture(root.path()).await,
    )
    .await
    .unwrap();
    engine.scan_torrent_folders().await.unwrap();
    assert_eq!(engine.scan_torrent_folders().await.unwrap(), 1);
    assert!(engine.torrent_watch_report().await.error.is_none());
    engine.shutdown().await.unwrap();
}

#[test]
fn watch_settings_reject_relative_duplicate_and_excessive_folders() {
    let mut settings = TorrentWatch {
        enabled: true,
        ..Default::default()
    };
    assert!(settings.validate().is_err());
    settings.folders.push("relative".into());
    assert!(settings.validate().is_err());
    let root = tempfile::tempdir().unwrap();
    settings.folders = vec![root.path().into(), root.path().into()];
    assert!(settings.validate().is_err());
    settings.folders = (0..17).map(|n| root.path().join(n.to_string())).collect();
    assert!(settings.validate().is_err());
}

#[tokio::test]
async fn failed_destination_does_not_leak_imported_metadata_or_hide_its_error() {
    let root = tempfile::tempdir().unwrap();
    let watch = root.path().join("watch");
    tokio::fs::create_dir(&watch).await.unwrap();
    tokio::fs::write(watch.join("example.torrent"), fixture(root.path()).await)
        .await
        .unwrap();
    let destination = root.path().join("blocked");
    tokio::fs::write(&destination, b"not a directory")
        .await
        .unwrap();
    let data = root.path().join("data");
    let engine = Engine::open(&data, root.path().join("downloads"))
        .await
        .unwrap();
    let mut settings = engine.settings().await.unwrap();
    settings.torrent_watch = TorrentWatch {
        enabled: true,
        folders: vec![watch],
        destination: Some(destination),
        ..Default::default()
    };
    engine.save_settings(settings).await.unwrap();
    engine.scan_torrent_folders().await.unwrap();
    assert_eq!(engine.scan_torrent_folders().await.unwrap(), 0);
    assert!(engine.torrent_watch_report().await.error.is_some());
    assert_eq!(engine.scan_torrent_folders().await.unwrap(), 0);
    assert!(engine.torrent_watch_report().await.error.is_some());
    assert!(engine.list().await.unwrap().is_empty());
    assert_eq!(std::fs::read_dir(data.join("torrents")).unwrap().count(), 0);
    engine.shutdown().await.unwrap();
}
