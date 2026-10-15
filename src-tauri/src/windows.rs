use crate::Backend;
use flux_core::{DownloadRequest, Status, Task};
use std::path::PathBuf;
use tauri::{Manager, State};

pub fn show(
    app: &tauri::AppHandle,
    view: &str,
    id: Option<&str>,
    initial_url: Option<&str>,
) -> Result<(), String> {
    if !matches!(view, "download" | "settings" | "details" | "power") {
        return Err("Unknown window".into());
    }
    if let Some(id) = id {
        uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
    }
    let label = format!("{view}-{}", id.unwrap_or("new"));
    if let Some(window) = app.get_webview_window(&label) {
        window.unminimize().map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        return window.set_focus().map_err(|e| e.to_string());
    }
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair("view", view);
    if let Some(id) = id {
        query.append_pair("id", id);
    }
    if let Some(value) = initial_url {
        query.append_pair("url", value);
    }
    let locale = app
        .state::<Backend>()
        .locale
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let words = crate::localization::words(&locale);
    let key = if view == "download" {
        "new_download"
    } else if view == "settings" {
        "settings"
    } else if view == "power" {
        "completion_action"
    } else {
        "details"
    };
    let title = format!("Flux · {}", words[key].as_str().unwrap_or("Flux"));
    let window = tauri::WebviewWindowBuilder::new(
        app,
        &label,
        tauri::WebviewUrl::App(format!("index.html?{}", query.finish()).into()),
    )
    .title(title)
    .inner_size(
        if view == "power" { 480.0 } else { 680.0 },
        if view == "power" { 280.0 } else { 660.0 },
    )
    .min_inner_size(
        if view == "power" { 400.0 } else { 570.0 },
        if view == "power" { 220.0 } else { 440.0 },
    )
    .background_color(tauri::window::Color(20, 22, 28, 255))
    .focused(true)
    .center()
    .build()
    .map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
pub async fn open_surface(
    app: tauri::AppHandle,
    view: String,
    id: Option<String>,
    initial_url: Option<String>,
) -> Result<(), String> {
    // WebView2 creation cannot run inside the main-thread IPC callback on Windows.
    tauri::async_runtime::spawn_blocking(move || {
        show(&app, &view, id.as_deref(), initial_url.as_deref())
    })
    .await
    .map_err(|error| error.to_string())?
}
#[tauri::command]
pub async fn close_surface(window: tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        return Err("Main window cannot be closed through this command".into());
    }
    window.close().map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn get_task(id: String, state: State<'_, Backend>) -> Result<Task, String> {
    state.engine.task(&id).await.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn approve_download(
    id: String,
    input: DownloadRequest,
    directory: PathBuf,
    start: bool,
    state: State<'_, Backend>,
) -> Result<Task, String> {
    state
        .engine
        .approve(&id, input, directory, start)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn reject_download(id: String, state: State<'_, Backend>) -> Result<Task, String> {
    state.engine.reject(&id).await.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn add_download_to(
    input: DownloadRequest,
    directory: Option<PathBuf>,
    start: bool,
    state: State<'_, Backend>,
) -> Result<Task, String> {
    state
        .engine
        .enqueue_with_directory(input, directory, start)
        .await
        .map_err(|e| e.to_string())
}
pub fn child_close(window: &tauri::Window) {
    let app = window.app_handle().clone();
    let label = window.label().to_owned();
    tauri::async_runtime::spawn(async move {
        if label == "power-new" {
            let _ = crate::power::cancel_power(app.clone()).await;
            return;
        }
        if let Some(id) = label.strip_prefix("download-") {
            let state = app.state::<Backend>();
            if state
                .engine
                .task(id)
                .await
                .is_ok_and(|task| task.status == Status::AwaitingApproval)
            {
                if let Err(error) = state.engine.reject(id).await {
                    eprintln!("Cannot reject pending download: {error}");
                    return;
                }
            }
        }
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.destroy();
        }
    });
}
