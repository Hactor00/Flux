use crate::{quit, Backend};
use flux_core::{Engine, MediaInfo, Settings, Status, Task};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use tokio_util::sync::CancellationToken;

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
#[tauri::command]
pub fn runtime_platform() -> &'static str {
    std::env::consts::OS
}
#[tauri::command]
pub fn open_release_page(app: tauri::AppHandle) -> Result<(), String> {
    app.opener()
        .open_url(
            "https://github.com/Hactor00/Flux/releases/latest",
            None::<&str>,
        )
        .map_err(error)
}
#[tauri::command]
pub async fn torrent_watch_report(
    state: State<'_, Backend>,
) -> Result<flux_core::WatchReport, String> {
    Ok(state.engine.torrent_watch_report().await)
}
#[tauri::command]
pub async fn prepare_update(state: State<'_, Backend>) -> Result<(), String> {
    state.engine.prepare_update().await.map_err(error)
}
#[tauri::command]
pub async fn abort_update(state: State<'_, Backend>) -> Result<(), String> {
    state.engine.abort_update().await.map_err(error)
}
#[tauri::command]
pub async fn host_accounts(
    state: State<'_, Backend>,
) -> Result<Vec<flux_core::debrid::AccountStatus>, String> {
    state.engine.host_accounts().await.map_err(error)
}
#[tauri::command]
pub async fn save_host_account(
    provider: flux_core::debrid::Provider,
    enabled: bool,
    key: Option<String>,
    state: State<'_, Backend>,
) -> Result<Vec<flux_core::debrid::AccountStatus>, String> {
    state
        .engine
        .save_host_account(provider, enabled, key)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn import_torrent(
    path: std::path::PathBuf,
    filename: Option<String>,
    directory: Option<std::path::PathBuf>,
    start: bool,
    options: Option<flux_core::TorrentOptions>,
    transfer: Option<TorrentTransferOptions>,
    state: State<'_, Backend>,
) -> Result<Task, String> {
    let transfer = transfer.unwrap_or_default();
    let input = flux_core::DownloadRequest {
        filename: filename.unwrap_or_else(|| {
            flux_core::preferences::safe_filename(
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("Torrent"),
            )
        }),
        torrent: Some(options.unwrap_or_default()),
        speed_limit: transfer.speed_limit,
        priority: transfer.priority,
        start_at: transfer.start_at,
        stop_at: transfer.stop_at,
        expected_sha256: transfer.expected_sha256,
        auto_rename: true,
        ..Default::default()
    };
    state
        .engine
        .import_torrent_request(path, input, directory, start)
        .await
        .map_err(error)
}
#[derive(Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TorrentTransferOptions {
    speed_limit: u64,
    priority: i8,
    start_at: Option<u64>,
    stop_at: Option<u64>,
    expected_sha256: Option<String>,
}
#[tauri::command]
pub async fn set_torrent_options(
    id: String,
    options: flux_core::TorrentOptions,
    speed_limit: Option<u64>,
    state: State<'_, Backend>,
) -> Result<Task, String> {
    state
        .engine
        .set_torrent_options(&id, options, speed_limit)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn inspect_collection(
    url: String,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<flux_core::extractor::ResolvedCollection, String> {
    flux_core::extractor::resolve_collection(&url, offset.unwrap_or(0), limit.unwrap_or(50))
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn add_batch(
    inputs: Vec<flux_core::DownloadRequest>,
    directory: Option<std::path::PathBuf>,
    start: Option<bool>,
    state: State<'_, Backend>,
) -> Result<Vec<Task>, String> {
    state
        .engine
        .enqueue_batch_to(
            inputs,
            directory.unwrap_or_else(|| state.engine.default_download_directory()),
            start.unwrap_or(true),
        )
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn get_settings(state: State<'_, Backend>) -> Result<Settings, String> {
    let mut settings = state.engine.settings().await.map_err(error)?;
    if settings.download_directory.is_none() {
        settings.download_directory = Some(state.engine.default_download_directory());
    }
    settings.launch_at_startup = crate::startup::enabled()?;
    Ok(settings)
}
#[tauri::command]
pub async fn suggest_directory(
    filename: String,
    content_type: Option<String>,
    state: State<'_, Backend>,
) -> Result<String, String> {
    state
        .engine
        .routed_directory(
            &flux_core::DownloadRequest {
                filename,
                content_type,
                ..Default::default()
            },
            None,
        )
        .await
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(error)
}
#[tauri::command]
pub async fn prepare_download_prefix(
    prefix: String,
    filename: String,
    directory: Option<std::path::PathBuf>,
    state: State<'_, Backend>,
) -> Result<flux_core::filename_prefix::PrefixedDestination, String> {
    let base = state
        .engine
        .routed_directory(
            &flux_core::DownloadRequest {
                filename: filename.clone(),
                ..Default::default()
            },
            directory,
        )
        .await
        .map_err(error)?;
    flux_core::filename_prefix::prepare(&prefix, &filename, &base)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn save_settings(
    input: Settings,
    app: tauri::AppHandle,
    state: State<'_, Backend>,
) -> Result<Settings, String> {
    input.validate().map_err(error)?;
    let previous_startup = crate::startup::snapshot()?;
    crate::startup::set(input.launch_at_startup)?;
    let settings = match state.engine.save_settings(input).await {
        Ok(settings) => settings,
        Err(failure) => {
            if let Err(rollback) = crate::startup::restore(previous_startup) {
                return Err(format!(
                    "{}; startup rollback failed: {rollback}",
                    error(failure)
                ));
            }
            return Err(error(failure));
        }
    };
    state
        .tray
        .store(settings.minimize_to_tray, Ordering::SeqCst);
    *state.locale.lock().map_err(error)? = settings.locale.clone();
    let words = crate::localization::words(&settings.locale);
    let menu = app.state::<LocalizedMenu>();
    menu.open
        .set_text(words["downloads"].as_str().unwrap_or("Flux"))
        .map_err(error)?;
    menu.quit
        .set_text(words["close"].as_str().unwrap_or("Flux"))
        .map_err(error)?;
    for (label, window) in app.webview_windows() {
        let key = if label.starts_with("download-") {
            Some("new_download")
        } else if label.starts_with("details-") {
            Some("details")
        } else if label.starts_with("settings-") {
            Some("settings")
        } else if label.starts_with("power-") {
            Some("completion_action")
        } else {
            None
        };
        if let Some(key) = key {
            window
                .set_title(&format!("Flux · {}", words[key].as_str().unwrap_or("Flux")))
                .map_err(error)?;
        }
    }
    app.emit("settings-changed", settings.clone())
        .map_err(error)?;
    Ok(settings)
}
#[tauri::command]
pub async fn pause_all(state: State<'_, Backend>) -> Result<(), String> {
    state.engine.pause_all().await.map_err(error)
}
#[tauri::command]
pub async fn start_queue(state: State<'_, Backend>) -> Result<(), String> {
    let mut settings = state.engine.settings().await.map_err(error)?;
    settings.queue_enabled = true;
    state.engine.save_settings(settings).await.map_err(error)?;
    for task in state.engine.list().await.map_err(error)? {
        if task.status == Status::Paused {
            state.engine.queue(&task.id).await.map_err(error)?;
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn remove_task(id: String, state: State<'_, Backend>) -> Result<(), String> {
    state.engine.remove(&id).await.map_err(error)
}
#[tauri::command]
pub async fn set_priority(
    id: String,
    priority: i8,
    state: State<'_, Backend>,
) -> Result<Task, String> {
    state
        .engine
        .set_priority(&id, priority)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn expand_batch(input: String) -> Result<Vec<String>, String> {
    flux_core::expand_batch(&input).await.map_err(error)
}
#[tauri::command]
pub async fn inspect_media(
    url: String,
    referer: Option<String>,
    task_id: Option<String>,
    state: State<'_, Backend>,
) -> Result<MediaInfo, String> {
    if let Some(id) = task_id {
        return state.engine.inspect_task_media(&id).await.map_err(error);
    }
    state
        .engine
        .inspect_media(url, referer, None)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn open_task(
    id: String,
    reveal: bool,
    app: tauri::AppHandle,
    state: State<'_, Backend>,
) -> Result<(), String> {
    let task = state.engine.task(&id).await.map_err(error)?;
    if task.status != Status::Complete {
        return Err("File is not complete".into());
    }
    let path = tokio::fs::canonicalize(&task.destination)
        .await
        .map_err(error)?;
    if reveal {
        app.opener().reveal_item_in_dir(path).map_err(error)
    } else {
        app.opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(error)
    }
}
#[tauri::command]
pub async fn resolve_media(
    url: String,
    task_id: Option<String>,
    state: State<'_, Backend>,
) -> Result<flux_core::extractor::ResolvedMedia, String> {
    if let Some(id) = task_id {
        let task = state.engine.task(&id).await.map_err(error)?;
        return flux_core::extractor::resolve_with_session(&url, &task.request.credentials)
            .await
            .map_err(error);
    }
    flux_core::extractor::resolve(&url).await.map_err(error)
}
#[tauri::command]
pub async fn register_browser(
    app: tauri::AppHandle,
    state: State<'_, crate::integration::BrowserIntegration>,
) -> Result<String, String> {
    let status = state.status(true).await?;
    let _ = app.emit("browser-integration-changed", &status);
    status
        .extension_id
        .ok_or_else(|| status.error.unwrap_or_else(|| "Registration failed".into()))
}
#[tauri::command]
pub async fn browser_integration(
    state: State<'_, crate::integration::BrowserIntegration>,
) -> Result<crate::integration::IntegrationStatus, String> {
    state.status(false).await
}
#[tauri::command]
pub async fn dismiss_browser_reminder(
    app: tauri::AppHandle,
    state: State<'_, crate::integration::BrowserIntegration>,
) -> Result<(), String> {
    state.dismiss().await?;
    let status = state.status(false).await?;
    let _ = app.emit("browser-integration-changed", status);
    Ok(())
}
struct LocalizedMenu {
    open: tauri::menu::MenuItem<tauri::Wry>,
    quit: tauri::menu::MenuItem<tauri::Wry>,
}

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    app.manage(crate::power::PowerState::default());
    let data = flux_core::default_data_directory()?;
    let integration = crate::integration::BrowserIntegration::new(data.clone());
    app.manage(integration.clone());
    if !std::env::args().any(|argument| argument == "--shutdown-request") {
        tauri::async_runtime::spawn(async move {
            let _ = integration.status(false).await;
        });
    }
    let engine_data = data.clone();
    #[cfg(target_os = "linux")]
    let category_root = app
        .path()
        .download_dir()
        .or_else(|_| app.path().home_dir().map(|home| home.join("Downloads")))?;
    #[cfg(not(target_os = "linux"))]
    let category_root = app.path().download_dir()?;
    let downloads = category_root.join("Flux");
    let engine = tauri::async_runtime::block_on(tauri::async_runtime::spawn(async move {
        Engine::open_with_category_root(engine_data, downloads, category_root).await
    }))??;
    let settings = tauri::async_runtime::block_on(engine.settings())?;
    let cancel = CancellationToken::new();
    let scheduler_engine = engine.clone();
    let scheduler_cancel = cancel.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = scheduler_engine.run_scheduler(scheduler_cancel).await {
            eprintln!("Scheduler failed: {error:#}");
        }
    });
    let broker_engine = engine.clone();
    let broker_cancel = cancel.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = flux_core::protocol::serve(broker_engine, &data, broker_cancel).await {
            eprintln!("Broker failed: {error:#}");
        }
    });
    let mut events = engine.subscribe();
    let handle = app.handle().clone();
    let event_cancel = cancel.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::select! {
                _ = event_cancel.cancelled() => break,
                event = events.recv() => match event {
                    Ok(progress) => { let _ = handle.emit("download-progress", progress); },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break
                }
            }
        }
    });
    let background_engine = engine.clone();
    let handle = app.handle().clone();
    let background_cancel = cancel.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        interval.tick().await;
        let mut previous_clipboard = String::new();
        let mut shown_proposals = std::collections::HashSet::new();
        let mut completed: std::collections::HashSet<String> = background_engine
            .list()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|task| task.status == Status::Complete)
            .map(|task| task.id)
            .collect();
        loop {
            tokio::select! { _ = background_cancel.cancelled() => break, _ = interval.tick() => {} }
            let settings = match background_engine.settings().await {
                Ok(settings) => settings,
                Err(_) => continue,
            };
            if settings.clipboard_monitor {
                if let Ok(text) = handle.clipboard().read_text() {
                    let text = text.trim();
                    if text != previous_clipboard
                        && text.len() <= 16384
                        && (text.starts_with("https://") || text.starts_with("http://"))
                    {
                        previous_clipboard = text.into();
                        if settings.clipboard_mode == flux_core::ClipboardMode::SilentQueue {
                            let engine = background_engine.clone();
                            let handle = handle.clone();
                            let source = text.to_owned();
                            tauri::async_runtime::spawn(async move {
                                if let Err(error) = engine.enqueue_clipboard(&source).await {
                                    let _ = handle.emit("background-error", error.to_string());
                                }
                            });
                        } else {
                            let handle = handle.clone();
                            let source = text.to_owned();
                            let _ = tauri::async_runtime::spawn_blocking(move || {
                                crate::windows::show(&handle, "download", None, Some(&source))
                            })
                            .await;
                        }
                    }
                }
            }
            let tasks = background_engine.list().await.unwrap_or_default();
            if let Err(error) = crate::power::poll(&handle, &tasks).await {
                let _ = handle.emit("background-error", error);
            }
            for task in tasks {
                if task.status == Status::AwaitingApproval
                    && !shown_proposals.contains(&task.id)
                    && crate::windows::show(&handle, "download", Some(&task.id), None).is_ok()
                {
                    shown_proposals.insert(task.id.clone());
                }
                if task.status == Status::Complete
                    && completed.insert(task.id)
                    && settings.notifications
                {
                    let _ = handle
                        .notification()
                        .builder()
                        .title("Flux")
                        .body(task.request.filename)
                        .show();
                }
            }
        }
    });
    app.manage(Backend {
        engine,
        cancel,
        closing: AtomicBool::new(false),
        tray: AtomicBool::new(settings.minimize_to_tray),
        locale: std::sync::Mutex::new(settings.locale.clone()),
    });
    use tauri::{
        menu::{Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };
    let words = crate::localization::words(&settings.locale);
    let open = MenuItem::with_id(
        app,
        "open",
        words["downloads"].as_str().unwrap_or("Flux"),
        true,
        None::<&str>,
    )?;
    let exit = MenuItem::with_id(
        app,
        "quit",
        words["close"].as_str().unwrap_or("Flux"),
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(app, &[&open, &exit])?;
    app.manage(LocalizedMenu { open, quit: exit });
    TrayIconBuilder::new()
        .icon(
            app.default_window_icon()
                .ok_or("Missing application icon")?
                .clone(),
        )
        .tooltip("Flux")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;
    if !std::env::args().any(|arg| arg == "--background" || arg == "--shutdown-request") {
        if let Some(window) = app.get_webview_window("main") {
            window.show()?;
        }
    }
    Ok(())
}
