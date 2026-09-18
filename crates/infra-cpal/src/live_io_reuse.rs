//! Responsibility: decides whether a live chain's resolved device config can build its next streams.
//!
//! #957: a preset switch is a structural edit, so it opens new streams (#881).
//! Resolving the devices for them again is a round of CoreAudio queries that
//! took 1.1–1.8 s on the owner's Quantum HD 8 — while the blocks themselves
//! build in ~40 ms. When neither the chain's I/O nor the device settings
//! moved, the config the live streams were resolved against is still the
//! answer, so the new build starts from it.

use anyhow::Result;
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;

use crate::controller::ProjectRuntimeController;
use crate::resolved::ResolvedChainAudioConfig;

/// Do the device settings the live streams were resolved with still match the
/// project's, for every device those streams use? A device the live config
/// resolved without settings must still have none.
pub(crate) fn settings_still_match(
    live: &[DeviceSettings],
    live_device_ids: &[String],
    project: &[DeviceSettings],
) -> bool {
    let find = |settings: &[DeviceSettings], id: &str| {
        settings.iter().find(|s| s.device_id.0 == id).cloned()
    };
    live_device_ids
        .iter()
        .all(|id| find(live, id) == find(project, id))
}

impl ProjectRuntimeController {
    /// The resolved config of `chain`'s live streams, when the next build can
    /// reuse it: the chain is streaming, its I/O is unchanged and so are the
    /// settings of every device it uses. `None` means resolve from the devices.
    pub(crate) fn reusable_live_config(
        &self,
        project: &Project,
        chain: &Chain,
    ) -> Result<Option<ResolvedChainAudioConfig>> {
        let Some(resolved) = self
            .active_chains
            .get(&chain.id)
            .and_then(|active| active.resolved.as_ref())
        else {
            return Ok(None);
        };
        if self.chain_io_changed(project, chain)? {
            return Ok(None);
        }
        let live: Vec<DeviceSettings> = resolved
            .inputs
            .iter()
            .filter_map(|i| i.settings.clone())
            .chain(resolved.outputs.iter().filter_map(|o| o.settings.clone()))
            .collect();
        let device_ids: Vec<String> = resolved
            .stream_signature
            .inputs
            .iter()
            .map(|i| i.device_id.clone())
            .chain(
                resolved
                    .stream_signature
                    .outputs
                    .iter()
                    .map(|o| o.device_id.clone()),
            )
            .collect();
        Ok(
            settings_still_match(&live, &device_ids, &project.device_settings)
                .then(|| resolved.clone()),
        )
    }
}

#[cfg(test)]
#[path = "live_io_reuse_tests.rs"]
mod tests;
