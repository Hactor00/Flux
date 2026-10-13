use flux_core::protocol::{self, Command, Request, VERSION};
use tokio::io::AsyncWriteExt;

#[tokio::test]
async fn large_torrent_lists_use_compact_tasks_and_bounded_file_pages() {
    let root = tempfile::tempdir().unwrap();
    let engine = flux_core::Engine::open(root.path().join("data"), root.path().join("out"))
        .await
        .unwrap();
    let task = engine
        .enqueue(flux_core::DownloadRequest {
            url: "https://example.invalid/file".into(),
            filename: "snapshot.bin".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let files: Vec<_> = (0..2500)
        .map(|index| flux_core::TorrentFile {
            index,
            path: format!("directory/{index}-{}.bin", "x".repeat(100)),
            length: 16384,
            selected: true,
            sha256: None,
        })
        .collect();
    let torrent = flux_core::TorrentSnapshot {
        info_hash: "a".repeat(40),
        state: "paused".into(),
        peers: 0,
        uploaded: 0,
        upload_bps: 0,
        file_count: files.len(),
        files,
    };
    let database = rusqlite::Connection::open(root.path().join("data/tasks.sqlite")).unwrap();
    database
        .execute(
            "UPDATE tasks SET body=json_set(body,'$.torrent',json(?2)) WHERE id=?1",
            rusqlite::params![task.id, serde_json::to_string(&torrent).unwrap()],
        )
        .unwrap();
    drop(database);
    assert!(
        serde_json::to_vec(&engine.task(&task.id).await.unwrap())
            .unwrap()
            .len()
            > protocol::MAX_FRAME
    );
    let response = protocol::dispatch(
        &engine,
        Request {
            version: VERSION,
            request_id: uuid::Uuid::new_v4().to_string(),
            command: Command::Task {
                id: task.id.clone(),
            },
        },
    )
    .await;
    assert!(response.ok, "{:?}", response.error);
    assert!(serde_json::to_vec(&response).unwrap().len() < protocol::MAX_FRAME);
    let value = response.result.unwrap();
    assert_eq!(value["torrent"]["file_count"], 2500);
    assert!(value["torrent"]["files"].as_array().unwrap().is_empty());
    let mut offset = 0;
    while offset < 2500 {
        let response = protocol::dispatch(
            &engine,
            Request {
                version: VERSION,
                request_id: uuid::Uuid::new_v4().to_string(),
                command: Command::TorrentFiles {
                    id: task.id.clone(),
                    offset,
                    limit: 50,
                },
            },
        )
        .await;
        assert!(response.ok, "{:?}", response.error);
        assert!(serde_json::to_vec(&response).unwrap().len() < protocol::MAX_FRAME);
        let value = response.result.unwrap();
        let page = value["files"].as_array().unwrap();
        assert_eq!(page.len(), 50);
        for (position, file) in page.iter().enumerate() {
            assert_eq!(file["index"].as_u64().unwrap() as usize, offset + position);
        }
        offset = value["next_offset"].as_u64().unwrap() as usize;
    }
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn history_pages_stay_below_native_frame_limit_without_losing_tasks() {
    use flux_core::{DownloadRequest, Engine};
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::open(root.path().join("data"), root.path().join("out"))
        .await
        .unwrap();
    let inputs = (0..70)
        .map(|index| DownloadRequest {
            url: format!("https://example.invalid/{index}?{}", "a".repeat(12000)),
            filename: format!("{index}.bin"),
            ..DownloadRequest::default()
        })
        .collect();
    engine.enqueue_batch(inputs).await.unwrap();
    let mut offset = 0;
    let mut ids = std::collections::HashSet::new();
    loop {
        let response = protocol::dispatch(
            &engine,
            Request {
                version: VERSION,
                request_id: uuid::Uuid::new_v4().to_string(),
                command: Command::ListPage { offset, limit: 50 },
            },
        )
        .await;
        assert!(response.ok, "{:?}", response.error);
        assert!(serde_json::to_vec(&response).unwrap().len() < protocol::MAX_FRAME);
        let result = response.result.unwrap();
        for task in result["tasks"].as_array().unwrap() {
            assert!(ids.insert(task["id"].as_str().unwrap().to_owned()));
        }
        let next = result["next_offset"].as_u64().unwrap() as usize;
        assert!(next > offset);
        if !result["has_more"].as_bool().unwrap() {
            break;
        }
        offset = next;
    }
    assert_eq!(ids.len(), 70);
}
#[tokio::test]
async fn framing_roundtrip_and_oversize_rejection() {
    let (mut writer, mut reader) = tokio::io::duplex(4096);
    let input = Request {
        version: VERSION,
        request_id: uuid::Uuid::new_v4().to_string(),
        command: Command::List,
    };
    protocol::write_frame(&mut writer, &input).await.unwrap();
    let decoded: Request =
        serde_json::from_slice(&protocol::read_frame(&mut reader).await.unwrap().unwrap()).unwrap();
    assert_eq!(decoded.request_id, input.request_id);
    writer
        .write_all(&((protocol::MAX_FRAME + 1) as u32).to_ne_bytes())
        .await
        .unwrap();
    assert!(protocol::read_frame(&mut reader).await.is_err());
}
#[tokio::test]
async fn fragmented_prefix_and_truncated_body() {
    let (mut writer, mut reader) = tokio::io::duplex(4096);
    tokio::spawn(async move {
        for byte in 10u32.to_ne_bytes() {
            writer.write_all(&[byte]).await.unwrap();
            tokio::task::yield_now().await;
        }
        writer.write_all(b"short").await.unwrap();
    });
    assert!(protocol::read_frame(&mut reader).await.is_err());
}

#[tokio::test]
async fn broker_authentication_versioning_and_shutdown() {
    use flux_core::{
        protocol::{AuthenticatedRequest, Discovery, Response},
        Engine,
    };
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let engine = Engine::open(&data, root.path().join("output"))
        .await
        .unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    let mut server = tokio::spawn({
        let engine = engine.clone();
        let data = data.clone();
        let cancel = cancel.clone();
        async move { protocol::serve(engine, &data, cancel).await }
    });
    let discovery_future = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if let Ok(bytes) = tokio::fs::read(data.join("broker.json")).await {
                if let Ok(discovery) = serde_json::from_slice(&bytes) {
                    break discovery;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    });
    let discovery: Discovery = tokio::select! { result = discovery_future => result.unwrap(), result = &mut server => panic!("Broker stopped before discovery: {result:?}") };
    for (token, version, accepted) in [
        ("wrong".to_owned(), VERSION, false),
        (discovery.token.clone(), VERSION + 1, true),
        (discovery.token.clone(), VERSION, true),
    ] {
        let mut peer =
            tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, discovery.port))
                .await
                .unwrap();
        let request = Request {
            version,
            request_id: uuid::Uuid::new_v4().to_string(),
            command: Command::List,
        };
        protocol::write_frame(&mut peer, &AuthenticatedRequest { token, request })
            .await
            .unwrap();
        let frame = protocol::read_frame(&mut peer).await.unwrap();
        if accepted {
            let response: Response = serde_json::from_slice(&frame.unwrap()).unwrap();
            assert_eq!(response.ok, version == VERSION);
        } else {
            assert!(frame.is_none());
        }
    }
    cancel.cancel();
    server.await.unwrap().unwrap();
    assert!(!data.join("broker.json").exists());
    engine.shutdown().await.unwrap();
}
