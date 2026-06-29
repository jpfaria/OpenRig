//! Decision logic for the `[ui-stall]` event-loop watchdog (#693, #721).
//!
//! The watchdog (in [`crate::desktop_app`]) runs on its own thread that wakes
//! every `tick` (250 ms) and measures the gap since the Slint event loop last
//! answered a posted heartbeat. A large gap was warned on directly — but that
//! conflates two very different situations:
//!
//! * a **genuine freeze**: the UI thread is stuck, so it stops answering while
//!   the rest of the process (this watchdog thread included) keeps running on
//!   schedule; and
//! * an **OS-induced pause**: macOS App Nap / display sleep / timer coalescing
//!   parks the *whole process* while it sits idle in the background, so the UI
//!   loop answers late through no fault of its own (#721).
//!
//! The distinguisher needs no per-thread CPU accounting or platform APIs: if
//! only the event loop is frozen, this watchdog thread still wakes about every
//! `tick`, so its own observed wake interval stays small while the UI gap grows.
//! If the process was parked, the watchdog thread was frozen too, so its wake
//! interval balloons in lockstep with the UI gap. A genuine stall is therefore
//! "UI gap over threshold *and* the watchdog itself woke on time".

use std::time::Duration;

/// Whether a measured event-loop `ui_gap` is a genuine UI freeze rather than
/// the OS parking the whole process while idle in the background.
///
/// * `ui_gap` — how long the event loop has gone without answering.
/// * `watchdog_wake_interval` — how long this watchdog thread actually slept on
///   its last iteration. ~`tick` under normal scheduling; balloons toward
///   `ui_gap` when the OS parked the process.
/// * `tick` — the watchdog's nominal wake period.
/// * `threshold` — the gap above which a freeze is worth reporting.
pub fn is_genuine_ui_stall(
    ui_gap: Duration,
    watchdog_wake_interval: Duration,
    tick: Duration,
    threshold: Duration,
) -> bool {
    if ui_gap < threshold {
        return false;
    }
    // The watchdog thread was itself parked for far longer than its tick, in
    // lockstep with the UI gap → the OS suspended the whole process; the event
    // loop never actually froze. Allow a few ticks of ordinary scheduling
    // jitter before deciding the thread "woke on time".
    watchdog_wake_interval <= tick.saturating_mul(2)
}
