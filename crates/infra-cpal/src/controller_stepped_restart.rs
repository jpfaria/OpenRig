//! Responsibility: restarts a chain whose input arrives stepped.
//!
//! #979: on the rig a stepped input (the buffer-seam pattern) was cured only
//! by switching the chain off and on. The engine marks the runtime that reads
//! such an input.
//!
//! #1081: on cpal the restart is the device's, not the chain's: every OpenRig
//! stream on the device that input reads is closed and opened again
//! (`controller_device_restart`), because a new set opened beside the old one
//! inherits OpenRig's IO on the device — the thing that broke. A chain on
//! another device is not touched. JACK, whose live swap is unwired (#672),
//! still goes through the toggle.

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

    /// Restart the streams of `chain_id`'s stepped input. `Ok(false)` when
    /// there is nothing to restart: the chain is not running here, is off,
    /// left the project, or no input of it is stepped any more — a device
    /// restart for another chain on the same device already started its
    /// verdict over.
    pub fn restart_chain_streams(&mut self, project: &Project, chain_id: &ChainId) -> Result<bool> {
        let Some(chain) = project.chains.iter().find(|c| &c.id == chain_id) else {
            return Ok(false);
        };
        if !chain.enabled || self.runtime_graph.runtimes_for(chain_id).is_empty() {
            return Ok(false);
        }
        #[cfg(not(all(target_os = "linux", feature = "jack")))]
        {
            let devices = self.stepped_devices_of(chain_id);
            if devices.is_empty() {
                return Ok(false);
            }
            log::warn!(
                "chain '{}': input arrives stepped, restarting every stream on {}",
                chain_id.0,
                devices.join(", ")
            );
            let restarted = self.restart_device_streams(project, &devices);
            log::warn!(
                "device restart reopened {} chain(s): {}",
                restarted.len(),
                restarted
                    .iter()
                    .map(|id| id.0.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        #[cfg(all(target_os = "linux", feature = "jack"))]
        {
            log::warn!(
                "chain '{}': input arrives stepped, restarting its streams",
                chain_id.0
            );
            self.kill_chain_streams(chain_id);
            self.upsert_chain(project, chain)?;
        }
        Ok(true)
    }
}
