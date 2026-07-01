//! Issue #717 — an armed DI loop plays on its own dedicated, isolated runtime
//! (a copy of the chain's block graph), never injected into the guitar's
//! runtime. `arm_di_stream` builds that separate runtime and holds it; the
//! guitar runtime is left untouched, so guitar and DI coexist fully isolated
//! (invariant #4).

use std::sync::Arc;

use anyhow::Result;

use domain::ids::ChainId;
use domain::io_binding::IoBinding;
use engine::runtime::build_chain_runtime_state;
use engine::spsc::SpscRing;
use engine::DiPcm;
use project::chain::Chain;

use crate::{LiveRuntimeSlot, ProjectRuntimeController};

/// A live dedicated DI runtime for one chain, alive only while the DI is armed.
/// Holds the isolated runtime via its slot; dropping the handle tears it down.
pub(crate) struct DiStreamHandle {
    pub(crate) slot: LiveRuntimeSlot,
}

impl ProjectRuntimeController {
    /// Build a fresh, independent runtime from `chain`'s block graph, feed it
    /// the loop, and hold it — NEVER the guitar runtime (#717, invariant #4).
    /// The engine defaults every route's elastic cushion here; Task 4 sizes it
    /// to the chain's chosen output once that output is resolved.
    pub fn arm_di_stream(
        &self,
        chain: &Chain,
        pcm: Arc<DiPcm>,
        registry: &[IoBinding],
    ) -> Result<()> {
        let runtime = Arc::new(build_chain_runtime_state(
            chain,
            self.sample_rate as f32,
            &[],
            registry,
        )?);
        let rate = runtime.sample_rate() as u32;
        runtime.set_di_loop(Some(Arc::new(pcm.to_loop_at(rate))));
        self.di_streams
            .borrow_mut()
            .insert(chain.id.clone(), DiStreamHandle {
                slot: LiveRuntimeSlot::new(runtime),
            });
        Ok(())
    }

    /// Tear the chain's dedicated DI runtime down (drops the runtime + loop).
    pub fn disarm_di_stream(&self, chain_id: &ChainId) {
        self.di_streams.borrow_mut().remove(chain_id);
    }

    /// Whether a dedicated DI runtime is currently armed for the chain.
    pub fn di_stream_active(&self, chain_id: &ChainId) -> bool {
        self.di_streams.borrow().contains_key(chain_id)
    }

    /// Length of the loop carried by the chain's dedicated DI runtime, if
    /// armed. Mirrors [`Self::chain_di_loop_len`] but reads the DI runtime, not
    /// the guitar — proving the loop rides the separate stream.
    pub fn di_stream_loop_len(&self, chain_id: &ChainId) -> Option<usize> {
        self.di_streams
            .borrow()
            .get(chain_id)
            .and_then(|h| h.slot.load().di_loop_len())
    }

    /// Subscribe the DI runtime's per-stream OUTPUT tap (post-FX stereo), for
    /// the dedicated DI graph's meters. Mirrors [`Self::subscribe_stream_tap`]
    /// but reads the isolated DI runtime, not the guitar. `None` if not armed.
    pub fn di_subscribe_stream_tap(
        &self,
        chain_id: &ChainId,
        stream_index: usize,
        capacity_per_channel: usize,
    ) -> Option<[Arc<SpscRing<f32>>; 2]> {
        self.di_streams
            .borrow()
            .get(chain_id)
            .map(|h| h.slot.load().subscribe_stream_tap(stream_index, capacity_per_channel))
    }

    /// How many streams the chain's DI runtime runs (0 if not armed).
    pub fn di_stream_count(&self, chain_id: &ChainId) -> usize {
        self.di_streams
            .borrow()
            .get(chain_id)
            .map(|h| h.slot.load().stream_count())
            .unwrap_or(0)
    }

    /// One processing step for the chain's DI runtime — the per-buffer clock the
    /// DI worker runs. The armed loop substitutes the (silent) device input, so
    /// stepping fills the runtime's meter taps and output route from the loop.
    pub fn di_drive_once(&self, chain_id: &ChainId, frames: usize) {
        if let Some(h) = self.di_streams.borrow().get(chain_id) {
            let silence = vec![0.0f32; frames];
            crate::slot_processing::process_input_buffer(&h.slot, 0, &silence, 1);
        }
    }
}
