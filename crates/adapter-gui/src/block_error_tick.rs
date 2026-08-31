//! Responsibility: decides what one block-error tick has to report.
//!
//! Split out of `desktop_app_polling` (#913). The 200 ms tick does three
//! things and the ORDER is what makes the app stay responsive: it installs the
//! chain rebuilds the control worker finished off-thread (#672), it runs the
//! VST3 teardowns the worker deferred to this thread (#778 — dropping a plugin
//! off-main crashes), and only then reads the errors.
//!
//! The read is DRAINING: whatever it takes, nobody else will see. The queue is
//! lock-free and dropped from the audio thread when full, so during an error
//! storm the UI only ever sees a fraction — that is intentional, and it is why
//! only the FIRST message is surfaced.

use application::live_source::LiveSource;
use application::runtime_control::RuntimeControl;

/// Run one tick and return the message to show, if any.
pub(crate) fn error_tick(live: &dyn LiveSource, control: &dyn RuntimeControl) -> Option<String> {
    control.apply_finished_rebuilds();
    project::vst3_editor::drain_deferred_vst3_teardowns();
    let errors = live.block_errors()?;
    errors.first().map(|error| error.message.clone())
}

#[cfg(test)]
#[path = "block_error_tick_tests.rs"]
mod tests;
