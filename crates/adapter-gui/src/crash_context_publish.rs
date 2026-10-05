//! Responsibility: publishes the session's running audio setup to crash reporting.

use crate::state::ProjectSession;

/// `live_rate` is the running runtime's rate (`None` with nothing running);
/// `backend` names the audio host. Both come from the module that owns the
/// runtime, so this one never sees the backend crate.
pub(crate) fn publish_runtime_context(
    session: &ProjectSession,
    live_rate: Option<u32>,
    backend: &str,
) {
    let context = crate::crash_context_audio::audio_context(
        &session.project.borrow(),
        &session.io_bindings.borrow(),
        live_rate,
        backend,
    );
    crate::crash_context::publish_audio(context);
}

#[cfg(test)]
#[path = "crash_context_publish_tests.rs"]
mod tests;
