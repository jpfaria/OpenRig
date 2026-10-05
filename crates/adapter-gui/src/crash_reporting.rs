//! Responsibility: installs the crash reporter `config.yaml` asks for.
//!
//! Release builds bake a default DSN at compile time (`SENTRY_DSN`, a CI
//! secret); `crash_reporting.dsn` in `config.yaml` overrides it and
//! `provider: none` turns reporting off. Dev builds have no default.

use std::sync::OnceLock;
use std::time::Duration;

use infra_filesystem::crash_reporting_config::{CrashReportingConfig, CrashReportingProvider};

use crate::crash_reporter::CrashReporter;
use crate::crash_reporter_sentry::SentryReporter;

const EXIT_FLUSH: Duration = Duration::from_secs(2);

static REPORTER: OnceLock<Box<dyn CrashReporter>> = OnceLock::new();

#[derive(Debug, PartialEq)]
pub enum Backend {
    Sentry { dsn: String },
}

/// Pure: which backend to start, if any. A DSN in the config wins over the
/// build's; an empty DSN counts as none.
pub fn choose(config: &CrashReportingConfig, build_dsn: Option<&str>) -> Option<Backend> {
    match config.provider {
        CrashReportingProvider::None => None,
        CrashReportingProvider::Sentry => {
            let dsn = config.dsn.as_deref().or(build_dsn)?;
            (!dsn.is_empty()).then(|| Backend::Sentry {
                dsn: dsn.to_string(),
            })
        }
    }
}

/// The slot the log bridge reads; empty until `init` installs a reporter.
pub fn slot() -> &'static OnceLock<Box<dyn CrashReporter>> {
    &REPORTER
}

pub fn reporter() -> Option<&'static dyn CrashReporter> {
    REPORTER.get().map(|r| r.as_ref())
}

/// Flushes queued reports when dropped; keep it alive for the whole run.
pub struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(reporter) = reporter() {
            reporter.flush(EXIT_FLUSH);
        }
    }
}

pub fn init(config: &CrashReportingConfig) -> Option<Guard> {
    let reporter: Box<dyn CrashReporter> = match choose(config, option_env!("SENTRY_DSN"))? {
        Backend::Sentry { dsn } => Box::new(SentryReporter::start(&dsn)?),
    };
    REPORTER.set(reporter).ok()?;
    crate::crash_panic_forward::install();
    Some(Guard)
}
