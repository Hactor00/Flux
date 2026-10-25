mod batch;
mod bencode_guard;
pub mod browser_connection;
pub mod browser_identity;
pub mod debrid;
mod disk;
mod disk_writer;
mod engine;
pub mod extractor;
pub mod filename_prefix;
mod healing;
mod http;
mod limiter;
pub mod locale;
mod manifest;
mod media;
mod media_assets;
mod media_cache;
mod media_progress;
mod model;
pub mod portable;
pub mod preferences;
pub mod protocol;
pub mod release;
mod secrets;
pub mod seeding;
pub mod signing;
pub mod source_info;
mod store;
mod torrent;
mod torrent_native;
mod torrent_watch;
mod transport;
mod watchdog;

pub use batch::expand_batch;
pub use engine::Engine;
pub use healing::RefreshRequest;
pub use media::{inspect_media, MediaInfo, MediaStream};
pub use model::{
    BrowserCredentials, BrowserSource, DownloadRequest, ManifestSnapshot, MediaChapter,
    MediaOptions, MediaProgress, MediaProvider, Progress, Settings, Status, SubtitleMode,
    SubtitleTrack, Task, TorrentFile, TorrentOptions, TorrentSnapshot, TransportPreference,
};
pub use preferences::{CategoryFolders, ClipboardMode, ShutdownAction};
pub use torrent_watch::{TorrentWatch, WatchMode, WatchReport};
pub fn bundled_runtime(name: &str) -> Option<std::path::PathBuf> {
    let current = std::env::current_exe().ok()?;
    let parent = current.parent()?;
    let candidates = vec![parent.join(name), parent.join("resources").join(name)];
    #[cfg(target_os = "linux")]
    let candidates = candidates
        .into_iter()
        .chain([std::path::Path::new("/usr/lib/flux").join(name)]);
    candidates.into_iter().find(|path| path.is_file())
}
pub fn default_data_directory() -> anyhow::Result<std::path::PathBuf> {
    if let Some(root) = portable::root() {
        return Ok(root.join("Data/Flux"));
    }
    #[cfg(windows)]
    let directory = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
    #[cfg(not(windows))]
    let directory = dirs::data_local_dir();
    directory
        .map(|directory| directory.join("Flux"))
        .ok_or_else(|| anyhow::anyhow!("Local application data directory is unavailable"))
}
pub fn protect_credentials(
    data: &std::path::Path,
    credentials: &[BrowserCredentials],
) -> anyhow::Result<String> {
    secrets::Secrets::open(data)?.protect_credentials(credentials)
}
