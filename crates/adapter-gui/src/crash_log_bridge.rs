//! Responsibility: forwards log records to the installed crash reporter.
//!
//! `error!` becomes a report carrying the contexts, `warn!`/`info!` become
//! breadcrumbs, `debug!`/`trace!` stay local. The wrapped logger (stderr +
//! session file) always gets every record first, reporter or not.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::OnceLock;

use crate::crash_reporter::{CrashReporter, Report};

pub struct ReportingLogger {
    inner: Box<dyn log::Log>,
    reporter: &'static OnceLock<Box<dyn CrashReporter>>,
}

impl ReportingLogger {
    /// `reporter` may still be empty: records reach it once it is set.
    pub fn new(
        inner: Box<dyn log::Log>,
        reporter: &'static OnceLock<Box<dyn CrashReporter>>,
    ) -> Self {
        Self { inner, reporter }
    }
}

impl log::Log for ReportingLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        self.inner.log(record);
        let Some(reporter) = self.reporter.get() else {
            return;
        };
        let level = record.level();
        if level > log::Level::Info {
            return;
        }
        let message = record.args().to_string();
        // A failing reporter loses the record, never the app.
        let _ = catch_unwind(AssertUnwindSafe(|| {
            if level == log::Level::Error {
                reporter.capture(Report {
                    target: record.target(),
                    message: &message,
                    contexts: crate::crash_context::contexts(),
                });
            } else {
                reporter.breadcrumb(level, record.target(), &message);
            }
        }));
    }

    fn flush(&self) {
        self.inner.flush();
    }
}
