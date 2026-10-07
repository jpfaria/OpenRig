//! Responsibility: reopens every stream OpenRig holds on one device.
//!
//! #1081: coreaudiod runs one IO context per process per device, alive while
//! any of the process's streams on that device runs. When the device's input
//! goes stale under OpenRig's context (the HD 8 after a voice-processing app
//! started), a restart that opens the new streams before it closes the old
//! ones hands the new streams that same context. So this closes every OpenRig
//! stream on the device first — each chain that reads or plays there, the
//! metronome, the player, the drums, the isolated DI and looper outputs —
//! waits [`DEVICE_REOPEN_PAUSE`] and opens them again on the same runtimes.
//!
//! The cost is a short cut of OpenRig on that device: nothing plays between
//! the close and the reopened sets' fade-in. A chain on another device is not
//! touched — the selection is by device identity (invariant #4).

use std::sync::Arc;
use std::time::Duration;

use domain::ids::ChainId;
use engine::runtime::ChainRuntimeState;
use project::chain::Chain;
use project::project::Project;

use crate::resolved::ResolvedChainAudioConfig;
use crate::retired_streams::StreamSwap;
use crate::{LiveRuntimeSlot, ProjectRuntimeController};

/// Time between closing OpenRig's last stream on the device and opening the
/// first new one. Not measured: it covers the dsp workers behind the closed
/// input streams (they poll their stop flag every 100 µs) and coreaudiod
/// stopping the IO it ran for them.
const DEVICE_REOPEN_PAUSE: Duration = Duration::from_millis(100);

/// The live chains that read or play on any of `devices`, in id order. `live`
/// holds each chain with the devices of its streams.
pub(crate) fn chains_on_devices(
    devices: &[String],
    live: &[(ChainId, Vec<String>)],
) -> Vec<ChainId> {
    let mut chains: Vec<ChainId> = live
        .iter()
        .filter(|(_, used)| used.iter().any(|device| devices.contains(device)))
        .map(|(chain_id, _)| chain_id.clone())
        .collect();
    chains.sort_by(|a, b| a.0.cmp(&b.0));
    chains.dedup();
    chains
}

/// The devices the `stepped` cpal inputs read, once each. `input_devices` is
/// the chain's input entries in order; cpal input N is the Nth distinct device.
pub(crate) fn stepped_input_devices(input_devices: &[String], stepped: &[usize]) -> Vec<String> {
    let mut distinct: Vec<&String> = Vec::new();
    for device in input_devices {
        if !distinct.contains(&device) {
            distinct.push(device);
        }
    }
    let mut devices: Vec<String> = Vec::new();
    for &index in stepped {
        let Some(&device) = distinct.get(index) else {
            continue;
        };
        if !device.is_empty() && !devices.contains(device) {
            devices.push(device.clone());
        }
    }
    devices
}

/// How one closed chain comes back.
enum Reopen {
    /// New streams at once, on its live runtimes, from the device config its
    /// streams were resolved against.
    Streams {
        chain: Chain,
        resolved: ResolvedChainAudioConfig,
        slots: Vec<(usize, LiveRuntimeSlot)>,
    },
    /// Nothing live to build from: a fresh activation.
    Activation(Chain),
}

impl ProjectRuntimeController {
    /// The devices the stepped runtimes of `chain_id` read.
    pub(crate) fn stepped_devices_of(&self, chain_id: &ChainId) -> Vec<String> {
        let Some(active) = self.active_chains.get(chain_id) else {
            return Vec::new();
        };
        let inputs: Vec<String> = active
            .stream_signature
            .inputs
            .iter()
            .map(|input| input.device_id.clone())
            .collect();
        let stepped: Vec<usize> = self
            .runtime_graph
            .runtimes_with_groups_for(chain_id)
            .into_iter()
            .filter(|(_, runtime)| runtime.input_stepped())
            .map(|(group, runtime)| runtime.input_cpal_index().unwrap_or(group))
            .collect();
        stepped_input_devices(&inputs, &stepped)
    }

