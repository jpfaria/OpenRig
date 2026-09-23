//! Responsibility: replays block toggles that raced an in-flight build onto the runtime that lands.
//!
//! Issue #967. A live edit (a scene, a preset, a knob that swaps a model)
//! builds the chain's next runtime on the control worker from a snapshot of
//! the chain. A footswitch pressed while that build runs is applied in place on
//! the runtime that is live NOW — and the build that lands a moment later was
//! made from the older snapshot, so publishing it put the old state back while
//! the UI showed the new one. Every toggle made while its chain has a build in
//! flight is remembered here and queued on the new runtime once it is live.

use domain::ids::{BlockId, ChainId};

use crate::ProjectRuntimeController;

impl ProjectRuntimeController {
    fn has_build_in_flight(&self, chain_id: &ChainId) -> bool {
        self.pending_rebuilds.iter().any(|(id, _)| id == chain_id)
            || self
                .pending_activations
                .iter()
                .any(|(id, _, _)| id == chain_id)
    }

    /// Remember `block` → `enabled` if a build of `chain_id` is in flight; the
    /// latest toggle of a block wins.
    pub(crate) fn remember_toggle_if_building(
        &self,
        chain_id: &ChainId,
        block: &BlockId,
        enabled: bool,
    ) {
        if !self.has_build_in_flight(chain_id) {
            return;
        }
        let mut replays = self.toggle_replays.borrow_mut();
        replays.retain(|(c, b, _)| !(c == chain_id && b == block));
        replays.push((chain_id.clone(), block.clone(), enabled));
    }

    /// A build of `chain_id` just went live: queue every remembered toggle on
    /// its runtimes. They are forgotten once no other build of the chain is in
    /// flight — a later build was also made from a snapshot older than them.
    pub(crate) fn replay_toggles(&self, chain_id: &ChainId) {
        let mut replays = self.toggle_replays.borrow_mut();
        if !replays.iter().any(|(c, _, _)| c == chain_id) {
            return;
        }
        let runtimes = self.runtime_graph.runtimes_for(chain_id);
        for (_, block, enabled) in replays.iter().filter(|(c, _, _)| c == chain_id) {
            for runtime in &runtimes {
                if let Err(e) = engine::runtime::set_block_enabled(runtime, block, *enabled) {
                    log::warn!(
                        "chain '{}': a toggle of '{}' made during a rebuild could not be \
                         replayed on the new runtime: {e}",
                        chain_id.0,
                        block.0
                    );
                }
            }
        }
        if !self.has_build_in_flight(chain_id) {
            replays.retain(|(c, _, _)| c != chain_id);
        }
    }
}
