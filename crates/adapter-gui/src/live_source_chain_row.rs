//! Responsibility: serves the chain rows the screen lists.

use std::cell::RefCell;
use std::rc::Rc;

use application::live_source::{ChainRuntimeReading, LiveSource};
use application::query_di::DiLoopReading;
use domain::ids::ChainId;
use engine::LooperStatus;
use infra_cpal::ProjectRuntimeController;

use crate::live_source_gui::di_reading;

/// #127: what a chain ROW redraws itself from, on the meter tick.
///
/// Three per-chain readings the project cannot answer: what the loops are
/// doing (and at what rate), whether the DI is playing and how loud, and
/// whether the chain has a live runtime plus its xrun/underrun counters. All
/// three are finished readings, all three carry the chain's identity, and
/// together they are what let `meter_wiring_poll` stop holding the backend.
pub(crate) struct ChainRowLiveSource {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl LiveSource for ChainRowLiveSource {
    /// Same reading, same helper, as the whole-project [`GuiLiveSource::di_loop`]
    /// MCP reads — so the tile and the transport cannot drift.
    fn chain_di_loop(&self, chain: &ChainId) -> Option<DiLoopReading> {
        let borrow = self.runtime.borrow();
        di_reading(borrow.as_ref()?, chain)
    }

    /// One borrow, three numbers, all this chain's own. `is_empty` on
    /// `runtimes_for_chain` is what gates REC: an enabled chain whose runtime
    /// is still cold-starting has nothing to capture into yet.
    ///
    /// The counters are plain atomic reads off the audio thread — never the
    /// `processing` lock (#580).
    fn chain_runtime(&self, chain: &ChainId) -> Option<ChainRuntimeReading> {
        let borrow = self.runtime.borrow();
        let controller = borrow.as_ref()?;
        Some(ChainRuntimeReading {
            live: !controller.runtimes_for_chain(chain).is_empty(),
            xruns: controller.chain_xrun_count(chain),
            underruns: controller.chain_underrun_count(chain),
        })
    }

    fn chain_loopers(&self, chain: &ChainId) -> Option<Result<(Vec<LooperStatus>, u32), String>> {
        let borrow = self.runtime.borrow();
        let controller = borrow.as_ref()?;
        Some(Ok((
            controller.chain_looper_statuses(chain),
            controller.sample_rate(),
        )))
    }
}

/// Build the chain row's read seam over the app's shared runtime handle.
pub(crate) fn chain_row_live_source(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Rc<dyn LiveSource> {
    Rc::new(ChainRowLiveSource {
        runtime: Rc::clone(runtime),
    })
}
