//! Responsibility: publishes the session's running audio setup to crash reporting (#1070).

use std::cell::RefCell;

use infra_cpal::ProjectRuntimeController;

use crate::state::ProjectSession;

pub(crate) fn publish_runtime_context(
    project_runtime: &RefCell<Option<ProjectRuntimeController>>,
    session: &ProjectSession,
) {
    let live_rate = project_runtime.borrow().as_ref().map(|r| r.sample_rate());
    let context = crate::sentry_audio_context::audio_context(
        &session.project.borrow(),
        &session.io_bindings.borrow(),
        live_rate,
    );
    crate::sentry_event_context::publish_audio(context);
}

#[cfg(test)]
#[path = "sentry_runtime_publish_tests.rs"]
mod tests;
