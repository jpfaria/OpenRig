//! Responsibility: wires the per-chain latency probe.
//! Latency probe wiring — installs the per-chain latency badge's click
//! handler and the expiry timer.
//!
//! Owns no UI state of its own: the click handler runs the engine probe
//! synchronously on a fresh, isolated runtime (see [`engine::probe`])
//! and writes the measurement straight onto the chain item; the timer
//! only manages the 10-second display window before clearing the badge.
//!
//! Lives in its own module — does not bloat `lib.rs` (issue #276).

use application::live_source::LiveSource;

use crate::state::ProjectSession;
use crate::AppWindow;
use crate::ProjectChainItem;
use slint::{Timer, TimerMode, VecModel, Weak};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Per-chain-index expiry timestamps — "show the measured latency for
/// this chain until this instant". Owned by `lib.rs` and shared with
/// both the click handler and the expiry timer.
pub type ProbeWindows = Rc<RefCell<HashMap<usize, Instant>>>;

/// Construct an empty [`ProbeWindows`] map.
pub fn new_windows() -> ProbeWindows {
    Rc::new(RefCell::new(HashMap::new()))
}

/// Install the click handler on the sonar button.
///
/// Each click runs [`engine::probe::measure_chain_dsp_latency_ms`]
/// against the current chain and writes the result onto the chain
/// model item, opening a 10-second display window managed by
/// [`install_expiry_timer`].
pub fn install_handler(
    window: &AppWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_chains: Rc<VecModel<ProjectChainItem>>,
    probe_windows: ProbeWindows,
    live: Rc<dyn LiveSource>,
) {
    window.on_probe_chain_latency(move |index| {
        let session_borrow = project_session.borrow();
        let Some(session) = session_borrow.as_ref() else {
            return;
        };
        // #829: rate/buffer resolution and the probe itself live in
        // `application::query_latency`, shared with `openrig://chains/{id}/latency`
        // — the badge and every transport measure the same way (#723).
        crate::latency_probe_run::probe_chain_latency(
            session,
            live.as_ref(),
            &project_chains,
            &probe_windows,
            index as usize,
            Instant::now(),
        );
    });
}

/// Start a repeating 500 ms timer that clears expired badges.
///
/// The returned [`Timer`] must be kept alive by the caller — dropping
/// it stops the timer.
pub fn install_expiry_timer(
    weak_window: Weak<AppWindow>,
    project_chains: Rc<VecModel<ProjectChainItem>>,
    probe_windows: ProbeWindows,
) -> Timer {
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(500), move || {
        if weak_window.upgrade().is_none() {
            return;
        }
        crate::latency_badge_expiry::clear_expired_badges(
            &project_chains,
            &probe_windows,
            Instant::now(),
        );
    });
    timer
}
