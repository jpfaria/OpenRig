//! Responsibility: serves the looper readings of a chain.

use std::cell::RefCell;
use std::rc::Rc;

use application::live_source::LiveSource;
use application::looper_edit::LoopEditReading;
use domain::ids::ChainId;
use engine::loop_edit;
use engine::LooperStatus;
use infra_cpal::ProjectRuntimeController;

/// #127/#323: the loopers' live reading, on its own.
///
/// The looper panel redraws a chain's rows right after every dispatch, and it
/// needs exactly one thing the project cannot tell it: what each loop is
/// DOING, and at what rate it is counting. That is a finished reading — never
/// PCM — so it comes through the same seam MCP reads, and `looper_callbacks`
/// no longer holds the audio backend to get it.
///
/// Unlike [`GuiLiveSource::chain_loopers`] this answers `None` with no
/// controller instead of resolving a rate off the binding registry: the panel
/// draws its rows from the persisted config in that case, and a rate for a
/// device nobody opened would be a fiction (#723). MCP asks a different
/// question — "what rate would this chain run at?" — and gets the resolved
/// answer there.
pub(crate) struct LooperLiveSource {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl LiveSource for LooperLiveSource {
    fn chain_loopers(&self, chain: &ChainId) -> Option<Result<(Vec<LooperStatus>, u32), String>> {
        let borrow = self.runtime.borrow();
        let controller = borrow.as_ref()?;
        Some(Ok((
            controller.chain_looper_statuses(chain),
            controller.sample_rate(),
        )))
    }

    /// #826: the loop's envelope for the waveform editor. The peaks are
    /// reduced HERE, on the side that owns the samples — the buffer itself
    /// never crosses the seam (the module's PCM rule).
    fn chain_loop_edit(
        &self,
        chain: &ChainId,
        looper: u64,
        buckets: usize,
    ) -> Option<LoopEditReading> {
        let borrow = self.runtime.borrow();
        let controller = borrow.as_ref()?;
        let pcm = controller.export_chain_looper_raw(chain, looper)?;
        let (can_undo, can_redo) = controller.looper_edit_history_depth(chain, looper);
        let len_frames = pcm.len() / 2;
        Some(LoopEditReading {
            peaks: loop_edit::peaks(&pcm, buckets),
            len_frames,
            length_label: crate::looper_view::clock_label(len_frames, controller.sample_rate()),
            playing: controller.looper_is_playing(chain, looper),
            can_undo: can_undo > 0,
            can_redo: can_redo > 0,
        })
    }
}

/// Build the looper panel's read seam over the app's shared runtime handle.
pub(crate) fn looper_live_source(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Rc<dyn LiveSource> {
    Rc::new(LooperLiveSource {
        runtime: Rc::clone(runtime),
    })
}
