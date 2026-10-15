use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    AwaitingApproval,
    Cancelled,
    Queued,
    Running,
    Paused,
    Verifying,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DownloadRequest {
    pub url: String,
    pub filename: String,
    #[serde(default = "default_connections")]
    pub connections: u8,
    #[serde(default)]
    pub speed_limit: u64,
    #[serde(default)]
    pub expected_sha256: Option<String>,
    #[serde(default)]
    pub priority: i8,
    #[serde(default)]
    pub start_at: Option<u64>,
    #[serde(default)]
    pub stop_at: Option<u64>,
    #[serde(default)]
    pub media: Option<MediaOptions>,
    #[serde(default)]
    pub torrent: Option<TorrentOptions>,
    #[serde(default)]
    pub referer: Option<String>,
    #[serde(default)]
    pub auto_rename: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<BrowserCredentials>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_credentials: Option<String>,
    #[serde(default)]
    pub browser_source: Option<BrowserSource>,
    #[serde(default)]
    pub transport: TransportPreference,
    #[serde(default)]
    pub content_type: Option<String>,
}
fn default_connections() -> u8 {
    0
}

impl Default for DownloadRequest {
    fn default() -> Self {
        Self {
            url: String::new(),
            filename: String::new(),
            connections: default_connections(),
            speed_limit: 0,
            expected_sha256: None,
            priority: 0,
            start_at: None,
            stop_at: None,
            media: None,
            torrent: None,
            referer: None,
            auto_rename: false,
            credentials: Vec::new(),
            protected_credentials: None,
            browser_source: None,
            transport: TransportPreference::Auto,
            content_type: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MediaOptions {
    #[serde(default)]
    pub manifest_snapshot: Option<ManifestSnapshot>,
    #[serde(default)]
    pub manifest_video_id: Option<String>,
    #[serde(default)]
    pub manifest_audio_id: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub provider: Option<MediaProvider>,
    #[serde(default)]
    pub segment_urls: Vec<String>,
    #[serde(default)]
    pub audio_url: Option<String>,
    #[serde(default)]
    pub video_stream: Option<u32>,
    #[serde(default)]
    pub audio_stream: Option<u32>,
    #[serde(default)]
    pub duration_seconds: Option<u32>,
    #[serde(default = "default_container")]
    pub container: String,
    #[serde(default)]
    pub subtitles: Vec<SubtitleTrack>,
    #[serde(default)]
    pub subtitle_mode: SubtitleMode,
    #[serde(default)]
    pub chapters: Vec<MediaChapter>,
    #[serde(default)]
    pub normalize_audio: bool,
}
impl Default for MediaOptions {
    fn default() -> Self {
        Self {
            manifest_snapshot: None,
            manifest_video_id: None,
            manifest_audio_id: None,
            kind: None,
            provider: None,
            segment_urls: Vec::new(),
            audio_url: None,
            video_stream: None,
            audio_stream: None,
            duration_seconds: None,
            container: default_container(),
            subtitles: Vec::new(),
            subtitle_mode: SubtitleMode::None,
            chapters: Vec::new(),
            normalize_audio: false,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManifestSnapshot {
    pub url: String,
    pub text: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleMode {
    #[default]
    None,
    Embed,
    External,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SubtitleTrack {
    pub url: Option<String>,
    pub index: Option<u32>,
    pub language: String,
    pub title: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MediaChapter {
    pub start_ms: u64,
    pub end_ms: u64,
    pub title: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BrowserSource {
    pub page_url: String,
    #[serde(default)]
    pub frame_url: Option<String>,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportPreference {
    #[default]
    Auto,
    Tcp,
    Http3,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MediaProvider {
    pub url: String,
    pub format_id: Option<String>,
    pub height: Option<u64>,
    #[serde(default)]
    pub audio_format_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct TorrentOptions {
    pub seeding: Option<crate::seeding::Rule>,
    pub upload_limit: u64,
    pub peer_limit: u16,
    pub only_files: Vec<usize>,
    pub initial_peers: Vec<std::net::SocketAddr>,
}
impl Default for TorrentOptions {
    fn default() -> Self {
        Self {
            seeding: None,
            upload_limit: 0,
            peer_limit: 80,
            only_files: Vec::new(),
            initial_peers: Vec::new(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentFile {
    pub index: usize,
    pub path: String,
    pub length: u64,
    pub selected: bool,
    pub sha256: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentSnapshot {
    #[serde(default)]
    pub seeded_seconds: u64,
    pub info_hash: String,
    pub state: String,
    pub peers: u32,
    pub uploaded: u64,
    pub upload_bps: u64,
    pub files: Vec<TorrentFile>,
    #[serde(default)]
    pub file_count: usize,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BrowserCredentials {
    pub origin: String,
    pub cookie: Option<String>,
    pub authorization: Option<String>,
    pub user_agent: Option<String>,
}
impl std::fmt::Debug for BrowserCredentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BrowserCredentials")
            .field("origin", &self.origin)
            .finish_non_exhaustive()
    }
}
fn default_container() -> String {
    "mp4".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub seeding: crate::seeding::Rule,
    pub torrent_upload_limit: u64,
    pub concurrent_tasks: u8,
    pub connections: u8,
    pub speed_limit: u64,
    pub queue_enabled: bool,
    pub clipboard_monitor: bool,
    pub minimize_to_tray: bool,
    pub notifications: bool,
    pub locale: String,
    pub download_directory: Option<PathBuf>,
    pub completion_action: crate::ShutdownAction,
    pub auto_categorize: bool,
    pub category_folders: crate::CategoryFolders,
    pub clipboard_mode: crate::ClipboardMode,
    pub launch_at_startup: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            seeding: crate::seeding::Rule::default(),
            torrent_upload_limit: 0,
            concurrent_tasks: 3,
            connections: default_connections(),
            speed_limit: 0,
            queue_enabled: true,
            clipboard_monitor: false,
            minimize_to_tray: true,
            notifications: true,
            locale: crate::locale::system().into(),
            download_directory: None,
            completion_action: crate::ShutdownAction::None,
            auto_categorize: false,
            category_folders: crate::CategoryFolders::default(),
            clipboard_mode: crate::ClipboardMode::Approval,
            launch_at_startup: false,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> anyhow::Result<()> {
        self.seeding.validate()?;
        anyhow::ensure!(
            self.torrent_upload_limit <= i32::MAX as u64,
            "Torrent upload limit exceeds two GiB per second"
        );
        self.category_folders.validate()?;
        anyhow::ensure!(
            (1..=8).contains(&self.concurrent_tasks),
            "Concurrent tasks must be between 1 and 8"
        );
        anyhow::ensure!(
            self.connections <= 32,
            "Connections must be automatic (0) or between 1 and 32"
        );
        anyhow::ensure!(
            [
                "en-US", "zh-CN", "hi-IN", "es-ES", "ar-SA", "fr-FR", "bn-BD", "pt-BR", "ru-RU",
                "ur-PK", "id-ID", "de-DE", "ja-JP", "pcm-NG", "mr-IN", "te-IN", "tr-TR", "ta-IN",
                "yue-HK", "vi-VN"
            ]
            .contains(&self.locale.as_str()),
            "Unsupported language"
        );
        if let Some(path) = &self.download_directory {
            anyhow::ensure!(path.is_absolute(), "Download directory must be absolute");
        }
        Ok(())
    }
}

pub(crate) fn validate_url(value: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !value.chars().any(char::is_control),
        "Control characters in URLs are forbidden"
    );
    let url = url::Url::parse(value)?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
        "Only HTTP(S) URLs are supported"
    );
    anyhow::ensure!(
        url.username().is_empty() && url.password().is_none(),
        "Credentials in URLs are forbidden"
    );
    anyhow::ensure!(value.len() <= 16384, "URL is too long");
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    #[serde(default)]
    pub created_at: Option<u64>,
    #[serde(default)]
    pub started_at: Option<u64>,
    #[serde(default)]
    pub finished_at: Option<u64>,
    #[serde(default)]
    pub timestamp_estimated: bool,
    #[serde(serialize_with = "redact_request")]
    pub request: DownloadRequest,
    pub status: Status,
    pub total: Option<u64>,
    #[serde(default)]
    pub total_estimated: bool,
    #[serde(default)]
    pub media_progress: Option<MediaProgress>,
    pub completed: u64,
    pub etag: Option<String>,
    pub ranged: bool,
    pub sha256: Option<String>,
    pub error: Option<String>,
    pub destination: PathBuf,
    #[serde(default)]
    pub automatic_destination: bool,
    #[serde(default)]
    pub source_validated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_url: Option<String>,
    #[serde(default)]
    pub media_outputs: Vec<MediaOutput>,
    #[serde(default)]
    pub direct_write: bool,
    #[serde(default)]
    pub torrent: Option<TorrentSnapshot>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaOutput {
    pub destination: PathBuf,
    pub sha256: String,
}
impl Task {
    pub fn summary(mut self) -> Self {
        if let Some(torrent) = &mut self.torrent {
            torrent.file_count = torrent.file_count.max(torrent.files.len());
            torrent.files.clear();
        }
        self
    }
}
fn redact_request<S: serde::Serializer>(
    request: &DownloadRequest,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut public = request.clone();
    public.credentials.clear();
    public.protected_credentials = None;
    public.serialize(serializer)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub id: String,
    pub completed: u64,
    pub total: Option<u64>,
    pub bytes_per_second: u64,
    #[serde(default)]
    pub total_estimated: bool,
    #[serde(default)]
    pub media: Option<MediaProgress>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaProgress {
    pub duration_ms: Option<u64>,
    pub processed_ms: u64,
    pub segments_total: Option<u64>,
    pub segments_completed: u64,
    pub percent: Option<f64>,
    pub eta_seconds: Option<u64>,
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Part {
    pub index: u32,
    pub start: u64,
    pub end: u64,
    pub done: u64,
    pub digest: Option<String>,
}
impl Part {
    pub fn len(&self) -> u64 {
        self.end - self.start + 1
    }
}

pub(crate) fn validate(request: &DownloadRequest) -> anyhow::Result<()> {
    if let Some(mime) = &request.content_type {
        anyhow::ensure!(
            mime.len() <= 256 && !mime.chars().any(char::is_control),
            "Invalid content type"
        );
    }
    anyhow::ensure!(
        serde_json::to_vec(request)?.len() <= crate::protocol::MAX_FRAME - 48 * 1024,
        "Download request exceeds the protocol size limit"
    );
    if crate::torrent::is_torrent(request) {
        crate::torrent::validate_request(request)?;
    } else {
        validate_url(&request.url)?;
    }
    anyhow::ensure!(
        request.credentials.len() <= 8,
        "At most eight authenticated origins are supported"
    );
    let mut origins = std::collections::HashSet::new();
    anyhow::ensure!(
        serde_json::to_vec(&request.credentials)?.len() <= 65536,
        "Authentication exceeds the total size limit"
    );
    for credentials in &request.credentials {
        anyhow::ensure!(
            origins.insert(&credentials.origin),
            "Duplicate authenticated origin"
        );
        anyhow::ensure!(
            [
                &credentials.cookie,
                &credentials.authorization,
                &credentials.user_agent
            ]
            .into_iter()
            .flatten()
            .map(String::len)
            .sum::<usize>()
                <= 16384,
            "Authentication headers exceed the origin limit"
        );
        validate_url(&credentials.origin)?;
        anyhow::ensure!(
            url::Url::parse(&credentials.origin)?
                .origin()
                .ascii_serialization()
                == credentials.origin,
            "Credentials must be scoped to a canonical origin"
        );
        for value in [
            &credentials.cookie,
            &credentials.authorization,
            &credentials.user_agent,
        ]
        .into_iter()
        .flatten()
        {
            anyhow::ensure!(
                value.len() <= 16384 && !value.chars().any(char::is_control),
                "Invalid authentication header"
            );
        }
    }
    anyhow::ensure!(
        (-10..=10).contains(&request.priority),
        "Priority must be between -10 and 10"
    );
    if let (Some(start), Some(stop)) = (request.start_at, request.stop_at) {
        anyhow::ensure!(stop > start, "Stop time must be later than start time");
    }
    if let Some(referer) = &request.referer {
        validate_url(referer)?;
    }
    if let Some(source) = &request.browser_source {
        validate_url(&source.page_url)?;
        if let Some(frame) = &source.frame_url {
            validate_url(frame)?;
        }
    }
    if let Some(media) = &request.media {
        for id in [&media.manifest_video_id, &media.manifest_audio_id]
            .into_iter()
            .flatten()
        {
            anyhow::ensure!(
                !id.is_empty()
                    && id.len() <= 128
                    && id
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')),
                "Invalid manifest representation ID"
            );
        }
        if let Some(snapshot) = &media.manifest_snapshot {
            validate_url(&snapshot.url)?;
            anyhow::ensure!(
                snapshot.text.len() <= crate::manifest::MAX_MANIFEST,
                "Manifest snapshot exceeds 512 KiB"
            );
            anyhow::ensure!(
                snapshot.text.trim_start().starts_with("#EXTM3U")
                    || snapshot.text.trim_start().starts_with('<'),
                "Invalid manifest snapshot"
            );
        }
        if let Some(provider) = &media.provider {
            validate_url(&provider.url)?;
            anyhow::ensure!(
                crate::extractor::is_provider_url(&provider.url),
                "Unsupported media provider"
            );
            anyhow::ensure!(
                provider.format_id.as_ref().is_none_or(|id| !id.is_empty()
                    && id.len() <= 128
                    && id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "-_+".contains(c))),
                "Invalid provider format"
            );
            anyhow::ensure!(
                provider
                    .height
                    .is_none_or(|height| (1..=16384).contains(&height)),
                "Invalid provider resolution"
            );
        }
        anyhow::ensure!(
            media.segment_urls.len() <= 500,
            "At most 500 explicit media segments are supported"
        );
        anyhow::ensure!(
            media
                .kind
                .as_deref()
                .is_none_or(|kind| matches!(kind, "file" | "hls" | "dash")),
            "Invalid media kind"
        );
        for segment in &media.segment_urls {
            validate_url(segment)?;
        }
        anyhow::ensure!(
            matches!(media.container.as_str(), "mp4" | "mkv" | "mp3" | "mka"),
            "Media container must be mp4, mkv, mp3 or mka"
        );
        anyhow::ensure!(
            media.subtitles.len() <= 32,
            "At most 32 subtitle tracks are supported"
        );
        anyhow::ensure!(media.chapters.len() <= 10000, "Too many chapters");
        anyhow::ensure!(
            !media.normalize_audio || media.container != "mka",
            "Lossless audio extraction cannot normalize audio"
        );
        anyhow::ensure!(
            media.subtitle_mode != SubtitleMode::Embed
                || !matches!(media.container.as_str(), "mp3" | "mka"),
            "Subtitles cannot be embedded in audio-only output"
        );
        let mut tracks = std::collections::HashSet::new();
        for track in &media.subtitles {
            anyhow::ensure!(
                track.url.is_some() != track.index.is_some(),
                "Subtitle must have exactly one source"
            );
            if let Some(url) = &track.url {
                validate_url(url)?;
            }
            anyhow::ensure!(
                track.index.is_none_or(|index| index <= 4096),
                "Subtitle index is too large"
            );
            anyhow::ensure!(
                track.language.len() <= 32
                    && track
                        .language
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "Invalid subtitle language"
            );
            anyhow::ensure!(
                track.title.len() <= 256 && !track.title.chars().any(char::is_control),
                "Invalid subtitle title"
            );
            anyhow::ensure!(
                tracks.insert((track.url.clone(), track.index)),
                "Duplicate subtitle source"
            );
        }
        let mut end = 0;
        for chapter in &media.chapters {
            anyhow::ensure!(
                chapter.start_ms >= end
                    && chapter.end_ms > chapter.start_ms
                    && chapter.end_ms <= 7 * 86400 * 1000,
                "Invalid chapter timeline"
            );
            anyhow::ensure!(
                chapter.title.len() <= 512 && !chapter.title.chars().any(char::is_control),
                "Invalid chapter title"
            );
            end = chapter.end_ms;
        }
        if let Some(audio) = &media.audio_url {
            validate_url(audio)?;
        }
        if let Some(duration) = media.duration_seconds {
            anyhow::ensure!(
                (1..=86400).contains(&duration),
                "Recording duration must be between 1 and 86400 seconds"
            );
        }
        for stream in [media.video_stream, media.audio_stream]
            .into_iter()
            .flatten()
        {
            anyhow::ensure!(stream <= 4096, "Stream index is too large");
        }
    }
    anyhow::ensure!(
        request.connections <= 32,
        "Connections must be automatic (0) or between 1 and 32"
    );
    let name = &request.filename;
    anyhow::ensure!(
        !name.is_empty()
            && name.len() <= 200
            && !name.ends_with(['.', ' '])
            && !name
                .chars()
                .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
            && name != "."
            && name != "..",
        "Invalid filename"
    );
    let stem = name.split('.').next().unwrap_or_default().to_uppercase();
    anyhow::ensure!(
        ![
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"
        ]
        .contains(&stem.as_str()),
        "Reserved filename"
    );
    if let Some(hash) = &request.expected_sha256 {
        anyhow::ensure!(
            hash.len() == 64 && hex::decode(hash).is_ok(),
            "SHA-256 must contain 64 hexadecimal characters"
        );
    }
    Ok(())
}
