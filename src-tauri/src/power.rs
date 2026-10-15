use flux_core::{ShutdownAction, Status, Task};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};

#[derive(Clone, Serialize)]
pub struct PendingPower {
    pub id: String,
    pub action: ShutdownAction,
    pub deadline: u64,
    #[serde(skip)]
    snapshot: Vec<(String, Status)>,
    #[serde(skip)]
    due: std::time::Instant,
}
#[derive(Default)]
pub struct PowerState(pub Mutex<Option<PendingPower>>);
fn signature(tasks: &[Task]) -> Vec<(String, Status)> {
    let mut result: Vec<_> = tasks
        .iter()
        .map(|t| (t.id.clone(), t.status.clone()))
        .collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
#[tauri::command]
pub fn pending_power(state: State<'_, PowerState>) -> Result<Option<PendingPower>, String> {
    Ok(state.0.lock().map_err(|e| e.to_string())?.clone())
}
#[tauri::command]
pub async fn cancel_power(app: tauri::AppHandle) -> Result<(), String> {
    app.state::<PowerState>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .take();
    let backend = app.state::<crate::Backend>();
    let engine = &backend.engine;
    let mut settings = engine.settings().await.map_err(|e| e.to_string())?;
    settings.completion_action = ShutdownAction::None;
    let settings = engine
        .save_settings(settings)
        .await
        .map_err(|e| e.to_string())?;
    app.emit("settings-changed", settings)
        .map_err(|e| e.to_string())?;
    app.emit("power-pending", Option::<PendingPower>::None)
        .map_err(|e| e.to_string())?;
    if let Some(window) = app.get_webview_window("power-new") {
        window.destroy().map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub async fn poll(app: &tauri::AppHandle, tasks: &[Task]) -> Result<(), String> {
    let pending = app
        .state::<PowerState>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    if let Some(pending) = pending {
        if app
            .state::<crate::Backend>()
            .engine
            .settings()
            .await
            .map_err(|e| e.to_string())?
            .completion_action
            != pending.action
        {
            return cancel_power(app.clone()).await;
        }
        if signature(tasks) != pending.snapshot {
            return cancel_power(app.clone()).await;
        }
        if std::time::Instant::now() < pending.due {
            return Ok(());
        }
        // Atomically claim the ticket. A late cancel cannot leave a second execution pending.
        let claimed = app
            .state::<PowerState>()
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .take();
        if claimed.as_ref().map(|p| &p.id) != Some(&pending.id) {
            return Ok(());
        }
        let backend = app.state::<crate::Backend>();
        let engine = &backend.engine;
        let mut settings = engine.settings().await.map_err(|e| e.to_string())?;
        settings.completion_action = ShutdownAction::None;
        engine
            .save_settings(settings)
            .await
            .map_err(|e| e.to_string())?;
        if signature(&engine.list().await.map_err(|e| e.to_string())?) != pending.snapshot {
            return cancel_power(app.clone()).await;
        }
        if pending.action == ShutdownAction::ExitApp {
            crate::quit(app);
            return Ok(());
        }
        engine.pause_all().await.map_err(|e| e.to_string())?;
        let action = pending.action;
        let result = tauri::async_runtime::spawn_blocking(move || native::execute(action))
            .await
            .map_err(|e| e.to_string())?;
        cancel_power(app.clone()).await?;
        result
    } else if let Some(action) = app
        .state::<crate::Backend>()
        .engine
        .poll_completion_action()
        .await
        .map_err(|e| e.to_string())?
    {
        let pending = PendingPower {
            id: uuid::Uuid::new_v4().to_string(),
            action,
            deadline: now() + 30_000,
            snapshot: signature(tasks),
            due: std::time::Instant::now() + std::time::Duration::from_secs(30),
        };
        *app.state::<PowerState>()
            .0
            .lock()
            .map_err(|e| e.to_string())? = Some(pending.clone());
        app.emit("power-pending", Some(pending.clone()))
            .map_err(|e| e.to_string())?;
        let handle = app.clone();
        if let Err(error) = tauri::async_runtime::spawn_blocking(move || {
            crate::windows::show(&handle, "power", None, None)
        })
        .await
        .map_err(|e| e.to_string())?
        {
            cancel_power(app.clone()).await?;
            return Err(error);
        }
        let updated = {
            let state = app.state::<PowerState>();
            let mut value = state.0.lock().map_err(|e| e.to_string())?;
            value
                .as_mut()
                .filter(|value| value.id == pending.id)
                .map(|value| {
                    value.deadline = now() + 30_000;
                    value.due = std::time::Instant::now() + std::time::Duration::from_secs(30);
                    value.clone()
                })
        };
        if let Some(updated) = updated {
            app.emit("power-pending", Some(updated))
                .map_err(|e| e.to_string())?;
        } else {
            cancel_power(app.clone()).await?;
        }
        Ok(())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
mod native {
    use super::ShutdownAction;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_NOT_ALL_ASSIGNED, HANDLE},
        Security::{
            AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES,
            SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
        },
        System::{
            Power::SetSuspendState,
            Shutdown::{
                ExitWindowsEx, EWX_POWEROFF, SHTDN_REASON_FLAG_PLANNED,
                SHTDN_REASON_MAJOR_APPLICATION, SHTDN_REASON_MINOR_OTHER,
            },
            Threading::{GetCurrentProcess, OpenProcessToken},
        },
    };
    struct Privilege {
        token: HANDLE,
        previous: TOKEN_PRIVILEGES,
    }
    impl Drop for Privilege {
        fn drop(&mut self) {
            unsafe {
                AdjustTokenPrivileges(
                    self.token,
                    0,
                    &self.previous,
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
                CloseHandle(self.token);
            }
        }
    }
    fn enable() -> Result<Privilege, String> {
        unsafe {
            let mut token = std::ptr::null_mut();
            if OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
                &mut token,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let mut guard = Privilege {
                token,
                previous: std::mem::zeroed(),
            };
            let name: Vec<u16> = "SeShutdownPrivilege\0".encode_utf16().collect();
            let mut luid = std::mem::zeroed();
            if LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let desired = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES {
                    Luid: luid,
                    Attributes: SE_PRIVILEGE_ENABLED,
                }],
            };
            let mut length = 0;
            if AdjustTokenPrivileges(
                token,
                0,
                &desired,
                std::mem::size_of::<TOKEN_PRIVILEGES>() as u32,
                &mut guard.previous,
                &mut length,
            ) == 0
                || GetLastError() == ERROR_NOT_ALL_ASSIGNED
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
            Ok(guard)
        }
    }
    pub fn execute(action: ShutdownAction) -> Result<(), String> {
        let _privilege = enable()?;
        let success = unsafe {
            match action {
                ShutdownAction::Sleep => SetSuspendState(false, false, false),
                ShutdownAction::Hibernate => SetSuspendState(true, false, false),
                ShutdownAction::ShutdownComputer => {
                    ExitWindowsEx(
                        EWX_POWEROFF,
                        SHTDN_REASON_MAJOR_APPLICATION
                            | SHTDN_REASON_MINOR_OTHER
                            | SHTDN_REASON_FLAG_PLANNED,
                    ) != 0
                }
                _ => return Err("Invalid native power action".into()),
            }
        };
        if success {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error().to_string())
        }
    }
}
#[cfg(not(any(windows, target_os = "linux")))]
mod native {
    pub fn execute(_: flux_core::ShutdownAction) -> Result<(), String> {
        Err("Computer power actions require Windows".into())
    }
}
#[cfg(target_os = "linux")]
mod native {
    pub fn execute(action: flux_core::ShutdownAction) -> Result<(), String> {
        let operation = match action {
            flux_core::ShutdownAction::Sleep => "suspend",
            flux_core::ShutdownAction::Hibernate => "hibernate",
            flux_core::ShutdownAction::ShutdownComputer => "poweroff",
            _ => return Err("Invalid power action".into()),
        };
        let status = std::process::Command::new("systemctl")
            .args(["--no-ask-password", operation])
            .status()
            .map_err(|error| error.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("The system denied the power action or does not support it".into())
        }
    }
}
