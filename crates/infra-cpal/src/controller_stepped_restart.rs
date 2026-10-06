//! Responsibility: restarts a chain whose input arrives stepped.
//!
//! #979: on the rig a stepped input (the buffer-seam pattern) was cured only
//! by switching the chain off and on. The engine marks the runtime that reads
//! such an input; this restarts that one chain — a fresh activation builds new
//! streams and a new runtime — and nothing of another chain is touched.
//!
//! #1081: the restart never cuts the sound. On cpal the live streams keep
//! playing while the new ones come up, and fade out under them (the install's
//! handover). Only JACK, whose live swap is unwired (#672), still goes through
//! the toggle.

use anyhow::Result;

use domain::ids::ChainId;
use project::project::Project;

use crate::ProjectRuntimeController;

impl ProjectRuntimeController {
    /// The chains with a runtime marked stepped, in id order.
    pub fn stepped_input_chains(&self) -> Vec<ChainId> {
        let mut chains: Vec<ChainId> = self
            .runtime_graph
            .chains
            .iter()
            .filter(|(_, runtime)| runtime.input_stepped())
            .map(|((chain_id, _), _)| chain_id.clone())
            .collect();
        chains.sort_by(|a, b| a.0.cmp(&b.0));
        chains.dedup();
        chains
    }

    /// Switch `chain_id` off and on. `Ok(false)` when there is nothing to
    /// restart: the chain is not running here, is off, or left the project.
    pub fn restart_chain_streams(&mut self, project: &Project, chain_id: &ChainId) -> Result<bool> {
        let Some(chain) = project.chains.iter().find(|c| &c.id == chain_id) else {
            return Ok(false);
        };
        if !chain.enabled || self.runtime_graph.runtimes_for(chain_id).is_empty() {
            return Ok(false);
        }
        log::warn!(
            "chain '{}': input arrives stepped, restarting its streams",
            chain_id.0
        );
        #[cfg(not(all(target_os = "linux", feature = "jack")))]
        self.submit_chain_activation(project, chain)?;
        #[cfg(all(target_os = "linux", feature = "jack"))]
        {
            self.kill_chain_streams(chain_id);
            self.upsert_chain(project, chain)?;
        }
        Ok(true)
    }
}
