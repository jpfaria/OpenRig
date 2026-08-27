//! Responsibility: serves the metronome reading.

use std::cell::RefCell;
use std::rc::Rc;

use application::live_source::{LiveSource, MetronomeReading};
use infra_cpal::ProjectRuntimeController;

use crate::live_source_gui::metronome_reading;

/// #127: the metronome's live reading, on its own.
///
/// The click is an independent pipeline (invariant #4): its position depends
/// on no chain, no project row and no analyzer session, so this carries none
/// of the handles [`GuiLiveSource`] needs. It exists so `metronome_wiring` can
/// read the beat through the SEAM — the same one MCP reads — instead of
/// holding the audio backend itself.
pub(crate) struct MetronomeLiveSource {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl LiveSource for MetronomeLiveSource {
    fn metronome(&self) -> Option<MetronomeReading> {
        metronome_reading(&self.runtime)
    }
}

/// Build the metronome's read seam. Called by `desktop_app`, the module that
/// allocates the shared runtime handle in the first place.
pub(crate) fn metronome_live_source(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Rc<dyn LiveSource> {
    Rc::new(MetronomeLiveSource {
        runtime: Rc::clone(runtime),
    })
}
