//! Responsibility: serves the drum machine's live position.

use std::cell::RefCell;
use std::rc::Rc;

use application::live_source::LiveSource;
use feature_dsp::drums::DrumPosition;
use infra_cpal::ProjectRuntimeController;

/// Where the drums are in the bar, from the callback's own lock-free cell.
///
/// `None` ⇒ no runtime is hosted, never a fabricated beat. A closed stream
/// reads as stopped even if the last position published said playing.
pub(crate) fn drums_position(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Option<DrumPosition> {
    let borrow = runtime.borrow();
    let controller = borrow.as_ref()?;
    if !controller.drums_active() {
        return Some(DrumPosition::default());
    }
    Some(controller.drums_shared().position())
}

/// The drums are their own pipeline, so their read seam carries only the
/// runtime handle — no chain, row or analyzer.
struct DrumsLiveSource {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl LiveSource for DrumsLiveSource {
    fn drums(&self) -> Option<DrumPosition> {
        drums_position(&self.runtime)
    }
}

/// Build the drums' read seam over the app's shared runtime handle.
pub(crate) fn drums_live_source(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Rc<dyn LiveSource> {
    Rc::new(DrumsLiveSource {
        runtime: Rc::clone(runtime),
    })
}

#[cfg(test)]
#[path = "live_source_drums_tests.rs"]
mod tests;
