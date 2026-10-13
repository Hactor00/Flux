use anyhow::{bail, Result};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;
pub(crate) struct Limiter {
    rate: AtomicU64,
    state: Mutex<(f64, Instant)>,
    changed: Notify,
}
impl Limiter {
    pub fn new(rate: u64) -> Self {
        Self {
            rate: AtomicU64::new(rate),
            state: Mutex::new((0.0, Instant::now())),
            changed: Notify::new(),
        }
    }
    pub async fn set_rate(&self, rate: u64) {
        if self.rate.load(Ordering::Acquire) == rate {
            return;
        }
        let mut state = self.state.lock().await;
        self.rate.store(rate, Ordering::Release);
        *state = (0.0, Instant::now());
        self.changed.notify_waiters();
    }
    pub async fn acquire(&self, bytes: usize, cancel: &CancellationToken) -> Result<()> {
        if self.rate.load(Ordering::Acquire) == 0 {
            return Ok(());
        }
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let wait = {
                let mut state = self.state.lock().await;
                let rate = self.rate.load(Ordering::Acquire);
                if rate == 0 {
                    return Ok(());
                }
                let now = Instant::now();
                state.0 = (state.0 + now.duration_since(state.1).as_secs_f64() * rate as f64)
                    .min(rate.max(65536) as f64);
                state.1 = now;
                if state.0 >= bytes as f64 {
                    state.0 -= bytes as f64;
                    return Ok(());
                }
                Duration::from_secs_f64((bytes as f64 - state.0) / rate as f64)
            };
            tokio::select! {_=cancel.cancelled()=>bail!("Cancelled"),_=changed=>{},_=tokio::time::sleep(wait)=>{}}
        }
    }
}
