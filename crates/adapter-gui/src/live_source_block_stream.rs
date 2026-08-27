//! Responsibility: serves the live stream a block publishes.

use std::cell::RefCell;
use std::rc::Rc;

use application::live_source::LiveSource;
use domain::ids::BlockId;
use infra_cpal::ProjectRuntimeController;

/// #127: what a block editor's stream panel reads.
///
/// A utility block may publish a small table of reduced entries (`key` /
/// `value` / `text` / `peak`) from its worker thread — the tuner block's note
/// readout was the original one. That is a finished READING, so the editor
/// window, the inline drawer and the compact view all take it through the seam
/// instead of holding the audio backend to poll it.
pub(crate) struct BlockStreamLiveSource {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl LiveSource for BlockStreamLiveSource {
    /// The engine collapses "no such block" and "nothing published" into
    /// `None`; both mean the same thing to a panel (show nothing), so they
    /// become the empty table here. What must NOT collapse is "no runtime" —
    /// that stays `None`, so a panel can tell a quiet block from a stopped rig.
    fn block_stream(&self, block: &BlockId) -> Option<Vec<block_core::StreamEntry>> {
        let borrow = self.runtime.borrow();
        let controller = borrow.as_ref()?;
        Some(controller.poll_stream(block).unwrap_or_default())
    }
}

/// Build the block editor's read seam over the app's shared runtime handle.
pub(crate) fn block_stream_live_source(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Rc<dyn LiveSource> {
    Rc::new(BlockStreamLiveSource {
        runtime: Rc::clone(runtime),
    })
}
