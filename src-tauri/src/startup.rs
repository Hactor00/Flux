#[cfg(windows)]
const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
#[cfg(windows)]
pub type Snapshot = Option<winreg::RegValue>;
#[cfg(not(windows))]
pub struct Snapshot;

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

#[cfg(not(windows))]
pub fn snapshot() -> Result<Snapshot, String> {
    Ok(Snapshot)
}
#[cfg(not(windows))]
pub fn restore(_: Snapshot) -> Result<(), String> {
    Ok(())
}
#[cfg(not(windows))]
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
#[cfg(not(windows))]
pub fn enabled() -> Result<bool, String> {
    Ok(false)
}
#[cfg(not(windows))]
pub fn set(enabled: bool) -> Result<(), String> {
    if enabled {
        Err("Startup registration requires Windows".into())
    } else {
        Ok(())
    }
}
