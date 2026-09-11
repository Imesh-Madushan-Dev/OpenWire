//! Lock-free traffic counters.
//!
//! The relay loops touch these on every buffer flush, so they are plain atomics
//! with `Relaxed` ordering - the numbers are advisory telemetry, not a
//! synchronisation primitive.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::models::StatsSnapshot;

#[derive(Debug, Default)]
pub struct Metrics {
    up_bytes: AtomicU64,
    down_bytes: AtomicU64,
    active: AtomicU64,
    total: AtomicU64,
    /// `u32::MAX` is the sentinel for "not measured yet".
    latency_ms: AtomicU64,
}

const NO_LATENCY: u64 = u64::MAX;

impl Metrics {
    pub fn new() -> Self {
        Self {
            latency_ms: AtomicU64::new(NO_LATENCY),
            ..Default::default()
        }
    }

    pub fn add_up(&self, n: u64) {
        self.up_bytes.fetch_add(n, Ordering::Relaxed);
    }

    pub fn add_down(&self, n: u64) {
        self.down_bytes.fetch_add(n, Ordering::Relaxed);
    }

    pub fn open_connection(&self) {
        self.active.fetch_add(1, Ordering::Relaxed);
        self.total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn close_connection(&self) {
        // Saturating: a double-close must never wrap to u64::MAX.
        let _ = self
            .active
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_sub(1))
            });
    }

    pub fn set_latency(&self, ms: Option<u32>) {
        self.latency_ms
            .store(ms.map_or(NO_LATENCY, u64::from), Ordering::Relaxed);
    }

    pub fn latency(&self) -> Option<u32> {
        match self.latency_ms.load(Ordering::Relaxed) {
            NO_LATENCY => None,
            v => Some(v as u32),
        }
    }

    /// Zeroes the byte and connection counters at the start of a session.
    pub fn reset(&self) {
        self.up_bytes.store(0, Ordering::Relaxed);
        self.down_bytes.store(0, Ordering::Relaxed);
        self.active.store(0, Ordering::Relaxed);
        self.total.store(0, Ordering::Relaxed);
        self.latency_ms.store(NO_LATENCY, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            up_bytes: self.up_bytes.load(Ordering::Relaxed),
            down_bytes: self.down_bytes.load(Ordering::Relaxed),
            up_bps: 0,
            down_bps: 0,
            active_connections: self.active.load(Ordering::Relaxed),
            total_connections: self.total.load(Ordering::Relaxed),
            latency_ms: self.latency(),
            uptime_secs: 0,
        }
    }
}

/// Turns cumulative byte totals into a per-second rate.
#[derive(Debug, Default)]
pub struct RateSampler {
    last_up: u64,
    last_down: u64,
}

impl RateSampler {
    /// `elapsed_ms` is the real gap between ticks, so a stalled timer does not
    /// inflate the reported speed.
    pub fn sample(&mut self, snap: &mut StatsSnapshot, elapsed_ms: u64) {
        let elapsed_ms = elapsed_ms.max(1);
        snap.up_bps = (snap.up_bytes.saturating_sub(self.last_up)) * 1000 / elapsed_ms;
        snap.down_bps = (snap.down_bytes.saturating_sub(self.last_down)) * 1000 / elapsed_ms;
        self.last_up = snap.up_bytes;
        self.last_down = snap.down_bytes;
    }

    pub fn reset(&mut self) {
        self.last_up = 0;
        self.last_down = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_and_rates_behave() {
        let m = Metrics::new();
        assert_eq!(m.latency(), None);

        m.add_up(1_000);
        m.add_down(4_000);
        m.open_connection();
        m.open_connection();
        m.close_connection();
        m.close_connection();
        // Extra close must not wrap.
        m.close_connection();
        m.set_latency(Some(42));

        let mut snap = m.snapshot();
        assert_eq!(snap.active_connections, 0);
        assert_eq!(snap.total_connections, 2);
        assert_eq!(snap.latency_ms, Some(42));

        let mut sampler = RateSampler::default();
        sampler.sample(&mut snap, 500);
        // 4000 bytes in half a second = 8000 B/s.
        assert_eq!(snap.down_bps, 8_000);
        assert_eq!(snap.up_bps, 2_000);

        // No new traffic on the next tick means zero rate, not a repeat.
        let mut snap2 = m.snapshot();
        sampler.sample(&mut snap2, 1_000);
        assert_eq!(snap2.down_bps, 0);
    }
}
