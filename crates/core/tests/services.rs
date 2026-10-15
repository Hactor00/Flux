use flux_core::{debrid::Provider, DownloadRequest, Engine, Status};

#[tokio::test]
async fn completed_browser_torrent_accepts_seeding_rules_without_changing_files() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let engine = Engine::open(&data, root.path().join("downloads"))
        .await
        .unwrap();
    let mut task = engine
        .enqueue(DownloadRequest {
            url: "https://source.test/fixture.torrent".into(),
            filename: "BrowserTorrent".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    task.status = Status::Complete;
    task.completed = 1;
    task.total = Some(1);
    task.torrent = Some(flux_core::TorrentSnapshot {
        info_hash: "a".repeat(40),
        state: "complete".into(),
        peers: 0,
        uploaded: 0,
        upload_bps: 0,
        seeded_seconds: 0,
        file_count: 1,
        files: vec![flux_core::TorrentFile {
            index: 0,
            path: "fixture.bin".into(),
            length: 1,
            selected: true,
            sha256: None,
        }],
    });
    let db = rusqlite::Connection::open(data.join("tasks.sqlite")).unwrap();
    db.execute(
        "UPDATE tasks SET body=?1 WHERE id=?2",
        rusqlite::params![serde_json::to_string(&task).unwrap(), task.id],
    )
    .unwrap();
    drop(db);
    let options = flux_core::TorrentOptions {
        seeding: Some(flux_core::seeding::Rule::Infinite),
        ..Default::default()
    };
    let updated = engine
        .set_torrent_options(&task.id, options.clone(), None)
        .await
        .unwrap();
    assert_eq!(updated.status, Status::Complete);
    assert_eq!(updated.request.torrent.unwrap().seeding, options.seeding);
    assert!(engine
        .set_torrent_options(
            &task.id,
            flux_core::TorrentOptions {
                only_files: vec![0],
                ..options
            },
            None
        )
        .await
        .is_err());
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn service_keys_are_encrypted_and_never_exposed_by_public_status() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let engine = Engine::open(&data, root.path().join("downloads"))
        .await
        .unwrap();
    let key = "contract-key-private-9347";
    let accounts = engine
        .save_host_account(Provider::RealDebrid, true, Some(key.into()))
        .await
        .unwrap();
    assert!(accounts
        .iter()
        .any(|account| account.provider == Provider::RealDebrid
            && account.enabled
            && account.configured));
    assert!(!serde_json::to_string(&accounts).unwrap().contains(key));
    let database = rusqlite::Connection::open(data.join("tasks.sqlite")).unwrap();
    let stored: Vec<u8> = database
        .query_row(
            "SELECT secret FROM host_accounts WHERE provider='real_debrid'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!stored
        .windows(key.len())
        .any(|bytes| bytes == key.as_bytes()));
    engine
        .save_host_account(Provider::RealDebrid, false, None)
        .await
        .unwrap();
    assert!(engine
        .host_accounts()
        .await
        .unwrap()
        .iter()
        .any(|account| account.provider == Provider::RealDebrid
            && !account.enabled
            && account.configured));
    assert!(engine
        .save_host_account(Provider::AllDebrid, true, Some("invalid\nheader".into()))
        .await
        .is_err());
    engine
        .save_host_account(Provider::RealDebrid, false, Some(String::new()))
        .await
        .unwrap();
    assert!(engine
        .host_accounts()
        .await
        .unwrap()
        .iter()
        .all(|account| !account.configured));
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn update_checkpoint_restores_queue_after_restart_and_abort() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let downloads = root.path().join("downloads");
    let engine = Engine::open(&data, &downloads).await.unwrap();
    let task = engine
        .enqueue(DownloadRequest {
            url: "https://example.invalid/file.bin".into(),
            filename: "file.bin".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    engine.prepare_update().await.unwrap();
    assert_eq!(engine.task(&task.id).await.unwrap().status, Status::Paused);
    assert!(!engine.settings().await.unwrap().queue_enabled);
    assert!(engine.start(&task.id).await.is_err());
    assert!(engine.prepare_update().await.is_err());
    engine.shutdown().await.unwrap();
    drop(engine);
    let engine = Engine::open(&data, &downloads).await.unwrap();
    assert_eq!(engine.task(&task.id).await.unwrap().status, Status::Queued);
    assert!(engine.settings().await.unwrap().queue_enabled);
    engine.prepare_update().await.unwrap();
    engine.abort_update().await.unwrap();
    assert_eq!(engine.task(&task.id).await.unwrap().status, Status::Queued);
    assert!(engine.settings().await.unwrap().queue_enabled);
    engine.shutdown().await.unwrap();
}
