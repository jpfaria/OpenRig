//! Responsibility: defines the vendor-neutral contract every crash reporter implements.
//!
//! The app only ever talks to `CrashReporter`; which vendor sits behind it
//! is a `config.yaml` choice made once at startup.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::Value;

/// One error worth a report: where it came from, what it said, and the
/// contexts (audio setup, host) at that moment.
pub struct Report<'a> {
    pub target: &'a str,
    pub message: &'a str,
    pub contexts: BTreeMap<String, Value>,
}

/// Best effort by contract: an implementation never panics or blocks the
/// caller on network I/O; a failing vendor just loses the report.
pub trait CrashReporter: Send + Sync {
    fn capture(&self, report: Report<'_>);
    fn breadcrumb(&self, level: log::Level, target: &str, message: &str);
    fn flush(&self, timeout: Duration);
}
