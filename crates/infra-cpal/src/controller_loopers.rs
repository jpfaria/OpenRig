//! Issue #323 — the controller's looper facade.
//!
//! A chain can be served by SEVERAL parallel runtimes (one per input entry,
//! #703). Each of them owns its own looper slots, records its own input and
//! plays its own material back into its own pipeline — the stream-isolation
//! law: nothing is shared, nothing is mixed across runtimes in our code.
//!
//! So a chain-level op is applied to every runtime of the chain, each with its
//! OWN layer buffer, and a chain-level status is the reading of whichever
//! runtime actually holds material (the input the user played into). Reads are
//! wait-free atomic loads — never the `processing` lock the audio thread
//! try-locks (#580).

use std::sync::Arc;

use domain::ids::ChainId;
use engine::runtime::ChainRuntimeState;
use engine::{LooperOp, LooperStatus};

use crate::controller::ProjectRuntimeController;

impl ProjectRuntimeController {
    /// Every runtime serving `chain_id`.
    pub fn runtimes_for_chain(&self, chain_id: &ChainId) -> Vec<Arc<ChainRuntimeState>> {
        self.runtime_graph.runtimes_for(chain_id)
    }

    /// Queue an op on every runtime of the chain. `make` is called once per
    /// runtime so an op carrying a layer buffer allocates one buffer PER
    /// runtime — two audio threads must never write the same memory.
    /// Returns how many runtimes accepted the op.
    pub fn push_chain_looper_op(
        &self,
        chain_id: &ChainId,
        make: impl Fn(&Arc<ChainRuntimeState>) -> Option<LooperOp>,
    ) -> usize {
        let mut queued = 0;
        for runtime in self.runtimes_for_chain(chain_id) {
            if let Some(op) = make(&runtime) {
                if runtime.push_looper_op(op).is_ok() {
                    queued += 1;
                }
            }
        }
        queued
    }

    /// The chain-level reading of one looper: the runtime that holds the most
    /// material wins, so a chain whose second input never recorded still
    /// reports the loop the user actually played.
    pub fn chain_looper_status(&self, chain_id: &ChainId, uid: u64) -> Option<LooperStatus> {
        self.runtimes_for_chain(chain_id)
            .iter()
            .filter_map(|rt| rt.looper_status(uid))
            .max_by_key(|s| (s.len_frames, s.layers))
    }

    /// Chain-level reading of every looper, in slot order.
    pub fn chain_looper_statuses(&self, chain_id: &ChainId) -> Vec<LooperStatus> {
        let mut out: Vec<LooperStatus> = Vec::new();
        for runtime in self.runtimes_for_chain(chain_id) {
            for status in runtime.looper_statuses() {
                match out.iter_mut().find(|s| s.uid == status.uid) {
                    Some(existing) => {
                        if (status.len_frames, status.layers)
                            > (existing.len_frames, existing.layers)
                        {
                            *existing = status;
                        }
                    }
                    None => out.push(status),
                }
            }
        }
        out
    }

    /// Collect and drop the layer buffers the audio threads handed back.
    /// Called from the GUI tick; freeing memory is forbidden on the audio
    /// thread (invariant #8). Returns how many buffers were dropped.
    pub fn drain_chain_looper_layers(&self, chain_id: &ChainId) -> usize {
        self.runtimes_for_chain(chain_id)
            .iter()
            .map(|rt| rt.drain_retired_layers().len())
            .sum()
    }
}