    /// Close every stream OpenRig holds on `devices`, wait, and open them
    /// again. Returns the chains it restarted.
    pub(crate) fn restart_device_streams(
        &mut self,
        project: &Project,
        devices: &[String],
    ) -> Vec<ChainId> {
        let live: Vec<(ChainId, Vec<String>)> = self
            .active_chains
            .iter()
            .map(|(chain_id, active)| {
                let signature = &active.stream_signature;
                let used = signature
                    .inputs
                    .iter()
                    .map(|input| input.device_id.clone())
                    .chain(signature.outputs.iter().map(|o| o.device_id.clone()))
                    .collect();
                (chain_id.clone(), used)
            })
            .collect();
        let mut plans = Vec::new();
        for chain_id in chains_on_devices(devices, &live) {
            match project
                .chains
                .iter()
                .find(|chain| chain.id == chain_id && chain.enabled)
            {
                Some(chain) => plans.push((chain_id.clone(), self.reopen_plan(&chain_id, chain))),
                // Off, or gone from the project: it has nothing to reopen.
                None => self.kill_chain_streams(&chain_id),
            }
        }

        // Close. Silence first, then every stream on the device.
        let runtimes: Vec<Arc<ChainRuntimeState>> = plans
            .iter()
            .flat_map(|(chain_id, _)| self.runtime_graph.runtimes_for(chain_id))
            .collect();
        for runtime in &runtimes {
            runtime.set_draining();
        }
        for (chain_id, _) in &plans {
            drop(self.active_chains.remove(chain_id));
        }
        let metronome = self.release_metronome_on(devices);
        let player = self.release_player_on(devices);
        let drums = self.release_drums_on(devices);
        let isolated = self.release_isolated_outputs_on(devices);
        std::thread::sleep(DEVICE_REOPEN_PAUSE);

        // Open. What the closed streams heard is not held against the new ones.
        for runtime in &runtimes {
            runtime.reset_input_seams();
            runtime.clear_draining();
        }
        let mut restarted = Vec::with_capacity(plans.len());
        for (chain_id, plan) in plans {
            self.reopen_chain(project, &chain_id, plan);
            restarted.push(chain_id);
        }
        if let Some((device, targets)) = metronome {
            if let Err(error) = self.start_metronome(&device, &targets) {
                log::error!("the metronome did not reopen after the device restart: {error}");
            }
        }
        if let Some((device, targets)) = player {
            if let Err(error) = self.open_player_output(&device, &targets) {
                log::error!("the player did not reopen after the device restart: {error}");
            }
        }
        if let Some((device, targets)) = drums {
            if let Err(error) = self.start_drums(&device, &targets) {
                log::error!("the drums did not reopen after the device restart: {error}");
            }
        }
        self.reopen_isolated_outputs(project, &isolated);
        restarted
    }

    fn reopen_plan(&self, chain_id: &ChainId, chain: &Chain) -> Reopen {
        let resolved = self
            .active_chains
            .get(chain_id)
            .and_then(|active| active.resolved.clone());
        let mut slots: Vec<(usize, LiveRuntimeSlot)> = self
            .chain_slots
            .iter()
            .filter(|((id, _), _)| id == chain_id)
            .map(|((_, group), slot)| (*group, slot.handle()))
            .collect();
        slots.sort_by_key(|(group, _)| *group);
        match resolved {
            Some(resolved) if !slots.is_empty() => Reopen::Streams {
                chain: chain.clone(),
                resolved,
                slots,
            },
            _ => Reopen::Activation(chain.clone()),
        }
    }

    fn reopen_chain(&mut self, project: &Project, chain_id: &ChainId, plan: Reopen) {
        let chain = match plan {
            Reopen::Activation(chain) => chain,
            Reopen::Streams {
                chain,
                resolved,
                slots,
            } => {
                let di_cells: Vec<_> = (0..resolved.outputs.len())
                    .map(|j| self.di_playback_cell(chain_id, j))
                    .collect();
                self.stream_generation += 1;
                match crate::build_active_chain_runtime(
                    chain_id,
                    &chain,
                    resolved,
                    slots,
                    &self.io_bindings,
                    &di_cells,
                    self.stream_generation,
                    StreamSwap::replacing(),
                ) {
                    Ok(active) => {
                        self.streams.streams_built(
                            chain_id,
                            self.stream_generation,
                            active._input_streams.len(),
                            active._output_streams.len(),
                        );
                        self.active_chains.insert(chain_id.clone(), active);
                        return;
                    }
                    Err(error) => {
                        log::error!(
                            "chain '{}': its streams did not reopen after the device restart, \
                             activating it again: {error}",
                            chain_id.0
                        );
                        chain
                    }
                }
            }
        };
        if let Err(error) = self.submit_chain_activation(project, &chain) {
            log::error!(
                "chain '{}': activation after the device restart failed: {error}",
                chain_id.0
            );
        }
    }
}

#[cfg(test)]
#[path = "controller_device_restart_tests.rs"]
mod tests;
