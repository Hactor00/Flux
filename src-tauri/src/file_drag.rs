#[cfg(windows)]
use tauri::Manager;
use tauri::State;
#[tauri::command]
pub async fn drag_completed(
    ids: Vec<String>,
    window: tauri::WebviewWindow,
    state: State<'_, crate::Backend>,
) -> Result<(), String> {
    if window.label() != "main" || ids.is_empty() || ids.len() > 100 {
        return Err("Invalid file drag request".into());
    }
    let mut paths = Vec::new();
    for id in ids {
        let task = state.engine.task(&id).await.map_err(|e| e.to_string())?;
        if task.status != flux_core::Status::Complete {
            return Err("Only completed files can be dragged".into());
        }
        let path = tokio::fs::canonicalize(&task.destination)
            .await
            .map_err(|e| e.to_string())?;
        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|e| e.to_string())?;
        if !metadata.is_file() && !(task.torrent.is_some() && metadata.is_dir()) {
            return Err("Drag source is not a completed file or torrent folder".into());
        }
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    #[cfg(windows)]
    {
        let (send, receive) = tokio::sync::oneshot::channel();
        let source = window.clone();
        window
            .app_handle()
            .run_on_main_thread(move || {
                let result = drag::start_drag(
                    &source,
                    drag::DragItem::Files(paths),
                    drag::Image::Raw(include_bytes!("../icons/icon.png").to_vec()),
                    |_, _| {},
                    drag::Options::default(),
                );
                let _ = send.send(result.map_err(|e| e.to_string()));
            })
            .map_err(|e| e.to_string())?;
        receive.await.map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = paths;
        Err("Native file drag-out is available on Windows".into())
    }
}
