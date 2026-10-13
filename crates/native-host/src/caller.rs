use anyhow::{ensure, Context, Result};
use std::path::Path;

pub const FIREFOX_ID: &str =
    include_str!("../../../extension-firefox/extension-id.txt").trim_ascii();

/// Authorize the engine-specific launcher arguments before reading stdin.
pub fn authorize(data: &Path, executable: &Path, arguments: &[String]) -> Result<()> {
    let first = arguments
        .first()
        .context("Native Messaging caller is required")?;
    if first.starts_with("chrome-extension://") {
        let allowed: Vec<String> =
            serde_json::from_slice(&std::fs::read(data.join("allowed-origins.json"))?)?;
        ensure!(
            allowed.contains(first),
            "Extension origin is not authorized"
        );
        return Ok(());
    }
    ensure!(
        arguments.len() == 2,
        "Firefox launcher arguments are required"
    );
    ensure!(
        arguments[1] == FIREFOX_ID,
        "Firefox extension is not authorized"
    );
    let registered = data.join("io.flux.download.firefox.json");
    let contents = std::fs::read(&registered)?;
    let supplied = Path::new(first);
    ensure!(
        supplied.is_absolute(),
        "Firefox manifest path must be absolute"
    );
    #[cfg(windows)]
    ensure!(
        supplied.canonicalize()? == registered.canonicalize()?,
        "Firefox manifest path is not registered"
    );
    #[cfg(not(windows))]
    {
        let home = std::path::PathBuf::from(std::env::var_os("HOME").context("HOME unavailable")?);
        #[cfg(target_os = "linux")]
        let trusted = home.join(".mozilla/native-messaging-hosts/io.flux.download.json");
        #[cfg(target_os = "macos")]
        let trusted = home
            .join("Library/Application Support/Mozilla/NativeMessagingHosts/io.flux.download.json");
        ensure!(
            supplied.canonicalize()? == trusted.canonicalize()?,
            "Firefox manifest path is not registered"
        );
        ensure!(
            std::fs::read(supplied)? == contents,
            "Firefox registration is inconsistent"
        );
    }
    let manifest: serde_json::Value = serde_json::from_slice(&contents)?;
    ensure!(
        manifest["name"] == "io.flux.download" && manifest["type"] == "stdio",
        "Invalid Firefox host manifest"
    );
    ensure!(
        manifest["allowed_extensions"] == serde_json::json!([FIREFOX_ID]),
        "Invalid Firefox authorization list"
    );
    let host = manifest["path"]
        .as_str()
        .context("Firefox host path is missing")?;
    ensure!(
        Path::new(host).is_absolute(),
        "Firefox host path must be absolute"
    );
    ensure!(
        Path::new(host).canonicalize()? == executable.canonicalize()?,
        "Firefox manifest refers to another host"
    );
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, std::path::PathBuf, Vec<String>) {
        let root = tempfile::tempdir().unwrap();
        let host = root.path().join("flux-native-host.exe");
        std::fs::write(&host, []).unwrap();
        let manifest = root.path().join("io.flux.download.firefox.json");
        std::fs::write(&manifest, serde_json::to_vec(&serde_json::json!({"name":"io.flux.download","type":"stdio","path":host,"allowed_extensions":[FIREFOX_ID]})).unwrap()).unwrap();
        let args = vec![manifest.to_string_lossy().into_owned(), FIREFOX_ID.into()];
        (root, host, args)
    }
    #[test]
    fn accepts_registered_firefox_caller() {
        let (root, host, args) = fixture();
        authorize(root.path(), &host, &args).unwrap();
    }
    #[test]
    fn rejects_wrong_extension_and_missing_arguments() {
        let (root, host, mut args) = fixture();
        args[1] = "other@extension.test".into();
        assert!(authorize(root.path(), &host, &args).is_err());
        assert!(authorize(root.path(), &host, &args[..1]).is_err());
        assert!(authorize(root.path(), &host, &[]).is_err());
    }
    #[test]
    fn rejects_unregistered_manifest_copy() {
        let (root, host, mut args) = fixture();
        let copy = root.path().join("unregistered.json");
        std::fs::copy(&args[0], &copy).unwrap();
        args[0] = copy.to_string_lossy().into_owned();
        assert!(authorize(root.path(), &host, &args).is_err());
    }
    #[test]
    fn rejects_changed_executable_or_allowlist() {
        let (root, host, args) = fixture();
        let other = root.path().join("other.exe");
        std::fs::write(&other, []).unwrap();
        assert!(authorize(root.path(), &other, &args).is_err());
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&args[0]).unwrap()).unwrap();
        manifest["allowed_extensions"] = serde_json::json!([FIREFOX_ID, "other@extension.test"]);
        std::fs::write(&args[0], serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(authorize(root.path(), &host, &args).is_err());
    }
    #[test]
    fn preserves_chromium_origin_authorization() {
        let (root, host, _) = fixture();
        let origin = "chrome-extension://iheejmamcoeigoallciifjminjnnnlbj/";
        std::fs::write(
            root.path().join("allowed-origins.json"),
            serde_json::to_vec(&vec![origin]).unwrap(),
        )
        .unwrap();
        authorize(
            root.path(),
            &host,
            &[origin.into(), "--parent-window=0".into()],
        )
        .unwrap();
        assert!(authorize(root.path(), &host, &["chrome-extension://unknown/".into()]).is_err());
    }
}
