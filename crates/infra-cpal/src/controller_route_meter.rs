//! Responsibility: finds the runtime that plays one output route of a chain.
//!
//! #1074: one jack is one pipeline fanning out to every output, so each
//! output's meter reads its own route — the runtime that writes it, never a
//! sibling's.

use std::sync::Arc;

use domain::ids::ChainId;
use engine::runtime::ChainRuntimeState;

use crate::controller::ProjectRuntimeController;

impl ProjectRuntimeController {
    /// The runtime of `chain_id` that writes chain output `route`, if any.
    pub fn runtime_writing_route(
        &self,
        chain_id: &ChainId,
        route: usize,
    ) -> Option<Arc<ChainRuntimeState>> {
        self.runtime_graph
            .runtimes_for(chain_id)
            .into_iter()
            .find(|runtime| runtime.writes_output(route))
    }
}
