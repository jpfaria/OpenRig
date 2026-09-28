//! Responsibility: installs off-thread runtime builds as soon as they land.
//!
//! Issue #967. A scene switch, a preset switch or any live edit builds the
//! chain's next runtime on the control worker in a few milliseconds — and then
//! waited for the 200 ms error-poll tick to be swapped in, so the owner heard
//! the change up to a fifth of a second after pressing the switch. This timer
//! only services that install (`RuntimeControl::apply_finished_rebuilds`); a
//! tick with nothing queued is two empty queue checks.

use std::rc::Rc;
use std::time::Duration;

use application::runtime_control::RuntimeControl;
use slint::{Timer, TimerMode};

/// How often a finished build is looked for — well under a period of the
/// slowest thing the owner can press twice.
pub(crate) const INSTALL_TICK: Duration = Duration::from_millis(5);

/// Start the install timer. The caller keeps it alive for the app's lifetime.
pub(crate) fn start(control: Rc<dyn RuntimeControl>) -> Timer {
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, INSTALL_TICK, move || {
        control.apply_finished_rebuilds();
    });
    timer
}

#[cfg(test)]
#[path = "rebuild_install_timer_tests.rs"]
mod tests;
