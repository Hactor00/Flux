use crate::extractor::{ResolvedAudio, ResolvedFormat, ResolvedMedia};
use anyhow::{ensure, Context, Result};
use std::collections::HashMap;
use url::Url;

pub(crate) const MAX_MANIFEST: usize = 512 * 1024;

pub(crate) fn xml(bytes: &[u8]) -> Result<xmltree::Element> {
    ensure!(bytes.len() <= 2 * 1024 * 1024, "Manifest exceeds two MiB");
    let mut reader = quick_xml::Reader::from_reader(bytes);
    let mut depth = 0usize;
    let mut nodes = 0usize;
    loop {
        use quick_xml::events::Event;
        match reader.read_event()? {
            Event::Start(_) => {
                depth += 1;
                nodes += 1;
            }
            Event::Empty(_) => nodes += 1,
            Event::End(_) => {
                ensure!(depth > 0, "Invalid XML nesting");
                depth -= 1;
            }
            Event::DocType(_) => anyhow::bail!("XML document types are forbidden"),
            Event::Eof => break,
            _ => {}
        }
        ensure!(
            depth <= 32 && nodes <= 16_384,
            "Manifest structure exceeds the safety limit"
        );
    }
    ensure!(depth == 0, "Truncated XML manifest");
    Ok(xmltree::Element::parse(bytes)?)
}

fn remote(base: &Url, relative: &str) -> Result<String> {
    let url = base.join(relative)?;
    crate::model::validate_url(url.as_str())?;
    ensure!(
        base.scheme() != "https" || url.scheme() == "https",
        "Manifest transport downgrade is forbidden"
    );
    Ok(url.into())
}

fn format(url: String, id: String, kind: &str) -> ResolvedFormat {
    ResolvedFormat {
        id,
        url,
        kind: Some(kind.into()),
        manifest_video_id: None,
        manifest_audio_id: None,
        alternative_ids: Vec::new(),
        audio_url: None,
        height: None,
        fps: None,
        audio_only: false,
        audio_codec: String::new(),
        codec: "video".into(),
        size: None,
        container: "mp4".into(),
    }
}

pub(crate) fn parse(url: &str, text: &str) -> Result<ResolvedMedia> {
    crate::model::validate_url(url)?;
    ensure!(text.len() <= MAX_MANIFEST, "Manifest exceeds 512 KiB");
    let base = Url::parse(url)?;
    let text = text.trim_start_matches('\u{feff}').trim();
    let title = base
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|name| !name.is_empty())
        .unwrap_or("Video")
        .chars()
        .take(128)
        .collect();
    let (formats, audio_tracks, duration) = if text.starts_with("#EXTM3U") {
        hls(&base, text)?
    } else {
        dash(&base, text)?
    };
    ensure!(
        !formats.is_empty() && formats.len() <= 128 && audio_tracks.len() <= 32,
        "Invalid manifest stream count"
    );
    let (subtitles, chapters) = crate::media_assets::discover(&base, text)?;
    Ok(ResolvedMedia {
        title,
        duration,
        formats,
        audio_tracks,
        subtitles,
        chapters,
    })
}

