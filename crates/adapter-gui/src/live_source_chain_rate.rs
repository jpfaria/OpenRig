//! Responsibility: serves the sample rate a chain is running at.

use std::cell::RefCell;
use std::rc::Rc;

use application::live_source::LiveSource;
use domain::ids::ChainId;
use domain::io_binding::IoBinding;
use infra_cpal::ProjectRuntimeController;
use project::project::Project;

use crate::state::ProjectSession;

/// The rate one chain's streams run at (live controller) or would be opened at
/// (resolved from the project + this machine's bindings, exactly as
/// `build_streams` does). Keyed by the chain's own id, so a sibling chain never
/// leaks into the answer (`CLAUDE.md` LAW).
pub(crate) fn resolve_chain_rate(
    runtime: &RefCell<Option<ProjectRuntimeController>>,
    project: &Project,
    io_bindings: &[IoBinding],
    chain: &ChainId,
) -> Option<f32> {
    if let Some(controller) = runtime.borrow().as_ref() {
        return Some(controller.sample_rate() as f32);
    }
    infra_cpal::resolve_project_chain_sample_rates(project, io_bindings)
        .ok()
        .and_then(|rates| rates.get(chain).copied())
}

/// #127: the latency badge's read seam — the rate the probe must run at.
///
/// Its own `LiveSource` because it is asked from the chains screen, where the
/// project is behind the app's session cell rather than borrowed for the call
/// (the shape [`GuiLiveSource`] takes). Answers only `chain_sample_rate`; every
/// other reading stays at the trait's default.
pub(crate) struct ChainRateLiveSource {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
}

impl LiveSource for ChainRateLiveSource {
    fn chain_sample_rate(&self, chain: &ChainId) -> Option<f32> {
        let borrow = self.project_session.borrow();
        let session = borrow.as_ref()?;
        let project = session.project.borrow();
        let bindings = session.io_bindings.borrow();
        resolve_chain_rate(&self.runtime, &project, &bindings, chain)
    }
}

/// Build the latency badge's read seam over the app's shared handles.
pub(crate) fn chain_rate_live_source(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) -> Rc<dyn LiveSource> {
    Rc::new(ChainRateLiveSource {
        runtime: Rc::clone(runtime),
        project_session: Rc::clone(project_session),
    })
}
