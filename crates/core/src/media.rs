use crate::{model, DownloadRequest};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader},
    process::Command,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaStream {
    pub index: u32,
    pub codec_type: String,
    pub codec_name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    #[serde(default)]
    pub fps: Option<f64>,
    pub language: Option<String>,
    pub title: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaInfo {
    pub streams: Vec<MediaStream>,
    pub duration_seconds: Option<f64>,
    pub format: String,
    pub subtitles: Vec<crate::SubtitleTrack>,
    pub chapters: Vec<crate::MediaChapter>,
}

pub(crate) fn executable(name: &str) -> PathBuf {
    let variable = format!("FLUX_{}", name.to_uppercase());
    if let Some(path) = std::env::var_os(variable) {
        return path.into();
    }
    let bundled_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    if let Some(path) = crate::bundled_runtime(&bundled_name) {
        return path;
    }
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            let file = parent.join(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.into()
            });
            if file.is_file() {
                return file;
            }
            let bundled = parent
                .join("resources")
                .join(file.file_name().unwrap_or_default());
            if bundled.is_file() {
                return bundled;
            }
        }
    }
    name.into()
}
pub(crate) fn command(name: &str) -> Command {
    let mut command = Command::new(executable(name));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    command
}
pub(crate) async fn bounded_output(
    mut pipe: impl AsyncRead + Unpin,
    limit: usize,
) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let size = pipe.read(&mut buffer).await?;
        if size == 0 {
            break;
        }
        let available = limit.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..size.min(available)]);
        // Drain the pipe even at the limit, so the subprocess never blocks on stderr.
    }
    Ok(output)
}
async fn bounded_tail_output(mut pipe: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>> {
    let mut output = VecDeque::with_capacity(limit);
    let mut buffer = [0; 8192];
    loop {
        let size = pipe.read(&mut buffer).await?;
        if size == 0 {
            break;
        }
        output.extend(&buffer[..size]);
        let excess = output.len().saturating_sub(limit);
        output.drain(..excess);
    }
    Ok(output.into())
}
fn input_options(command: &mut Command, referer: Option<&str>) {
    command.args([
        "-protocol_whitelist",
        "http,https,tcp,tls,crypto",
        "-rw_timeout",
        "300000000",
        "-user_agent",
        "Flux/1.0",
    ]);
    if let Some(referer) = referer {
        command.args(["-headers", &format!("Referer: {referer}\r\n")]);
    }
}
async fn hls_options(input: &mut Command, executable: &str) -> Result<()> {
    static PROBE_OPTIONS: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    static FFMPEG_OPTIONS: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    let options = if executable == "ffprobe" {
        &PROBE_OPTIONS
    } else {
        &FFMPEG_OPTIONS
    };
    let supported = options
        .get_or_try_init(|| async {
            let output = tokio::time::timeout(
                Duration::from_secs(10),
                command(executable)
                    .args(["-hide_banner", "-h", "demuxer=hls"])
                    .output(),
            )
            .await??;
            ensure!(
                output.status.success(),
                "Cannot inspect HLS demuxer options"
            );
            anyhow::Ok(String::from_utf8(output.stdout)?)
        })
        .await?;
    // The gateway accepts only HTTP(S) resources. A media segment's suffix is
    // not evidence of its container; some CDNs intentionally use .jpg names.
    for (option, value) in [
        ("allowed_extensions", "ALL"),
        ("allowed_segment_extensions", "ALL"),
        ("extension_picky", "0"),
        ("seg_max_retry", "2"),
    ] {
        if supported.contains(&format!("-{option} ")) {
            input.args([&format!("-{option}"), value]);
        }
    }
    Ok(())
}
pub async fn inspect_media(url: &str, referer: Option<&str>) -> Result<MediaInfo> {
    model::validate_url(url)?;
    if let Some(referer) = referer {
        model::validate_url(referer)?;
    }
    let mut command = command("ffprobe");
    command.args(["-v", "error", "-print_format", "json", "-show_entries", "stream=index,codec_name,codec_type,width,height,avg_frame_rate,r_frame_rate:stream_tags=language,title:format=duration,format_name:chapter=start_time,end_time:chapter_tags=title"]);
    input_options(&mut command, referer);
    hls_options(&mut command, "ffprobe").await?;
    command.arg(url);
    let mut child = command.spawn().context("FFprobe could not be started")?;
    let stdout = child.stdout.take().context("Missing FFprobe output")?;
    let stderr = child.stderr.take().context("Missing FFprobe errors")?;
    let result = tokio::time::timeout(Duration::from_secs(45), async {
        let (status, output, errors) = tokio::try_join!(
            async { Ok::<_, anyhow::Error>(child.wait().await?) },
            bounded_output(stdout, 4 * 1024 * 1024),
            bounded_output(stderr, 16 * 1024)
        )?;
        ensure!(
            status.success(),
            "FFprobe failed: {}",
            String::from_utf8_lossy(&errors)
        );
        let value: serde_json::Value = serde_json::from_slice(&output)?;
        let entries = value["streams"]
            .as_array()
            .context("Media contains no streams")?;
        ensure!(entries.len() <= 256, "Media contains too many streams");
        let streams: Vec<MediaStream> = entries
            .iter()
            .map(|stream| MediaStream {
                index: stream["index"].as_u64().unwrap_or(0) as u32,
                codec_type: stream["codec_type"].as_str().unwrap_or("unknown").into(),
                codec_name: stream["codec_name"].as_str().unwrap_or("unknown").into(),
                width: stream["width"].as_u64().map(|v| v as u32),
                height: stream["height"].as_u64().map(|v| v as u32),
                fps: ["avg_frame_rate", "r_frame_rate"]
                    .into_iter()
                    .find_map(|key| {
                        let (numerator, denominator) = stream[key].as_str()?.split_once('/')?;
                        let rate =
                            numerator.parse::<f64>().ok()? / denominator.parse::<f64>().ok()?;
                        (rate.is_finite() && rate > 0.0 && rate <= 1000.0).then_some(rate)
                    }),
                language: stream["tags"]["language"]
                    .as_str()
                    .map(|value| value.chars().take(128).collect()),
                title: stream["tags"]["title"].as_str().map(|value| {
                    value
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(128)
                        .collect()
                }),
            })
            .collect();
        Ok(MediaInfo {
            subtitles: streams
                .iter()
                .filter(|stream| {
                    stream.codec_type == "subtitle"
                        && matches!(
                            stream.codec_name.as_str(),
                            "webvtt" | "subrip" | "mov_text" | "text" | "ass" | "ssa"
                        )
                })
                .map(|stream| crate::SubtitleTrack {
                    url: None,
                    index: Some(stream.index),
                    language: stream
                        .language
                        .clone()
                        .filter(|s| {
                            s.len() <= 32
                                && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                        })
                        .unwrap_or_else(|| "und".into()),
                    title: stream
                        .title
                        .clone()
                        .unwrap_or_else(|| stream.codec_name.clone()),
                })
                .collect(),
            chapters: value["chapters"]
                .as_array()
                .map(|chapters| {
                    chapters
                        .iter()
                        .take(10000)
                        .filter_map(|chapter| {
                            let start = chapter["start_time"].as_str()?.parse::<f64>().ok()?;
                            let end = chapter["end_time"].as_str()?.parse::<f64>().ok()?;
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
                                title: chapter["tags"]["title"]
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
            streams,
            duration_seconds: value["format"]["duration"]
                .as_str()
                .and_then(|v| v.parse().ok()),
            format: value["format"]["format_name"]
                .as_str()
                .unwrap_or("unknown")
                .into(),
        })
    })
    .await
    .context("Media inspection timed out")?;
    if result.is_err() {
        let _ = child.kill().await;
    }
    result
}

pub(crate) fn is_media(request: &DownloadRequest) -> bool {
    request.media.is_some()
        || url::Url::parse(&request.url).is_ok_and(|url| {
            let path = url.path().to_ascii_lowercase();
            path.ends_with(".m3u8") || path.ends_with(".mpd")
        })
}

pub(crate) async fn assemble(
    request: &DownloadRequest,
    output: &Path,
    cancel: CancellationToken,
    output_bytes: Arc<AtomicU64>,
    output_time: Arc<AtomicU64>,
    concat: Option<&Path>,
    gateway: &crate::media_cache::Gateway,
) -> Result<()> {
    let mut command = command("ffmpeg");
    command.args([
        "-hide_banner",
        "-loglevel",
        "warning",
        "-nostdin",
        "-n",
        "-progress",
        "pipe:1",
        "-nostats",
    ]);
    if let Some(path) = concat {
        command.current_dir(path.parent().context("Missing segment directory")?);
        command
            .args([
                "-protocol_whitelist",
                "file",
                "-f",
                "concat",
                "-safe",
                "1",
                "-i",
            ])
            .arg(path.file_name().context("Missing segment descriptor")?);
    } else {
        input_options(&mut command, request.referer.as_deref());
        if gateway.is_hls_input(&request.url).await? {
            hls_options(&mut command, "ffmpeg").await?;
        }
        command.args([
            "-reconnect",
            "1",
            "-reconnect_streamed",
            "1",
            "-reconnect_delay_max",
            "10",
            "-i",
            &request.url,
        ]);
    }
    let media = request.media.as_ref();
    let audio_url = media.and_then(|m| m.audio_url.as_deref());
    if let Some(audio) = audio_url {
        input_options(&mut command, request.referer.as_deref());
        if gateway.is_hls_input(audio).await? {
            hls_options(&mut command, "ffmpeg").await?;
        }
        command.args(["-i", audio]);
    }
    let mut subtitle_maps = Vec::new();
    let mut next_input = if audio_url.is_some() { 2 } else { 1 };
    if let Some(media) = media {
        for (position, track) in media.subtitles.iter().enumerate() {
            let source = if let Some(url) = &track.url {
                let path = Path::new(url);
                ensure!(
                    path.is_absolute()
                        && path.parent() == output.parent()
                        && path.file_name().is_some_and(|name| name
                            .to_string_lossy()
                            .starts_with("subtitle-input-")),
                    "Subtitle input must be a prepared task file"
                );
                command.args(["-protocol_whitelist", "file"]);
                command.args(["-i", url]);
                let index = next_input;
                next_input += 1;
                format!("{index}:s:0")
            } else {
                format!("0:{}", track.index.context("Missing subtitle source")?)
            };
            subtitle_maps.push(source);
            if media.subtitle_mode == crate::SubtitleMode::External {
                let sidecar = output.with_file_name(format!("media-subtitle-{position}.srt"));
                if tokio::fs::try_exists(&sidecar).await? {
                    tokio::fs::remove_file(&sidecar).await?;
                }
                let mut extraction = command_from(&command);
                extraction.args([
                    "-map",
                    subtitle_maps.last().context("Missing subtitle mapping")?,
                    "-c:s",
                    "srt",
                    "-f",
                    "srt",
                ]);
                if let Some(seconds) = media.duration_seconds {
                    extraction.args(["-t", &seconds.to_string()]);
                }
                extraction.arg(sidecar);
                run_auxiliary(extraction, &cancel, 32 * 1024).await?;
            }
        }
    }
    let chapter_input = if media.is_some_and(|media| !media.chapters.is_empty()) {
        let metadata = output.with_file_name("media-chapters.ffmeta");
        tokio::fs::write(
            &metadata,
            crate::media_assets::metadata(&media.context("Missing media")?.chapters),
        )
        .await?;
        command.args(["-f", "ffmetadata", "-i"]).arg(metadata);
        Some(next_input)
    } else {
        None
    };
    if media.is_some_and(|m| matches!(m.container.as_str(), "mp3" | "mka")) {
        command.arg("-vn");
    } else if let Some(index) = media.and_then(|m| m.video_stream) {
        command.args(["-map", &format!("0:{index}")]);
    } else {
        command.args(["-map", "0:v:0?"]);
    }
    if let Some(index) = media.and_then(|m| m.audio_stream) {
        command.args([
            "-map",
            &format!("{}:{index}", if audio_url.is_some() { 1 } else { 0 }),
        ]);
    } else {
        command.args([
            "-map",
            if audio_url.is_some() {
                "1:a:0?"
            } else {
                "0:a:0?"
            },
        ]);
    }
    if let Some(seconds) = media.and_then(|m| m.duration_seconds) {
        command.args(["-t", &seconds.to_string()]);
    }
    if let Some(index) = chapter_input {
        command.args(["-map_chapters", &index.to_string()]);
    } else {
        command.args(["-map_chapters", "0"]);
    }
    if let Some(media) = media.filter(|m| m.subtitle_mode == crate::SubtitleMode::Embed) {
        for (index, source) in subtitle_maps.iter().enumerate() {
            command.args([
                "-map",
                source,
                &format!("-metadata:s:s:{index}"),
                &format!(
                    "language={}",
                    metadata_language(&media.subtitles[index].language)
                ),
                &format!("-metadata:s:s:{index}"),
                &format!("title={}", media.subtitles[index].title),
            ]);
        }
        if !subtitle_maps.is_empty() {
            command.args([
                "-c:s",
                if media.container == "mp4" {
                    "mov_text"
                } else {
                    "srt"
                },
            ]);
        }
    }
    let normalized = if media.is_some_and(|m| m.normalize_audio) {
        let mut analysis = command_from(&command);
        analysis.args([
            "-loglevel",
            "info",
            "-vn",
            "-sn",
            "-dn",
            "-af",
            "loudnorm=I=-23:TP=-1:LRA=7:print_format=json",
            "-f",
            "null",
            "-",
        ]);
        let diagnostics = run_auxiliary(analysis, &cancel, 64 * 1024).await?;
        Some(normalization_filter(&diagnostics)?)
    } else {
        None
    };
    if let Some(filter) = &normalized {
        command.args(["-af", filter]);
    }
    let container = media.map(|m| m.container.as_str()).unwrap_or("mp4");
    if container == "mp3" {
        command.args(["-c:a", "libmp3lame", "-q:a", "2", "-f", "mp3"]);
    } else if container == "mka" {
        command.args(["-c:a", "copy", "-f", "matroska"]);
    } else {
        command.args([
            "-c:v",
            "copy",
            "-c:a",
            if normalized.is_some() { "aac" } else { "copy" },
            "-f",
            if container == "mkv" {
                "matroska"
            } else {
                "mp4"
            },
        ]);
    }
    if container == "mp4" {
        command.args(["-movflags", "+faststart"]);
    }
    command.arg(output);
    let mut child = command.spawn().context("FFmpeg could not be started")?;
    let stdout = child.stdout.take().context("Missing FFmpeg output")?;
    let stderr = child.stderr.take().context("Missing FFmpeg errors")?;
    let errors = tokio::spawn(bounded_output(stderr, 32 * 1024));
    let progress = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Some(line) = lines.next_line().await? {
            if let Some(value) = line
                .strip_prefix("out_time_us=")
                .and_then(|v| v.parse::<u64>().ok())
            {
                output_time.fetch_max(value / 1000, Ordering::Relaxed);
            }
            if let Some(value) = line
                .strip_prefix("total_size=")
                .and_then(|v| v.parse::<u64>().ok())
            {
                output_bytes.store(value, Ordering::Relaxed);
            }
        }
        anyhow::Ok(())
    });
    let status = tokio::select! {
        value = child.wait() => value?,
        _ = cancel.cancelled() => {
            child.kill().await?;
            let _ = child.wait().await;
            let _ = progress.await;
            let _ = errors.await;
            anyhow::bail!("Cancelled");
        }
    };
    progress.await??;
    let errors = errors.await??;
    ensure!(
        status.success(),
        "FFmpeg failed: {}",
        String::from_utf8_lossy(&errors)
    );
    ensure!(
        tokio::fs::metadata(output).await?.len() > 0,
        "FFmpeg produced an empty file"
    );
    Ok(())
}
fn command_from(source: &Command) -> Command {
    let mut command = command("ffmpeg");
    command.args(source.as_std().get_args());
    if let Some(directory) = source.as_std().get_current_dir() {
        command.current_dir(directory);
    }
    command
}
async fn run_auxiliary(
    mut command: Command,
    cancel: &CancellationToken,
    limit: usize,
) -> Result<Vec<u8>> {
    let mut child = command
        .spawn()
        .context("Media processing could not start")?;
    let stdout = child.stdout.take().context("Missing media output")?;
    let stderr = child.stderr.take().context("Missing media diagnostics")?;
    let result = tokio::select! {
        _ = cancel.cancelled() => { let _ = child.kill().await; let _ = child.wait().await; anyhow::bail!("Cancelled"); },
        result = async {
            let (status, _, errors) = tokio::try_join!(async { anyhow::Ok(child.wait().await?) }, bounded_output(stdout, 4096), bounded_tail_output(stderr, limit))?;
            ensure!(status.success(), "Media processing failed: {}", String::from_utf8_lossy(&errors));
            anyhow::Ok(errors)
        } => result,
    };
    result
}
fn normalization_filter(output: &[u8]) -> Result<String> {
    let text = String::from_utf8_lossy(output);
    let start = text
        .rfind('{')
        .context("Loudness analysis contains no measurements")?;
    let end = text[start..]
        .find('}')
        .context("Invalid loudness analysis")?
        + start;
    let measurements: serde_json::Value = serde_json::from_str(&text[start..=end])?;
    let mut parameters = String::from("loudnorm=I=-23:TP=-1:LRA=7:linear=true");
    for (input, output) in [
        ("input_i", "measured_I"),
        ("input_tp", "measured_TP"),
        ("input_lra", "measured_LRA"),
        ("input_thresh", "measured_thresh"),
        ("target_offset", "offset"),
    ] {
        let value = measurements[input]
            .as_str()
            .context("Missing loudness measurement")?
            .parse::<f64>()?;
        if !value.is_finite() {
            return Ok("anull".into());
        }
        ensure!(value.abs() <= 1000.0, "Invalid loudness measurement");
        parameters.push_str(&format!(":{output}={value}"));
    }
    Ok(parameters)
}
fn metadata_language(language: &str) -> &str {
    match language.split('-').next().unwrap_or("und") {
        "en" => "eng",
        "zh" => "zho",
        "hi" => "hin",
        "es" => "spa",
        "ar" => "ara",
        "fr" => "fra",
        "bn" => "ben",
        "pt" => "por",
        "ru" => "rus",
        "ur" => "urd",
        "id" => "ind",
        "de" => "deu",
        "ja" => "jpn",
        "mr" => "mar",
        "te" => "tel",
        "tr" => "tur",
        "ta" => "tam",
        "vi" => "vie",
        value if value.len() == 3 => value,
        _ => "und",
    }
}
#[cfg(test)]
mod processing_tests {
    use super::*;
    #[tokio::test]
    async fn late_loudness_measurements_survive_large_diagnostics() {
        let mut input = vec![b'x'; 100000];
        input.extend_from_slice(br#"{"input_i":"-30.0","input_tp":"-20.0","input_lra":"0.0","input_thresh":"-40.0","target_offset":"0.1"}"#);
        let output = bounded_tail_output(input.as_slice(), 4096).await.unwrap();
        assert!(normalization_filter(&output)
            .unwrap()
            .contains("measured_I=-30"));
        assert_eq!(metadata_language("tr-TR"), "tur");
        assert_eq!(metadata_language("en-US"), "eng");
    }
}