fn hls(base: &Url, text: &str) -> Result<(Vec<ResolvedFormat>, Vec<ResolvedAudio>, Option<f64>)> {
    let mut audio = Vec::new();
    let mut groups = HashMap::new();
    let mut audio_groups = Vec::new();
    let mut formats = Vec::new();
    let mut next = None;
    let mut duration = 0.0;
    for line in text.lines().map(str::trim) {
        ensure!(line.len() <= 16_384, "Manifest line exceeds the size limit");
        if let Some(attributes) = line.strip_prefix("#EXT-X-MEDIA:") {
            let attributes = crate::media_assets::hls_attributes(attributes)?;
            if attributes.get("TYPE").is_some_and(|value| value == "AUDIO") {
                if let Some(uri) = attributes.get("URI") {
                    let url = remote(base, uri)?;
                    let id = attributes
                        .get("GROUP-ID")
                        .cloned()
                        .unwrap_or_else(|| format!("audio-{}", audio.len()));
                    if attributes
                        .get("DEFAULT")
                        .is_some_and(|value| value == "YES")
                        || !groups.contains_key(&id)
                    {
                        groups.insert(id.clone(), url.clone());
                    }
                    audio.push(ResolvedAudio {
                        id: format!("{id}-{}", audio.len()),
                        url,
                        codec: "audio".into(),
                        language: attributes.get("LANGUAGE").cloned(),
                        bitrate: None,
                        size: None,
                        manifest_id: None,
                    });
                }
            }
        } else if let Some(attributes) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            next = Some(crate::media_assets::hls_attributes(attributes)?);
        } else if let Some(attributes) = line
            .strip_prefix("#EXT-X-KEY:")
            .or_else(|| line.strip_prefix("#EXT-X-SESSION-KEY:"))
        {
            let attributes = crate::media_assets::hls_attributes(attributes)?;
            ensure!(
                attributes
                    .get("METHOD")
                    .is_none_or(|value| value == "NONE" || value == "AES-128")
                    && attributes
                        .get("KEYFORMAT")
                        .is_none_or(|value| value == "identity"),
                "Protected media requires its licensed player"
            );
        } else if let Some(value) = line.strip_prefix("#EXTINF:") {
            let value: f64 = value.split(',').next().unwrap_or("").parse()?;
            ensure!(
                value.is_finite() && value >= 0.0,
                "Invalid segment duration"
            );
            duration += value;
        } else if !line.is_empty() && !line.starts_with('#') {
            if let Some(attributes) = next.take() {
                let mut variant =
                    format(remote(base, line)?, format!("hls-{}", formats.len()), "hls");
                variant.height = attributes
                    .get("RESOLUTION")
                    .and_then(|value| value.split_once('x'))
                    .and_then(|(_, height)| height.parse().ok());
                variant.fps = attributes
                    .get("FRAME-RATE")
                    .and_then(|value| value.parse::<f64>().ok())
                    .filter(|value| value.is_finite() && (0.0..=240.0).contains(value));
                variant.codec = attributes
                    .get("CODECS")
                    .cloned()
                    .unwrap_or_else(|| "video".into());
                variant.audio_url = attributes
                    .get("AUDIO")
                    .and_then(|id| groups.get(id))
                    .cloned();
                if let Some(group) = attributes.get("AUDIO") {
                    audio_groups.push((formats.len(), group.clone()));
                }
                formats.push(variant);
            }
        }
        ensure!(
            formats.len() <= 128 && audio.len() <= 32,
            "Manifest contains too many streams"
        );
    }
    ensure!(next.is_none(), "HLS variant URI is missing");
    for (index, group) in audio_groups {
        formats[index].audio_url = groups.get(&group).cloned();
    }
    ensure!(
        duration.is_finite() && duration <= 366.0 * 86400.0,
        "Manifest duration exceeds one year"
    );
    if formats.is_empty() {
        ensure!(
            text.contains("#EXTINF:") || text.contains("#EXT-X-TARGETDURATION:"),
            "Invalid HLS media playlist"
        );
        formats.push(format(base.to_string(), "hls-auto".into(), "hls"));
    }
    formats.sort_by_key(|format| std::cmp::Reverse(format.height.unwrap_or(0)));
    Ok((formats, audio, (duration > 0.0).then_some(duration)))
}

fn dash(base: &Url, text: &str) -> Result<(Vec<ResolvedFormat>, Vec<ResolvedAudio>, Option<f64>)> {
    let root = xml(text.as_bytes())?;
    ensure!(root.name == "MPD", "Unsupported media manifest");
    let mut formats = Vec::new();
    let mut audio = Vec::new();
    collect(&root, base, &HashMap::new(), &mut formats, &mut audio)?;
    let default_audio = audio.first().and_then(|track| track.manifest_id.clone());
    for format in &mut formats {
        format.manifest_audio_id = default_audio.clone();
    }
    if formats.is_empty() && !audio.is_empty() {
        let mut value = format(base.to_string(), "dash-audio".into(), "dash");
        value.audio_only = true;
        value.container = "mp3".into();
        value.manifest_audio_id = default_audio;
        formats.push(value);
    }
    formats.sort_by_key(|format| std::cmp::Reverse(format.height.unwrap_or(0)));
    Ok((formats, audio, None))
}

fn collect(
    element: &xmltree::Element,
    base: &Url,
    parent: &HashMap<String, String>,
    formats: &mut Vec<ResolvedFormat>,
    audio: &mut Vec<ResolvedAudio>,
) -> Result<()> {
    ensure!(
        element.name != "ContentProtection",
        "Protected media requires its licensed player"
    );
    let mut attributes = parent.clone();
    for key in [
        "mimeType",
        "contentType",
        "codecs",
        "height",
        "frameRate",
        "lang",
    ] {
        if let Some(value) = element.attributes.get(key) {
            ensure!(
                value.len() <= 256,
                "Manifest attribute exceeds the size limit"
            );
            attributes.insert(key.into(), value.clone());
        }
    }
    if element.name == "Representation" {
        let id = element
            .attributes
            .get("id")
            .context("DASH representation ID is missing")?;
        ensure!(
            id.len() <= 128
                && id
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')),
            "Invalid DASH representation ID"
        );
        let mime = attributes.get("mimeType").map(String::as_str).unwrap_or("");
        let kind = attributes
            .get("contentType")
            .map(String::as_str)
            .unwrap_or("");
        if mime.starts_with("video/") || kind == "video" || attributes.contains_key("height") {
            let mut value = format(base.to_string(), format!("dash-{id}"), "dash");
            value.manifest_video_id = Some(id.clone());
            value.height = attributes
                .get("height")
                .and_then(|value| value.parse().ok());
            value.codec = attributes
                .get("codecs")
                .cloned()
                .unwrap_or_else(|| "video".into());
            value.fps = attributes
                .get("frameRate")
                .and_then(|value| {
                    let (numerator, denominator) = value.split_once('/').unwrap_or((value, "1"));
                    Some(numerator.parse::<f64>().ok()? / denominator.parse::<f64>().ok()?)
                })
                .filter(|value| value.is_finite() && *value > 0.0 && *value <= 240.0);
            formats.push(value);
        } else if mime.starts_with("audio/") || kind == "audio" {
            audio.push(ResolvedAudio {
                id: format!("dash-{id}"),
                url: base.to_string(),
                codec: attributes
                    .get("codecs")
                    .cloned()
                    .unwrap_or_else(|| "audio".into()),
                language: attributes.get("lang").cloned(),
                bitrate: element
                    .attributes
                    .get("bandwidth")
                    .and_then(|value| value.parse().ok()),
                size: None,
                manifest_id: Some(id.clone()),
            });
        }
    }
    for child in &element.children {
        if let xmltree::XMLNode::Element(child) = child {
            collect(child, base, &attributes, formats, audio)?;
        }
    }
    ensure!(
        formats.len() <= 128 && audio.len() <= 32,
        "Manifest contains too many streams"
    );
    Ok(())
}

