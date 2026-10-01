//! Responsibility: gathers the evidence of a chain whose input arrives stepped.
//!
//! Taken right before the chain's automatic restart, while the streams that
//! delivered the stepped input are still alive: once they are killed, what
//! they saw is gone.

use std::time::SystemTime;

use domain::ids::ChainId;

use crate::input_evidence::{SteppedInputEvidence, StreamEvidence};
use crate::ProjectRuntimeController;

impl ProjectRuntimeController {
    /// What this chain's input streams and runtimes hold right now. Reads
    /// only: nothing of the chain or of another chain changes.
    pub fn stepped_input_evidence(&self, chain_id: &ChainId) -> SteppedInputEvidence {
        SteppedInputEvidence {
            chain_id: chain_id.0.clone(),
            taken_at: SystemTime::now(),
            input_stepped: self
                .runtime_graph
                .runtimes_for(chain_id)
                .iter()
                .any(|runtime| runtime.input_stepped()),
            streams: live_streams(chain_id),
            routes: self.chain_output_route_stats(chain_id),
        }
    }
}

/// The input streams of the chain, as their rings hold them. Only the macOS
/// input callback keeps a ring; JACK has none.
#[cfg(any(not(all(target_os = "linux", feature = "jack")), test))]
fn live_streams(chain_id: &ChainId) -> Vec<StreamEvidence> {
    crate::input_evidence_registry::live_rings(&chain_id.0)
        .iter()
        .map(|ring| StreamEvidence {
            identity: ring.identity().clone(),
            snapshot: ring.snapshot(),
        })
        .collect()
}

#[cfg(all(target_os = "linux", feature = "jack", not(test)))]
fn live_streams(_chain_id: &ChainId) -> Vec<StreamEvidence> {
    Vec::new()
}

#[cfg(test)]
#[path = "controller_stepped_evidence_tests.rs"]
mod tests;
