//! Responsibility: installs one chain's resolved audio config into the live runtime.
//!
//! The stream signature decides the shape: an unchanged signature keeps the
//! streams up (a knob edit must not drop audio), a changed one tears the
//! chain's streams down before the replacement is built.

use anyhow::Result;

use project::chain::Chain;
use project::project::Project;

use crate::controller::ProjectRuntimeController;
use crate::elastic::compute_elastic_targets_for_chain;
use crate::resolved::ResolvedChainAudioConfig;

#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::resolved::stream_signatures_require_client_rebuild;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::chain_resolve::resolve_chain_audio_config;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::host::get_host;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::validation::validate_chain_channels_against_devices;

impl ProjectRuntimeController {
    pub fn upsert_chain(&mut self, project: &Project, chain: &Chain) -> Result<()> {
        self.upsert_chain_modal(project, chain, false)
    }

    /// #454-T5: upsert that lets the previous preset/scene tail ring out in
    /// parallel (spillover) instead of being cut. Same lock-free path.
    pub fn upsert_chain_spillover(&mut self, project: &Project, chain: &Chain) -> Result<()> {
        self.upsert_chain_modal(project, chain, true)
    }

    fn upsert_chain_modal(
        &mut self,
        project: &Project,
        chain: &Chain,
        spillover: bool,
    ) -> Result<()> {
        log::info!(
            "upserting chain '{}', enabled={}",
            chain.id.0,
            chain.enabled
        );
        if !chain.enabled {
            // #522 pause (O(1) re-enable); #808 still re-render an armed DI.
            self.pause_chain(&chain.id);
            self.rearm_di_stream_after_rebuild(chain);
            return Ok(());
        }
        // Issue #522: fast-path resume of a paused chain — clear draining
        // and return; no CPAL queries, no NAM reload, no graph rebuild.
        //
        // Issue #545: fan over every input-group runtime, not just the
        // first. The previous `runtime_for_chain` call only touched
        // group 0, so chains with multiple physical input devices
        // stayed half-muted after toggle-on. Mirrors the fan-out in
        // `pause_chain`.
        if self.active_chains.contains_key(&chain.id) {
            let runtimes = self.runtime_graph.runtimes_for(&chain.id);
            if let Some(first) = runtimes.first() {
                if first.is_draining() {
                    log::info!(
                        "resuming paused chain '{}' across {} input group(s) (fast path)",
                        chain.id.0,
                        runtimes.len(),
                    );
                    for runtime in &runtimes {
                        runtime.clear_draining();
                    }
                    return Ok(());
                }
            }
        }

        #[cfg(all(target_os = "linux", feature = "jack"))]
        {
            // Delegate the ordered teardown + jackd spawn to the supervisor —
            // ensure_jack_servers handles would_restart + self.stop() + the
            // ensure_server retry loop.
            self.ensure_jack_servers(project)?;
            let resolved =
                crate::jack_resolve_chain_config(chain, &self.supervisor, &self.io_bindings)?;
            return self.upsert_chain_with_resolved(chain, resolved, spillover);
        }

        #[cfg(not(all(target_os = "linux", feature = "jack")))]
        {
            let host = get_host();
            validate_chain_channels_against_devices(host, chain, &self.io_bindings)?;
            let resolved = resolve_chain_audio_config(host, project, chain, &self.io_bindings)?;
            self.upsert_chain_with_resolved(chain, resolved, spillover)
        }
    }

    // `schedule_chain_activation` (issue #672 cold activation + #808 DI re-arm)
    // lives in `controller_chain_activation.rs` (line-cap split).

