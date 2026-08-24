//! Responsibility: syncs the live runtime with the project's enabled chains.
//!
//! Brings up what the project gained, tears down what it lost, and leaves
//! every surviving chain to the upsert path.

use anyhow::Result;
use std::collections::HashMap;

#[cfg(all(target_os = "linux", feature = "jack"))]
use domain::ids::ChainId;
use project::project::Project;

use crate::controller::ProjectRuntimeController;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::chain_resolve::{resolve_chain_audio_config, resolve_enabled_chain_audio_configs};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::host::get_host;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::validation::{
    validate_chain_channels_against_devices, validate_channels_against_devices,
};

impl ProjectRuntimeController {
    pub fn sync_project(&mut self, project: &Project) -> Result<()> {
        log::debug!(
            "syncing project runtime with {} chains",
            project.chains.len()
        );

        // On Linux with JACK feature, only start jackd when the project has
        // at least one enabled chain that actually needs audio. Launching
        // jackd opens the ALSA PCM for each card, which exercises the USB
        // audio stack — we must not do that passively while the user is just
        // editing chain settings with everything bypassed.
        #[cfg(all(target_os = "linux", feature = "jack"))]
        {
            let needs_audio = project.chains.iter().any(|c| c.enabled);
            if !needs_audio {
                log::debug!("sync_project: no enabled chains, idling supervisor");
                if !self.active_chains.is_empty() {
                    log::info!("sync_project: no enabled chains, tearing down runtime");
                    self.stop();
                }
                if let Err(e) = self.supervisor.shutdown_all() {
                    log::warn!("sync_project: supervisor.shutdown_all failed: {}", e);
                }
                return Ok(());
            }
            // The supervisor drives the ordered teardown for us: ensure_jack_servers
            // calls would_restart to check the pre-kill condition and tears down
            // active chains before SIGTERM. See issue #308 for the invariants.
            self.ensure_jack_servers(project)?;
            return self.sync_project_jack_direct(project);
        }

        #[cfg(not(all(target_os = "linux", feature = "jack")))]
        {
            let host = get_host();
            // #693: on a cold start (nothing active, nothing pending) every
            // enabled chain goes through the off-thread activation, which
            // validates + resolves on the control worker — skip the
            // hundreds-of-ms CoreAudio queries here so the caller (the GUI
            // thread) returns immediately. Live syncs keep the upfront
            // validation so errors still surface synchronously.
            let cold_start = self.active_chains.is_empty()
                && self.pending_activations.is_empty()
                && self.pending_rebuilds.is_empty();
            let mut resolved_chains = if cold_start {
                HashMap::new()
            } else {
                validate_channels_against_devices(project, host, &self.io_bindings)?;
                resolve_enabled_chain_audio_configs(host, project, &self.io_bindings)?
            };

            let removed_chain_ids = self
                .active_chains
                .keys()
                .filter(|chain_id| !resolved_chains.contains_key(*chain_id))
                .cloned()
                .collect::<Vec<_>>();
            for chain_id in removed_chain_ids {
                log::info!("removing chain '{}' from runtime", chain_id.0);
                if let Some(runtime) = self.runtime_graph.runtime_for_chain(&chain_id) {
                    runtime.set_draining();
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                self.active_chains.remove(&chain_id);
                self.runtime_graph.remove_chain(&chain_id);
            }

            // #716 (invariant #4): two or more ACTIVE inputs may not share the
            // same device+channel. Refuse to bring up a chain whose input tap
            // is already claimed by an earlier enabled chain (first wins);
            // within-chain duplicates are caught too. Output may be shared.
            let input_conflicts = engine::runtime_endpoints::input_conflicting_chains(
                project.chains.iter(),
                &self.io_bindings,
            );

            for chain in &project.chains {
                if !chain.enabled {
                    continue;
                }
                if input_conflicts.contains(&chain.id) {
                    log::warn!(
                        "chain '{}' not activated: one of its input device+channel taps is already in use by another active chain (#716)",
                        chain.id.0
                    );
                    continue;
                }

                // #693: a cold bring-up must not hold the caller (the GUI
                // thread) — reuse the #672 off-thread activation: validate +
                // resolve + heavy build (NAM/IR, routes) on the control
                // worker, streams installed by the poll tick. #740: this now
                // covers multi-device chains too, so only an already-streaming
                // chain stays on the synchronous (live-rebuild) path.
                if self.schedule_chain_activation(project, chain)? {
                    continue;
                }
                // #762: an already-streaming chain whose IO is unchanged (a
                // block/model/preset edit or a re-sync) must rebuild OFF the
                // GUI thread (#672), never load NAM synchronously on the caller.
                // Only a real re-bind (IO changed) falls through to the
                // synchronous stream rebuild below.
                if self.request_offthread_rebuild_if_live(project, chain)? {
                    continue;
                }
                let resolved = match resolved_chains.remove(&chain.id) {
                    Some(resolved) => resolved,
                    None => {
                        // Cold start skipped the upfront resolve; this is the
                        // synchronous fallback for a chain the scheduler
                        // declined (e.g. already streaming).
                        validate_chain_channels_against_devices(host, chain, &self.io_bindings)?;
                        resolve_chain_audio_config(host, project, chain, &self.io_bindings)?
                    }
                };
                self.upsert_chain_with_resolved(chain, resolved, false)?;
            }

            Ok(())
        }
    }

    /// Sync project using only the jack crate — zero CPAL/ALSA access.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    fn sync_project_jack_direct(&mut self, project: &Project) -> Result<()> {
        log::info!("sync_project: JACK direct mode (no CPAL/ALSA)");

        // Remove chains that are no longer in the project
        let active_ids: Vec<ChainId> = self.active_chains.keys().cloned().collect();
        for chain_id in active_ids {
            let still_exists = project.chains.iter().any(|c| c.enabled && c.id == chain_id);
            if !still_exists {
                log::info!("removing chain '{}' from runtime", chain_id.0);
                // Signal the audio callback to stop processing blocks BEFORE
                // deactivating the JACK client — prevents use-after-free in C++
                // NAM destructors ("terminate called without active exception").
                if let Some(runtime) = self.runtime_graph.runtime_for_chain(&chain_id) {
                    runtime.set_draining();
                    // Give the JACK callback time to finish its current cycle.
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                self.active_chains.remove(&chain_id);
                self.runtime_graph.remove_chain(&chain_id);
            }
        }

        for chain in &project.chains {
            if !chain.enabled {
                continue;
            }
            let resolved =
                crate::jack_resolve_chain_config(chain, &self.supervisor, &self.io_bindings)?;
            self.upsert_chain_with_resolved(chain, resolved, false)?;
        }

        Ok(())
    }
}
