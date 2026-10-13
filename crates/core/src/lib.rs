mod batch;
mod disk;
mod engine;
pub mod extractor;
mod healing;
mod http;
mod limiter;
mod media;
mod media_assets;
mod media_cache;
mod media_progress;
mod model;
pub mod preferences;
pub mod protocol;
mod secrets;
mod store;
mod torrent;
mod transport;

pub use batch::expand_batch;
pub use engine::Engine;
pub use healing::RefreshRequest;
pub use media::{inspect_media, MediaInfo, MediaStream};
pub use model::{
    BrowserCredentials, BrowserSource, DownloadRequest, MediaChapter, MediaOptions, MediaProgress,
    MediaProvider, Progress, Settings, Status, SubtitleMode, SubtitleTrack, Task, TorrentFile,
    TorrentOptions, TorrentSnapshot, TransportPreference,
};
pub use preferences::{CategoryFolders, ClipboardMode, ShutdownAction};
pub fn default_data_directory() -> anyhow::Result<std::path::PathBuf> {
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
