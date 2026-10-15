use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};
use tokio::{fs, io::AsyncReadExt};

const MAX_FILES: usize = 4096;
const MAX_METAINFO: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchMode {
    #[default]
    Approval,
    Automatic,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TorrentWatch {
    pub enabled: bool,
    pub folders: Vec<PathBuf>,
    pub destination: Option<PathBuf>,
    pub mode: WatchMode,
}
impl TorrentWatch {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.folders.len() <= 16,
            "At most sixteen watch folders are supported"
        );
        ensure!(
            !self.enabled || !self.folders.is_empty(),
            "Select a torrent watch folder first"
        );
        let mut unique = HashSet::new();
        for path in self.folders.iter().chain(self.destination.iter()) {
            ensure!(
                path.is_absolute() && !path.as_os_str().is_empty(),
                "Watch folder paths must be absolute"
            );
        }
        for path in &self.folders {
            ensure!(unique.insert(path), "Duplicate torrent watch folder");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct WatchReport {
    pub imported: u64,
    pub error: Option<String>,
}
#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    length: u64,
    modified: SystemTime,
}
pub(crate) struct Candidate {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}
#[derive(Default)]
pub(crate) struct Scanner {
    observed: HashMap<PathBuf, Stamp>,
    processed: HashMap<PathBuf, Stamp>,
    failures: HashMap<PathBuf, (Instant, String)>,
    pub report: WatchReport,
}
impl Scanner {
    pub fn reject(&mut self, path: PathBuf, error: String) {
        self.processed.remove(&path);
        self.failures.insert(
            path,
            (Instant::now() + Duration::from_secs(30), error.clone()),
        );
        self.report.error = Some(error);
    }
    pub fn accept(&mut self, path: &std::path::Path) {
        self.failures.remove(path);
    }
    pub async fn scan(&mut self, settings: &TorrentWatch) -> Result<Vec<Candidate>> {
        self.report.error = None;
        if !settings.enabled {
            self.observed.clear();
            self.processed.clear();
            self.failures.clear();
            return Ok(Vec::new());
        }
        settings.validate()?;
        let mut seen = HashMap::new();
        let mut ready = Vec::new();
        let mut bytes_ready = 0;
        for folder in &settings.folders {
            let mut entries = match fs::read_dir(folder).await {
                Ok(entries) => entries,
                Err(error) => {
                    self.report.error = Some(format!("{}: {error}", folder.display()));
                    continue;
                }
            };
            loop {
                let entry = match entries.next_entry().await {
                    Ok(Some(entry)) => entry,
                    Ok(None) => break,
                    Err(error) => {
                        self.report.error = Some(format!("{}: {error}", folder.display()));
                        break;
                    }
                };
                let path = entry.path();
                if !path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("torrent"))
                {
                    continue;
                }
                if seen.len() >= MAX_FILES {
                    self.report.error =
                        Some("Watch folders exceed the 4096 torrent file limit".into());
                    break;
                }
                let metadata = match fs::symlink_metadata(&path).await {
                    Ok(metadata) if metadata.is_file() => metadata,
                    _ => continue,
                };
                if metadata.len() == 0 {
                    continue;
                }
                if metadata.len() > MAX_METAINFO {
                    self.report.error = Some(format!(
                        "{}: torrent metadata exceeds eight MiB",
                        path.display()
                    ));
                    continue;
                }
                let modified = match metadata.modified() {
                    Ok(modified) => modified,
                    Err(error) => {
                        self.report.error = Some(format!("{}: {error}", path.display()));
                        continue;
                    }
                };
                let stamp = Stamp {
                    length: metadata.len(),
                    modified,
                };
                seen.insert(path.clone(), stamp.clone());
                if self.observed.get(&path) != Some(&stamp) {
                    self.failures.remove(&path);
                }
                if self
                    .failures
                    .get(&path)
                    .is_some_and(|(retry, _)| Instant::now() < *retry)
                {
                    continue;
                }
                if self.observed.get(&path) != Some(&stamp)
                    || self.processed.get(&path) == Some(&stamp)
                {
                    continue;
                }
                if ready.len() >= 64 || bytes_ready + stamp.length > 16 * 1024 * 1024 {
                    continue;
                }
                let result = async {
                    let mut file = fs::File::open(&path).await?.take(MAX_METAINFO + 1);
                    let mut bytes = Vec::with_capacity(stamp.length as usize);
                    file.read_to_end(&mut bytes).await?;
                    ensure!(
                        bytes.len() as u64 == stamp.length,
                        "Torrent file changed while reading"
                    );
                    let after = fs::symlink_metadata(&path).await?;
                    ensure!(
                        after.is_file()
                            && after.len() == stamp.length
                            && after.modified()? == stamp.modified,
                        "Torrent file changed while reading"
                    );
                    Ok::<_, anyhow::Error>(Candidate {
                        path: path.clone(),
                        bytes,
                    })
                }
                .await
                .with_context(|| format!("Cannot read {}", path.display()));
                match result {
                    Ok(candidate) => {
                        bytes_ready += stamp.length;
                        self.processed.insert(path, stamp);
                        ready.push(candidate);
                    }
                    Err(error) => self.report.error = Some(error.to_string()),
                }
            }
        }
        self.processed
            .retain(|path, stamp| seen.get(path) == Some(stamp));
        self.observed = seen;
        self.failures
            .retain(|path, _| self.observed.contains_key(path));
        if self.report.error.is_none() {
            self.report.error = self
                .failures
                .values()
                .next()
                .map(|(_, error)| error.clone());
        }
        Ok(ready)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn candidate_limit_applies_across_all_folders_without_losing_observations() {
        let root = tempfile::tempdir().unwrap();
        let folders = vec![root.path().join("first"), root.path().join("second")];
        for folder in &folders {
            fs::create_dir(folder).await.unwrap();
            for index in 0..40 {
                fs::write(folder.join(format!("{index}.torrent")), b"metadata")
                    .await
                    .unwrap();
            }
        }
        let settings = TorrentWatch {
            enabled: true,
            folders,
            ..Default::default()
        };
        let mut scanner = Scanner::default();
        assert!(scanner.scan(&settings).await.unwrap().is_empty());
        assert_eq!(scanner.scan(&settings).await.unwrap().len(), 64);
        assert_eq!(scanner.scan(&settings).await.unwrap().len(), 16);
        assert!(scanner.scan(&settings).await.unwrap().is_empty());
    }
}
