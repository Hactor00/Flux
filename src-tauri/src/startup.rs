#[cfg(windows)]
const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
#[cfg(windows)]
pub type Snapshot = Option<winreg::RegValue>;
#[cfg(not(any(windows, target_os = "linux")))]
pub struct Snapshot;
#[cfg(target_os = "linux")]
pub type Snapshot = Option<Vec<u8>>;

#[cfg(windows)]
pub fn snapshot() -> Result<Snapshot, String> {
    let registry = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let key = match registry.open_subkey(KEY) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    match key.get_raw_value("Flux") {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(windows)]
pub fn restore(snapshot: Snapshot) -> Result<(), String> {
    let registry = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let (key, _) = registry.create_subkey(KEY).map_err(|e| e.to_string())?;
    match snapshot {
        Some(value) => key.set_raw_value("Flux", &value).map_err(|e| e.to_string()),
        None => set(false),
    }
}

#[cfg(windows)]
pub fn remove_owned() -> Result<(), String> {
    use winreg::types::FromRegValue;
    if let Some(value) = snapshot()? {
        if let Ok(value) = String::from_reg_value(&value) {
            if value.eq_ignore_ascii_case(&command()?) {
                set(false)?;
            }
        }
    }
    Ok(())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn snapshot() -> Result<Snapshot, String> {
    Ok(Snapshot)
}
#[cfg(not(any(windows, target_os = "linux")))]
pub fn restore(_: Snapshot) -> Result<(), String> {
    Ok(())
}
#[cfg(not(any(windows, target_os = "linux")))]
pub fn remove_owned() -> Result<(), String> {
    Ok(())
}
#[cfg(windows)]
pub fn enabled() -> Result<bool, String> {
    let registry = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let key = match registry.open_subkey(KEY) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    let value: String = match key.get_value("Flux") {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    Ok(!value.is_empty())
}
#[cfg(windows)]
fn command() -> Result<String, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(format!("\"{}\" --background", executable.display()))
}
#[cfg(windows)]
pub fn set(enabled: bool) -> Result<(), String> {
    let registry = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let (key, _) = registry.create_subkey(KEY).map_err(|e| e.to_string())?;
    if enabled {
        key.set_value("Flux", &command()?)
            .map_err(|e| e.to_string())
    } else {
        match key.delete_value("Flux") {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}
#[cfg(not(any(windows, target_os = "linux")))]
pub fn enabled() -> Result<bool, String> {
    Ok(false)
}
#[cfg(not(any(windows, target_os = "linux")))]
pub fn set(enabled: bool) -> Result<(), String> {
    if enabled {
        Err("Startup registration requires Windows".into())
    } else {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn autostart_path() -> Result<std::path::PathBuf, String> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
        })
        .ok_or("Home directory is unavailable")?;
    Ok(config.join("autostart/io.flux.download.desktop"))
}
#[cfg(target_os = "linux")]
fn autostart_content() -> Result<Vec<u8>, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let executable = executable
        .to_str()
        .ok_or("Application path is not valid UTF-8")?;
    if executable.contains(['\n', '\r']) {
        return Err("Invalid application path".into());
    }
    let escaped = executable
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    Ok(format!("[Desktop Entry]\nType=Application\nName=Flux\nExec=\"{escaped}\" --background\nTerminal=false\nX-GNOME-Autostart-enabled=true\n").into_bytes())
}
#[cfg(target_os = "linux")]
pub fn snapshot() -> Result<Snapshot, String> {
    match std::fs::read(autostart_path()?) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}
#[cfg(target_os = "linux")]
pub fn restore(snapshot: Snapshot) -> Result<(), String> {
    match snapshot {
        Some(bytes) => crate::install::write_changed(&autostart_path()?, &bytes)
            .map_err(|error| error.to_string()),
        None => set(false),
    }
}
#[cfg(target_os = "linux")]
pub fn enabled() -> Result<bool, String> {
    Ok(snapshot()?.as_deref() == Some(autostart_content()?.as_slice()))
}
#[cfg(target_os = "linux")]
pub fn set(enabled: bool) -> Result<(), String> {
    let path = autostart_path()?;
    if enabled {
        crate::install::write_changed(&path, &autostart_content()?)
            .map_err(|error| error.to_string())
    } else {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}
#[cfg(target_os = "linux")]
pub fn remove_owned() -> Result<(), String> {
    if enabled()? {
        set(false)?;
    }
    Ok(())
}
