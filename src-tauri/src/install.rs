use std::{error::Error, path::Path};
type InstallResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
pub fn register() -> InstallResult<String> {
    let id = include_str!("../../extension/extension-id.txt").trim();
    let executable = std::env::current_exe()?;
    let directory = executable
        .parent()
        .ok_or("Application directory unavailable")?;
    let host = directory.join(if cfg!(windows) {
        "flux-native-host.exe"
    } else {
        "flux-native-host"
    });
    if !host.is_file() {
        return Err("Native host executable is missing from this installation".into());
    }
    let data = flux_core::default_data_directory()?;
    std::fs::create_dir_all(&data)?;
    let origins = chromium_origins()?;
    let manifest = serde_json::json!({"name":"io.flux.download","description":"Flux download bridge","path":host,"type":"stdio","allowed_origins":origins});
    let manifest_path = data.join("native-host.json");
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    let firefox_manifest_path = data.join("io.flux.download.firefox.json");
    let firefox = serde_json::json!({
        "name": "io.flux.download",
        "description": "Flux Native Messaging Bridge for Firefox",
        "path": host,
        "type": "stdio",
        "allowed_extensions": [include_str!("../../extension-firefox/extension-id.txt").trim()]
    });
    std::fs::write(&firefox_manifest_path, serde_json::to_vec_pretty(&firefox)?)?;
    std::fs::write(
        data.join("allowed-origins.json"),
        serde_json::to_vec(&origins)?,
    )?;
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let registry = RegKey::predef(HKEY_CURRENT_USER);
        for (path, file) in registry_paths() {
            let (key, _) = registry.create_subkey(path)?;
            key.set_value("", &data.join(file).to_string_lossy().as_ref())?;
        }
        let (key, _) = registry.create_subkey("Software\\Flux")?;
        key.set_value("BridgeDirectory", &directory.to_string_lossy().as_ref())?;
    }
    #[cfg(not(windows))]
    for (directory, file) in native_directories()? {
        std::fs::create_dir_all(&directory)?;
        std::fs::copy(data.join(file), directory.join("io.flux.download.json"))?;
    }
    Ok(id.into())
}
pub fn unregister() -> InstallResult<()> {
    crate::startup::remove_owned().map_err(std::io::Error::other)?;
    let executable = std::env::current_exe()?;
    let directory = executable
        .parent()
        .ok_or("Application directory unavailable")?;
    let data = flux_core::default_data_directory()?;
    let manifest_path = data.join("native-host.json");
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let registry = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(key) = registry.open_subkey("Software\\Flux") else {
            return Ok(());
        };
        let owner: String = match key.get_value("BridgeDirectory") {
            Ok(owner) => owner,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if Path::new(&owner) != directory {
            return Ok(());
        }
        for (path, file) in registry_paths() {
            if let Ok(key) = registry.open_subkey(path) {
                let current: String = key.get_value("")?;
                if Path::new(&current) == data.join(file) {
                    registry.delete_subkey_all(path)?;
                }
            }
        }
        registry
            .open_subkey_with_flags("Software\\Flux", winreg::enums::KEY_WRITE)?
            .delete_value("BridgeDirectory")?;
    }
    #[cfg(not(windows))]
    {
        let owner: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
        if owner["path"]
            .as_str()
            .and_then(|path| Path::new(path).parent())
            != Some(directory)
        {
            return Ok(());
        }
        for (directory, file) in native_directories()? {
            let target = directory.join("io.flux.download.json");
            if std::fs::read(&target).ok() == std::fs::read(data.join(file)).ok() {
                remove(&target)?;
            }
        }
    }
    remove(&manifest_path)?;
    remove(&data.join("io.flux.download.firefox.json"))?;
    remove(&data.join("allowed-origins.json"))?;
    Ok(())
}
fn remove(path: &Path) -> InstallResult<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
fn chromium_origins() -> InstallResult<Vec<String>> {
    let mut ids: Vec<String> = serde_json::from_str(include_str!(
        "../../native-host/chromium-extension-ids.json"
    ))?;
    ids.push(
        include_str!("../../extension/extension-id.txt")
            .trim()
            .to_owned(),
    );
    ids.sort_unstable();
    ids.dedup();
    if ids.len() > 8
        || ids
            .iter()
            .any(|id| id.len() != 32 || !id.bytes().all(|byte| (b'a'..=b'p').contains(&byte)))
    {
        return Err("Invalid packaged Chromium extension IDs".into());
    }
    Ok(ids
        .into_iter()
        .map(|id| format!("chrome-extension://{id}/"))
        .collect())
}

