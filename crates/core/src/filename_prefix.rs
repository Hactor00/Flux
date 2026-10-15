use anyhow::{ensure, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Serialize)]
pub struct PrefixedDestination {
    pub filename: String,
    pub directory: PathBuf,
}

fn split(prefix: &str, filename: &str) -> Result<(Vec<String>, String)> {
    ensure!(prefix.len() <= 512, "Filename prefix is too long");
    crate::model::validate_filename(filename)?;
    let normalized = prefix.replace('\\', "/");
    let mut parts: Vec<_> = normalized.split('/').collect();
    let suffix = parts.pop().unwrap_or_default();
    ensure!(parts.len() <= 16, "Too many prefix subfolders");
    for part in &parts {
        crate::model::validate_filename(part)?;
    }
    let filename = format!("{suffix}{filename}");
    crate::model::validate_filename(&filename)?;
    Ok((parts.into_iter().map(str::to_owned).collect(), filename))
}

pub async fn prepare(prefix: &str, filename: &str, base: &Path) -> Result<PrefixedDestination> {
    let (folders, filename) = split(prefix, filename)?;
    ensure!(base.is_absolute(), "Download directory must be absolute");
    fs::create_dir_all(base).await?;
    let root = fs::canonicalize(base).await?;
    let mut directory = root.clone();
    for folder in folders {
        let next = directory.join(folder);
        match fs::create_dir(&next).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        let metadata = fs::symlink_metadata(&next).await?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "Prefix subfolders must be regular directories"
        );
        directory = fs::canonicalize(&next).await?;
        ensure!(
            directory.starts_with(&root),
            "Prefix escapes the download directory"
        );
    }
    Ok(PrefixedDestination {
        filename,
        directory,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn prefixes_create_subfolders_and_preserve_the_file_extension() {
        let temp = tempfile::tempdir().unwrap();
        let result = prepare("Courses/Rust/lesson-", "video.mp4", temp.path())
            .await
            .unwrap();
        assert_eq!(result.filename, "lesson-video.mp4");
        assert_eq!(
            result.directory,
            fs::canonicalize(temp.path())
                .await
                .unwrap()
                .join("Courses/Rust")
        );
        assert!(result.directory.is_dir());
        assert_eq!(
            prepare("Courses\\Rust\\", "notes.pdf", temp.path())
                .await
                .unwrap()
                .filename,
            "notes.pdf"
        );
        assert_eq!(
            prepare("", "plain.zip", temp.path())
                .await
                .unwrap()
                .filename,
            "plain.zip"
        );
    }

    #[tokio::test]
    async fn invalid_prefixes_never_create_subfolders() {
        let temp = tempfile::tempdir().unwrap();
        for prefix in [
            "../",
            "/root/",
            "C:/",
            "a//b/",
            "a/../",
            "CON/",
            "a./",
            "a /",
            "\0",
            "bad:",
            "a/../bad-",
        ] {
            assert!(
                prepare(prefix, "file.zip", temp.path()).await.is_err(),
                "{prefix:?}"
            );
        }
        assert!(prepare("a/", "../file.zip", temp.path()).await.is_err());
        assert!(prepare(&"x".repeat(200), "file.zip", temp.path())
            .await
            .is_err());
        assert!(std::fs::read_dir(temp.path()).unwrap().next().is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn prefix_subfolders_cannot_follow_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), temp.path().join("link")).unwrap();
        assert!(prepare("link/sub/", "file.zip", temp.path()).await.is_err());
        assert!(!outside.path().join("sub").exists());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn prefix_subfolders_cannot_follow_junctions() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let link = temp.path().join("link");
        let status = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(outside.path())
            .output()
            .unwrap();
        assert!(status.status.success());
        let result = prepare("link/sub/", "file.zip", temp.path()).await;
        std::fs::remove_dir(&link).unwrap();
        assert!(result.is_err());
        assert!(!outside.path().join("sub").exists());
    }
}
