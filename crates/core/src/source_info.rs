use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SourceInfo {
    pub size: Option<u64>,
    pub estimated: bool,
    pub content_type: String,
    pub files: Vec<crate::TorrentFile>,
}

#[derive(Default)]
pub(crate) struct MediaSizes(std::sync::Mutex<std::collections::VecDeque<MediaSize>>);

struct MediaSize {
    url: String,
    audio: Option<String>,
    bytes: u64,
    observed: std::time::Instant,
}

impl MediaSizes {
    pub fn remember(&self, media: &crate::extractor::ResolvedMedia) {
        let mut sizes = self.0.lock().unwrap_or_else(|e| e.into_inner());
        sizes.retain(|size| size.observed.elapsed() < std::time::Duration::from_secs(300));
        for format in &media.formats {
            if let Some(bytes) = format.size.filter(|bytes| *bytes > 0) {
                sizes.push_back(MediaSize {
                    url: format.url.clone(),
                    audio: format.audio_url.clone(),
                    bytes,
                    observed: std::time::Instant::now(),
                });
                while sizes.len() > 256 {
                    sizes.pop_front();
                }
            }
        }
    }

    pub fn get(&self, request: &crate::DownloadRequest) -> Option<u64> {
        let media = request.media.as_ref()?;
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .rev()
            .find(|size| {
                size.url == request.url
                    && size.audio == media.audio_url
                    && size.observed.elapsed() < std::time::Duration::from_secs(300)
            })
            .map(|size| size.bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_sizes_match_both_streams_and_expire() {
        let cache = MediaSizes::default();
        let metadata: crate::extractor::ResolvedMedia = serde_json::from_value(serde_json::json!({
            "title":"Video", "duration":10, "formats":[{
                "id":"137", "url":"https://cdn.example/video", "audio_url":"https://cdn.example/audio",
                "height":1080, "fps":30, "audio_only":false, "audio_codec":"aac", "codec":"h264",
                "size":706111078, "container":"mp4"
            }]
        })).unwrap();
        cache.remember(&metadata);
        let mut request = crate::DownloadRequest {
            url: "https://cdn.example/video".into(),
            media: Some(crate::MediaOptions {
                audio_url: Some("https://cdn.example/audio".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(cache.get(&request), Some(706111078));
        request.media.as_mut().unwrap().audio_url = Some("https://cdn.example/other-audio".into());
        assert_eq!(cache.get(&request), None);
        request.media.as_mut().unwrap().audio_url = Some("https://cdn.example/audio".into());
        cache.0.lock().unwrap()[0].observed -= std::time::Duration::from_secs(301);
        assert_eq!(cache.get(&request), None);
        for _ in 0..300 {
            cache.remember(&metadata);
        }
        assert_eq!(cache.0.lock().unwrap().len(), 256);
    }
}
