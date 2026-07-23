//! Issue #672 / #740 / #762 — the off-thread LIVE-rebuild path.
//!
//! A param/block/preset edit on an ALREADY-running chain whose I/O topology is
//! unchanged reuses the live stream config and rebuilds only the DSP graph, on
//! the control worker — so the GUI never freezes on a device resolve or a NAM
//! reload. Split out of `controller.rs` (per-file line cap) as a cohesive unit:
//! the "is this a live edit?" test (`chain_io_changed`) and the scheduler that
//! acts on it (`request_offthread_rebuild_if_live`).

use anyhow::Result;

use project::chain::Chain;
use project::project::Project;

use crate::ProjectRuntimeController;

impl ProjectRuntimeController {
    /// Issue #672 — if `chain` is already streaming with UNCHANGED IO topology,
    /// rebuild its runtime off the frontend thread (the model-swap freeze) and
    /// return `true`. Returns `false` when the chain is not live or its IO
    /// changed, so the caller falls back to the synchronous stream-rebuild path.
    ///
    /// Param/knob edits are NOT routed here — they keep the engine's cheap
    /// in-place lock-free update. Only the model/type-swap callbacks call this.
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    pub fn request_offthread_rebuild_if_live(
        &mut self,
        project: &Project,
        chain: &Chain,
    ) -> Result<bool> {
        if !self.active_chains.contains_key(&chain.id) {
            return Ok(false); // cold activation — caller does the synchronous build
        }
        // #740: a re-bind (device/channel change) needs a synchronous stream
        // rebuild; detect it CHEAPLY (binding vs live signature, no CoreAudio).
        if self.chain_io_changed(project, chain)? {
            return Ok(false);
        }
        // I/O unchanged: a param/block/preset edit reuses the running stream
        // config, so derive the rebuild params from the LIVE signature instead of
        // a synchronous CoreAudio resolve (the ~hundreds-ms freeze the owner felt
        // on every edit, on top of the off-thread NAM reload). The heavy DSP
        // rebuild then runs on the worker; the GUI returns immediately.
        let (sample_rate, device_sample_rates, out_buffers) = {
            let sig = &self
                .active_chains
                .get(&chain.id)
                .expect("just checked active")
                .stream_signature;
            let sample_rate = sig
                .inputs
                .first()
                .map(|i| i.sample_rate as f32)
                .unwrap_or(48_000.0);
            let device_sample_rates: std::collections::HashMap<domain::ids::DeviceId, f32> = sig
                .inputs
                .iter()
                .map(|i| {
                    (
                        domain::ids::DeviceId(i.device_id.clone()),
                        i.sample_rate as f32,
                    )
                })
                .collect();
            let out_buffers: Vec<u32> = sig.outputs.iter().map(|o| o.buffer_size_frames).collect();
            (sample_rate, device_sample_rates, out_buffers)
        };
        let elastic_targets = crate::elastic::elastic_targets_from_output_buffers(&out_buffers);
        // #779: a chain containing a VST3 must NOT be rebuilt fresh off-thread.
        // A fresh build calls `createInstance` on the control worker while the
        // audio thread is inside the old instance's `process()` — a concurrent
        // JUCE op that SIGSEGVs (the pairing #778's lock cannot cover, since
        // `process()` runs RT and must not lock). Update the LIVE runtime IN
        // PLACE instead: `update_chain_runtime_state` reuses the existing VST3
        // instance (a param change becomes `setParameter`, never a reload) and
        // mutates under the runtime's processing lock, so it is RT-safe and the
        // cpal callbacks keep the same `Arc`. Chains with no VST3 keep the
        // off-thread fresh rebuild (safe: re-instantiating a NAM/native block
        // touches no shared JUCE state).
        if chain_contains_vst3(chain) {
            let groups: Vec<usize> = self
                .runtime_graph
                .chains
                .keys()
                .filter(|(cid, _)| cid == &chain.id)
                .map(|(_, g)| *g)
                .collect();
            for group in groups {
                if let Some(runtime) = self.runtime_graph.chains.get(&(chain.id.clone(), group)) {
                    let group_rate = device_sample_rates
                        .values()
                        .next()
                        .copied()
                        .unwrap_or(sample_rate);
                    engine::runtime::update_chain_runtime_state(
                        runtime,
                        chain,
                        group_rate,
                        false,
                        &elastic_targets,
                        &self.io_bindings,
                    )?;
                }
            }
            self.rearm_di_stream_after_rebuild(chain);
            return Ok(true);
        }
        self.schedule_chain_rebuild(chain, sample_rate, device_sample_rates, elastic_targets);
        // A monitored DI is a dedicated pre-render built from the chain's DSP at
        // arm time (issue #717/#771). The synchronous `upsert_chain` and the
        // cold-activation poll both re-render it after an edit; this off-thread
        // fast path (added by #740/#762) used to skip that, so once live edits
        // started taking it, changing the chain while monitoring the DI produced
        // NO audible change — the DI kept playing the stale render. Re-render it
        // from the new config here too. The re-arm builds its OWN routed runtime
        // off-thread and is a no-op when nothing is armed, so it neither depends
        // on the guitar rebuild landing nor blocks the GUI.
        self.rearm_di_stream_after_rebuild(chain);
        Ok(true)
    }

