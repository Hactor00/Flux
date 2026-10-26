use std::time::Duration;

pub(crate) const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub(crate) struct Stalled;

impl std::fmt::Display for Stalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Transfer stalled; reconnecting from the verified offset")
    }
}

impl std::error::Error for Stalled {}

pub(crate) struct Rate {
    samples: std::collections::VecDeque<(tokio::time::Instant, u64)>,
}
impl Rate {
    pub fn new(bytes: u64) -> Self {
        Self {
            samples: [(tokio::time::Instant::now(), bytes)].into(),
        }
    }
    pub fn sample(&mut self, now: tokio::time::Instant, bytes: u64) -> u64 {
        self.samples.push_back((now, bytes));
        while self.samples.len() > 2
            && now.duration_since(self.samples[1].0) >= Duration::from_secs(3)
        {
            self.samples.pop_front();
        }
        let (at, previous) = self.samples[0];
        let elapsed = now.duration_since(at).as_secs_f64();
        if elapsed < 0.25 {
            return 0;
        }
        (bytes.saturating_sub(previous) as f64 / elapsed) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rate_smooths_bursts_and_reaches_zero_after_a_real_stop() {
        let mut rate = Rate::new(0);
        let start = rate.samples[0].0;
        assert_eq!(rate.sample(start, 0), 0);
        assert_eq!(rate.sample(start + Duration::from_secs(1), 1000), 1000);
        assert_eq!(rate.sample(start + Duration::from_secs(2), 1000), 500);
        assert_eq!(rate.sample(start + Duration::from_secs(3), 3000), 1000);
        assert_eq!(rate.sample(start + Duration::from_secs(6), 3000), 0);
    }
}
