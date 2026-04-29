//! `ProjectRuntimeController` — the long-lived runtime owner that the
//! application layer (`adapter-gui`, `vst3`, ...) drives.
//!
//! Holds:
//!
//! - `runtime_graph: RuntimeGraph` — the engine-side per-chain
//!   `Arc<ChainRuntimeState>`s. Each chain's audio thread reads from
//!   here.
//! - `active_chains: HashMap<ChainId, ActiveChainRuntime>` — the cpal
//!   `Stream`s plus, on Linux+JACK, the live JACK `AsyncClient` and DSP
//!   worker handle.
//! - `supervisor` (Linux+JACK only) — `JackSupervisor<LiveJackBackend>`,
//!   the single owner of every `jackd` process.
//!
//! The impl block is a single ~590-LOC unit because every method either
//! mutates `active_chains` + `runtime_graph` together (sync, upsert,
//! teardown) or reads them in lock-step (poll_stream, poll_errors,
//! tap subscriptions). Splitting by method group would force every
//! caller of an inherent method to pull in a trait.

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::ChainId;
use engine::runtime::RuntimeGraph;
use project::chain::Chain;
use project::project::Project;

use crate::active_runtime::ActiveChainRuntime;
use crate::elastic::compute_elastic_targets_for_chain;
use crate::resolved::ResolvedChainAudioConfig;

#[cfg(all(target_os = "linux", feature = "jack"))]
use anyhow::bail;
#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::host::using_jack_direct;
#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::jack_supervisor;
#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::resolved::stream_signatures_require_client_rebuild;
#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::usb_proc::{detect_all_usb_audio_cards, UsbAudioCard};

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::chain_resolve::{resolve_chain_audio_config, resolve_enabled_chain_audio_configs};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::host::get_host;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::validation::{validate_channels_against_devices, validate_chain_channels_against_devices};

pub struct ProjectRuntimeController {
    pub(crate) runtime_graph: RuntimeGraph,
    pub(crate) active_chains: HashMap<ChainId, ActiveChainRuntime>,
    /// Single owner of every jackd process openrig controls on Linux. Replaces
    /// the former ensure_jack_running / stop_jackd_for / jack_meta_for set of
    /// free functions with an explicit state machine (issue #308).
    #[cfg(all(target_os = "linux", feature = "jack"))]
    pub(crate) supervisor: jack_supervisor::JackSupervisor<jack_supervisor::LiveJackBackend>,
}

impl ProjectRuntimeController {
    pub fn start(project: &Project) -> Result<Self> {
        log::info!("starting project runtime controller");
        let mut controller = Self {
            runtime_graph: RuntimeGraph {
                chains: HashMap::new(),
            },
            active_chains: HashMap::new(),
            #[cfg(all(target_os = "linux", feature = "jack"))]
            supervisor: jack_supervisor::JackSupervisor::new(
                jack_supervisor::LiveJackBackend::new(),
            ),
        };
        controller.sync_project(project)?;
        Ok(controller)
    }

