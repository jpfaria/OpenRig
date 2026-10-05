//! Responsibility: publishes the session's running audio setup to crash reporting.

use std::cell::RefCell;

use infra_cpal::ProjectRuntimeController;

use crate::state::ProjectSession;

pub(crate) fn publish_runtime_context(
    project_runtime: &RefCell<Option<ProjectRuntimeController>>,
    session: &ProjectSession,
) {
    let live_rate = project_runtime.borrow().as_ref().map(|r| r.sample_rate());
    let context = crate::crash_context_audio::audio_context(
        &session.project.borrow(),
        &session.io_bindings.borrow(),
        live_rate,
    );
    crate::crash_context::publish_audio(context);
}

#[cfg(test)]
#[path = "crash_context_publish_tests.rs"]
mod tests;
