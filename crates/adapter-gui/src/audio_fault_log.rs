//! Responsibility: logs audio faults at the level Sentry turns into events (#1065).
//!
//! The message text stays constant per chain so Sentry groups every
//! occurrence into one issue; the counts go in a preceding `warn!`,
//! which Sentry keeps as a breadcrumb on the event. Called from GUI
//! timers only, never from the audio thread.

pub fn report_overload(chain: &str, new_xruns: u64, new_underruns: u64) {
    log::warn!("audio overload detail on chain '{chain}': {new_xruns} new xrun(s), {new_underruns} new underrun(s)");
    log::error!("audio overload on chain '{chain}' (xrun/underrun)");
}

pub fn report_backend_lost() {
    log::error!("audio backend unhealthy, attempting reconnection");
}