#[cfg(windows)]
fn registry_paths() -> [(&'static str, &'static str); 5] {
    [
        (
            "Software\\Google\\Chrome\\NativeMessagingHosts\\io.flux.download",
            "native-host.json",
        ),
        (
            "Software\\Microsoft\\Edge\\NativeMessagingHosts\\io.flux.download",
            "native-host.json",
        ),
        (
            "Software\\Chromium\\NativeMessagingHosts\\io.flux.download",
            "native-host.json",
        ),
        (
            "Software\\BraveSoftware\\Brave-Browser\\NativeMessagingHosts\\io.flux.download",
            "native-host.json",
        ),
        (
            "Software\\Mozilla\\NativeMessagingHosts\\io.flux.download",
            "io.flux.download.firefox.json",
        ),
    ]
}
#[cfg(not(windows))]
fn native_directories() -> InstallResult<Vec<(std::path::PathBuf, &'static str)>> {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").ok_or("HOME unavailable")?);
    #[cfg(target_os = "linux")]
    let paths = vec![
        (
            home.join(".config/google-chrome/NativeMessagingHosts"),
            "native-host.json",
        ),
        (
            home.join(".config/chromium/NativeMessagingHosts"),
            "native-host.json",
        ),
        (
            home.join(".config/microsoft-edge/NativeMessagingHosts"),
            "native-host.json",
        ),
        (
            home.join(".config/BraveSoftware/Brave-Browser/NativeMessagingHosts"),
            "native-host.json",
        ),
        (
            home.join(".mozilla/native-messaging-hosts"),
            "io.flux.download.firefox.json",
        ),
    ];
    #[cfg(target_os = "macos")]
    let paths = vec![
        (
            home.join("Library/Application Support/Google/Chrome/NativeMessagingHosts"),
            "native-host.json",
        ),
        (
            home.join("Library/Application Support/Chromium/NativeMessagingHosts"),
            "native-host.json",
        ),
        (
            home.join("Library/Application Support/Microsoft Edge/NativeMessagingHosts"),
            "native-host.json",
        ),
        (
            home.join(
                "Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts",
            ),
            "native-host.json",
        ),
        (
            home.join("Library/Application Support/Mozilla/NativeMessagingHosts"),
            "io.flux.download.firefox.json",
        ),
    ];
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_origins_include_the_shared_extension_and_are_unique() {
        let origins = chromium_origins().unwrap();
        let expected = format!(
            "chrome-extension://{}/",
            include_str!("../../extension/extension-id.txt").trim()
        );
        assert!(origins.contains(&expected));
        let unique: std::collections::HashSet<_> = origins.iter().collect();
        assert_eq!(unique.len(), origins.len());
    }

    #[cfg(windows)]
    #[test]
    fn every_supported_browser_has_an_explicit_native_host_binding() {
        let paths = registry_paths();
        for browser in [
            "Google\\Chrome",
            "Microsoft\\Edge",
            "BraveSoftware\\Brave-Browser",
            "Chromium",
            "Mozilla",
        ] {
            assert!(paths.iter().any(|(path, _)| path.contains(browser)));
        }
        assert_eq!(
            paths
                .iter()
                .filter(|(_, file)| *file == "io.flux.download.firefox.json")
                .count(),
            1
        );
    }
}
