//! Responsibility: serves the drum machine's live position.

use std::cell::RefCell;
use std::rc::Rc;

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
