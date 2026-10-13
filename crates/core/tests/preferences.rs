use axum::{
    body::Body,
    http::{Response, StatusCode},
    routing::get,
    Router,
};
use flux_core::{DownloadRequest, Engine, Settings, ShutdownAction, Status};
fn input(name: &str) -> DownloadRequest {
    DownloadRequest {
        url: "https://example.test/file".into(),
        filename: name.into(),
        ..Default::default()
    }
}
fn assert_destination(actual: std::path::PathBuf, expected: std::path::PathBuf) {
    let expected = std::fs::canonicalize(expected.parent().unwrap())
        .unwrap()
        .join(expected.file_name().unwrap());
    assert_eq!(actual, expected);
}
#[tokio::test]
async fn categorization_is_opt_in_persistent_and_respects_manual_folders() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("output");
    let data = root.path().join("data");
    let engine = Engine::open(&data, &output).await.unwrap();
    assert_destination(
        engine
            .enqueue(input("before.zip"))
            .await
            .unwrap()
            .destination,
        output.join("before.zip"),
    );
    let mut settings = engine.settings().await.unwrap();
    settings.auto_categorize = true;
    settings.category_folders.video = Some(root.path().join("movies"));
    engine.save_settings(settings).await.unwrap();
    assert_destination(
        engine.enqueue(input("film.mp4")).await.unwrap().destination,
        root.path().join("movies/film.mp4"),
    );
    let music = engine
        .enqueue(DownloadRequest {
            content_type: Some("audio/flac".into()),
            ..input("track")
        })
        .await
        .unwrap();
    assert_destination(music.destination.clone(), output.join("Music/track"));
    let manual = root.path().join("manual");
    assert_destination(
        engine
            .enqueue_to(input("manual.mp4"), manual.clone(), false)
            .await
            .unwrap()
            .destination,
        manual.join("manual.mp4"),
    );
    let tasks = engine
        .enqueue_batch(vec![input("archive.7z"), input("book.epub")])
        .await
        .unwrap();
    assert_destination(
        tasks[0].destination.clone(),
        output.join("Compressed/archive.7z"),
    );
    assert_destination(
        tasks[1].destination.clone(),
        output.join("Documents/book.epub"),
    );
    engine.shutdown().await.unwrap();
    drop(engine);
    let reopened = Engine::open(&data, &output).await.unwrap();
    assert!(reopened.settings().await.unwrap().auto_categorize);
    assert_eq!(
        reopened.task(&music.id).await.unwrap().destination,
        music.destination
    );
}
#[tokio::test]
async fn response_mime_routes_an_unknown_name_without_overriding_explicit_destination() {
    let server = Router::new().route(
        "/file",
        get(|| async {
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/pdf")
                .body(Body::from("verified document"))
                .unwrap()
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/file", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        axum::serve(listener, server).await.unwrap();
    });
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("output");
    let engine = Engine::open(root.path().join("data"), &output)
        .await
        .unwrap();
    let mut settings = engine.settings().await.unwrap();
    settings.auto_categorize = true;
    settings.completion_action = ShutdownAction::ExitApp;
    engine.save_settings(settings).await.unwrap();
    let task = engine
        .enqueue(DownloadRequest {
            url: url.clone(),
            ..input("opaque")
        })
        .await
        .unwrap();
    engine.start(&task.id).await.unwrap();
    let done = engine.wait(&task.id).await.unwrap();
    assert_eq!(done.status, Status::Complete);
    assert_destination(done.destination, output.join("Documents/opaque"));
    assert_eq!(
        engine.poll_completion_action().await.unwrap(),
        Some(ShutdownAction::ExitApp)
    );
    assert_eq!(engine.poll_completion_action().await.unwrap(), None);
    let mut settings = engine.settings().await.unwrap();
    settings.completion_action = ShutdownAction::None;
    engine.save_settings(settings.clone()).await.unwrap();
    settings.completion_action = ShutdownAction::ExitApp;
    engine.save_settings(settings).await.unwrap();
    let manual = root.path().join("manual");
    let task = engine
        .enqueue_to(
            DownloadRequest {
                url,
                ..input("opaque-manual")
            },
            manual.clone(),
            true,
        )
        .await
        .unwrap();
    engine.start(&task.id).await.unwrap();
    assert_destination(
        engine.wait(&task.id).await.unwrap().destination,
        manual.join("opaque-manual"),
    );
    assert_eq!(
        engine.poll_completion_action().await.unwrap(),
        Some(ShutdownAction::ExitApp)
    );
    handle.abort();
}
#[test]
fn legacy_settings_keep_all_new_side_effects_disabled() {
    let settings: Settings = serde_json::from_str("{}").unwrap();
    assert!(!settings.auto_categorize && !settings.launch_at_startup);
    assert_eq!(settings.completion_action, ShutdownAction::None);
    assert_eq!(settings.clipboard_mode, flux_core::ClipboardMode::Approval);
}

#[tokio::test]
async fn clipboard_tasks_are_queued_and_persist_without_approval() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let output = root.path().join("output");
    let engine = Engine::open(&data, &output).await.unwrap();
    let mut settings = engine.settings().await.unwrap();
    settings.auto_categorize = true;
    settings.clipboard_mode = flux_core::ClipboardMode::SilentQueue;
    engine.save_settings(settings).await.unwrap();
    let task = engine
        .enqueue_clipboard("https://example.test/Some%20book.pdf")
        .await
        .unwrap();
    assert_eq!(task.status, Status::Queued);
    assert_eq!(task.request.filename, "Some book.pdf");
    assert_destination(
        task.destination.clone(),
        output.join("Documents/Some book.pdf"),
    );
    drop(engine);
    let reopened = Engine::open(&data, &output).await.unwrap();
    assert_eq!(
        reopened.task(&task.id).await.unwrap().status,
        Status::Queued
    );
    assert_eq!(
        reopened.settings().await.unwrap().clipboard_mode,
        flux_core::ClipboardMode::SilentQueue
    );
}
