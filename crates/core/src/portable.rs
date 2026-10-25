use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn root_for(executable: &Path) -> Option<PathBuf> {
    let parent = executable.parent()?;
    if !parent.join(".flux-portable").is_file() {
        return None;
    }
    // PortableApps layout: FluxPortable/App/Flux/<executable>.
    let root = parent.parent()?.parent()?;
    Some(if parent == root.join("App/Flux") {
        root.to_owned()
    } else {
        parent.to_owned()
    })
}

pub fn root() -> Option<PathBuf> {
    root_for(&std::env::current_exe().ok()?)
}

pub fn downloads() -> Option<PathBuf> {
    root().map(|root| root.join("Data/Downloads"))
}

pub(crate) fn relocate(value: &mut serde_json::Value, old: &Path, new: &Path) {
    match value {
        serde_json::Value::String(text) => {
            if let Ok(relative) = Path::new(text).strip_prefix(old) {
                *text = new.join(relative).to_string_lossy().into_owned();
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                relocate(item, old, new);
            }
        }
        serde_json::Value::Object(items) => {
            for item in items.values_mut() {
                relocate(item, old, new);
            }
        }
        _ => {}
    }
}

pub(crate) async fn restore_location(store: &crate::store::Store, data: &Path) -> Result<()> {
    let Some(root) = root() else {
        return Ok(());
    };
    // Explicit --data test/CLI directories must never be migrated as portable data.
    if data != root.join("Data/Flux") {
        return Ok(());
    }
    let record = data.join("portable-root.json");
    let previous: Option<PathBuf> = match tokio::fs::read(&record).await {
        Ok(bytes) => Some(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if let Some(previous) = previous.filter(|previous| previous != &root) {
        store
            .relocate_portable(previous.clone(), root.clone())
            .await?;
        let torrent_data = data.join("torrents");
        if let Ok(mut entries) = tokio::fs::read_dir(torrent_data).await {
            while let Some(entry) = entries.next_entry().await? {
                if entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".destination.json")
                {
                    let mut value = serde_json::from_slice(&tokio::fs::read(entry.path()).await?)?;
                    relocate(&mut value, &previous, &root);
                    tokio::fs::write(entry.path(), serde_json::to_vec(&value)?).await?;
                }
            }
        }
    }
    tokio::fs::write(record, serde_json::to_vec(&root)?).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_layout_and_path_relocation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("FluxPortable");
        std::fs::create_dir_all(root.join("App/Flux")).unwrap();
        assert!(root_for(&root.join("App/Flux/flux-desktop.exe")).is_none());
        std::fs::write(root.join("App/Flux/.flux-portable"), []).unwrap();
        assert_eq!(
            root_for(&root.join("App/Flux/flux-native-host.exe")),
            Some(root.clone())
        );
        let moved = temp.path().join("Moved");
        let external = temp.path().join("external/file.mp4");
        let mut value = serde_json::json!({"destination":root.join("Data/Downloads/movie.mp4"),"external":external});
        relocate(&mut value, &root, &moved);
        assert_eq!(
            value["destination"],
            moved
                .join("Data/Downloads/movie.mp4")
                .to_string_lossy()
                .as_ref()
        );
        assert_eq!(value["external"], external.to_string_lossy().as_ref());
    }
}
