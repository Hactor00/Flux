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
    pub audio_tracks: Vec<ResolvedAudio>,
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
    pub fps: Option<f64>,
    pub audio_only: bool,
    pub audio_codec: String,
    pub codec: String,
    pub size: Option<u64>,
    pub container: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ResolvedAudio {
    pub id: String,
    pub url: String,
    pub codec: String,
    pub language: Option<String>,
    pub bitrate: Option<f64>,
    pub size: Option<u64>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct CollectionEntry {
    pub url: String,
    pub title: String,
    pub duration: Option<f64>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ResolvedCollection {
    pub title: String,
    pub entries: Vec<CollectionEntry>,
    pub next_offset: usize,
    pub has_more: bool,
    pub total: Option<usize>,
}
pub async fn resolve(url: &str) -> Result<ResolvedMedia> {
    resolve_with_session(url, &[]).await
}
pub async fn resolve_with_session(
    url: &str,
    credentials: &[crate::BrowserCredentials],
) -> Result<ResolvedMedia> {
    crate::model::validate_url(url)?;
    ensure!(
        is_provider_url(url),
        "Provider is not supported by this resolver"
    );
    let output = run_resolver(url, &["--no-playlist"], credentials).await?;
    parse(&output)
}
struct CookieJar {
    directory: std::path::PathBuf,
    path: std::path::PathBuf,
}
impl Drop for CookieJar {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir(&self.directory);
    }
}
fn cookie_lines(url: &str, credentials: &[crate::BrowserCredentials]) -> Result<String> {
    let target = url::Url::parse(url)?;
    let Some(credential) = credentials
        .iter()
        .find(|item| item.origin == target.origin().ascii_serialization())
    else {
        return Ok(String::new());
    };
    let Some(cookie) = &credential.cookie else {
        return Ok(String::new());
    };
    ensure!(
        cookie.len() <= 32768 && !cookie.contains(['\r', '\n', '\0']),
        "Invalid session cookies"
    );
    let host = target.host_str().context("Missing session host")?;
    let secure = if target.scheme() == "https" {
        "TRUE"
    } else {
        "FALSE"
    };
    let mut lines = String::from("# Netscape HTTP Cookie File\n");
    for pair in cookie.split(';').take(300) {
        let Some((name, value)) = pair.trim().split_once('=') else {
            continue;
        };
        if name.is_empty()
            || name.contains(['\t', ' ', '\r', '\n'])
            || value.contains(['\t', '\r', '\n'])
        {
            continue;
        }
        lines.push_str(&format!("{host}\tFALSE\t/\t{secure}\t0\t{name}\t{value}\n"));
    }
    Ok(lines)
}
fn session_jar(url: &str, credentials: &[crate::BrowserCredentials]) -> Result<Option<CookieJar>> {
    use std::io::Write;
    let lines = cookie_lines(url, credentials)?;
    if lines.is_empty() {
        return Ok(None);
    }
    let directory = std::env::temp_dir().join(format!("flux-session-{}", uuid::Uuid::new_v4()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
    }
    #[cfg(not(unix))]
    std::fs::create_dir(&directory)?;
    let jar = CookieJar {
        path: directory.join("cookies.txt"),
        directory,
    };
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&jar.path)?;
    file.write_all(lines.as_bytes())?;
    file.flush()?;
    Ok(Some(jar))
}
async fn run_resolver(
    url: &str,
    selection: &[&str],
    credentials: &[crate::BrowserCredentials],
) -> Result<Value> {
    static RESOLVERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
    let _permit = tokio::time::timeout(Duration::from_secs(30), RESOLVERS.acquire())
        .await
        .context("Media resolver is busy")??;
    let mut command = crate::media::command("yt-dlp");
    let jar = session_jar(url, credentials)?;
    if let Some(jar) = &jar {
        command.arg("--cookies").arg(&jar.path);
    }
    let node = crate::media::executable("node");
    command
        .args([
            "--ignore-config",
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
        .args(selection)
        .arg("--")
        .arg(url);
    let mut child = command.spawn().context("Media resolver could not start")?;
    let stdout = child.stdout.take().context("Resolver output unavailable")?;
    let stderr = child
        .stderr
        .take()
        .context("Resolver diagnostics unavailable")?;
    let result = tokio::time::timeout(Duration::from_secs(90), async {
        let (status, output, diagnostics) = tokio::join!(
            child.wait(),
            crate::media::bounded_output(stdout, 8 * 1024 * 1024),
            crate::media::bounded_output(stderr, 4096)
        );
        anyhow::Ok((status?, output?, diagnostics?))
    })
    .await;
    let (status, output, diagnostics) = match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(error);
        }
        Err(error) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(error).context("Media resolution timed out");
        }
    };
    ensure!(
        status.success(),
        "{}",
        String::from_utf8_lossy(&diagnostics)
    );
    serde_json::from_slice(&output).context("Invalid resolver metadata")
}
pub fn is_provider_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|parsed| {
        matches!(parsed.scheme(), "https" | "http")
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.host_str().is_some_and(|host| {
                [
                    "youtube.com",
                    "youtube-nocookie.com",
                    "youtu.be",
                    "vimeo.com",
                    "dailymotion.com",
                    "dai.ly",
                    "twitch.tv",
                    "soundcloud.com",
                    "bandcamp.com",
                    "facebook.com",
                    "fb.watch",
                    "instagram.com",
                    "tiktok.com",
                    "reddit.com",
                    "redd.it",
                    "twitter.com",
                    "x.com",
                    "bilibili.com",
                    "ted.com",
                    "archive.org",
                ]
                .iter()
                .any(|provider| host == *provider || host.ends_with(&format!(".{provider}")))
            })
    })
}
pub async fn resolve_collection(
    url: &str,
    offset: usize,
    limit: usize,
) -> Result<ResolvedCollection> {
    resolve_collection_with_session(url, offset, limit, &[]).await
}
pub async fn resolve_collection_with_session(
    url: &str,
    offset: usize,
    limit: usize,
    credentials: &[crate::BrowserCredentials],
) -> Result<ResolvedCollection> {
    crate::model::validate_url(url)?;
    ensure!(is_provider_url(url), "Unsupported collection provider");
    ensure!(
        (1..=50).contains(&limit) && offset < 500 && offset + limit <= 500,
        "Collection page must contain 1 to 50 entries within the first 500 items"
    );
    let range = format!("{}:{}", offset + 1, offset + limit + 1);
    let info = run_resolver(
        url,
        &[
            "--yes-playlist",
            "--flat-playlist",
            "--playlist-items",
            &range,
        ],
        credentials,
    )
    .await?;
    parse_collection(&info, offset, limit)
}
fn text(value: &Value, fallback: &str, limit: usize) -> String {
    value
        .as_str()
        .unwrap_or(fallback)
        .chars()
        .filter(|c| !c.is_control())
        .take(limit)
        .collect()
}
fn duration(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0 && *seconds <= 7.0 * 86400.0)
}
fn parse_collection(info: &Value, offset: usize, limit: usize) -> Result<ResolvedCollection> {
    let source = info["entries"]
        .as_array()
        .context("No collection entries available")?;
    let mut entries = Vec::new();
    let mut urls = std::collections::HashSet::new();
    let mut bytes = 2048usize;
    let mut examined = 0;
    for entry in source.iter().take(limit) {
        let selected = entry["webpage_url"]
            .as_str()
            .or_else(|| entry["url"].as_str());
        let Some(url) =
            selected.filter(|url| crate::model::validate_url(url).is_ok() && is_provider_url(url))
        else {
            examined += 1;
            continue;
        };
        let item = CollectionEntry {
            url: url.into(),
            title: text(&entry["title"], "Media", 200),
            duration: duration(&entry["duration"]),
        };
        let size = serde_json::to_vec(&item)?.len();
        if bytes + size > crate::protocol::MAX_FRAME - 1024 {
            break;
        }
        examined += 1;
        if urls.insert(url.to_owned()) {
            entries.push(item);
            bytes += size;
        }
    }
    ensure!(
        examined > 0 || source.is_empty(),
        "Collection item exceeds protocol size limit"
    );
    let next_offset = offset + examined;
    Ok(ResolvedCollection {
        title: text(&info["title"], "Collection", 200),
        entries,
        next_offset,
        has_more: next_offset < 500 && source.len() > examined,
        total: info["playlist_count"]
            .as_u64()
            .and_then(|total| usize::try_from(total).ok()),
    })
}
fn parse(info: &Value) -> Result<ResolvedMedia> {
    let list = info["formats"]
        .as_array()
        .context("No media formats available")?;
    let mut audio_formats: Vec<_> = list
        .iter()
        .filter(|f| {
            f["vcodec"] == "none"
                && f["has_drm"] != true
                && f["acodec"].as_str().is_some_and(|codec| codec != "none")
                && f["url"]
                    .as_str()
                    .is_some_and(|url| crate::model::validate_url(url).is_ok())
        })
        .collect();
    audio_formats.sort_by_key(|f| {
        (
            (f["protocol"] == "https") as u8,
            (f["ext"] == "m4a") as u8,
            (!f["format_note"].as_str().unwrap_or("").contains("DRC")) as u8,
            f["abr"].as_f64().unwrap_or(0.0) as u64,
        )
    });
    let audio = audio_formats.last().copied();
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
                    fps: duration(&item["fps"]).filter(|fps| *fps <= 1000.0),
                    audio_only: false,
                    audio_codec: if item["acodec"] == "none" {
                        audio
                            .map(|f| text(&f["acodec"], "", 128))
                            .unwrap_or_default()
                    } else {
                        text(&item["acodec"], "", 128)
                    },
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
    let mut formats: Vec<_> = grouped
        .into_iter()
        .rev()
        .take(40)
        .map(|(height, (_, mut format))| {
            format.alternative_ids = aliases.remove(&height).unwrap_or_default();
            format
        })
        .collect();
    if formats.is_empty() {
        if let Some(item) = audio {
            formats.push(ResolvedFormat {
                id: text(&item["format_id"], "audio", 128),
                alternative_ids: Vec::new(),
                url: item["url"].as_str().context("Missing audio URL")?.into(),
                audio_url: None,
                height: None,
                fps: None,
                audio_only: true,
                audio_codec: text(&item["acodec"], "", 128),
                codec: text(&item["acodec"], "", 128),
                size: item["filesize"]
                    .as_u64()
                    .or(item["filesize_approx"].as_u64()),
                container: "mp3".into(),
            });
        }
    }
    ensure!(
        !formats.is_empty(),
        "No playable unencrypted media formats found"
    );
    let mut result = ResolvedMedia {
        audio_tracks: audio_formats
            .iter()
            .rev()
            .take(32)
            .map(|item| ResolvedAudio {
                id: text(&item["format_id"], "", 128),
                url: item["url"].as_str().unwrap_or_default().into(),
                codec: text(&item["acodec"], "", 128),
                language: item["language"].as_str().map(|value| {
                    value
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                        .take(32)
                        .collect()
                }),
                bitrate: duration(&item["abr"]),
                size: item["filesize"]
                    .as_u64()
                    .or(item["filesize_approx"].as_u64()),
            })
            .collect(),
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
        title: text(&info["title"], "Media", 200),
        duration: duration(&info["duration"]),
        formats,
    };
    while serde_json::to_vec(&result)?.len() > crate::protocol::MAX_FRAME - 2048 {
        if !result.chapters.is_empty() {
            result.chapters.pop();
        } else if !result.audio_tracks.is_empty() {
            result.audio_tracks.pop();
        } else if result.formats.len() > 1 {
            result.formats.pop();
        } else if !result.subtitles.is_empty() {
            result.subtitles.pop();
        } else {
            anyhow::bail!("Media metadata exceeds protocol size limit");
        }
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    #[test]
    fn provider_boundaries_and_audio_only_sources_are_supported() {
        assert!(super::is_provider_url("https://player.vimeo.com/video/123"));
        assert!(super::is_provider_url(
            "https://soundcloud.com/artist/track"
        ));
        assert!(!super::is_provider_url(
            "https://youtube.com.attacker.test/watch"
        ));
        assert!(!super::is_provider_url(
            "https://user:password@youtube.com/watch"
        ));
        let result=super::parse(&serde_json::json!({"title":"Track","formats":[{"format_id":"aac","vcodec":"none","acodec":"aac","url":"https://cdn.example/track","ext":"m4a","abr":128}]})).unwrap();
        assert!(result.formats[0].audio_only);
        assert_eq!(result.formats[0].container, "mp3");
        assert_eq!(result.audio_tracks[0].id, "aac");
    }
    #[test]
    fn collection_pagination_deduplicates_and_rejects_non_provider_entries() {
        let info = serde_json::json!({"title":"Channel", "playlist_count":5,"entries":[
            {"url":"https://www.youtube.com/watch?v=1","title":"One"},
            {"url":"https://www.youtube.com/watch?v=1","title":"Repeated"},
            {"url":"file:///private","title":"Unsafe"},
            {"url":"https://vimeo.com/123","title":"Four"}
        ]});
        let page = super::parse_collection(&info, 10, 3).unwrap();
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.next_offset, 13);
        assert!(page.has_more);
        assert_eq!(page.total, Some(5));
    }
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
