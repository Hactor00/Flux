use std::time::Duration;

pub(crate) const IDLE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug)]
pub(crate) struct Stalled;

impl std::fmt::Display for Stalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Transfer stalled; reconnecting from the verified offset")
    }
}

impl std::error::Error for Stalled {}

#[derive(Default)]
pub(crate) struct Monitor {
    elapsed: Duration,
    bytes: u64,
    baseline: f64,
    windows: u32,
    slow_windows: u32,
}

impl Monitor {
    pub fn record(&mut self, bytes: usize, network_wait: Duration) -> bool {
        self.elapsed += network_wait;
        self.bytes = self.bytes.saturating_add(bytes as u64);
        if self.elapsed < Duration::from_secs(2) {
            return false;
        }
        let speed = self.bytes as f64 / self.elapsed.as_secs_f64();
        self.windows += 1;
        if self.windows <= 2 {
            self.baseline = speed;
        } else if speed < self.baseline * 0.1 {
            self.slow_windows += 1;
        } else {
            self.slow_windows = 0;
            self.baseline = self.baseline * 0.75 + speed * 0.25;
        }
        self.elapsed = Duration::ZERO;
        self.bytes = 0;
        self.slow_windows >= 3
    }
}
