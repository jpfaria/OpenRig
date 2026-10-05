//! Responsibility: starts the Sentry client when the build carries a DSN (#1060).
//!
//! The DSN is baked in at compile time (`SENTRY_DSN`, a CI secret on
//! release builds). Dev builds have none, so nothing is ever sent.

pub type Guard = sentry::ClientInitGuard;

pub fn init() -> Option<Guard> {
    let dsn = option_env!("SENTRY_DSN").filter(|d| !d.is_empty())?;
    Some(sentry::init((dsn, options())))
}

/// The client options every build sends with; tests reuse them (#1070).
pub fn options() -> sentry::ClientOptions {
    let mut options = sentry::ClientOptions::new();
    options.release = Some(concat!("openrig@", env!("CARGO_PKG_VERSION")).into());
    options.attach_stacktrace = true;
    // #1070: every event carries the audio setup and the host sample.
    options.before_send = Some(std::sync::Arc::new(crate::sentry_event_context::attach));
    options
}