pub(crate) fn select_dash(
    document: &mut xmltree::Element,
    video: Option<&str>,
    audio: Option<&str>,
) -> Result<()> {
    fn prune(
        element: &mut xmltree::Element,
        inherited: &str,
        video: Option<&str>,
        audio: Option<&str>,
        found: &mut (bool, bool),
    ) {
        let mime = element
            .attributes
            .get("mimeType")
            .map(String::as_str)
            .unwrap_or(inherited)
            .to_owned();
        let content = element
            .attributes
            .get("contentType")
            .map(String::as_str)
            .unwrap_or("")
            .to_owned();
        element.children.retain_mut(|child| {
            let xmltree::XMLNode::Element(child) = child else { return true; };
            if child.name == "Representation" {
                let child_mime = child.attributes.get("mimeType").map(String::as_str).unwrap_or(&mime);
                let selected = if child_mime.starts_with("video/") || content == "video" || child.attributes.contains_key("height") { video.map(|id| (id, true)) }
                    else if child_mime.starts_with("audio/") || content == "audio" { audio.map(|id| (id, false)) } else { None };
                if let Some((id, is_video)) = selected {
                    if child.attributes.get("id").is_none_or(|value| value != id) { return false; }
                    if is_video { found.0 = true; } else { found.1 = true; }
                }
            }
            prune(child, &mime, video, audio, found);
            child.name != "AdaptationSet" || child.children.iter().any(|child| matches!(child, xmltree::XMLNode::Element(element) if element.name == "Representation"))
        });
    }
    let mut found = (video.is_none(), audio.is_none());
    prune(document, "", video, audio, &mut found);
    ensure!(
        found.0 && found.1,
        "Selected DASH representation is no longer available"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hls_metadata_preserves_quality_audio_and_signed_relative_urls() {
        let media = parse("https://cdn.test/path/master?token=123", "#EXTM3U\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",NAME=\"English\",LANGUAGE=\"en\",DEFAULT=YES,URI=\"audio.m3u8?sig=456\"\n#EXT-X-STREAM-INF:BANDWIDTH=5000000,RESOLUTION=1920x1080,FRAME-RATE=60,CODECS=\"avc1,mp4a\",AUDIO=\"a\"\n1080.m3u8?sig=789\n").unwrap();
        assert_eq!(media.formats[0].height, Some(1080));
        assert_eq!(media.formats[0].fps, Some(60.0));
        assert_eq!(
            media.formats[0].audio_url.as_deref(),
            Some("https://cdn.test/path/audio.m3u8?sig=456")
        );
        assert_eq!(
            media.formats[0].url,
            "https://cdn.test/path/1080.m3u8?sig=789"
        );
    }
    #[test]
    fn dash_selection_removes_other_representations_and_rejects_entities() {
        let text = r#"<MPD><Period><AdaptationSet mimeType="video/mp4"><Representation id="720" height="720"/><Representation id="1080" height="1080" frameRate="60000/1001"/></AdaptationSet><AdaptationSet mimeType="audio/mp4" lang="tr"><Representation id="audio"/></AdaptationSet></Period></MPD>"#;
        let media = parse("https://cdn.test/video.mpd", text).unwrap();
        assert_eq!(media.formats[0].height, Some(1080));
        assert_eq!(media.formats[0].manifest_audio_id.as_deref(), Some("audio"));
        let mut root = xml(text.as_bytes()).unwrap();
        select_dash(&mut root, Some("1080"), Some("audio")).unwrap();
        let mut output = Vec::new();
        root.write(&mut output).unwrap();
        assert!(!String::from_utf8(output).unwrap().contains("id=\"720\""));
        assert!(xml(b"<!DOCTYPE MPD [<!ENTITY key SYSTEM 'file:///secret'>]><MPD/>").is_err());
        assert!(parse(
            "https://cdn.test/video.mpd",
            "<MPD><ContentProtection/></MPD>"
        )
        .is_err());
    }
}
