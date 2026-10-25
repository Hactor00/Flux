use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SourceInfo {
    pub size: Option<u64>,
    pub content_type: String,
    pub files: Vec<crate::TorrentFile>,
}
