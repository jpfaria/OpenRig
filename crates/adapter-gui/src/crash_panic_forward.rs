//! Responsibility: forwards panics to the installed crash reporter.
//!
//! Chained after the session-log hook, so the report on disk is written
//! before any network attempt.

use std::time::Duration;

use crate::crash_reporter::Report;

const PANIC_FLUSH: Duration = Duration::from_secs(2);

pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(reporter) = crate::crash_reporting::reporter() {
            let message = format!("panic: {info}");
            reporter.capture(Report {
                target: "panic",
                message: &message,
                contexts: crate::crash_context::contexts(),
            });
            reporter.flush(PANIC_FLUSH);
        }
        previous(info);
    }));
}