    pub(crate) fn upsert_chain_with_resolved(
        &mut self,
        chain: &Chain,
        resolved: ResolvedChainAudioConfig,
        spillover: bool,
    ) -> Result<()> {
        // Rebuild the JACK client + DSP worker only when the I/O layout
        // actually changed (input/output channels, mode, sample rate, etc).
        // A block toggle / param edit keeps the same stream_signature and
        // goes through the soft-reconfig path so we don't drop audio every
        // time the user tweaks a knob. A channel (un)check flips the
        // signature and triggers teardown+rebuild (issue #294 original).
        //
        // Known caveat: some edits that DO preserve the signature have been
        // observed to leave the in-place block pipeline reading silence on
        // Linux/JACK. The workaround is toggling the chain off+on — if you
        // hit that, widen this predicate for the specific edit that broke
        // flow, don't flip the whole thing back to unconditional rebuild
        // (that regresses block toggles on RT kernels).
        // On Linux/JACK we register the DEVICE's max channels at client
        // creation, not the chain's chosen subset — so a channel-selection
        // change (mono[0] ↔ mono[1] ↔ stereo) does NOT change port count and
        // does NOT require a client rebuild. Only device_id / sample_rate /
        // buffer_size / port-total changes demand a new AsyncClient.
        //
        // Rebuilding the client on every channel toggle is what hits the
        // libjack "Cannot open shm segment" regression from issue #294 /
        // #308. Keeping the client alive sidesteps the corruption entirely.
        #[cfg(all(target_os = "linux", feature = "jack"))]
        let needs_stream_rebuild = self
            .active_chains
            .get(&chain.id)
            .map(|active| {
                stream_signatures_require_client_rebuild(
                    &active.stream_signature,
                    &resolved.stream_signature,
                )
            })
            .unwrap_or(true);

        #[cfg(not(all(target_os = "linux", feature = "jack")))]
        let needs_stream_rebuild = self
            .active_chains
            .get(&chain.id)
            .map(|active| active.stream_signature != resolved.stream_signature)
            .unwrap_or(true);

        // #669: track the real device sample rate the runtime is built at, so
        // the DI-loop loader resamples loops to it instead of a stale 48000.
        self.sample_rate = resolved.sample_rate as u32;

        // Tear down the previous ActiveChainRuntime BEFORE mutating shared
        // runtime state or building the replacement. Otherwise HashMap::insert
        // drops the old runtime only after the new one is fully constructed,
        // which on JACK leaves the old client alive while the new one tries
        // to register with the same name — the new client gets a suffixed
        // name, connect_ports_by_name binds to the old client's ports, and
        // when the old runtime is finally dropped the new client is orphaned.
        if needs_stream_rebuild {
            self.teardown_active_chain_for_rebuild(&chain.id);
        }

        let elastic_targets =
            compute_elastic_targets_for_chain(chain, &resolved, &self.io_bindings);
        // upsert_chain (re)builds every per-input runtime for this chain and
        // returns the first; fetch the full ordered (group, runtime) list
        // from the graph so the cpal layer can wire each physical input
        // device to its own runtime and mix them at the shared output
        // (issue #350 phase 3).
        if spillover {
            self.runtime_graph.upsert_chain_spillover(
                chain,
                resolved.sample_rate,
                &resolved.by_device,
                needs_stream_rebuild,
                &elastic_targets,
                &self.io_bindings,
            )?;
        } else {
            self.runtime_graph.upsert_chain(
                chain,
                resolved.sample_rate,
                &resolved.by_device,
                needs_stream_rebuild,
                &elastic_targets,
                &self.io_bindings,
            )?;
        }

        if needs_stream_rebuild {
            let runtimes = self.runtime_graph.runtimes_with_groups_for(&chain.id);
            // Issue #672: wrap each group runtime in a LiveRuntimeSlot, keep a
            // handle so the control worker can publish a rebuilt runtime into the
            // exact slot the new streams read, then build the streams from them.
            let slots = crate::build_chain_slots(&runtimes);
            for (group, slot) in &slots {
                self.chain_slots
                    .insert((chain.id.clone(), *group), slot.handle());
            }
            // #771: one DI playback cell per output stream, shared with
            // arm_di_stream through the controller map.
            let di_cells: Vec<_> = (0..resolved.outputs.len())
                .map(|j| self.di_playback_cell(&chain.id, j))
                .collect();
            let active = crate::build_active_chain_runtime(
                &chain.id,
                chain,
                resolved,
                slots,
                &self.io_bindings,
                &di_cells,
            )?;
            self.active_chains.insert(chain.id.clone(), active);
            // #771: an armed DI re-renders against the fresh streams.
            self.rearm_di_stream_after_rebuild(chain);
        }

        Ok(())
    }
}
