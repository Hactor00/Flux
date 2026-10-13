//! Bounded manifest discovery and subtitle processing without remote file access.
use crate::{MediaChapter, SubtitleTrack};
use anyhow::{ensure, Context, Result};
use std::collections::HashSet;
use url::Url;

pub(crate) const MAX_ASSET: usize = 8 * 1024 * 1024;
pub(crate) fn hls_attributes(line: &str) -> Result<std::collections::HashMap<String, String>> {
    let mut result = std::collections::HashMap::new();
    let expression = regex::Regex::new(r#"([A-Z0-9-]+)=(?:"([^"]*)"|([^,]*))"#)?;
    for captures in expression.captures_iter(line) {
        result.insert(
            captures[1].to_owned(),
            captures
                .get(2)
                .or_else(|| captures.get(3))
                .context("Missing HLS attribute")?
                .as_str()
                .to_owned(),
        );
    }
    Ok(result)
}
fn clean(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(limit)
        .collect()
}
fn language(value: &str) -> String {
    if value.len() <= 32
        && !value.is_empty()
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        value.into()
    } else {
        "und".into()
    }
}
fn remote(base: &Url, value: &str) -> Result<String> {
    let url = base.join(value)?;
    crate::model::validate_url(url.as_str())?;
    Ok(url.into())
}
pub(crate) fn discover(base: &Url, text: &str) -> Result<(Vec<SubtitleTrack>, Vec<MediaChapter>)> {
    ensure!(text.len() <= MAX_ASSET, "Manifest exceeds discovery limit");
    let mut subtitles = Vec::new();
    let mut chapters = Vec::new();
    if text.trim_start_matches('\u{feff}').starts_with("#EXTM3U") {
        let program_start = text
            .lines()
            .find_map(|line| line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:"))
            .and_then(date_ms);
        for line in text.lines() {
            if let Some(attributes) = line.strip_prefix("#EXT-X-DATERANGE:") {
                let fields = hls_attributes(attributes)?;
                if fields
                    .get("CLASS")
                    .is_some_and(|value| value.to_lowercase().contains("chapter"))
                {
                    let start = fields.get("START-DATE").and_then(|value| date_ms(value));
                    let duration = fields
                        .get("DURATION")
                        .and_then(|value| value.parse::<f64>().ok())
                        .filter(|value| value.is_finite() && *value > 0.0);
                    if let (Some(base), Some(start)) = (program_start, start) {
                        let end = fields
                            .get("END-DATE")
                            .and_then(|value| date_ms(value))
                            .or_else(|| {
                                duration.map(|duration| start + (duration * 1000.0) as i128)
                            });
                        if let Some(end) = end.filter(|end| {
                            *end > start && start >= base && *end - base <= 7 * 86400 * 1000
                        }) {
                            chapters.push(MediaChapter {
                                start_ms: (start - base) as u64,
                                end_ms: (end - base) as u64,
                                title: clean(
                                    fields
                                        .get("X-TITLE")
                                        .or_else(|| fields.get("ID"))
                                        .map(String::as_str)
                                        .unwrap_or(""),
                                    256,
                                ),
                            });
                        }
                    }
                }
            }
            if let Some(attributes) = line.strip_prefix("#EXT-X-MEDIA:") {
                let fields = hls_attributes(attributes)?;
                if fields.get("TYPE").is_some_and(|value| value == "SUBTITLES") {
                    if let Some(uri) = fields.get("URI") {
                        subtitles.push(SubtitleTrack {
                            url: Some(remote(base, uri)?),
                            index: None,
                            language: language(
                                fields.get("LANGUAGE").map(String::as_str).unwrap_or("und"),
                            ),
                            title: clean(fields.get("NAME").map(String::as_str).unwrap_or(""), 128),
                        });
                    }
                }
            }
        }
    } else if text.trim_start().starts_with('<') {
        let root = xmltree::Element::parse(text.as_bytes())?;
        collect_dash(&root, base, &mut subtitles)?;
        collect_dash_chapters(&root, 0, &mut chapters);
    }
    let mut seen = HashSet::new();
    subtitles.retain(|track| seen.insert(track.url.clone()));
    ensure!(
        subtitles.len() <= 32,
        "Manifest contains too many subtitles"
    );
    chapters.sort_by_key(|chapter: &MediaChapter| chapter.start_ms);
    let mut end = 0;
    chapters.retain(|chapter| {
        if chapter.start_ms < end {
            false
        } else {
            end = chapter.end_ms;
            true
        }
    });
    ensure!(
        chapters.len() <= 10000,
        "Manifest contains too many chapters"
    );
    Ok((subtitles, chapters))
}
fn date_ms(value: &str) -> Option<i128> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|date| date.unix_timestamp_nanos() / 1_000_000)
}
fn collect_dash_chapters(
    element: &xmltree::Element,
    period_start: u64,
    chapters: &mut Vec<MediaChapter>,
) {
    let period_start = if element.name == "Period" {
        element
            .attributes
            .get("start")
            .and_then(|value| crate::media_progress::iso_duration(value))
            .unwrap_or(period_start)
    } else {
        period_start
    };
    if element.name == "EventStream"
        && element
            .attributes
            .get("schemeIdUri")
            .is_some_and(|scheme| scheme.to_lowercase().contains("chapter"))
    {
        let scale = element
            .attributes
            .get("timescale")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(1);
        if (1..=1_000_000_000).contains(&scale) {
            for node in element.children.iter().take(10000) {
                if let xmltree::XMLNode::Element(event) = node {
                    if let (Some(start), Some(duration)) = (
                        event
                            .attributes
                            .get("presentationTime")
                            .and_then(|v| v.parse::<u64>().ok()),
                        event
                            .attributes
                            .get("duration")
                            .and_then(|v| v.parse::<u64>().ok()),
                    ) {
                        let offset = element
                            .attributes
                            .get("presentationTimeOffset")
                            .and_then(|value| value.parse::<u64>().ok())
                            .unwrap_or(0);
                        let start = period_start.saturating_add(
                            start.saturating_sub(offset).saturating_mul(1000) / scale,
                        );
                        let end = start.saturating_add(duration.saturating_mul(1000) / scale);
                        if end > start && end <= 7 * 86400 * 1000 && chapters.len() < 10001 {
                            chapters.push(MediaChapter {
                                start_ms: start,
                                end_ms: end,
                                title: clean(&event.get_text().unwrap_or_default(), 256),
                            });
                        }
                    }
                }
            }
        }
    }
    for node in &element.children {
        if let xmltree::XMLNode::Element(child) = node {
            collect_dash_chapters(child, period_start, chapters);
        }
    }
}
fn collect_dash(
    element: &xmltree::Element,
    base: &Url,
    output: &mut Vec<SubtitleTrack>,
) -> Result<()> {
    let base = match element
        .get_child("BaseURL")
        .and_then(|child| child.get_text())
    {
        Some(value) => Url::parse(&remote(base, &value)?)?,
        None => base.clone(),
    };
    if element.name == "AdaptationSet"
        && (element
            .attributes
            .get("contentType")
            .is_some_and(|v| v == "text")
            || element
                .attributes
                .get("mimeType")
                .is_some_and(|v| v == "text/vtt" || v == "application/x-subrip"))
    {
        // Segmented timed text remains available through FFprobe's embedded stream indices.
        let mut sources = Vec::new();
        for node in &element.children {
            if let xmltree::XMLNode::Element(child) = node {
                if child.name == "Representation"
                    && child.get_child("SegmentTemplate").is_none()
                    && child.get_child("SegmentList").is_none()
                {
                    if let Some(value) = child
                        .get_child("BaseURL")
                        .and_then(|value| value.get_text())
                    {
                        sources.push(remote(&base, &value)?);
                    }
                }
            }
        }
        if sources.is_empty() && element.get_child("BaseURL").is_some() {
            sources.push(base.to_string());
        }
        for source in sources {
            let extension = Url::parse(&source)?.path().to_lowercase();
            if extension.ends_with(".vtt") || extension.ends_with(".srt") {
                output.push(SubtitleTrack {
                    url: Some(source),
                    index: None,
                    language: language(
                        element
                            .attributes
                            .get("lang")
                            .map(String::as_str)
                            .unwrap_or("und"),
                    ),
                    title: clean(
                        &element
                            .get_child("Label")
                            .and_then(|label| label.get_text())
                            .unwrap_or_default(),
                        128,
                    ),
                });
            }
        }
    }
    for child in &element.children {
        if let xmltree::XMLNode::Element(child) = child {
            collect_dash(child, &base, output)?;
        }
    }
    Ok(())
}
pub(crate) fn metadata(chapters: &[MediaChapter]) -> String {
    let mut output = String::from(";FFMETADATA1\n");
    for chapter in chapters {
        let title = chapter
            .title
            .replace('\\', "\\\\")
            .replace('=', "\\=")
            .replace(';', "\\;")
            .replace('#', "\\#");
        output.push_str(&format!(
            "[CHAPTER]\nTIMEBASE=1/1000\nSTART={}\nEND={}\ntitle={}\n",
            chapter.start_ms, chapter.end_ms, title
        ));
    }
    output
}
fn milliseconds(value: &str) -> Result<u64> {
    let parts: Vec<_> = value.split(':').collect();
    ensure!((2..=3).contains(&parts.len()), "Invalid subtitle timestamp");
    let seconds = parts.last().context("Missing seconds")?.parse::<f64>()?;
    let minutes: u64 = parts[parts.len() - 2].parse()?;
    let hours: u64 = if parts.len() == 3 {
        parts[0].parse()?
    } else {
        0
    };
    ensure!(
        seconds.is_finite() && (0.0..60.0).contains(&seconds) && minutes < 60 && hours <= 168,
        "Subtitle timestamp is out of bounds"
    );
    Ok(hours * 3600000 + minutes * 60000 + (seconds * 1000.0).round() as u64)
}
fn timestamp(value: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        value / 3600000,
        value / 60000 % 60,
        value / 1000 % 60,
        value % 1000
    )
}
pub(crate) fn merge_webvtt(segments: &[(f64, String)]) -> Result<String> {
    let mut result = String::from("WEBVTT\n\n");
    let mut offset = 0u64;
    let mut clock_origin: Option<i64> = None;
    let mut seen = HashSet::new();
    for (duration, segment) in segments {
        ensure!(
            duration.is_finite() && *duration > 0.0 && *duration <= 86400.0,
            "Invalid subtitle segment duration"
        );
        let mut clock = None;
        for line in segment.lines() {
            if let Some(map) = line.strip_prefix("X-TIMESTAMP-MAP=") {
                let fields: std::collections::HashMap<_, _> = map
                    .split(',')
                    .filter_map(|field| field.split_once(':'))
                    .collect();
                if let (Some(local), Some(mpeg)) = (fields.get("LOCAL"), fields.get("MPEGTS")) {
                    let ticks: u64 = mpeg.parse()?;
                    ensure!(ticks < (1u64 << 33), "Invalid subtitle MPEG timestamp");
                    clock = Some((ticks / 90) as i64 - milliseconds(local)? as i64);
                }
            }
        }
        let shift = clock.map(|clock| {
            let origin = *clock_origin.get_or_insert(clock);
            let delta = clock - origin;
            if delta < -(1i64 << 31) / 90 {
                delta + (1i64 << 33) / 90
            } else {
                delta
            }
        });
        let normalized = segment.replace("\r\n", "\n");
        for block in normalized.split("\n\n") {
            let lines: Vec<_> = block.lines().collect();
            let Some(position) = lines.iter().position(|line| line.contains("-->")) else {
                continue;
            };
            let (start, end) = lines[position]
                .split_once("-->")
                .context("Invalid subtitle cue")?;
            let start = milliseconds(start.trim())?;
            let end = milliseconds(
                end.split_whitespace()
                    .next()
                    .context("Missing subtitle end")?,
            )?;
            ensure!(end > start, "Invalid subtitle cue interval");
            // Without a timestamp map, HLS uses an identity mapping. Overlapping
            // cues in adjacent segments must not be shifted by the segment duration.
            let delta = shift.unwrap_or(0);
            let start = (start as i64 + delta).max(0) as u64;
            let end = (end as i64 + delta).max(0) as u64;
            let text = lines[position + 1..].join("\n");
            if end > start && seen.insert((start, end, text.clone())) {
                result.push_str(&format!(
                    "{} --> {}\n{}\n\n",
                    timestamp(start),
                    timestamp(end),
                    text
                ));
            }
        }
        offset = offset.saturating_add((duration * 1000.0).round() as u64);
        ensure!(offset <= 7 * 86400 * 1000, "Subtitle playlist is too long");
        ensure!(
            result.len() <= MAX_ASSET,
            "Merged subtitles exceed size limit"
        );
    }
    ensure!(!seen.is_empty(), "Subtitle playlist contains no cues");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unmapped_overlapping_hls_cues_keep_absolute_timing() {
        let segment = "WEBVTT\n\n00:00:03.500 --> 00:00:04.500\nOverlap\n";
        let output = merge_webvtt(&[(4.0, segment.into()), (4.0, segment.into())]).unwrap();
        assert_eq!(output.matches("Overlap").count(), 1);
        assert!(output.contains("00:00:03.500 --> 00:00:04.500"));
    }
    #[test]
    fn discovers_hls_languages_and_rejects_local_files() {
        let base = Url::parse("https://example.org/master.m3u8").unwrap();
        let (tracks, _) = discover(&base, "#EXTM3U\n#EXT-X-MEDIA:TYPE=SUBTITLES,LANGUAGE=\"tr\",NAME=\"Türkçe, CC\",URI=\"tr/list.m3u8\"").unwrap();
        assert_eq!(tracks[0].language, "tr");
        assert_eq!(tracks[0].title, "Türkçe, CC");
        assert_eq!(
            tracks[0].url.as_deref(),
            Some("https://example.org/tr/list.m3u8")
        );
        assert!(discover(
            &base,
            "#EXTM3U\n#EXT-X-MEDIA:TYPE=SUBTITLES,URI=\"file:///etc/passwd\""
        )
        .is_err());
    }
    #[test]
    fn discovers_dash_text_and_escapes_metadata() {
        let (tracks, _) = discover(&Url::parse("https://example.org/a/manifest.mpd").unwrap(), r#"<MPD><Period><AdaptationSet mimeType="text/vtt" lang="en"><Representation><BaseURL>sub.vtt</BaseURL></Representation></AdaptationSet></Period></MPD>"#).unwrap();
        assert_eq!(
            tracks[0].url.as_deref(),
            Some("https://example.org/a/sub.vtt")
        );
        assert!(metadata(&[MediaChapter {
            start_ms: 0,
            end_ms: 2000,
            title: "one;two=three#four".into()
        }])
        .contains("one\\;two\\=three\\#four"));
    }
    #[test]
    fn joins_hls_vtt_timestamps_and_deduplicates_cues() {
        let first = "WEBVTT\nX-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:900000\n\n00:00:00.100 --> 00:00:00.900\nFirst\n";
        let next = "WEBVTT\nX-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:990000\n\n00:00:00.100 --> 00:00:00.900\nNext\n";
        let output = merge_webvtt(&[(1.0, first.into()), (1.0, next.into())]).unwrap();
        assert!(output.contains("00:00:01.100 --> 00:00:01.900"));
        assert!(output.contains("First") && output.contains("Next"));
    }
    #[test]
    fn discovers_only_explicit_manifest_chapter_events() {
        let base = Url::parse("https://example.org/manifest").unwrap();
        let (_, chapters) = discover(&base, "#EXTM3U\n#EXT-X-PROGRAM-DATE-TIME:2026-10-12T10:00:00Z\n#EXT-X-DATERANGE:ID=\"one\",CLASS=\"chapter\",START-DATE=\"2026-10-12T10:00:02Z\",DURATION=4,X-TITLE=\"First\"").unwrap();
        assert_eq!(chapters[0].start_ms, 2000);
        assert_eq!(chapters[0].end_ms, 6000);
        let (_, chapters) = discover(&base, r#"<MPD><Period><EventStream schemeIdUri="urn:chapters" timescale="1000"><Event presentationTime="0" duration="5000">One</Event></EventStream></Period></MPD>"#).unwrap();
        assert_eq!(chapters[0].end_ms, 5000);
    }
}