    /// JACK build: the live-slot swap is not wired for the JACK backend yet
    /// (issue #672 does the cpal path first), so always fall back to the
    /// synchronous path.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    pub fn request_offthread_rebuild_if_live(
        &mut self,
        _project: &Project,
        _chain: &Chain,
    ) -> Result<bool> {
        Ok(false)
    }

    /// #716: `true` when `chain` is already streaming AND its resolved I/O
    /// topology (the devices/channels its bindings point at) differs from what
    /// is live — i.e. the user re-bound its E/S. A param/block `upsert` keeps the
    /// existing streams, so an E/S swap would NOT take effect until a project
    /// reopen; the caller must REBUILD the chain's streams when this is true.
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    pub fn chain_io_changed(&self, _project: &Project, chain: &Chain) -> Result<bool> {
        let Some(active) = self.active_chains.get(&chain.id) else {
            return Ok(false); // not streaming — nothing to compare
        };
        // #743: a re-bind is a device+channel change, which the binding registry
        // and the live stream signature already carry — compare them directly.
        // The old `resolve_chain_audio_config` here ran a CoreAudio device query
        // (hundreds of ms per device, ~750 ms on a four-device rig) on the GUI
        // thread every toggle-ON, freezing the UI for a check that needs no
        // hardware. A rate/buffer change arrives via the device-settings sync,
        // not this per-chain toggle path.
        let live_inputs: Vec<(domain::ids::DeviceId, Vec<usize>)> = active
            .stream_signature
            .inputs
            .iter()
            .map(|s| {
                (
                    domain::ids::DeviceId(s.device_id.clone()),
                    s.channels.clone(),
                )
            })
            .collect();
        let live_outputs: Vec<(domain::ids::DeviceId, Vec<usize>)> = active
            .stream_signature
            .outputs
            .iter()
            .map(|s| {
                (
                    domain::ids::DeviceId(s.device_id.clone()),
                    s.channels.clone(),
                )
            })
            .collect();
        let (bound_in, bound_out) =
            engine::runtime_endpoints::resolve_chain_io(chain, &self.io_bindings);
        let bound_inputs: Vec<(domain::ids::DeviceId, Vec<usize>)> = bound_in
            .into_iter()
            .map(|e| (e.device_id, e.channels))
            .collect();
        let bound_outputs: Vec<(domain::ids::DeviceId, Vec<usize>)> = bound_out
            .into_iter()
            .map(|e| (e.device_id, e.channels))
            .collect();
        Ok(crate::io_topology_changed(
            &live_inputs,
            &bound_inputs,
            &live_outputs,
            &bound_outputs,
        ))
    }

    /// JACK build: stream-topology live-swap is not wired yet (cpal path first).
    #[cfg(all(target_os = "linux", feature = "jack"))]
    pub fn chain_io_changed(&self, _project: &Project, _chain: &Chain) -> Result<bool> {
        Ok(false)
    }
}

/// Whether any block in the chain is a VST3 — its live rebuild must reuse the
/// instance in place rather than re-instantiate it (#779). Recurses into
/// `Select` sub-chains so a VST3 nested inside one is covered too.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn chain_contains_vst3(chain: &Chain) -> bool {
    chain.blocks.iter().any(block_contains_vst3)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn block_contains_vst3(block: &project::block::AudioBlock) -> bool {
    use project::block::AudioBlockKind;
    match &block.kind {
        AudioBlockKind::Core(core) => core.effect_type == block_core::EFFECT_TYPE_VST3,
        AudioBlockKind::Select(select) => select.options.iter().any(block_contains_vst3),
        _ => false,
    }
}
