//! Regression for #721: the `[ui-stall]` watchdog cried wolf while the app
//! was idle in the background.
//!
//! The watchdog runs on its own thread that wakes every ~250 ms and measures
//! the gap since the Slint event loop last answered. macOS App Nap / display
//! sleep / timer coalescing parks an idle background process, so a heartbeat
//! lands ~800 ms late even though the UI thread never froze. The old watchdog
//! warned on that wall-clock gap alone, indistinguishable from a real freeze.
//!
//! The fix: a genuine freeze leaves the rest of the process running, so the
//! watchdog thread keeps waking on schedule while the UI gap grows. An OS pause
//! freezes the watchdog thread too, so its own wake interval balloons in
//! lockstep with the UI gap. `is_genuine_ui_stall` decides on that basis.

use std::time::Duration;

use adapter_gui::ui_stall::is_genuine_ui_stall;

const TICK: Duration = Duration::from_millis(250);
const THRESHOLD: Duration = Duration::from_millis(600);

#[test]
fn os_suspension_gap_is_not_a_stall() {
    // The exact #721 symptom: ~800 ms UI gap while idle/backgrounded, but the
    // watchdog thread was parked for the same ~800 ms — the whole process slept.
    assert!(
        !is_genuine_ui_stall(
            Duration::from_millis(800),
            Duration::from_millis(790),
            TICK,
            THRESHOLD,
        ),
        "a process-wide OS pause (watchdog parked in lockstep) must not be reported as a stall",
    );
}

#[test]
fn frozen_loop_with_live_watchdog_is_a_real_stall() {
    // A genuine freeze: the UI loop is stuck, but the watchdog thread kept
    // waking on schedule (~one tick), so it can see the loop is unresponsive.
    assert!(
        is_genuine_ui_stall(
            Duration::from_millis(800),
            Duration::from_millis(255),
            TICK,
            THRESHOLD,
        ),
        "a large UI gap while the watchdog woke on time is a genuine freeze",
    );
}

#[test]
fn gap_under_threshold_never_warns() {
    assert!(
        !is_genuine_ui_stall(
            Duration::from_millis(120),
            Duration::from_millis(250),
            TICK,
            THRESHOLD,
        ),
        "gaps below the warn threshold are normal scheduling jitter",
    );
}