    /// Translate a detected USB audio card + project-level device settings
    /// into a [`jack_supervisor::JackConfig`] suitable for `ensure_server`.
    /// Kept as a free helper on the controller so `sync_project` and
    /// `upsert_chain` share the same translation.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    pub(crate) fn jack_config_for_card(
        card: &UsbAudioCard,
        project: &Project,
    ) -> jack_supervisor::JackConfig {
        let matched = project
            .device_settings
            .iter()
            .find(|s| s.device_id.0 == card.device_id);
        let sample_rate = matched.map(|s| s.sample_rate).unwrap_or(48_000);
        let buffer_size = matched.map(|s| s.buffer_size_frames).unwrap_or(64);
        let nperiods = matched.map(|s| s.nperiods).unwrap_or(3);
        let realtime = matched.map(|s| s.realtime).unwrap_or(true);
        let rt_priority = matched.map(|s| s.rt_priority).unwrap_or(70);
        jack_supervisor::JackConfig {
            sample_rate,
            buffer_size,
            nperiods,
            realtime,
            rt_priority,
            card_num: card.card_num.parse().unwrap_or(0),
            capture_channels: card.capture_channels,
            playback_channels: card.playback_channels,
        }
    }

    /// Ensure every connected card has its jackd in the desired config. When
    /// a restart will be triggered for any card that still has active chains,
    /// drop the chains first — dropping an `AsyncClient` after its jackd has
    /// been SIGTERMed leaves the libjack global state in the
    /// `ClientStatus(FAILURE | SERVER_ERROR)` limbo documented in issue #294.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    fn ensure_jack_servers(&mut self, project: &Project) -> Result<()> {
        let cards = detect_all_usb_audio_cards();
        if cards.is_empty() {
            bail!("no USB audio interface found — connect a device before starting audio");
        }

        let configs: Vec<(jack_supervisor::ServerName, jack_supervisor::JackConfig)> = cards
            .iter()
            .map(|card| {
                (
                    jack_supervisor::ServerName::from(card.server_name.clone()),
                    Self::jack_config_for_card(card, project),
                )
            })
            .collect();

        // Fast path — buffer-only deltas go through jack_set_buffer_size
        // on a live client, no jackd restart, no libjack state corruption.
        // This is the behaviour the user already has on macOS/CoreAudio:
        // change the buffer and audio continues without interruption.
        let mut remaining: Vec<(&jack_supervisor::ServerName, &jack_supervisor::JackConfig)> =
            Vec::with_capacity(configs.len());
        for (name, cfg) in &configs {
            if self.supervisor.only_buffer_changed(name, cfg) {
                let server_device_id = format!("jack:{}", name);
                let live_client = self.active_chains.values().find(|ac| {
                    ac.stream_signature
                        .inputs
                        .first()
                        .map(|s| s.device_id.as_str() == server_device_id)
                        .unwrap_or(false)
                });
                match live_client {
                    Some(ac) => match ac.set_live_buffer_size(cfg.buffer_size) {
                        Ok(()) => {
                            self.supervisor.mark_buffer_resized(name, cfg.buffer_size);
                            log::info!(
                                "ensure_jack_servers: '{}' buffer_size → {} applied live (no restart)",
                                name,
                                cfg.buffer_size
                            );
                            continue;
                        }
                        Err(e) => {
                            log::warn!(
                                "ensure_jack_servers: live buffer resize failed on '{}' ({}), falling back to restart",
                                name,
                                e
                            );
                        }
                    },
                    None => {
                        log::debug!(
                            "ensure_jack_servers: no live client bound to '{}', skipping soft resize",
                            name
                        );
                    }
                }
            }
            remaining.push((name, cfg));
        }

        let any_would_restart = remaining
            .iter()
            .any(|(name, cfg)| self.supervisor.would_restart(name, cfg));
        if any_would_restart && !self.active_chains.is_empty() {
            log::info!(
                "ensure_jack_servers: JACK restart imminent, tearing down {} chain(s) first",
                self.active_chains.len()
            );
            self.stop();
            // Give libjack's client-side threads a moment to finish winding
            // down after `jack_deactivate` / `jack_client_close`. Without
            // this, killing jackd immediately after dropping AsyncClients
            // has been observed to leave libjack process-wide state confused
            // and the next `Client::new` fails with "Cannot open shm
            // segment" (issue #294 / #308). 500 ms is the shortest delay
            // that reliably clears the residual threads on the deployment
            // targets we test against.
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        for (name, config) in remaining {
            // The predictive teardown above already cleared any active chains
            // bound to a restarting server. The hook is a safety net.
            let mut hook = |_: &jack_supervisor::ServerName| {};
            self.supervisor.ensure_server(name, config, &mut hook)?;
        }
        Ok(())
    }

    pub fn sync_project(&mut self, project: &Project) -> Result<()> {
        log::debug!("syncing project runtime with {} chains", project.chains.len());

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
            validate_channels_against_devices(project, host)?;
            let mut resolved_chains = resolve_enabled_chain_audio_configs(host, project)?;

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

            for chain in &project.chains {
                if !chain.enabled {
                    continue;
                }

                let resolved = resolved_chains
                    .remove(&chain.id)
                    .ok_or_else(|| anyhow!("chain '{}' missing resolved audio config", chain.id.0))?;
                self.upsert_chain_with_resolved(chain, resolved)?;
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
            let resolved = crate::jack_resolve_chain_config(chain, &self.supervisor)?;
            self.upsert_chain_with_resolved(chain, resolved)?;
        }

        Ok(())
    }

    pub fn upsert_chain(&mut self, project: &Project, chain: &Chain) -> Result<()> {
        log::info!("upserting chain '{}', enabled={}", chain.id.0, chain.enabled);
        if !chain.enabled {
            self.remove_chain(&chain.id);
            return Ok(());
        }

        #[cfg(all(target_os = "linux", feature = "jack"))]
        {
            // Delegate the ordered teardown + jackd spawn to the supervisor —
            // ensure_jack_servers handles would_restart + self.stop() + the
            // ensure_server retry loop.
            self.ensure_jack_servers(project)?;
            let resolved = crate::jack_resolve_chain_config(chain, &self.supervisor)?;
            return self.upsert_chain_with_resolved(chain, resolved);
        }

        #[cfg(not(all(target_os = "linux", feature = "jack")))]
        {
            let host = get_host();
            validate_chain_channels_against_devices(host, chain)?;
            let resolved = resolve_chain_audio_config(host, project, chain)?;
            self.upsert_chain_with_resolved(chain, resolved)
        }
    }

    pub fn remove_chain(&mut self, chain_id: &ChainId) {
        log::info!("removing chain '{}' from runtime", chain_id.0);
        if let Some(runtime) = self.runtime_graph.runtime_for_chain(chain_id) {
            runtime.set_draining();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        self.active_chains.remove(chain_id);
        self.runtime_graph.remove_chain(chain_id);
    }

    pub fn stop(&mut self) {
        log::info!("stopping project runtime controller");
        self.active_chains.clear();
        self.runtime_graph.chains.clear();
        // NOTE: supervisor.client_count is NOT decremented here. The
        // supervisor's register_client / unregister_client API is unused on
        // this call path — ordered teardown is driven by the caller via
        // `would_restart` + `self.stop()` in `ensure_jack_servers`, not by
        // the supervisor's internal hook. If a future change starts calling
        // register_client inside build_active_chain_runtime, add the
        // matching unregister_client calls here to keep the count honest.
    }

    pub fn is_running(&self) -> bool {
        !self.active_chains.is_empty()
    }

    /// Check whether the audio backend is still healthy.
    ///
    /// On Linux/JACK: returns false when the JACK server has disappeared (e.g.
    /// USB audio device unplugged → udev restarts jackd). The caller should
    /// tear down the runtime and attempt reconnection once JACK reappears.
    ///
    /// On macOS/Windows (CoreAudio/WASAPI): always returns true — device loss
    /// is detected through stream error callbacks, not polling.
    pub fn is_healthy(&mut self) -> bool {
        if self.active_chains.is_empty() {
            return true;
        }
        #[cfg(all(target_os = "linux", feature = "jack"))]
        if using_jack_direct() {
            // Delegate to the supervisor. health_check is non-destructive —
            // any verdict other than Healthy triggers the reconnect path in
            // the health timer (adapter-gui), which calls try_reconnect. The
            // next ensure_server fires a fresh spawn for any zombie or
            // not-running server.
            let verdicts = self.supervisor.health_check();
            return verdicts
                .values()
                .all(|v| matches!(v, jack_supervisor::HealthStatus::Healthy));
        }
        true
    }

    /// Attempt to reconnect after the audio backend became unhealthy.
    ///
    /// Tears down all active chains, forces the supervisor to stop every
    /// tracked jackd, and re-syncs the project. Returns Ok(true) if
    /// reconnection succeeded, Ok(false) if the backend is not yet available
    /// (no USB device).
    pub fn try_reconnect(&mut self, project: &Project) -> Result<bool> {
        log::info!("try_reconnect: checking if audio backend is available");

        #[cfg(all(target_os = "linux", feature = "jack"))]
        if using_jack_direct() && detect_all_usb_audio_cards().is_empty() {
            log::debug!("try_reconnect: no USB audio card found");
            return Ok(false);
        }

        // Tear down everything cleanly. On Linux this includes forcing the
        // supervisor to drop its tracked jackd — sync_project's ensure_server
        // then re-spawns with the desired config.
        self.stop();
        #[cfg(all(target_os = "linux", feature = "jack"))]
        if let Err(e) = self.supervisor.shutdown_all() {
            log::warn!("try_reconnect: supervisor.shutdown_all failed: {}", e);
        }

        match self.sync_project(project) {
            Ok(()) => {
                log::info!(
                    "try_reconnect: successfully reconnected with {} chains",
                    self.active_chains.len()
                );
                Ok(true)
            }
            Err(e) => {
                log::warn!("try_reconnect: sync_project failed: {}", e);
                Err(e)
            }
        }
    }

    /// Returns stream data for a block in any running chain.
    pub fn poll_stream(&self, block_id: &domain::ids::BlockId) -> Option<Vec<block_core::StreamEntry>> {
        for (_, runtime) in &self.runtime_graph.chains {
            if let Some(entries) = runtime.poll_stream(block_id) {
                return Some(entries);
            }
        }
        None
    }

    /// Drains and returns all block errors that occurred since the last call.
    pub fn poll_errors(&self) -> Vec<engine::runtime::BlockError> {
        self.runtime_graph.chains.values()
            .flat_map(|runtime| runtime.poll_errors())
            .collect()
    }

    /// Returns the measured real-time latency in milliseconds for a given chain.
    pub fn measured_latency_ms(&self, chain_id: &ChainId) -> Option<f32> {
        self.runtime_graph.chains.get(chain_id)
            .map(|runtime| runtime.measured_latency_ms())
    }

    /// Arms a latency probe on the given chain: the next input callback
    /// injects a short beep, and the first output callback that sees it
    /// updates `measured_latency_ms`. No-op if the chain has no runtime.
    pub fn arm_latency_probe(&self, chain_id: &ChainId) {
        if let Some(runtime) = self.runtime_graph.chains.get(chain_id) {
            runtime.arm_latency_probe();
        }
    }

    /// Cancels any in-flight latency probe on the given chain. The UI
    /// calls this when the on-screen probe display window expires so a
    /// probe that never produced a detection does not stay armed.
    pub fn cancel_latency_probe(&self, chain_id: &ChainId) {
        if let Some(runtime) = self.runtime_graph.chains.get(chain_id) {
            runtime.cancel_latency_probe();
        }
    }

    /// Subscribe to raw pre-FX samples from a chain's input. See
    /// [`engine::runtime::ChainRuntimeState::subscribe_input_tap`] for the
    /// full contract. Returns an empty `Vec` if the chain has no runtime.
    ///
    /// `total_channels` should be at least `max(subscribed_channels) + 1`;
    /// any extra slots are unused. Pass the actual device-side channel
    /// count if you know it, otherwise compute it from the input entry.
    pub fn subscribe_input_tap(
        &self,
        chain_id: &ChainId,
        input_index: usize,
        total_channels: usize,
        subscribed_channels: &[usize],
        capacity_per_channel: usize,
    ) -> Vec<Arc<engine::spsc::SpscRing<f32>>> {
        match self.runtime_graph.chains.get(chain_id) {
            Some(runtime) => runtime.subscribe_input_tap(
                input_index,
                total_channels,
                subscribed_channels,
                capacity_per_channel,
            ),
            None => Vec::new(),
        }
    }

    /// Drop input taps with no surviving consumer handles across all
    /// chains. Cheap; intended to be called from a UI timer or window
    /// close handler.
    pub fn prune_dead_input_taps(&self) {
        for runtime in self.runtime_graph.chains.values() {
            runtime.prune_dead_input_taps();
        }
    }

    /// Subscribe to a per-stream stereo tap (post-FX, pre-mixdown) on a
    /// chain. Returns `[l_ring, r_ring]` — both rings always present
    /// because every stream is internally stereo. See
    /// [`engine::runtime::ChainRuntimeState::subscribe_stream_tap`] for
    /// the full contract. Returns rings that will stay empty if the
    /// chain has no runtime.
    pub fn subscribe_stream_tap(
        &self,
        chain_id: &ChainId,
        stream_index: usize,
        capacity_per_channel: usize,
    ) -> Option<[Arc<engine::spsc::SpscRing<f32>>; 2]> {
        self.runtime_graph
            .chains
            .get(chain_id)
            .map(|runtime| runtime.subscribe_stream_tap(stream_index, capacity_per_channel))
    }

    /// How many streams (input pipelines) a chain currently runs. Empty
    /// chains and chains without a runtime return 0.
    pub fn stream_count(&self, chain_id: &ChainId) -> usize {
        self.runtime_graph
            .chains
            .get(chain_id)
            .map(|runtime| runtime.stream_count())
            .unwrap_or(0)
    }

    /// Drop stream taps with no surviving consumer handles across all chains.
    pub fn prune_dead_stream_taps(&self) {
        for runtime in self.runtime_graph.chains.values() {
            runtime.prune_dead_stream_taps();
        }
    }

    /// Toggle the output-mute flag on every chain runtime. When true,
    /// the output stage zeros every frame — used by the Tuner window
    /// so the user can tune silently. Auto-cleared on window close.
    pub fn set_output_muted(&self, mute: bool) {
        for runtime in self.runtime_graph.chains.values() {
            runtime.set_output_muted(mute);
        }
    }

    fn upsert_chain_with_resolved(
        &mut self,
        chain: &Chain,
        resolved: ResolvedChainAudioConfig,
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

        let elastic_targets = compute_elastic_targets_for_chain(chain, &resolved);
        let runtime = self.runtime_graph.upsert_chain(
            chain,
            resolved.sample_rate,
            needs_stream_rebuild,
            &elastic_targets,
        )?;

        if needs_stream_rebuild {
            let active = crate::build_active_chain_runtime(&chain.id, chain, resolved, runtime)?;
            self.active_chains.insert(chain.id.clone(), active);
        }

        Ok(())
    }

    /// Drop the ActiveChainRuntime for `chain_id` so its JACK client / DSP
    /// worker / CPAL streams release their resources before a replacement is
    /// built. Drains the audio callback first (same dance as `remove_chain`)
    /// so NAM C++ destructors don't fire mid-callback.
    ///
    /// No-op when no runtime is active for that chain. Leaves
    /// `runtime_graph` untouched — the caller is about to re-upsert it.
    /// The draining flag set on the kept-alive `ChainRuntimeState` is cleared
    /// after the old streams are dropped so the upcoming rebuild's new
    /// CPAL/JACK callbacks don't inherit it and silence audio indefinitely
    /// (issue #316).
    pub(crate) fn teardown_active_chain_for_rebuild(&mut self, chain_id: &ChainId) {
        if !self.active_chains.contains_key(chain_id) {
            return;
        }
        let runtime = self.runtime_graph.runtime_for_chain(chain_id);
        if let Some(rt) = &runtime {
            rt.set_draining();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        self.active_chains.remove(chain_id);
        // The Arc<ChainRuntimeState> stays alive in `runtime_graph` and is
        // reused by the rebuild that follows. The new CPAL/JACK callbacks
        // call `process_input_f32`, which short-circuits on `is_draining()`
        // — so without this reset every rebuild after a signature change
        // (e.g. toggling an input channel) silences audio for every segment
        // on the chain, including sibling InputEntries that were not
        // touched, until the chain is fully removed and re-added.
        if let Some(rt) = runtime {
            rt.clear_draining();
        }
    }
}
