use std::{error::Error, io::Write, path::Path};
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
    write_changed(&manifest_path, &serde_json::to_vec_pretty(&manifest)?)?;
    let firefox_manifest_path = data.join("io.flux.download.firefox.json");
    let firefox = serde_json::json!({
        "name": "io.flux.download",
        "description": "Flux Native Messaging Bridge for Firefox",
        "path": host,
        "type": "stdio",
        "allowed_extensions": [include_str!("../../extension-firefox/extension-id.txt").trim()]
    });
    write_changed(
        &firefox_manifest_path,
        &serde_json::to_vec_pretty(&firefox)?,
    )?;
    write_changed(
        &data.join("allowed-origins.json"),
        &serde_json::to_vec(&origins)?,
    )?;
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let registry = RegKey::predef(HKEY_CURRENT_USER);
        for (path, file) in registry_paths() {
            let (key, _) = registry.create_subkey(path)?;
            let target = data.join(file).to_string_lossy().into_owned();
            if key.get_value::<String, _>("").ok().as_ref() != Some(&target) {
                key.set_value("", &target)?;
            }
        }
        let (key, _) = registry.create_subkey("Software\\Flux")?;
        let owner = directory.to_string_lossy().into_owned();
        if key.get_value::<String, _>("BridgeDirectory").ok().as_ref() != Some(&owner) {
            key.set_value("BridgeDirectory", &owner)?;
        }
    }
    #[cfg(not(windows))]
    for (directory, file) in native_directories()? {
        std::fs::create_dir_all(&directory)?;
        write_changed(
            &directory.join("io.flux.download.json"),
            &std::fs::read(data.join(file))?,
        )?;
    }
    Ok(id.into())
}

pub(crate) fn write_changed(path: &Path, bytes: &[u8]) -> InstallResult<()> {
    if std::fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    let parent = path.parent().ok_or("Target directory unavailable")?;
    let temporary = parent.join(format!(".flux-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> InstallResult<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
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
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    #[cfg(target_os = "linux")]
    let paths = vec![
        (
            config.join("google-chrome/NativeMessagingHosts"),
            "native-host.json",
        ),
        (
            config.join("chromium/NativeMessagingHosts"),
            "native-host.json",
        ),
        (
            config.join("microsoft-edge/NativeMessagingHosts"),
            "native-host.json",
        ),
        (
            config.join("BraveSoftware/Brave-Browser/NativeMessagingHosts"),
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
    fn manifest_replacement_retains_unchanged_files_and_cleans_failed_writes() {
        let directory =
            std::env::temp_dir().join(format!("flux-manifest-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("manifest.json");
        write_changed(&path, b"first").unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        write_changed(&path, b"first").unwrap();
        assert_eq!(
            modified,
            std::fs::metadata(&path).unwrap().modified().unwrap()
        );
        write_changed(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        let blocked = directory.join("blocked");
        std::fs::create_dir(&blocked).unwrap();
        assert!(write_changed(&blocked, b"invalid").is_err());
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn packaged_origins_include_the_shared_extension_and_are_unique() {
        let origins = chromium_origins().unwrap();
        let expected = format!(
            "chrome-extension://{}/",
            include_str!("../../extension/extension-id.txt").trim()
        );
        assert!(origins.contains(&expected));
        assert!(
            origins.contains(&"chrome-extension://okimieponfbegjhmfdnnefnddofooipk/".to_owned())
        );
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
