use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
pub struct ResolvedMedia {
    pub title: String,
    pub duration: Option<f64>,
    pub formats: Vec<ResolvedFormat>,
    #[serde(default)]
    pub subtitles: Vec<crate::SubtitleTrack>,
    #[serde(default)]
    pub chapters: Vec<crate::MediaChapter>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ResolvedFormat {
    pub id: String,
    #[serde(default)]
    pub alternative_ids: Vec<String>,
    pub url: String,
    pub audio_url: Option<String>,
    pub height: Option<u64>,
    pub codec: String,
    pub size: Option<u64>,
    pub container: String,
}
pub async fn resolve(url: &str) -> Result<ResolvedMedia> {
    crate::model::validate_url(url)?;
    let parsed = url::Url::parse(url)?;
    ensure!(
        matches!(
            parsed.host_str(),
            Some(
                "youtube.com"
                    | "www.youtube.com"
                    | "m.youtube.com"
                    | "youtu.be"
                    | "www.youtube-nocookie.com"
            )
        ),
        "Provider is not supported by this resolver"
    );
    let mut command = crate::media::command("yt-dlp");
    let node = crate::media::executable("node");
    command
        .args([
            "--ignore-config",
            "--no-playlist",
            "--skip-download",
            "--dump-single-json",
            "--no-warnings",
            "--socket-timeout",
            "15",
            "--retries",
            "2",
            "--extractor-retries",
            "1",
            "--js-runtimes",
        ])
        .arg(format!("node:{}", node.to_string_lossy()))
        .arg("--")
        .arg(url);
    let mut child = command.spawn().context("Media resolver could not start")?;
    let stdout = child.stdout.take().context("Resolver output unavailable")?;
    let stderr = child
        .stderr
        .take()
        .context("Resolver diagnostics unavailable")?;
    let (status, output, diagnostics) = tokio::time::timeout(Duration::from_secs(90), async {
        let (status, output, diagnostics) = tokio::join!(
            child.wait(),
            crate::media::bounded_output(stdout, 8 * 1024 * 1024),
            crate::media::bounded_output(stderr, 4096)
        );
        anyhow::Ok((status?, output?, diagnostics?))
    })
    .await
    .context("Media resolution timed out")??;
    ensure!(
        status.success(),
        "{}",
        String::from_utf8_lossy(&diagnostics)
    );
    parse(&serde_json::from_slice(&output).context("Invalid resolver metadata")?)
}
pub fn is_provider_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|parsed| {
        matches!(
            parsed.host_str(),
            Some(
                "youtube.com"
                    | "www.youtube.com"
                    | "m.youtube.com"
                    | "youtu.be"
                    | "www.youtube-nocookie.com"
            )
        )
    })
}
fn parse(info: &Value) -> Result<ResolvedMedia> {
    let list = info["formats"]
        .as_array()
        .context("No media formats available")?;
    let audio = list
        .iter()
        .filter(|f| {
            f["vcodec"] == "none"
                && f["has_drm"] != true
                && f["acodec"].as_str().is_some_and(|codec| codec != "none")
                && f["url"]
                    .as_str()
                    .is_some_and(|url| crate::model::validate_url(url).is_ok())
        })
        .max_by_key(|f| {
            (
                (f["protocol"] == "https") as u8,
                (f["ext"] == "m4a") as u8,
                (!f["format_note"].as_str().unwrap_or("").contains("DRC")) as u8,
                f["abr"].as_f64().unwrap_or(0.0) as u64,
            )
        });
    let mut grouped = std::collections::BTreeMap::new();
    let mut aliases: std::collections::BTreeMap<Option<u64>, Vec<String>> =
        std::collections::BTreeMap::new();
    for item in list.iter().rev() {
        let Some(url) = item["url"].as_str() else {
            continue;
        };
        let video_codec = item["vcodec"].as_str().unwrap_or("none");
        if video_codec == "none"
            || crate::model::validate_url(url).is_err()
            || item["has_drm"] == true
        {
            continue;
        }
        let audio_url = if item["acodec"] == "none" {
            audio.and_then(|f| f["url"].as_str()).map(str::to_owned)
        } else {
            None
        };
        if item["acodec"] == "none" && audio_url.is_none() {
            continue;
        }
        let height = item["height"].as_u64();
        let id: String = item["format_id"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(128)
            .collect();
        aliases.entry(height).or_default().push(id.clone());
        // Prefer direct transfers and broadly supported H.264/AAC over repeated
        // client-specific HLS, AV1 and VP9 variants of the same resolution.
        let rank = (
            (item["protocol"] == "https") as u8,
            (video_codec.starts_with("avc") || video_codec == "h264") as u8,
            (item["ext"] == "mp4") as u8,
            item["fps"].as_f64().unwrap_or(0.0) as u64,
            item["tbr"].as_f64().unwrap_or(0.0) as u64,
        );
        if grouped
            .get(&height)
            .is_some_and(|(previous, _)| *previous >= rank)
        {
            continue;
        }
        grouped.insert(
            height,
            (
                rank,
                ResolvedFormat {
                    id,
                    alternative_ids: Vec::new(),
                    url: url.into(),
                    audio_url,
                    height,
                    codec: video_codec.chars().take(128).collect(),
                    size: item["filesize"]
                        .as_u64()
                        .or(item["filesize_approx"].as_u64())
                        .map(|size| {
                            size.saturating_add(if item["acodec"] == "none" {
                                audio.and_then(|f| f["filesize"].as_u64()).unwrap_or(0)
                            } else {
                                0
                            })
                        }),
                    container: if item["ext"] == "webm" { "mkv" } else { "mp4" }.into(),
                },
            ),
        );
    }
    let formats: Vec<_> = grouped
        .into_iter()
        .rev()
        .take(40)
        .map(|(height, (_, mut format))| {
            format.alternative_ids = aliases.remove(&height).unwrap_or_default();
            format
        })
        .collect();
    ensure!(
        !formats.is_empty(),
        "No playable unencrypted video formats found"
    );
    Ok(ResolvedMedia {
        subtitles: info["subtitles"]
            .as_object()
            .map(|languages| {
                languages
                    .iter()
                    .take(32)
                    .filter_map(|(language, tracks)| {
                        if language.len() > 32
                            || !language
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '-')
                        {
                            return None;
                        }
                        let tracks = tracks.as_array()?;
                        let selected = tracks
                            .iter()
                            .filter(|track| matches!(track["ext"].as_str(), Some("vtt" | "srt")))
                            .max_by_key(|track| (track["ext"] == "vtt") as u8)?;
                        let url = selected["url"].as_str()?;
                        crate::model::validate_url(url).ok()?;
                        Some(crate::SubtitleTrack {
                            url: Some(url.into()),
                            index: None,
                            language: language.clone(),
                            title: selected["name"]
                                .as_str()
                                .unwrap_or(language)
                                .chars()
                                .filter(|c| !c.is_control())
                                .take(128)
                                .collect(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        chapters: info["chapters"]
            .as_array()
            .map(|chapters| {
                chapters
                    .iter()
                    .take(10000)
                    .filter_map(|chapter| {
                        let start = chapter["start_time"].as_f64()?;
                        let end = chapter["end_time"].as_f64()?;
                        if !start.is_finite()
                            || !end.is_finite()
                            || start < 0.0
                            || end <= start
                            || end > 7.0 * 86400.0
                        {
                            return None;
                        }
                        Some(crate::MediaChapter {
                            start_ms: (start * 1000.0) as u64,
                            end_ms: (end * 1000.0) as u64,
                            title: chapter["title"]
                                .as_str()
                                .unwrap_or("")
                                .chars()
                                .filter(|c| !c.is_control())
                                .take(256)
                                .collect(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        title: info["title"]
            .as_str()
            .unwrap_or("Video")
            .chars()
            .take(200)
            .collect(),
        duration: info["duration"].as_f64(),
        formats,
    })
}
#[cfg(test)]
mod tests {
    #[test]
    fn pairs_video_with_real_audio_and_excludes_drm() {
        let info = serde_json::json!({"title":"Real video", "formats":[{"format_id":"audio","vcodec":"none","acodec":"aac","url":"https://cdn.example/audio","ext":"m4a","abr":128},{"format_id":"video","vcodec":"h264","acodec":"none","url":"https://cdn.example/video","height":1080,"ext":"mp4"},{"format_id":"drm","vcodec":"h264","url":"https://cdn.example/drm","has_drm":true}]});
        let result = super::parse(&info).unwrap();
        assert_eq!(result.formats.len(), 1);
        assert_eq!(
            result.formats[0].audio_url.as_deref(),
            Some("https://cdn.example/audio")
        );
        assert_eq!(result.formats[0].height, Some(1080));
    }
    #[test]
    fn one_choice_per_resolution_prefers_direct_compatible_video() {
        let info = serde_json::json!({"title":"Video", "formats":[
            {"format_id":"140","vcodec":"none","acodec":"aac","url":"https://cdn.example/audio","ext":"m4a","abr":128,"filesize":100},
            {"format_id":"137","vcodec":"avc1.640028","acodec":"none","url":"https://cdn.example/direct","height":1080,"ext":"mp4","protocol":"https","filesize":1000},
            {"format_id":"614","vcodec":"vp09.00.40.08","acodec":"none","url":"https://cdn.example/hls","height":1080,"ext":"mp4","protocol":"m3u8_native"},
            {"format_id":"399","vcodec":"av01.0.08M.08","acodec":"none","url":"https://cdn.example/av1","height":1080,"ext":"mp4","protocol":"https"},
            {"format_id":"136","vcodec":"avc1","acodec":"none","url":"https://cdn.example/720","height":720,"ext":"mp4","protocol":"https"}
        ]});
        let result = super::parse(&info).unwrap();
        assert_eq!(result.formats.len(), 2);
        assert_eq!(result.formats[0].id, "137");
        assert_eq!(result.formats[0].size, Some(1100));
        assert!(result.formats[0].alternative_ids.contains(&"614".into()));
        assert_eq!(result.formats[1].height, Some(720));
    }
}
