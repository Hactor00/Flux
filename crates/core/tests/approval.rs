use flux_core::{DownloadRequest, Engine, Status};
#[tokio::test]
async fn unapproved_capture_survives_restart_and_cannot_be_started() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let output = root.path().join("out");
    let id = uuid::Uuid::new_v4().to_string();
    let input = DownloadRequest {
        url: "https://example.invalid/captured".into(),
        filename: "captured.bin".into(),
        ..DownloadRequest::default()
    };
    let engine = Engine::open(&data, &output).await.unwrap();
    engine.propose_id(id.clone(), input.clone()).await.unwrap();
    assert_eq!(
        engine.task(&id).await.unwrap().status,
        Status::AwaitingApproval
    );
    assert!(engine.queue(&id).await.is_err());
    assert!(engine.start(&id).await.is_err());
    engine.shutdown().await.unwrap();
    drop(engine);
    let engine = Engine::open(&data, &output).await.unwrap();
    assert_eq!(
        engine.task(&id).await.unwrap().status,
        Status::AwaitingApproval
    );
    let mut edited = input.clone();
    edited.filename = "chosen.bin".into();
    let chosen = root.path().join("chosen");
    let task = engine
        .approve(&id, edited, chosen.clone(), false)
        .await
        .unwrap();
    assert_eq!(task.status, Status::Paused);
    assert_eq!(task.destination.file_name().unwrap(), "chosen.bin");
    assert_eq!(
        task.destination.parent().unwrap(),
        std::fs::canonicalize(chosen).unwrap()
    );
    let replay = engine.propose_id(id.clone(), input).await.unwrap();
    assert_eq!(replay.status, Status::Paused);
    assert_eq!(engine.list().await.unwrap().len(), 1);
    assert!(engine
        .approve(&id, task.request, output, true)
        .await
        .is_err());
}
#[tokio::test]
async fn rejected_capture_cannot_resume_or_change_source() {
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::open(root.path().join("data"), root.path().join("out"))
        .await
        .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let input = DownloadRequest {
        url: "https://example.invalid/a".into(),
        filename: "a.bin".into(),
        ..DownloadRequest::default()
    };
    engine.propose_id(id.clone(), input.clone()).await.unwrap();
    let mut changed = input;
    changed.url = "https://example.invalid/b".into();
    assert!(engine
        .approve(&id, changed, root.path().into(), true)
        .await
        .is_err());
    assert_eq!(engine.reject(&id).await.unwrap().status, Status::Cancelled);
    assert!(engine.queue(&id).await.is_err());
    assert!(engine.start(&id).await.is_err());
    let replacement = engine
        .enqueue(DownloadRequest {
            url: "https://example.invalid/a".into(),
            filename: "a.bin".into(),
            ..DownloadRequest::default()
        })
        .await
        .unwrap();
    assert_eq!(replacement.destination.file_name().unwrap(), "a.bin");
}
#[tokio::test]
async fn batch_uses_the_chosen_folder_and_never_starts_later_tasks() {
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::open(root.path().join("data"), root.path().join("out"))
        .await
        .unwrap();
    let folder = root.path().join("chosen");
    let requests = ["one.zip", "two.zip"]
        .into_iter()
        .map(|filename| DownloadRequest {
            url: "https://example.invalid/archive".into(),
            filename: filename.into(),
            ..DownloadRequest::default()
        })
        .collect();
    let tasks = engine
        .enqueue_batch_to(requests, folder.clone(), false)
        .await
        .unwrap();
    assert_eq!(tasks.len(), 2);
    for task in tasks {
        assert_eq!(task.status, Status::Paused);
        assert_eq!(
            task.destination.parent().unwrap(),
            std::fs::canonicalize(&folder).unwrap()
        );
    }
}
