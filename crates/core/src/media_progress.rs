use crate::{DownloadRequest, MediaProgress};
use anyhow::{ensure, Result};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use url::Url;

const UNKNOWN: u64 = u64::MAX;
#[derive(Default)]
pub(crate) struct Resource {
    pub done: AtomicU64,
    total: AtomicU64,
    complete: AtomicBool,
    manifest: AtomicBool,
}
impl Resource {
    pub fn metadata(&self, total: Option<u64>, content_type: &str) {
        self.total
            .store(total.unwrap_or(UNKNOWN), Ordering::Relaxed);
        self.manifest.fetch_or(
            content_type.to_ascii_lowercase().contains("mpegurl")
                || content_type.to_ascii_lowercase().contains("dash+xml"),
            Ordering::Relaxed,
        );
    }
    pub fn finish(&self, size: u64) {
        self.done.store(size, Ordering::Relaxed);
        self.total.store(size, Ordering::Relaxed);
        self.complete.store(true, Ordering::Release);
    }
    pub fn reset(&self, done: u64) {
        self.done.store(done, Ordering::Relaxed);
        self.complete.store(false, Ordering::Relaxed);
    }
}
struct Segment {
    url: String,
    duration_ms: u64,
}
struct Plan {
    segments: Vec<Segment>,
    duration_ms: Option<u64>,
    live: bool,
}
struct State {
    resources: HashMap<String, Arc<Resource>>,
    plans: HashMap<String, Plan>,
}
pub(crate) struct Tracker {
    state: Mutex<State>,
    roots: Vec<String>,
    cap_ms: Option<u64>,
    adaptive_source: Option<String>,
}
#[derive(Default)]
pub(crate) struct Metrics {
    pub completed: u64,
    pub exact_total: Option<u64>,
    pub duration_ms: Option<u64>,
    pub ready_ms: u64,
    pub segments_total: Option<u64>,
    pub segments_completed: u64,
    pub live: bool,
    pub transfer_complete: bool,
}
impl Tracker {
    pub fn new(request: &DownloadRequest) -> Self {
        let media = request.media.as_ref();
        let audio = media.and_then(|m| m.audio_url.clone());
        let roots = if media.is_some_and(|m| m.container == "mp3") && audio.is_some() {
            vec![audio.unwrap_or_default()]
        } else {
            [Some(request.url.clone()), audio]
                .into_iter()
                .flatten()
                .collect()
        };
        let mut plans = HashMap::new();
        if let Some(media) = media.filter(|m| !m.segment_urls.is_empty()) {
            plans.insert(
                "explicit".into(),
                Plan {
                    segments: media
                        .segment_urls
                        .iter()
                        .map(|url| Segment {
                            url: url.clone(),
                            duration_ms: 0,
                        })
                        .collect(),
                    duration_ms: None,
                    live: false,
                },
            );
        }
        Self {
            state: Mutex::new(State {
                resources: HashMap::new(),
                plans,
            }),
            roots,
            cap_ms: media.and_then(|m| m.duration_seconds.map(|s| s as u64 * 1000)),
            adaptive_source: media
                .filter(|m| matches!(m.kind.as_deref(), Some("hls" | "dash")))
                .map(|_| request.url.clone()),
        }
    }
    pub fn resource(&self, remote: &Url) -> Result<Arc<Resource>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Media progress lock poisoned"))?;
        ensure!(
            state.resources.contains_key(remote.as_str()) || state.resources.len() < 65536,
            "Too many media progress resources"
        );
        Ok(state
            .resources
            .entry(remote.as_str().into())
            .or_insert_with(|| {
                Arc::new(Resource {
                    total: AtomicU64::new(UNKNOWN),
                    manifest: AtomicBool::new(
                        self.adaptive_source.as_deref() == Some(remote.as_str())
                            || remote.path().to_ascii_lowercase().ends_with(".m3u8")
                            || remote.path().to_ascii_lowercase().ends_with(".mpd"),
                    ),
                    ..Resource::default()
                })
            })
            .clone())
    }
    pub fn hls(&self, remote: &Url, bytes: &[u8]) -> Result<()> {
        let source = std::str::from_utf8(bytes)?;
        let mut duration = None;
        let mut segments = Vec::new();
        for line in source.lines().map(str::trim) {
            if let Some(value) = line.strip_prefix("#EXTINF:") {
                duration = milliseconds(value.split(',').next().unwrap_or(""));
            } else if !line.is_empty() && !line.starts_with('#') {
                if let Some(duration_ms) = duration.take() {
                    let url = remote.join(line)?.to_string();
                    crate::model::validate_url(&url)?;
                    segments.push(Segment { url, duration_ms });
                }
            }
        }
        if segments.is_empty() {
            return Ok(());
        }
        let live = !source
            .lines()
            .any(|line| matches!(line.trim(), "#EXT-X-ENDLIST" | "#EXT-X-PLAYLIST-TYPE:VOD"));
        let duration_ms = if live {
            None
        } else {
            Some(segments.iter().map(|s| s.duration_ms).sum())
        };
        self.plan(
            remote.as_str(),
            Plan {
                segments,
                duration_ms,
                live,
            },
        )
    }
    pub fn dash(&self, remote: &Url, document: &xmltree::Element) -> Result<()> {
        let live = document
            .attributes
            .get("type")
            .is_some_and(|v| v == "dynamic");
        let mut duration_ms = document
            .attributes
            .get("mediaPresentationDuration")
            .and_then(|v| iso_duration(v));
        if duration_ms.is_none() && !live {
            let mut end = 0;
            for period in document.children.iter().filter_map(|node| match node {
                xmltree::XMLNode::Element(e) if e.name == "Period" => Some(e),
                _ => None,
            }) {
                let start = period
                    .attributes
                    .get("start")
                    .and_then(|v| iso_duration(v))
                    .unwrap_or(end);
                if let Some(duration) = period
                    .attributes
                    .get("duration")
                    .and_then(|v| iso_duration(v))
                {
                    end = start.saturating_add(duration);
                }
            }
            if end > 0 {
                duration_ms = Some(end);
            }
        }
        self.plan(
            remote.as_str(),
            Plan {
                segments: vec![],
                duration_ms: if live { None } else { duration_ms },
                live,
            },
        )
    }
    fn plan(&self, key: &str, plan: Plan) -> Result<()> {
        ensure!(
            plan.segments.len() <= 65536,
            "Too many media timeline segments"
        );
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Media progress lock poisoned"))?;
        if let Some(resource) = state.resources.get(key) {
            resource.manifest.store(true, Ordering::Relaxed);
        }
        ensure!(
            state.plans.contains_key(key) || state.plans.len() < 256,
            "Too many media timelines"
        );
        state.plans.insert(key.into(), plan);
        Ok(())
    }
    pub fn snapshot(&self) -> Result<Metrics> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Media progress lock poisoned"))?;
        let mut result = Metrics::default();
        for resource in state
            .resources
            .values()
            .filter(|r| !r.manifest.load(Ordering::Relaxed))
        {
            result.completed = result
                .completed
                .saturating_add(resource.done.load(Ordering::Relaxed));
        }
        let roots: Option<Vec<_>> = self
            .roots
            .iter()
            .map(|url| {
                state.resources.get(url).filter(|r| {
                    !r.manifest.load(Ordering::Relaxed)
                        && r.total.load(Ordering::Relaxed) != UNKNOWN
                })
            })
            .collect();
        if let Some(roots) = roots {
            result.exact_total = Some(roots.iter().map(|r| r.total.load(Ordering::Relaxed)).sum());
            result.completed = roots.iter().map(|r| r.done.load(Ordering::Relaxed)).sum();
            result.transfer_complete = roots.iter().all(|r| r.complete.load(Ordering::Acquire));
        }
        let mut best = (0u64, 0u64);
        let root_leaf = self.roots.iter().any(|url| {
            state
                .plans
                .get(url)
                .is_some_and(|plan| !plan.segments.is_empty())
        });
        for (key, plan) in &state.plans {
            if root_leaf && !self.roots.contains(key) {
                continue;
            }
            result.live |= plan.live;
            if let Some(duration) = plan.duration_ms.filter(|v| *v > 0) {
                result.duration_ms = Some(result.duration_ms.unwrap_or(0).max(duration));
            }
            let mut ready = 0u64;
            let mut count = 0;
            let mut bytes = 0u64;
            for segment in &plan.segments {
                if let Some(resource) = state.resources.get(&segment.url) {
                    let done = resource.done.load(Ordering::Relaxed);
                    let total = resource.total.load(Ordering::Relaxed);
                    let complete = resource.complete.load(Ordering::Acquire);
                    bytes = bytes.saturating_add(done);
                    if complete {
                        count += 1;
                        ready += segment.duration_ms;
                    } else if total != UNKNOWN && total > 0 {
                        ready += (segment.duration_ms as f64
                            * (done as f64 / total as f64).min(1.0))
                            as u64;
                    }
                }
            }
            let score = (ready, bytes);
            if !plan.segments.is_empty() && score >= best {
                best = score;
                result.ready_ms = ready;
                result.segments_completed = count;
                result.segments_total = if plan.live {
                    None
                } else {
                    Some(plan.segments.len() as u64)
                };
                if result.exact_total.is_none() {
                    result.transfer_complete = !plan.live && count == plan.segments.len() as u64;
                }
            }
        }
        if let Some(cap) = self.cap_ms {
            result.duration_ms = Some(result.duration_ms.unwrap_or(cap).min(cap));
        }
        if result.duration_ms.is_some() {
            result.live = false;
        }
        Ok(result)
    }
}
fn milliseconds(value: &str) -> Option<u64> {
    let seconds: f64 = value.parse().ok()?;
    if !seconds.is_finite() || seconds <= 0.0 || seconds > 315_576_000.0 {
        return None;
    }
    Some((seconds * 1000.0).round() as u64)
}
pub(crate) fn iso_duration(value: &str) -> Option<u64> {
    let value = value.strip_prefix('P')?;
    let mut number = String::new();
    let mut time = false;
    let mut seconds = 0.0;
    for character in value.chars() {
        if character == 'T' && number.is_empty() {
            time = true;
            continue;
        }
        if character.is_ascii_digit() || character == '.' {
            number.push(character);
            continue;
        }
        let factor = match (time, character) {
            (false, 'D') => 86400.0,
            (true, 'H') => 3600.0,
            (true, 'M') => 60.0,
            (true, 'S') => 1.0,
            _ => return None,
        };
        let amount: f64 = number.parse().ok()?;
        if !amount.is_finite() || amount < 0.0 {
            return None;
        }
        seconds += amount * factor;
        number.clear();
    }
    if !number.is_empty() {
        return None;
    }
    milliseconds(&seconds.to_string())
}
#[derive(Default)]
pub(crate) struct Estimator {
    byte_rate: f64,
    media_rate: f64,
    previous_bytes: u64,
    previous_ms: u64,
}
impl Estimator {
    pub fn update(
        &mut self,
        stats: &Metrics,
        output_ms: u64,
        seconds: f64,
        fallback_total: Option<u64>,
        network_bytes: u64,
    ) -> (Option<u64>, bool, MediaProgress) {
        let processed = output_ms
            .max(stats.ready_ms)
            .min(stats.duration_ms.unwrap_or(u64::MAX));
        let seconds = seconds.max(0.001);
        let weight = 1.0 - (-seconds / 4.0).exp();
        let byte_rate = network_bytes.saturating_sub(self.previous_bytes) as f64 / seconds;
        let media_rate = processed.saturating_sub(self.previous_ms) as f64 / seconds;
        self.byte_rate = if self.byte_rate == 0.0 {
            byte_rate
        } else {
            self.byte_rate + weight * (byte_rate - self.byte_rate)
        };
        self.media_rate = if self.media_rate == 0.0 {
            media_rate
        } else {
            self.media_rate + weight * (media_rate - self.media_rate)
        };
        self.previous_bytes = network_bytes;
        self.previous_ms = processed;
        let sampled = stats
            .duration_ms
            .filter(|_| processed >= 1000 && stats.completed > 0)
            .map(|duration| {
                (stats.completed as f64 * duration as f64 / processed as f64)
                    .ceil()
                    .min(i64::MAX as f64) as u64
            });
        let total = stats.exact_total.or(sampled).or(fallback_total);
        let estimated = stats.exact_total.is_none() && total.is_some();
        let percent = if let Some(total) = stats.exact_total.filter(|v| *v > 0) {
            Some(100.0 * stats.completed as f64 / total as f64)
        } else if let Some(duration) = stats
            .duration_ms
            .filter(|_| processed > 0 || stats.completed == 0)
        {
            Some(100.0 * processed as f64 / duration as f64)
        } else if let Some(count) = stats
            .segments_total
            .filter(|v| *v > 0 && (stats.segments_completed > 0 || stats.completed == 0))
        {
            Some(100.0 * stats.segments_completed as f64 / count as f64)
        } else {
            total
                .filter(|v| *v > 0)
                .map(|total| 100.0 * stats.completed as f64 / total as f64)
        };
        let finalizing = stats.transfer_complete
            && (stats.duration_ms.is_none()
                || output_ms >= stats.duration_ms.unwrap_or(0).saturating_sub(500));
        let eta = if finalizing || stats.live {
            None
        } else if let Some(total) = total.filter(|_| self.byte_rate >= 1.0) {
            Some(
                ((total.saturating_sub(stats.completed) as f64 / self.byte_rate).ceil() as u64)
                    .max(1),
            )
        } else {
            stats
                .duration_ms
                .filter(|_| self.media_rate > 0.0)
                .map(|duration| {
                    ((duration.saturating_sub(processed) as f64 / self.media_rate).ceil() as u64)
                        .max(1)
                })
        };
        (
            total,
            estimated,
            MediaProgress {
                duration_ms: stats.duration_ms,
                processed_ms: processed,
                segments_total: stats.segments_total,
                segments_completed: stats.segments_completed,
                percent: if stats.live {
                    None
                } else {
                    percent.map(|p| p.clamp(0.0, 99.9))
                },
                eta_seconds: eta,
                phase: if stats.live {
                    "live"
                } else if finalizing {
                    "processing"
                } else {
                    "downloading"
                }
                .into(),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_size_vod_uses_duration_and_estimates_eta() {
        let tracker = Tracker::new(&DownloadRequest {
            url: "https://test/film.m3u8".into(),
            ..DownloadRequest::default()
        });
        tracker
            .hls(
                &Url::parse("https://test/film.m3u8").unwrap(),
                b"#EXTM3U\n#EXTINF:10,\na.jpg\n#EXTINF:20,\nb.jpg\n#EXT-X-ENDLIST\n",
            )
            .unwrap();
        tracker
            .resource(&Url::parse("https://test/a.jpg").unwrap())
            .unwrap()
            .finish(1000);
        let stats = tracker.snapshot().unwrap();
        let (size, estimated, progress) = Estimator::default().update(&stats, 0, 2.0, None, 1000);
        assert_eq!(size, Some(3000));
        assert!(estimated);
        assert_eq!(progress.percent, Some(100.0 / 3.0));
        assert_eq!(progress.eta_seconds, Some(4));
        assert_eq!(progress.segments_completed, 1);
        assert_eq!(progress.segments_total, Some(2));
    }
    #[test]
    fn live_window_has_no_fake_total_or_eta() {
        let tracker = Tracker::new(&DownloadRequest::default());
        let url = Url::parse("https://test/live").unwrap();
        tracker.hls(&url, b"#EXTM3U\n#EXTINF:10,\na.ts\n").unwrap();
        tracker
            .resource(&Url::parse("https://test/a.ts").unwrap())
            .unwrap()
            .finish(1000);
        let (_, _, progress) =
            Estimator::default().update(&tracker.snapshot().unwrap(), 10000, 2.0, None, 1000);
        assert_eq!(progress.percent, None);
        assert_eq!(progress.eta_seconds, None);
        assert_eq!(progress.phase, "live");
    }
    #[test]
    fn direct_media_size_and_replayed_bytes_are_not_double_counted() {
        let request = DownloadRequest {
            url: "https://test/video".into(),
            ..DownloadRequest::default()
        };
        let tracker = Tracker::new(&request);
        let url = Url::parse(&request.url).unwrap();
        let resource = tracker.resource(&url).unwrap();
        resource.metadata(Some(1000), "video/mp4");
        resource.done.store(400, Ordering::Relaxed);
        assert_eq!(tracker.snapshot().unwrap().exact_total, Some(1000));
        resource.reset(0);
        resource.done.store(400, Ordering::Relaxed);
        assert_eq!(tracker.snapshot().unwrap().completed, 400);
        resource.finish(1000);
        tracker.resource(&url).unwrap().finish(1000);
        assert_eq!(tracker.snapshot().unwrap().completed, 1000);
    }
    #[test]
    fn dash_duration_and_recording_cap_are_respected() {
        assert_eq!(iso_duration("P1DT2H3M4.5S"), Some(93_784_500));
        assert_eq!(iso_duration("P1M"), None);
        let tracker = Tracker::new(&DownloadRequest::default());
        tracker
            .dash(
                &Url::parse("https://test/a.mpd").unwrap(),
                &xmltree::Element::parse(b"<MPD mediaPresentationDuration=\"PT90S\"/>".as_slice())
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(tracker.snapshot().unwrap().duration_ms, Some(90000));
        let (_, _, progress) =
            Estimator::default().update(&tracker.snapshot().unwrap(), 45000, 2.0, None, 0);
        assert_eq!(progress.percent, Some(50.0));
        assert_eq!(progress.eta_seconds, Some(2));
        let request: DownloadRequest = serde_json::from_value(serde_json::json!({"url":"https://test/a.mpd","filename":"cap.mp4","media":{"container":"mp4","duration_seconds":10}})).unwrap();
        let clipped = Tracker::new(&request);
        clipped
            .dash(
                &Url::parse(&request.url).unwrap(),
                &xmltree::Element::parse(b"<MPD mediaPresentationDuration=\"PT90S\"/>".as_slice())
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(clipped.snapshot().unwrap().duration_ms, Some(10000));
    }
    #[test]
    fn selected_leaf_is_not_replaced_by_an_unrelated_timeline() {
        let request = DownloadRequest {
            url: "https://test/film".into(),
            ..DownloadRequest::default()
        };
        let tracker = Tracker::new(&request);
        tracker
            .hls(
                &Url::parse(&request.url).unwrap(),
                b"#EXTM3U\n#EXTINF:10,\na.ts\n#EXTINF:20,\nb.ts\n#EXT-X-ENDLIST\n",
            )
            .unwrap();
        tracker
            .hls(
                &Url::parse("https://test/other").unwrap(),
                b"#EXTM3U\n#EXTINF:90,\nz.ts\n#EXT-X-ENDLIST\n",
            )
            .unwrap();
        assert_eq!(tracker.snapshot().unwrap().duration_ms, Some(30000));
        tracker
            .resource(&Url::parse("https://test/a.ts").unwrap())
            .unwrap()
            .done
            .store(100, Ordering::Relaxed);
        let (_, _, progress) =
            Estimator::default().update(&tracker.snapshot().unwrap(), 0, 1.0, None, 100);
        assert_eq!(progress.percent, None);
    }
    #[test]
    fn cached_bytes_do_not_inflate_network_speed_for_eta() {
        let mut estimator = Estimator::default();
        let stats = Metrics {
            completed: 8000,
            exact_total: Some(10000),
            ..Metrics::default()
        };
        let (_, _, first) = estimator.update(&stats, 0, 1.0, None, 0);
        assert_eq!(first.eta_seconds, None);
        let (_, _, next) = estimator.update(
            &Metrics {
                completed: 9000,
                exact_total: Some(10000),
                ..Metrics::default()
            },
            0,
            1.0,
            None,
            1000,
        );
        assert_eq!(next.eta_seconds, Some(1));
    }
}
