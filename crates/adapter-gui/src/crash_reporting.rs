//! Responsibility: starts the Sentry client when the build carries a DSN (#1060).
//!
//! The DSN is baked in at compile time (`SENTRY_DSN`, a CI secret on
//! release builds). Dev builds have none, so nothing is ever sent.

pub type Guard = sentry::ClientInitGuard;

pub fn init() -> Option<Guard> {
    let dsn = option_env!("SENTRY_DSN").filter(|d| !d.is_empty())?;
    let mut options = sentry::ClientOptions::new();
    options.release = Some(concat!("openrig@", env!("CARGO_PKG_VERSION")).into());
    options.attach_stacktrace = true;
    Some(sentry::init((dsn, options)))
}
