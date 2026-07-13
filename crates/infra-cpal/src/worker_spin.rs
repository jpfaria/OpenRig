//! Issue #781 — the dsp-worker's empty-ring wait policy.
//!
//! While the ring holds no new buffer the worker either spins (keeps the core,
//! and the memory-bound NAM weight cache, warm) or sleeps (lets the core be
//! reused → the weights cool out of cache). Proved headless in
//! `engine/tests/issue_781_offcpu_stall`: sleeping the inter-buffer gap makes
//! the next NAM inference pay a 2.7x cold-cache tail (569us vs a 209us hot
//! floor) that crosses the period → the residual underrun; spinning the SAME
//! gap holds compute at the hot floor (0 cold tail).
//!
//! So the worker must spin the ACTIVE-playback gap (a buffer arrives roughly
//! every period, plus jitter) and only fall back to sleeping once the stream is
//! genuinely paused/drained — otherwise it burns a core forever when idle.

/// Buffer periods the worker keeps spinning (warm) after the last buffer before
/// it starts sleeping. During playback a buffer arrives every period, so a
/// window of two periods covers the gap plus scheduling jitter and the NAM
/// weight cache never cools mid-stream; beyond it the chain is paused/drained
/// and the worker yields the core. (#670 spun only 35% of ONE period and then
/// slept the rest of every inter-buffer gap, which is exactly when the cache
/// cooled — the #781 residual underrun.)
pub(crate) const SPIN_WARM_PERIODS: u64 = 2;

/// Should the worker keep spinning (warm) rather than sleep, given how long it
/// has been waiting for the next buffer? Spinning past the active-playback gap
/// keeps the NAM weights cache-resident and kills the cold-cache tail (#781).
#[inline]
pub(crate) fn worker_should_spin(waited_ns: u64, period_ns: u64) -> bool {
    waited_ns < period_ns.saturating_mul(SPIN_WARM_PERIODS)
}

/// One empty-ring wait step. `idle_since` records when the ring first went
/// empty; while inside the active-playback window the worker spins (keeping the
/// core and the NAM weight cache warm, #781), and only sleeps once the chain is
/// genuinely paused/drained so it does not burn a core forever.
pub(crate) fn worker_wait(idle_since: &mut Option<std::time::Instant>, period_ns: u64) {
    let since = *idle_since.get_or_insert_with(std::time::Instant::now);
    if worker_should_spin(since.elapsed().as_nanos() as u64, period_ns) {
        std::hint::spin_loop();
    } else {
        std::thread::sleep(std::time::Duration::from_micros(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERIOD_NS: u64 = 1_333_000; // 64 frames @ 48 kHz

    #[test]
    fn spins_through_the_active_playback_gap() {
        // During playback a buffer arrives ~every period; the worker waits up to
        // a full period (plus jitter) between buffers and MUST stay spinning the
        // whole time so the NAM weights never cool (#781). Half a period is
        // squarely inside an active gap.
        assert!(
            worker_should_spin(PERIOD_NS / 2, PERIOD_NS),
            "must keep spinning half a period into an active-playback gap — \
             sleeping here cools the NAM cache and the next inference pays the \
             cold-cache tail (the residual underrun)"
        );
        assert!(
            worker_should_spin(PERIOD_NS, PERIOD_NS),
            "must still be spinning a full period in (a late-but-active buffer)"
        );
    }

    #[test]
    fn sleeps_once_genuinely_idle() {
        // Far past any active gap: the chain is paused/drained, so the worker
        // must yield the core instead of burning it forever.
        assert!(
            !worker_should_spin(PERIOD_NS * 10, PERIOD_NS),
            "must sleep once genuinely idle (paused/drained), not spin forever"
        );
    }
}
