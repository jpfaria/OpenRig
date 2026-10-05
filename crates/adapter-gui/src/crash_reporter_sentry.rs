//! Responsibility: implements `CrashReporter` on top of Sentry.
//!
//! The only production file that names the vendor; swapping it means a new
//! sibling implementation plus one arm in `crash_reporting::choose`.

use std::collections::BTreeMap;
use std::time::Duration;

use sentry::protocol::{Breadcrumb, Context, Event, Level};
use serde_json::Value;

use crate::crash_reporter::{CrashReporter, Report};

pub struct SentryReporter {
    _client: Option<sentry::ClientInitGuard>,
}

impl SentryReporter {
    /// Starts a Sentry client for `dsn`. A DSN Sentry cannot parse turns
    /// reporting off with a warning instead of panicking at startup.
    pub fn start(dsn: &str) -> Option<Self> {
        let parsed = match dsn.parse::<sentry::types::Dsn>() {
            Ok(parsed) => parsed,
            Err(e) => {
                log::warn!("crash reporting disabled: invalid sentry dsn ({e})");
                return None;
            }
        };
        let mut options = Self::options();
        options.dsn = Some(parsed);
        Some(Self {
            _client: Some(sentry::init(options)),
        })
    }

    /// Reports through whatever client the current hub already has.
    pub fn on_current_hub() -> Self {
        Self { _client: None }
    }

    /// The client options every build sends with.
    pub fn options() -> sentry::ClientOptions {
        let mut options = sentry::ClientOptions::new();
        options.release = Some(concat!("openrig@", env!("CARGO_PKG_VERSION")).into());
        options.attach_stacktrace = true;
        options
    }
}

impl CrashReporter for SentryReporter {
    fn capture(&self, report: Report<'_>) {
        let mut event = Event {
            level: Level::Error,
            logger: Some(report.target.to_string()),
            message: Some(report.message.to_string()),
            ..Default::default()
        };
        for (name, value) in report.contexts {
            event.contexts.insert(name, as_context(value));
        }
        sentry::capture_event(event);
    }

    fn breadcrumb(&self, level: log::Level, target: &str, message: &str) {
        sentry::add_breadcrumb(Breadcrumb {
            level: match level {
                log::Level::Error => Level::Error,
                log::Level::Warn => Level::Warning,
                log::Level::Info => Level::Info,
                log::Level::Debug | log::Level::Trace => Level::Debug,
            },
            category: Some(target.to_string()),
            message: Some(message.to_string()),
            ..Default::default()
        });
    }

    fn flush(&self, timeout: Duration) {
        if let Some(client) = sentry::Hub::current().client() {
            client.flush(Some(timeout));
        }
    }
}

fn as_context(value: Value) -> Context {
    match value {
        Value::Object(map) => Context::Other(map.into_iter().collect::<BTreeMap<_, _>>()),
        other => Context::Other(BTreeMap::from([("value".to_string(), other)])),
    }
}
