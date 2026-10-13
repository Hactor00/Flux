//! Test-only bridge. This module is absent from ordinary CLI and desktop releases.
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use flux_core::Engine;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
#[derive(Clone)]
struct Harness {
    engine: Engine,
    token: String,
    origin: String,
    progress: Arc<Mutex<HashMap<String, flux_core::Progress>>>,
}
async fn handle(
    State(state): State<Harness>,
    Path(command): Path<String>,
    headers: HeaderMap,
    Json(args): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let token = headers
        .get("x-flux-test-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if token.len() != state.token.len()
        || token
            .bytes()
            .zip(state.token.bytes())
            .fold(0u8, |sum, (a, b)| sum | (a ^ b))
            != 0
    {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Test authentication failed".into(),
        ));
    }
    if headers
        .get("origin")
        .is_some_and(|value| value != state.origin.as_str())
    {
        return Err((StatusCode::FORBIDDEN, "Unexpected test origin".into()));
    }
    let result = async {
        let id = || {
            args["id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing task identifier"))
        };
        let value = match command.as_str() {
            "list_tasks" => serde_json::to_value(state.engine.list().await?)?,
            "get_settings" => {
                let mut settings = state.engine.settings().await?;
                if settings.download_directory.is_none() {
                    settings.download_directory = Some(state.engine.default_download_directory());
                }
                serde_json::to_value(settings)?
            }
            "get_task" => serde_json::to_value(state.engine.task(id()?).await?)?,
            "resolve_media" => serde_json::to_value(
                flux_core::extractor::resolve(
                    args["url"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Missing URL"))?,
                )
                .await?,
            )?,
            "add_download_to" => serde_json::to_value(
                state
                    .engine
                    .enqueue_to(
                        serde_json::from_value(args["input"].clone())?,
                        serde_json::from_value(args["directory"].clone())?,
                        args["start"].as_bool().unwrap_or(true),
                    )
                    .await?,
            )?,
            "propose_download" => serde_json::to_value(
                state
                    .engine
                    .propose_id(
                        uuid::Uuid::new_v4().to_string(),
                        serde_json::from_value(args["input"].clone())?,
                    )
                    .await?,
            )?,
            "approve_download" => serde_json::to_value(
                state
                    .engine
                    .approve(
                        id()?,
                        serde_json::from_value(args["input"].clone())?,
                        serde_json::from_value(args["directory"].clone())?,
                        args["start"].as_bool().unwrap_or(true),
                    )
                    .await?,
            )?,
            "reject_download" => serde_json::to_value(state.engine.reject(id()?).await?)?,
            "save_settings" => serde_json::to_value(
                state
                    .engine
                    .save_settings(serde_json::from_value(args["input"].clone())?)
                    .await?,
            )?,
            "add_download" => serde_json::to_value(
                state
                    .engine
                    .enqueue(serde_json::from_value(args["input"].clone())?)
                    .await?,
            )?,
            "add_batch" => serde_json::to_value(
                state
                    .engine
                    .enqueue_batch(serde_json::from_value(args["inputs"].clone())?)
                    .await?,
            )?,
            "pause_download" => serde_json::to_value(state.engine.pause(id()?).await?)?,
            "resume_download" => serde_json::to_value(state.engine.queue(id()?).await?)?,
            "remove_task" => {
                state.engine.remove(id()?).await?;
                serde_json::Value::Null
            }
            "set_priority" => serde_json::to_value(
                state
                    .engine
                    .set_priority(id()?, serde_json::from_value(args["priority"].clone())?)
                    .await?,
            )?,
            "pause_all" => {
                state.engine.pause_all().await?;
                serde_json::Value::Null
            }
            "start_queue" => {
                let mut settings = state.engine.settings().await?;
                settings.queue_enabled = true;
                state.engine.save_settings(settings).await?;
                for task in state.engine.list().await? {
                    if task.status == flux_core::Status::Paused {
                        state.engine.queue(&task.id).await?;
                    }
                }
                serde_json::Value::Null
            }
            "expand_batch" => serde_json::to_value(
                flux_core::expand_batch(
                    args["input"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Missing batch input"))?,
                )
                .await?,
            )?,
            "inspect_media" if args["taskId"].is_string() => serde_json::to_value(
                state
                    .engine
                    .inspect_task_media(args["taskId"].as_str().unwrap_or_default())
                    .await?,
            )?,
            "inspect_media" => serde_json::to_value(
                state
                    .engine
                    .inspect_media(
                        serde_json::from_value(args["url"].clone())?,
                        serde_json::from_value(args["referer"].clone())?,
                        None,
                    )
                    .await?,
            )?,
            "progress" => serde_json::to_value(
                state
                    .progress
                    .lock()
                    .await
                    .values()
                    .cloned()
                    .collect::<Vec<_>>(),
            )?,
            _ => anyhow::bail!("Unsupported test command"),
        };
        anyhow::Ok(value)
    }
    .await;
    result
        .map(Json)
        .map_err(|error| (StatusCode::BAD_REQUEST, format!("{error:#}")))
}
pub async fn run(
    engine: Engine,
    port: u16,
    origin: String,
    shutdown_file: PathBuf,
) -> anyhow::Result<()> {
    let token = std::env::var("FLUX_TEST_TOKEN")?;
    anyhow::ensure!(
        token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "A 256-bit test token is required"
    );
    let cancel = CancellationToken::new();
    let progress = Arc::new(Mutex::new(HashMap::new()));
    let mut events = engine.subscribe();
    let captured = progress.clone();
    let event_stop = cancel.clone();
    let recorder = tokio::spawn(async move {
        loop {
            tokio::select! {_=event_stop.cancelled()=>break,event=events.recv()=>{if let Ok(event)=event{captured.lock().await.insert(event.id.clone(),event);}}}
        }
    });
    let scheduler = tokio::spawn({
        let engine = engine.clone();
        let cancel = cancel.clone();
        async move { engine.run_scheduler(cancel).await }
    });
    let stopper = tokio::spawn({
        let cancel = cancel.clone();
        async move {
            loop {
                tokio::select! {_=cancel.cancelled()=>break,_=tokio::time::sleep(std::time::Duration::from_millis(100))=>{if tokio::fs::try_exists(&shutdown_file).await.unwrap_or(false){cancel.cancel();break;}}}
            }
        }
    });
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let router = Router::new()
        .route("/__flux_test/{command}", post(handle))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .with_state(Harness {
            engine: engine.clone(),
            token,
            origin,
            progress,
        });
    println!("Flux test bridge listening on 127.0.0.1:{port}");
    let result = axum::serve(listener, router)
        .with_graceful_shutdown(cancel.clone().cancelled_owned())
        .await;
    cancel.cancel();
    engine.shutdown().await?;
    scheduler.await??;
    recorder.await?;
    stopper.await?;
    result?;
    Ok(())
}
