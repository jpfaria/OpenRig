//! Responsibility: runs the stepped-input restart on its own timer.
//!
//! A stepped input is a buzz the owner hears until the device restart, so the
//! restart is looked for every [`STEPPED_TICK`] instead of waiting for the
//! audio health tick. A tick with no stepped chain reads one flag per runtime.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use application::live_source::LiveSource;
use application::runtime_control::RuntimeControl;
use slint::{Timer, TimerMode};

use crate::stepped_input_tick::{stepped_input_tick, SteppedRestarts};

/// How often a stepped chain is looked for.
pub(crate) const STEPPED_TICK: Duration = Duration::from_millis(250);

/// Start the timer. The caller keeps it alive for the app's lifetime.
pub(crate) fn start(live: Rc<dyn LiveSource>, control: Rc<dyn RuntimeControl>) -> Timer {
    let restarts = RefCell::new(SteppedRestarts::default());
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, STEPPED_TICK, move || {
        stepped_input_tick(live.as_ref(), control.as_ref(), &restarts);
    });
    timer
}

#[cfg(test)]
#[path = "stepped_input_timer_tests.rs"]
mod tests;
