#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod desktop;
mod file_drag;
mod install;
mod localization;
mod power;
mod shutdown;
mod startup;
mod windows;
use flux_core::{DownloadRequest, Engine, Task};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, State};
use tokio_util::sync::CancellationToken;

struct Backend {
    engine: Engine,
    cancel: CancellationToken,
    closing: AtomicBool,
    tray: AtomicBool,
    locale: std::sync::Mutex<String>,
}
#[tauri::command]
async fn list_tasks(state: State<'_, Backend>) -> Result<Vec<Task>, String> {
    state
        .engine
        .list()
        .await
        .map(|tasks| tasks.into_iter().map(Task::summary).collect())
        .map_err(|e| format!("{e:#}"))
}
#[tauri::command]
async fn add_download(input: DownloadRequest, state: State<'_, Backend>) -> Result<Task, String> {
    state
        .engine
        .enqueue(input)
        .await
        .map_err(|e| format!("{e:#}"))
}
#[tauri::command]
async fn pause_download(id: String, state: State<'_, Backend>) -> Result<Task, String> {
    state.engine.pause(&id).await.map_err(|e| format!("{e:#}"))
}
#[tauri::command]
async fn resume_download(id: String, state: State<'_, Backend>) -> Result<Task, String> {
    state.engine.queue(&id).await.map_err(|e| format!("{e:#}"))
}
fn quit(app: &tauri::AppHandle) {
    let state = app.state::<Backend>();
    if state.closing.swap(true, Ordering::SeqCst) {
        return;
    }
    state.cancel.cancel();
    let engine = state.engine.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match engine.shutdown().await {
            Ok(()) => app.exit(0),
            Err(error) => {
                eprintln!("Shutdown failed: {error:#}");
                app.exit(1);
            }
        }
    });
}
fn main() {
    if std::env::args().any(|argument| argument == "--shutdown") {
        if let Err(error) = shutdown::request() {
            eprintln!("Graceful shutdown failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    for argument in std::env::args() {
        let result = match argument.as_str() {
            "--register-browser" => Some(install::register().map(|_| ())),
            "--unregister-browser" => Some(install::unregister()),
            _ => None,
        };
        if let Some(result) = result {
            if let Err(error) = result {
                eprintln!("Browser bridge setup failed: {error}");
                std::process::exit(1);
            }
            return;
        }
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if args.iter().any(|arg| arg == "--shutdown-request") {
                quit(app);
                return;
            }
            if args.iter().any(|arg| arg == "--background") {
                return;
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            desktop::setup(app)?;
            if std::env::args().any(|arg| arg == "--shutdown-request") {
                quit(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_tasks,
            add_download,
            pause_download,
            resume_download,
            desktop::get_settings,
            desktop::add_batch,
            desktop::import_torrent,
            desktop::set_torrent_options,
            desktop::inspect_collection,
            desktop::save_settings,
            desktop::pause_all,
            desktop::start_queue,
            desktop::remove_task,
            desktop::set_priority,
            desktop::expand_batch,
            desktop::inspect_media,
            desktop::resolve_media,
            desktop::open_task,
            desktop::register_browser,
            desktop::suggest_directory,
            power::pending_power,
            power::cancel_power,
            file_drag::drag_completed,
            windows::open_surface,
            windows::close_surface,
            windows::get_task,
            windows::approve_download,
            windows::reject_download,
            windows::add_download_to
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if window.label() != "main" {
                    windows::child_close(window);
                    return;
                }
                if window.state::<Backend>().tray.load(Ordering::SeqCst) {
                    let _ = window.hide();
                } else {
                    quit(window.app_handle());
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("Flux desktop startup failed");
}
