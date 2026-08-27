//! Responsibility: warns when the Slint event loop stops answering.
//!
//! #693: a background thread posts a heartbeat into the event loop every
//! 250 ms; if the loop stops answering past ~600 ms a `[ui-stall]` warn names
//! the freeze with its duration, so real-session stalls self-report.
//!
//! #721: a large gap alone is ambiguous. A genuine freeze leaves the rest of
//! the process (this thread included) running, so the watchdog keeps waking on
//! schedule while the UI gap grows. An OS pause (macOS App Nap / display sleep
//! / timer coalescing on an idle background app) parks the whole process, so
//! this thread is frozen too and its own wake interval balloons in lockstep
//! with the gap. [`crate::ui_stall::is_genuine_ui_stall`] warns only in the
//! former case, so idle/backgrounded pauses no longer cry wolf.

/// Spawn the watchdog thread. It lives for the process — there is nothing to
/// join, and it costs one 250 ms wake.
pub(crate) fn spawn() {
    use crate::ui_stall::is_genuine_ui_stall;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    const TICK: Duration = Duration::from_millis(250);
    const THRESHOLD: Duration = Duration::from_millis(600);

    let beat = Arc::new(AtomicU64::new(0));
    let beat_bg = Arc::clone(&beat);
    std::thread::Builder::new()
        .name("ui-watchdog".into())
        .spawn(move || {
            let start = Instant::now();
            let mut warned_at: u64 = 0;
            let mut last_wake = Instant::now();
            loop {
                std::thread::sleep(TICK);
                // How long this thread was actually away: ~TICK under
                // normal scheduling, but it balloons toward the UI gap when
                // the OS parked the whole process.
                let now = Instant::now();
                let wake_interval = now.duration_since(last_wake);
                last_wake = now;

                let now_ms = start.elapsed().as_millis() as u64;
                let seen = beat_bg.load(Ordering::Relaxed);
                let gap = Duration::from_millis(now_ms.saturating_sub(seen));
                if seen > 0
                    && seen != warned_at
                    && is_genuine_ui_stall(gap, wake_interval, TICK, THRESHOLD)
                {
                    log::warn!(
                        "[ui-stall] event loop unresponsive for ~{}ms",
                        gap.as_millis()
                    );
                    warned_at = seen;
                }
                let beat_ui = Arc::clone(&beat_bg);
                let _ = slint::invoke_from_event_loop(move || {
                    // Runs ON the event loop: the stored instant is the
                    // moment the loop actually answered.
                    beat_ui.store(start.elapsed().as_millis() as u64, Ordering::Relaxed);
                });
            }
        })
        .ok();
}
