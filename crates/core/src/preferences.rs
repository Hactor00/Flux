use crate::{Status, Task};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownAction {
    #[default]
    None,
    ExitApp,
    Sleep,
    Hibernate,
    ShutdownComputer,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardMode {
    #[default]
    Approval,
    SilentQueue,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CategoryFolders {
    pub compressed: Option<PathBuf>,
    pub video: Option<PathBuf>,
    pub music: Option<PathBuf>,
    pub programs: Option<PathBuf>,
    pub documents: Option<PathBuf>,
}
impl CategoryFolders {
    pub fn validate(&self) -> anyhow::Result<()> {
        for path in [
            &self.compressed,
            &self.video,
            &self.music,
            &self.programs,
            &self.documents,
        ]
        .into_iter()
        .flatten()
        {
            anyhow::ensure!(
                path.is_absolute() && !path.as_os_str().is_empty(),
                "Category directory must be absolute"
            );
        }
        Ok(())
    }
    pub fn resolve(&self, category: Category, root: &Path) -> PathBuf {
        let (configured, name) = match category {
            Category::Compressed => (&self.compressed, "Compressed"),
            Category::Video => (&self.video, "Video"),
            Category::Music => (&self.music, "Music"),
            Category::Programs => (&self.programs, "Programs"),
            Category::Documents => (&self.documents, "Documents"),
        };
        configured.clone().unwrap_or_else(|| root.join(name))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Compressed,
    Video,
    Music,
    Programs,
    Documents,
}
pub fn safe_filename(value: &str) -> String {
    let mut name: String = value
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    name = name.trim_end_matches(['.', ' ']).to_owned();
    while name.len() > 175 {
        name.pop();
    }
    if name.is_empty() {
        name = "download".into();
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_lowercase();
    if matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
        || (stem.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
        name.insert(0, '_');
    }
    name
}
pub fn clipboard_request(value: &str, connections: u8) -> anyhow::Result<crate::DownloadRequest> {
    crate::model::validate_url(value)?;
    let url = url::Url::parse(value)?;
    let filename = url
        .path_segments()
        .and_then(|mut parts| parts.rfind(|p| !p.is_empty()))
        .unwrap_or("download");
    let mut request = crate::DownloadRequest {
        url: value.into(),
        filename: safe_filename(
            &percent_encoding::percent_decode_str(filename).decode_utf8_lossy(),
        ),
        connections,
        auto_rename: true,
        ..Default::default()
    };
    let extension = Path::new(url.path())
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "m3u8" | "mpd") {
        request.filename = format!(
            "{}.mp4",
            Path::new(&request.filename)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
        );
        request.media = Some(crate::MediaOptions {
            kind: Some(if extension == "m3u8" { "hls" } else { "dash" }.into()),
            ..Default::default()
        });
    }
    crate::model::validate(&request)?;
    Ok(request)
}

pub fn category_for(filename: &str, mime: Option<&str>) -> Option<Category> {
    let extension = Path::new(filename)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let by_extension = match extension.as_str() {
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso" => Some(Category::Compressed),
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "m4v" | "ts" => Some(Category::Video),
        "mp3" | "mka" | "flac" | "wav" | "aac" | "m4a" | "ogg" | "opus" => Some(Category::Music),
        "exe" | "msi" | "bat" | "cmd" | "apk" => Some(Category::Programs),
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "epub" | "txt" => {
            Some(Category::Documents)
        }
        _ => None,
    };
    if by_extension.is_some() {
        return by_extension;
    }
    let mime = mime
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if mime.starts_with("video/") {
        Some(Category::Video)
    } else if mime.starts_with("audio/") {
        Some(Category::Music)
    } else if matches!(
        mime.as_str(),
        "application/zip"
            | "application/x-rar-compressed"
            | "application/vnd.rar"
            | "application/x-7z-compressed"
            | "application/x-tar"
            | "application/gzip"
            | "application/x-iso9660-image"
    ) {
        Some(Category::Compressed)
    } else if matches!(
        mime.as_str(),
        "application/x-msdownload"
            | "application/x-msi"
            | "application/vnd.microsoft.portable-executable"
    ) {
        Some(Category::Programs)
    } else if mime.starts_with("text/")
        || matches!(
            mime.as_str(),
            "application/pdf" | "application/epub+zip" | "application/msword"
        )
        || mime.starts_with("application/vnd.openxmlformats-officedocument.")
    {
        Some(Category::Documents)
    } else {
        None
    }
}

#[derive(Default)]
pub(crate) struct CompletionWatch {
    action: ShutdownAction,
    targets: HashSet<String>,
    fired: bool,
}
impl CompletionWatch {
    pub fn configure(&mut self, action: ShutdownAction) {
        if self.action != action {
            *self = Self {
                action,
                ..Self::default()
            };
        }
    }
    pub fn observe(&mut self, action: ShutdownAction, tasks: &[Task]) -> Option<ShutdownAction> {
        self.configure(action);
        if action == ShutdownAction::None || self.fired {
            return None;
        }
        for task in tasks {
            if matches!(
                task.status,
                Status::Queued | Status::Running | Status::Verifying
            ) {
                self.targets.insert(task.id.clone());
            }
        }
        let completed: HashSet<&str> = tasks
            .iter()
            .filter(|t| t.status == Status::Complete)
            .map(|t| t.id.as_str())
            .collect();
        if !tasks.iter().any(|t| {
            matches!(
                t.status,
                Status::Queued
                    | Status::Running
                    | Status::Verifying
                    | Status::AwaitingApproval
                    | Status::Paused
            )
        }) && !self.targets.is_empty()
            && self
                .targets
                .iter()
                .all(|id| completed.contains(id.as_str()))
        {
            self.fired = true;
            Some(action)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extensions_take_precedence_and_mime_routes_unknown_names() {
        assert_eq!(
            category_for("Movie.MKV", Some("application/octet-stream")),
            Some(Category::Video)
        );
        assert_eq!(
            category_for("resource", Some("audio/aac; charset=binary")),
            Some(Category::Music)
        );
        assert_eq!(
            category_for("installer.exe", Some("video/mp4")),
            Some(Category::Programs)
        );
        assert_eq!(category_for("unknown.bin", None), None);
        assert_eq!(
            category_for("book", Some("application/epub+zip")),
            Some(Category::Documents)
        );
    }
    #[test]
    fn category_paths_require_absolute_custom_roots() {
        let invalid = CategoryFolders {
            video: Some(PathBuf::from("../outside")),
            ..CategoryFolders::default()
        };
        assert!(invalid.validate().is_err());
        assert_eq!(
            CategoryFolders::default().resolve(Category::Video, Path::new("root")),
            Path::new("root/Video")
        );
    }
    fn task(id: &str, status: Status) -> Task {
        serde_json::from_value(serde_json::json!({"id":id,"request":crate::DownloadRequest::default(),"status":status,"total":null,"completed":0,"etag":null,"ranged":false,"sha256":null,"error":null,"destination":"file"})).unwrap()
    }
    #[test]
    fn completion_requires_activity_success_and_fires_once() {
        let mut watch = CompletionWatch::default();
        let action = ShutdownAction::ShutdownComputer;
        assert_eq!(
            watch.observe(action, &[task("old", Status::Complete)]),
            None
        );
        assert_eq!(watch.observe(action, &[task("new", Status::Queued)]), None);
        assert_eq!(watch.observe(action, &[task("new", Status::Failed)]), None);
        assert_eq!(watch.observe(action, &[task("new", Status::Running)]), None);
        assert_eq!(
            watch.observe(action, &[task("new", Status::Complete)]),
            Some(action)
        );
        assert_eq!(
            watch.observe(action, &[task("new", Status::Complete)]),
            None
        );
    }
    #[test]
    fn paused_pending_and_removed_tasks_never_trigger_power() {
        let mut watch = CompletionWatch::default();
        let action = ShutdownAction::Sleep;
        watch.observe(action, &[task("new", Status::Running)]);
        assert_eq!(watch.observe(action, &[]), None);
        assert_eq!(watch.observe(action, &[task("new", Status::Paused)]), None);
        assert_eq!(
            watch.observe(
                action,
                &[
                    task("new", Status::Complete),
                    task("pending", Status::AwaitingApproval)
                ]
            ),
            None
        );
        assert_eq!(
            watch.observe(ShutdownAction::None, &[task("new", Status::Complete)]),
            None
        );
        assert_eq!(
            watch.observe(action, &[task("new", Status::Complete)]),
            None
        );
    }
    #[test]
    fn clipboard_requests_are_safe_and_do_not_require_approval() {
        let input = clipboard_request("https://example.test/Some%20book.pdf", 0).unwrap();
        assert_eq!(input.filename, "Some book.pdf");
        assert!(input.auto_rename);
        assert_eq!(
            clipboard_request("https://example.test/con.mp3", 0)
                .unwrap()
                .filename,
            "_con.mp3"
        );
        assert!(clipboard_request("https://user:secret@example.test/file", 0).is_err());
        let hls = clipboard_request("https://example.test/movie.m3u8", 0).unwrap();
        assert_eq!(hls.filename, "movie.mp4");
        assert_eq!(hls.media.unwrap().kind.as_deref(), Some("hls"));
        assert!(safe_filename(&"映".repeat(300)).len() <= 175);
    }
}
