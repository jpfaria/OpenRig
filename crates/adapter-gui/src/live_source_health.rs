//! Responsibility: serves the audio health reading.

use std::cell::RefCell;
use std::rc::Rc;

use application::live_source::{AudioHealthReading, BlockErrorReading, LiveSource};
use infra_cpal::ProjectRuntimeController;

/// #127: what the poll tick reads — the block errors the audio thread
/// reported, and whether the backend is still there.
///
/// Its own `LiveSource` for the same reason the metronome has one: neither
/// reading depends on a project row, an analyzer session or a chain, so
/// carrying [`GuiLiveSource`]'s handles would be a lie about what the tick
/// looks at. With this, `desktop_app_polling` reads the runtime through the
/// seam instead of holding the backend.
pub(crate) struct HealthLiveSource {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl LiveSource for HealthLiveSource {
    /// Drain the failures the audio thread posted, each already tagged with
    /// the chain that raised it. `None` ⇒ nothing is hosted, which is not the
    /// same as a hosted runtime with nothing to report.
    fn block_errors(&self) -> Option<Vec<BlockErrorReading>> {
        let borrow = self.runtime.borrow();
        let controller = borrow.as_ref()?;
        Some(
            controller
                .poll_errors()
                .into_iter()
                .map(|(chain, error)| BlockErrorReading {
                    chain,
                    block: error.block_id,
                    message: error.message,
                })
                .collect(),
        )
    }

    /// Is anything sounding, and does the backend still answer.
    ///
    /// `is_healthy` needs the controller mutably (the JACK supervisor's
    /// health check is a probe, not a getter), which is why this borrows the
    /// cell mutably for the length of the read — the same borrow the poll
    /// tick used to take itself.
    fn audio_health(&self) -> Option<AudioHealthReading> {
        let mut borrow = self.runtime.borrow_mut();
        let controller = borrow.as_mut()?;
        Some(AudioHealthReading {
            running: controller.is_running(),
            healthy: controller.is_healthy(),
        })
    }
}

/// Build the poll tick's read seam over the app's shared runtime handle.
pub(crate) fn health_live_source(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Rc<dyn LiveSource> {
    Rc::new(HealthLiveSource {
        runtime: Rc::clone(runtime),
    })
}
