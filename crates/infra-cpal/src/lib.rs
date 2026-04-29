use anyhow::{anyhow, bail, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{BufferSize, SampleFormat, Stream, StreamConfig, SupportedStreamConfig};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::SupportedStreamConfigRange;

// Single owner of the jackd lifecycle on Linux (issue #308). The supervisor
// types compile on any platform with the jack feature so unit tests can
// exercise the state machine via MockBackend in the macOS/Windows dev loop.
// On those platforms the module has no live consumer (LiveJackBackend and the
// RuntimeController supervisor field are linux+jack-only), hence the targeted
// allow below; Linux production builds keep the strict lint.
#[cfg(feature = "jack")]
#[cfg_attr(
    not(all(target_os = "linux", feature = "jack")),
    allow(dead_code, unused_imports)
)]
mod jack_supervisor;

mod host;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use host::get_host;
#[cfg(all(target_os = "linux", feature = "jack"))]
use host::jack_server_is_running;
#[cfg(all(target_os = "linux", feature = "jack"))]
use host::using_jack_direct;

#[cfg(all(target_os = "linux", feature = "jack"))]
mod usb_proc;
#[cfg(all(target_os = "linux", feature = "jack"))]
use usb_proc::{detect_all_usb_audio_cards, jack_server_is_running_for, UsbAudioCard};

// is_jack_host() removed — CPAL JACK host is never created.
// Use using_jack_direct() to check if the direct JACK backend is active.

use domain::ids::ChainId;
use engine::runtime::{
    process_input_f32, process_output_f32, ChainRuntimeState,
    RuntimeGraph,
};
use engine;

mod elastic;
use elastic::compute_elastic_targets_for_chain;

#[cfg(all(target_os = "linux", feature = "jack"))]
mod cpu_affinity;

#[cfg(all(target_os = "linux", feature = "jack"))]
mod jack_handlers;

mod active_runtime;
use active_runtime::ActiveChainRuntime;

use project::project::Project;
use project::block::{InputEntry, OutputEntry};
use project::chain::Chain;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceDescriptor {
    pub id: String,
    pub name: String,
    pub channels: usize,
}

mod resolved;
use resolved::{
    ChainStreamSignature, InputStreamSignature, OutputStreamSignature, ResolvedChainAudioConfig,
    ResolvedInputDevice, ResolvedOutputDevice,
};
#[cfg(all(target_os = "linux", feature = "jack"))]
use resolved::{stream_signatures_require_client_rebuild, MAX_JACK_FRAMES};


#[cfg(all(target_os = "linux", feature = "jack"))]
mod jack_direct;
#[cfg(all(target_os = "linux", feature = "jack"))]
use jack_direct::build_jack_direct_chain;

pub struct ProjectRuntimeController {
    runtime_graph: RuntimeGraph,
    active_chains: HashMap<ChainId, ActiveChainRuntime>,
    /// Single owner of every jackd process openrig controls on Linux. Replaces
    /// the former ensure_jack_running / stop_jackd_for / jack_meta_for set of
    /// free functions with an explicit state machine (issue #308).
    #[cfg(all(target_os = "linux", feature = "jack"))]
    supervisor: jack_supervisor::JackSupervisor<jack_supervisor::LiveJackBackend>,
}
mod device_enum;
pub use device_enum::{
    has_new_devices, invalidate_device_cache, list_devices, list_input_device_descriptors,
    list_output_device_descriptors,
};
#[cfg(all(target_os = "linux", feature = "jack"))]
pub use device_enum::jack_is_running;

mod device_settings;
pub use device_settings::apply_device_settings;
#[cfg(all(target_os = "linux", feature = "jack"))]
pub use device_settings::start_jack_in_background;

pub fn build_streams_for_project(
    project: &Project,
    runtime_graph: &RuntimeGraph,
) -> Result<Vec<Stream>> {
    log::info!("building audio streams for project");

    // On Linux with JACK, no CPAL streams are ever needed — streaming is handled
    // entirely by the jack crate in build_active_chain_runtime. Also, calling
    // validate_channels_against_devices() here would probe ALSA PCM and disturb
    // USB audio devices.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        let _ = project;       // not needed on Linux/JACK
        let _ = runtime_graph; // not needed on Linux/JACK: all streaming handled by jack crate
        return Ok(Vec::new());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = get_host();
        validate_channels_against_devices(project, host)?;
        let mut resolved_chains = resolve_enabled_chain_audio_configs(host, project)?;
        let mut streams = Vec::new();
        for chain in &project.chains {
            if !chain.enabled {
                continue;
            }
            let runtime = runtime_graph
                .chains
                .get(&chain.id)
                .cloned()
                .ok_or_else(|| anyhow!("chain '{}' has no runtime state", chain.id.0))?;
            let resolved = resolved_chains
                .remove(&chain.id)
                .ok_or_else(|| anyhow!("chain '{}' missing resolved audio config", chain.id.0))?;
            let (input_streams, output_streams) = build_chain_streams(&chain.id, resolved, runtime)?;
            streams.extend(input_streams);
            streams.extend(output_streams);
        }
        Ok(streams)
    }
}

/// Build a synthetic ResolvedChainAudioConfig using only the jack crate.
/// No CPAL or ALSA access. The resolved config is only used to provide
/// sample_rate and stream_signature to the runtime graph — the direct JACK
/// backend ignores inputs/outputs entirely.
///
/// Consumes cached meta from the supervisor — callers must guarantee that
/// `ensure_jack_servers` ran beforehand so every active card is in the
/// `Ready` state.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_resolve_chain_config(
    chain: &Chain,
    supervisor: &jack_supervisor::JackSupervisor<jack_supervisor::LiveJackBackend>,
) -> Result<ResolvedChainAudioConfig> {
    // Resolve the JACK server for this chain by inspecting its I/O device_ids.
    // Chain entries may have:
    //   - "jack:<server_name>"  → use that server directly
    //   - "hw:<N>"              → find the card at hw:N and use its server
    //   - anything else         → fall back to first supervised running server
    let cards = detect_all_usb_audio_cards();

    let supervisor_has_ready = |name: &str| {
        matches!(
            supervisor.state(&jack_supervisor::ServerName::from(name)),
            Some(jack_supervisor::JackServerState::Ready { .. })
        )
    };

    let resolve_server = |device_id: &str| -> Option<String> {
        if let Some(name) = device_id.strip_prefix("jack:") {
            return Some(name.to_string());
        }
        if let Some(hw_num) = device_id.strip_prefix("hw:") {
            if let Some(card) = cards.iter().find(|c| c.card_num == hw_num) {
                return Some(card.server_name.clone());
            }
        }
        cards.iter()
            .find(|c| supervisor_has_ready(&c.server_name))
            .map(|c| c.server_name.clone())
    };

    // Determine server from first input entry, or fallback to first
    // supervisor-ready card.
    let server_name = chain.input_blocks().into_iter()
        .flat_map(|(_, ib)| ib.entries.iter())
        .find_map(|entry| resolve_server(&entry.device_id.0))
        .or_else(|| {
            cards.iter()
                .find(|c| supervisor_has_ready(&c.server_name))
                .map(|c| c.server_name.clone())
        })
        .ok_or_else(|| anyhow!("no running JACK server found for chain"))?;

    let meta = supervisor.meta(&jack_supervisor::ServerName::from(server_name.clone()))?;
    let device_id = format!("jack:{}", server_name);
    let sample_rate = meta.sample_rate as f32;
    let in_channels = meta.capture_port_count as u16;
    let out_channels = meta.playback_port_count as u16;

    let input_sigs: Vec<InputStreamSignature> = chain.input_blocks().into_iter()
        .flat_map(|(_, ib)| ib.entries.iter())
        .map(|entry| InputStreamSignature {
            device_id: device_id.clone(),
            channels: entry.channels.clone(),
            stream_channels: in_channels,
            sample_rate: meta.sample_rate,
            buffer_size_frames: meta.buffer_size,
        })
        .collect();

    let output_sigs: Vec<OutputStreamSignature> = chain.output_blocks().into_iter()
        .flat_map(|(_, ob)| ob.entries.iter())
        .map(|entry| OutputStreamSignature {
            device_id: device_id.clone(),
            channels: entry.channels.clone(),
            stream_channels: out_channels,
            sample_rate: meta.sample_rate,
            buffer_size_frames: meta.buffer_size,
        })
        .collect();

    Ok(ResolvedChainAudioConfig {
        inputs: Vec::new(),
        outputs: Vec::new(),
        sample_rate,
        stream_signature: ChainStreamSignature {
            inputs: input_sigs,
            outputs: output_sigs,
        },
    })
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
    fn jack_config_for_card(
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
            let resolved = jack_resolve_chain_config(chain, &self.supervisor)?;
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
            let resolved = jack_resolve_chain_config(chain, &self.supervisor)?;
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
            let active = build_active_chain_runtime(&chain.id, chain, resolved, runtime)?;
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
    fn teardown_active_chain_for_rebuild(&mut self, chain_id: &ChainId) {
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

mod chain_resolve;
pub use chain_resolve::resolve_project_chain_sample_rates;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use chain_resolve::{resolve_chain_audio_config, resolve_enabled_chain_audio_configs};

#[cfg(not(all(target_os = "linux", feature = "jack")))]
mod validation;
pub(crate) use validation::validate_buffer_size;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) use validation::{
    find_input_device_by_id, find_output_device_by_id, validate_channels_against_devices,
    validate_chain_channels_against_devices,
};
fn build_input_stream_for_input(
    chain_id: &ChainId,
    input_index: usize,
    resolved_input_device: ResolvedInputDevice,
    runtime: Arc<ChainRuntimeState>,
) -> Result<Stream> {
    log::debug!(
        "building input stream for chain '{}' input_index={}",
        chain_id.0,
        input_index
    );
    let sample_format = resolved_input_device.supported.sample_format();
    let sample_rate = resolved_input_sample_rate(&resolved_input_device);
    let buffer_size_frames = resolved_input_buffer_size_frames(&resolved_input_device);
    log::debug!(
        "input stream config: chain='{}', input_index={}, sample_rate={}, buffer_size={}, format={:?}, channels={}",
        chain_id.0, input_index, sample_rate, buffer_size_frames, sample_format, resolved_input_device.supported.channels()
    );
    let stream_config = build_stream_config(
        resolved_input_device.supported.channels(),
        sample_rate,
        buffer_size_frames,
    );
    let device = resolved_input_device.device;
    let stream = match sample_format {
        SampleFormat::F32 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_f32(&runtime_for_data, input_index, data, channels);
                    }));
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::I16 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut converted = Vec::new();
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    converted.resize(data.len(), 0.0);
                    for (dst, src) in converted.iter_mut().zip(data.iter().copied()) {
                        *dst = src as f32 / i16::MAX as f32;
                    }
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_f32(&runtime_for_data, input_index, &converted, channels);
                    }));
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::U16 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut converted = Vec::new();
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    converted.resize(data.len(), 0.0);
                    for (dst, src) in converted.iter_mut().zip(data.iter().copied()) {
                        *dst = (src as f32 / u16::MAX as f32) * 2.0 - 1.0;
                    }
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_f32(&runtime_for_data, input_index, &converted, channels);
                    }));
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::I32 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut converted = Vec::new();
            device.build_input_stream(
                &stream_config,
                move |data: &[i32], _| {
                    converted.resize(data.len(), 0.0);
                    for (dst, src) in converted.iter_mut().zip(data.iter().copied()) {
                        *dst = src as f32 / i32::MAX as f32;
                    }
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_f32(&runtime_for_data, input_index, &converted, channels);
                    }));
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        other => {
            bail!(
                "unsupported input sample format for chain '{}': {:?}",
                chain_id.0,
                other
            );
        }
    };
    Ok(stream)
}

fn build_output_stream_for_output(
    chain_id: &ChainId,
    output_index: usize,
    resolved_output_device: ResolvedOutputDevice,
    runtime: Arc<ChainRuntimeState>,
) -> Result<Stream> {
    log::debug!(
        "building output stream for chain '{}' output_index={}",
        chain_id.0,
        output_index
    );
    let sample_format = resolved_output_device.supported.sample_format();
    let sample_rate = resolved_output_sample_rate(&resolved_output_device);
    let buffer_size_frames = resolved_output_buffer_size_frames(&resolved_output_device);
    log::debug!(
        "output stream config: chain='{}', output_index={}, sample_rate={}, buffer_size={}, format={:?}, channels={}",
        chain_id.0, output_index, sample_rate, buffer_size_frames, sample_format, resolved_output_device.supported.channels()
    );
    let stream_config = build_stream_config(
        resolved_output_device.supported.channels(),
        sample_rate,
        buffer_size_frames,
    );
    let device = resolved_output_device.device;
    let stream = match sample_format {
        SampleFormat::F32 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [f32], _| {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_output_f32(&runtime_for_data, output_index, out, channels);
                    }));
                },
                move |err| log::error!("[{}] output stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::I16 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut temp = Vec::new();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [i16], _| {
                    temp.resize(out.len(), 0.0);
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_output_f32(&runtime_for_data, output_index, &mut temp, channels);
                    }));
                    for (dst, src) in out.iter_mut().zip(temp.iter()) {
                        *dst =
                            (*src * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                    }
                },
                move |err| log::error!("[{}] output stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::U16 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut temp = Vec::new();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [u16], _| {
                    temp.resize(out.len(), 0.0);
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_output_f32(&runtime_for_data, output_index, &mut temp, channels);
                    }));
                    for (dst, src) in out.iter_mut().zip(temp.iter()) {
                        let normalized =
                            ((*src + 1.0) * 0.5 * u16::MAX as f32).clamp(0.0, u16::MAX as f32);
                        *dst = normalized as u16;
                    }
                },
                move |err| log::error!("[{}] output stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::I32 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut temp = Vec::new();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [i32], _| {
                    temp.resize(out.len(), 0.0);
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_output_f32(&runtime_for_data, output_index, &mut temp, channels);
                    }));
                    for (dst, src) in out.iter_mut().zip(temp.iter()) {
                        *dst = (*src * i32::MAX as f32)
                            .clamp(i32::MIN as f32, i32::MAX as f32) as i32;
                    }
                },
                move |err| log::error!("[{}] output stream error: {}", error_chain_id, err),
                None,
            )?
        }
        other => {
            bail!(
                "unsupported output sample format for chain '{}': {:?}",
                chain_id.0,
                other
            );
        }
    };
    Ok(stream)
}

pub(crate) fn build_stream_config(channels: u16, sample_rate: u32, buffer_size_frames: u32) -> StreamConfig {
    StreamConfig {
        channels,
        sample_rate,
        buffer_size: BufferSize::Fixed(buffer_size_frames),
    }
}

fn build_chain_streams(
    chain_id: &ChainId,
    resolved: ResolvedChainAudioConfig,
    runtime: Arc<ChainRuntimeState>,
) -> Result<(Vec<Stream>, Vec<Stream>)> {
    // Deduplicate input streams by device: one CPAL stream per unique device.
    // Multiple entries on the same device share the stream — the engine
    // reads each entry's channels from the same raw data buffer.
    let mut input_streams = Vec::new();
    let mut seen_devices: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, resolved_input) in resolved.inputs.into_iter().enumerate() {
        let device_key = resolved_input.device.id().map(|id| id.to_string()).unwrap_or_default();
        if !seen_devices.insert(device_key.clone()) {
            log::info!("input[{}] shares device '{}', reusing existing CPAL stream", i, device_key);
            continue;
        }
        let stream =
            build_input_stream_for_input(chain_id, i, resolved_input, runtime.clone())?;
        input_streams.push(stream);
    }

    let mut output_streams = Vec::new();
    for (j, resolved_output) in resolved.outputs.into_iter().enumerate() {
        let stream =
            build_output_stream_for_output(chain_id, j, resolved_output, runtime.clone())?;
        output_streams.push(stream);
    }

    Ok((input_streams, output_streams))
}

fn build_active_chain_runtime(
    chain_id: &ChainId,
    #[allow(unused_variables)] chain: &Chain,
    resolved: ResolvedChainAudioConfig,
    runtime: Arc<ChainRuntimeState>,
) -> Result<ActiveChainRuntime> {
    log::info!("building active chain runtime for '{}', sample_rate={}", chain_id.0, resolved.sample_rate);
    let stream_signature = resolved.stream_signature.clone();

    // On Linux with JACK: use the jack crate directly for zero-overhead audio.
    // This bypasses CPAL entirely — the JACK process callback runs in the
    // real-time thread with no extra buffering.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        if jack_server_is_running() {
            log::info!("JACK detected — using direct JACK backend (bypassing CPAL)");
            let (jack_client, dsp_worker) = build_jack_direct_chain(chain_id, chain, runtime)?;
            return Ok(ActiveChainRuntime {
                stream_signature,
                _input_streams: Vec::new(),
                _output_streams: Vec::new(),
                _jack_client: Some(jack_client),
                _dsp_worker: Some(dsp_worker),
            });
        }
    }

    let (input_streams, output_streams) = build_chain_streams(chain_id, resolved, runtime)?;
    for stream in &input_streams {
        stream.play()?;
    }
    for stream in &output_streams {
        stream.play()?;
    }
    log::info!(
        "audio streams started for chain '{}': {} input(s), {} output(s)",
        chain_id.0,
        input_streams.len(),
        output_streams.len()
    );
    Ok(ActiveChainRuntime {
        stream_signature,
        _input_streams: input_streams,
        _output_streams: output_streams,
        #[cfg(all(target_os = "linux", feature = "jack"))]
        _jack_client: None,
        #[cfg(all(target_os = "linux", feature = "jack"))]
        _dsp_worker: None,
    })
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn build_chain_stream_signature_multi(
    chain: &Chain,
    inputs: &[ResolvedInputDevice],
    outputs: &[ResolvedOutputDevice],
) -> ChainStreamSignature {
    let chain_input_entries: Vec<&InputEntry> = chain.input_blocks()
        .into_iter()
        .flat_map(|(_, ib)| ib.entries.iter())
        .collect();
    let input_sigs: Vec<InputStreamSignature> = if !chain_input_entries.is_empty() {
        chain_input_entries
            .iter()
            .zip(inputs.iter())
            .map(|(ci, ri)| InputStreamSignature {
                device_id: ci.device_id.0.clone(),
                channels: ci.channels.clone(),
                stream_channels: ri.supported.channels(),
                sample_rate: resolved_input_sample_rate(ri),
                buffer_size_frames: resolved_input_buffer_size_frames(ri),
            })
            .collect()
    } else {
        inputs
            .iter()
            .map(|ri| InputStreamSignature {
                device_id: String::new(),
                channels: Vec::new(),
                stream_channels: ri.supported.channels(),
                sample_rate: resolved_input_sample_rate(ri),
                buffer_size_frames: resolved_input_buffer_size_frames(ri),
            })
            .collect()
    };

    let chain_output_entries: Vec<&OutputEntry> = chain.output_blocks()
        .into_iter()
        .flat_map(|(_, ob)| ob.entries.iter())
        .collect();
    let output_sigs: Vec<OutputStreamSignature> = if !chain_output_entries.is_empty() {
        chain_output_entries
            .iter()
            .zip(outputs.iter())
            .map(|(co, ro)| OutputStreamSignature {
                device_id: co.device_id.0.clone(),
                channels: co.channels.clone(),
                stream_channels: ro.supported.channels(),
                sample_rate: resolved_output_sample_rate(ro),
                buffer_size_frames: resolved_output_buffer_size_frames(ro),
            })
            .collect()
    } else {
        outputs
            .iter()
            .map(|ro| OutputStreamSignature {
                device_id: String::new(),
                channels: Vec::new(),
                stream_channels: ro.supported.channels(),
                sample_rate: resolved_output_sample_rate(ro),
                buffer_size_frames: resolved_output_buffer_size_frames(ro),
            })
            .collect()
    };

    ChainStreamSignature {
        inputs: input_sigs,
        outputs: output_sigs,
    }
}

fn resolved_input_sample_rate(resolved: &ResolvedInputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.sample_rate)
        .unwrap_or_else(|| resolved.supported.sample_rate())
}

fn resolved_output_sample_rate(resolved: &ResolvedOutputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.sample_rate)
        .unwrap_or_else(|| resolved.supported.sample_rate())
}

fn resolved_input_buffer_size_frames(resolved: &ResolvedInputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.buffer_size_frames)
        .unwrap_or(256)
}

pub(crate) fn resolved_output_buffer_size_frames(resolved: &ResolvedOutputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.buffer_size_frames)
        .unwrap_or(256)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn required_channel_count(channels: &[usize]) -> usize {
    channels
        .iter()
        .copied()
        .max()
        .map(|channel| channel + 1)
        .unwrap_or(0)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn select_supported_stream_config(
    default_config: &SupportedStreamConfig,
    supported_ranges: &[SupportedStreamConfigRange],
    requested_sample_rate: Option<u32>,
    required_channels: usize,
    context: &str,
) -> Result<SupportedStreamConfig> {
    let target_sample_rate = requested_sample_rate.unwrap_or_else(|| default_config.sample_rate());
    let default_format = default_config.sample_format();

    let best = supported_ranges
        .iter()
        .filter(|range| range.channels() as usize >= required_channels)
        .filter_map(|range| range.try_with_sample_rate(target_sample_rate))
        .min_by_key(|config| {
            (
                (config.channels() as usize != required_channels) as u8,
                (config.sample_format() != default_format) as u8,
                (config.channels() as usize).saturating_sub(required_channels),
            )
        });

    best.ok_or_else(|| {
        anyhow!(
            "{} invalid: no supported config for sample_rate={} with at least {} channels",
            context,
            target_sample_rate,
            required_channels
        )
    })
}

#[cfg(test)]
fn resolve_chain_runtime_sample_rate(
    chain_id: &str,
    input: &SupportedStreamConfig,
    output: &SupportedStreamConfig,
) -> Result<f32> {
    if input.sample_rate() != output.sample_rate() {
        bail!(
            "chain '{}' invalid: input sample_rate={} differs from output sample_rate={}",
            chain_id,
            input.sample_rate(),
            output.sample_rate()
        );
    }

    Ok(input.sample_rate() as f32)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolve_multi_io_sample_rate(
    chain_id: &str,
    inputs: &[ResolvedInputDevice],
    outputs: &[ResolvedOutputDevice],
) -> Result<f32> {
    let mut rate: Option<u32> = None;
    for ri in inputs {
        let sr = resolved_input_sample_rate(ri);
        if let Some(prev) = rate {
            if prev != sr {
                bail!(
                    "chain '{}' invalid: mismatched sample rates across inputs ({} vs {})",
                    chain_id,
                    prev,
                    sr
                );
            }
        }
        rate = Some(sr);
    }
    for ro in outputs {
        let sr = resolved_output_sample_rate(ro);
        if let Some(prev) = rate {
            if prev != sr {
                bail!(
                    "chain '{}' invalid: mismatched sample rates across I/O ({} vs {})",
                    chain_id,
                    prev,
                    sr
                );
            }
        }
        rate = Some(sr);
    }
    rate.map(|r| r as f32)
        .ok_or_else(|| anyhow!("chain '{}' has no inputs or outputs", chain_id))
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn max_supported_input_channels(device: &cpal::Device) -> Result<usize> {
    let max_supported = match device.supported_input_configs() {
        Ok(configs) => {
            let max = configs.map(|config| config.channels() as usize).max();
            log::info!("[max_supported_input_channels] supported_input_configs max={:?}", max);
            max
        }
        Err(e) => {
            log::warn!("[max_supported_input_channels] supported_input_configs error: {}", e);
            return Err(e.into());
        }
    };
    let default_channels = match device.default_input_config() {
        Ok(config) => {
            let ch = config.channels() as usize;
            log::info!("[max_supported_input_channels] default_input_config channels={}", ch);
            Some(ch)
        }
        Err(e) => {
            log::info!("[max_supported_input_channels] default_input_config error: {}", e);
            None
        }
    };
    max_supported_channels(default_channels, max_supported)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn max_supported_output_channels(device: &cpal::Device) -> Result<usize> {
    let max_supported = match device.supported_output_configs() {
        Ok(configs) => {
            let max = configs.map(|config| config.channels() as usize).max();
            log::info!("[max_supported_output_channels] supported_output_configs max={:?}", max);
            max
        }
        Err(e) => {
            log::warn!("[max_supported_output_channels] supported_output_configs error: {}", e);
            return Err(e.into());
        }
    };
    let default_channels = match device.default_output_config() {
        Ok(config) => {
            let ch = config.channels() as usize;
            log::info!("[max_supported_output_channels] default_output_config channels={}", ch);
            Some(ch)
        }
        Err(e) => {
            log::info!("[max_supported_output_channels] default_output_config error: {}", e);
            None
        }
    };
    max_supported_channels(default_channels, max_supported)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn max_supported_channels(
    default_channels: Option<usize>,
    max_supported_channels: Option<usize>,
) -> Result<usize> {
    max_supported_channels
        .or(default_channels)
        .ok_or_else(|| anyhow!("device exposes no supported channels"))
}

#[cfg(test)]
mod tests {
    use super::{build_stream_config, resolve_chain_runtime_sample_rate, AudioDeviceDescriptor, ProjectRuntimeController};
    use cpal::BufferSize;
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    use super::{max_supported_channels, required_channel_count, select_supported_stream_config, validate_buffer_size};
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    use cpal::{SampleFormat, SupportedBufferSize, SupportedStreamConfigRange};

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    fn supported_range(
        channels: u16,
        min_sample_rate: u32,
        max_sample_rate: u32,
    ) -> SupportedStreamConfigRange {
        SupportedStreamConfigRange::new(
            channels,
            min_sample_rate,
            max_sample_rate,
            SupportedBufferSize::Range { min: 64, max: 1024 },
            SampleFormat::F32,
        )
    }

    // ── AudioDeviceDescriptor ───────────────────────────────────────

    #[test]
    fn audio_device_descriptor_construction_stores_fields() {
        let desc = AudioDeviceDescriptor {
            id: "coreaudio:abc123".to_string(),
            name: "USB Audio Interface".to_string(),
            channels: 2,
        };
        assert_eq!(desc.id, "coreaudio:abc123");
        assert_eq!(desc.name, "USB Audio Interface");
        assert_eq!(desc.channels, 2);
    }

    #[test]
    fn audio_device_descriptor_equality_same_values_returns_true() {
        let a = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Device".to_string(),
            channels: 4,
        };
        let b = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Device".to_string(),
            channels: 4,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn audio_device_descriptor_equality_different_id_returns_false() {
        let a = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Device".to_string(),
            channels: 4,
        };
        let b = AudioDeviceDescriptor {
            id: "dev2".to_string(),
            name: "Device".to_string(),
            channels: 4,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn audio_device_descriptor_clone_produces_equal_copy() {
        let original = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "My Device".to_string(),
            channels: 8,
        };
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn audio_device_descriptor_debug_format_contains_fields() {
        let desc = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Test".to_string(),
            channels: 2,
        };
        let debug = format!("{:?}", desc);
        assert!(debug.contains("dev1"));
        assert!(debug.contains("Test"));
    }

    // ── select_supported_stream_config ──────────────────────────────

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn select_supported_stream_config_accepts_non_default_sample_rate_when_device_supports_it() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![
            supported_range(2, 44_100, 96_000),
            supported_range(1, 44_100, 96_000),
        ];

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            Some(44_100),
            2,
            "test-device",
        )
        .expect("supported non-default sample rate should resolve");

        assert_eq!(resolved.sample_rate(), 44_100);
        assert_eq!(resolved.channels(), 2);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn select_supported_stream_config_no_requested_rate_uses_default() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![supported_range(2, 44_100, 96_000)];

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            None,
            2,
            "test-device",
        )
        .expect("should use default sample rate");

        assert_eq!(resolved.sample_rate(), 48_000);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn select_supported_stream_config_unsupported_rate_returns_error() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![supported_range(2, 44_100, 44_100)];

        let result = select_supported_stream_config(
            &default_config,
            &supported,
            Some(96_000),
            2,
            "test-device",
        );

        assert!(result.is_err());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn select_supported_stream_config_insufficient_channels_returns_error() {
        let default_config = supported_range(1, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![supported_range(1, 44_100, 96_000)];

        let result = select_supported_stream_config(
            &default_config,
            &supported,
            Some(48_000),
            4,
            "test-device",
        );

        assert!(result.is_err());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn select_supported_stream_config_picks_minimum_channels_matching() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![
            supported_range(8, 44_100, 96_000),
            supported_range(2, 44_100, 96_000),
        ];

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            Some(48_000),
            2,
            "test-device",
        )
        .unwrap();

        assert_eq!(resolved.channels(), 2);
    }

    // ── resolve_chain_runtime_sample_rate ────────────────────────────

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn resolve_chain_runtime_sample_rate_rejects_mismatched_input_and_output_sample_rates() {
        let input = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let output = supported_range(2, 44_100, 44_100).with_max_sample_rate();

        let error = resolve_chain_runtime_sample_rate("chain:0", &input, &output)
            .expect_err("mismatched rates should fail");

        assert!(error.to_string().contains("sample_rate"));
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn resolve_chain_runtime_sample_rate_matching_rates_returns_rate() {
        let input = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let output = supported_range(2, 48_000, 48_000).with_max_sample_rate();

        let rate = resolve_chain_runtime_sample_rate("chain:0", &input, &output).unwrap();

        assert_eq!(rate, 48_000.0);
    }

    // ── max_supported_channels ──────────────────────────────────────

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn max_supported_channels_prefers_supported_capacity_over_default() {
        let resolved =
            max_supported_channels(Some(2), Some(8)).expect("supported channels should resolve");

        assert_eq!(resolved, 8);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn max_supported_channels_uses_default_when_supported_list_is_empty() {
        let resolved =
            max_supported_channels(Some(2), None).expect("default channels should resolve");

        assert_eq!(resolved, 2);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn max_supported_channels_both_none_returns_error() {
        let result = max_supported_channels(None, None);
        assert!(result.is_err());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn max_supported_channels_only_supported_uses_supported() {
        let resolved =
            max_supported_channels(None, Some(6)).expect("should use supported channels");
        assert_eq!(resolved, 6);
    }

    // ── required_channel_count ──────────────────────────────────────

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn required_channel_count_empty_returns_zero() {
        assert_eq!(required_channel_count(&[]), 0);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn required_channel_count_single_channel_zero_returns_one() {
        assert_eq!(required_channel_count(&[0]), 1);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn required_channel_count_stereo_returns_two() {
        assert_eq!(required_channel_count(&[0, 1]), 2);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn required_channel_count_non_contiguous_returns_max_plus_one() {
        assert_eq!(required_channel_count(&[0, 3, 7]), 8);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn required_channel_count_single_high_channel_returns_correct() {
        assert_eq!(required_channel_count(&[5]), 6);
    }

    // ── validate_buffer_size ────────────────────────────────────────

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn validate_buffer_size_within_range_succeeds() {
        let supported = SupportedBufferSize::Range { min: 64, max: 1024 };
        let result = validate_buffer_size(256, &supported, "test");
        assert!(result.is_ok());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn validate_buffer_size_at_min_boundary_succeeds() {
        let supported = SupportedBufferSize::Range { min: 64, max: 1024 };
        let result = validate_buffer_size(64, &supported, "test");
        assert!(result.is_ok());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn validate_buffer_size_at_max_boundary_succeeds() {
        let supported = SupportedBufferSize::Range { min: 64, max: 1024 };
        let result = validate_buffer_size(1024, &supported, "test");
        assert!(result.is_ok());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn validate_buffer_size_below_min_returns_error() {
        let supported = SupportedBufferSize::Range { min: 64, max: 1024 };
        let result = validate_buffer_size(32, &supported, "test");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("outside supported range"));
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn validate_buffer_size_above_max_returns_error() {
        let supported = SupportedBufferSize::Range { min: 64, max: 1024 };
        let result = validate_buffer_size(2048, &supported, "test");
        assert!(result.is_err());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn validate_buffer_size_unknown_always_succeeds() {
        let supported = SupportedBufferSize::Unknown;
        let result = validate_buffer_size(9999, &supported, "test");
        assert!(result.is_ok());
    }

    // ── build_stream_config ─────────────────────────────────────────

    #[test]
    fn build_stream_config_sets_channels_and_rate() {
        let config = build_stream_config(2, 48_000, 256);
        assert_eq!(config.channels, 2);
        assert_eq!(config.sample_rate, 48_000);
        assert_eq!(config.buffer_size, BufferSize::Fixed(256));
    }

    #[test]
    fn build_stream_config_mono_128_buffer() {
        let config = build_stream_config(1, 44_100, 128);
        assert_eq!(config.channels, 1);
        assert_eq!(config.sample_rate, 44_100);
        assert_eq!(config.buffer_size, BufferSize::Fixed(128));
    }

    // ── build_stream_config edge cases ──────────────────────────────────────

    #[test]
    fn build_stream_config_high_sample_rate() {
        let config = build_stream_config(2, 96_000, 512);
        assert_eq!(config.channels, 2);
        assert_eq!(config.sample_rate, 96_000);
        assert_eq!(config.buffer_size, BufferSize::Fixed(512));
    }

    #[test]
    fn build_stream_config_large_buffer() {
        let config = build_stream_config(8, 48_000, 1024);
        assert_eq!(config.channels, 8);
        assert_eq!(config.buffer_size, BufferSize::Fixed(1024));
    }

    // ── validate_buffer_size edge cases ─────────────────────────────────────

    #[test]
    fn validate_buffer_size_exactly_one_element_range_succeeds() {
        let supported = SupportedBufferSize::Range { min: 256, max: 256 };
        let result = validate_buffer_size(256, &supported, "test");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_buffer_size_exactly_one_element_range_rejects_other() {
        let supported = SupportedBufferSize::Range { min: 256, max: 256 };
        let result = validate_buffer_size(128, &supported, "test");
        assert!(result.is_err());
    }

    // ── required_channel_count more edge cases ──────────────────────────────

    #[test]
    fn required_channel_count_duplicate_channels() {
        // Duplicate channels should still return max+1
        assert_eq!(required_channel_count(&[0, 0, 0]), 1);
    }

    #[test]
    fn required_channel_count_unsorted_channels() {
        assert_eq!(required_channel_count(&[3, 1, 5, 2]), 6);
    }

    // ── max_supported_channels additional tests ─────────────────────────────

    #[test]
    fn max_supported_channels_same_default_and_supported() {
        let resolved = max_supported_channels(Some(4), Some(4)).unwrap();
        assert_eq!(resolved, 4);
    }

    #[test]
    fn max_supported_channels_zero_default_with_some_supported() {
        let resolved = max_supported_channels(Some(0), Some(2)).unwrap();
        assert_eq!(resolved, 2);
    }

    // ── select_supported_stream_config additional tests ─────────────────────

    #[test]
    fn select_supported_stream_config_empty_ranges_returns_error() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported: Vec<SupportedStreamConfigRange> = vec![];

        let result = select_supported_stream_config(
            &default_config,
            &supported,
            Some(48_000),
            2,
            "test-device",
        );

        assert!(result.is_err(), "empty ranges should return error");
    }

    #[test]
    fn select_supported_stream_config_zero_channels_required() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![supported_range(2, 44_100, 96_000)];

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            Some(48_000),
            0,
            "test-device",
        )
        .expect("zero required channels should match any range");

        assert!(resolved.channels() >= 1);
    }

    #[test]
    fn select_supported_stream_config_prefers_exact_channel_match() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![
            supported_range(4, 44_100, 96_000),
            supported_range(2, 44_100, 96_000),
            supported_range(8, 44_100, 96_000),
        ];

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            Some(48_000),
            2,
            "test-device",
        )
        .unwrap();

        assert_eq!(resolved.channels(), 2, "should prefer exact channel count");
    }

    // ── resolve_chain_runtime_sample_rate tests ─────────────────────────────

    #[test]
    fn resolve_chain_runtime_sample_rate_high_rate_matching() {
        let input = supported_range(2, 96_000, 96_000).with_max_sample_rate();
        let output = supported_range(2, 96_000, 96_000).with_max_sample_rate();
        let rate = resolve_chain_runtime_sample_rate("chain:0", &input, &output).unwrap();
        assert_eq!(rate, 96_000.0);
    }

    #[test]
    fn resolve_chain_runtime_sample_rate_low_rate_matching() {
        let input = supported_range(2, 44_100, 44_100).with_max_sample_rate();
        let output = supported_range(2, 44_100, 44_100).with_max_sample_rate();
        let rate = resolve_chain_runtime_sample_rate("chain:0", &input, &output).unwrap();
        assert_eq!(rate, 44_100.0);
    }

    // ── AudioDeviceDescriptor additional tests ──────────────────────────────

    #[test]
    fn audio_device_descriptor_different_channels_not_equal() {
        let a = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Device".to_string(),
            channels: 2,
        };
        let b = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Device".to_string(),
            channels: 4,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn audio_device_descriptor_different_name_not_equal() {
        let a = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Device A".to_string(),
            channels: 2,
        };
        let b = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Device B".to_string(),
            channels: 2,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn audio_device_descriptor_zero_channels() {
        let desc = AudioDeviceDescriptor {
            id: "dev0".to_string(),
            name: "Null".to_string(),
            channels: 0,
        };
        assert_eq!(desc.channels, 0);
    }

    // ── InputStreamSignature / OutputStreamSignature equality ────────────────

    #[test]
    fn input_stream_signature_equality() {
        use super::InputStreamSignature;
        let a = InputStreamSignature {
            device_id: "dev1".to_string(),
            channels: vec![0, 1],
            stream_channels: 2,
            sample_rate: 48_000,
            buffer_size_frames: 256,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn input_stream_signature_different_rate_not_equal() {
        use super::InputStreamSignature;
        let a = InputStreamSignature {
            device_id: "dev1".to_string(),
            channels: vec![0, 1],
            stream_channels: 2,
            sample_rate: 48_000,
            buffer_size_frames: 256,
        };
        let b = InputStreamSignature {
            sample_rate: 44_100,
            ..a.clone()
        };
        assert_ne!(a, b);
    }

    #[test]
    fn output_stream_signature_equality() {
        use super::OutputStreamSignature;
        let a = OutputStreamSignature {
            device_id: "dev1".to_string(),
            channels: vec![0, 1],
            stream_channels: 2,
            sample_rate: 48_000,
            buffer_size_frames: 256,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn output_stream_signature_different_channels_not_equal() {
        use super::OutputStreamSignature;
        let a = OutputStreamSignature {
            device_id: "dev1".to_string(),
            channels: vec![0, 1],
            stream_channels: 2,
            sample_rate: 48_000,
            buffer_size_frames: 256,
        };
        let b = OutputStreamSignature {
            channels: vec![0],
            ..a.clone()
        };
        assert_ne!(a, b);
    }

    // ── ChainStreamSignature equality ───────────────────────────────────────

    #[test]
    fn chain_stream_signature_equality() {
        use super::{ChainStreamSignature, InputStreamSignature, OutputStreamSignature};
        let a = ChainStreamSignature {
            inputs: vec![InputStreamSignature {
                device_id: "dev1".to_string(),
                channels: vec![0],
                stream_channels: 1,
                sample_rate: 48_000,
                buffer_size_frames: 256,
            }],
            outputs: vec![OutputStreamSignature {
                device_id: "dev2".to_string(),
                channels: vec![0, 1],
                stream_channels: 2,
                sample_rate: 48_000,
                buffer_size_frames: 256,
            }],
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn chain_stream_signature_different_inputs_not_equal() {
        use super::{ChainStreamSignature, InputStreamSignature};
        let a = ChainStreamSignature {
            inputs: vec![InputStreamSignature {
                device_id: "dev1".to_string(),
                channels: vec![0],
                stream_channels: 1,
                sample_rate: 48_000,
                buffer_size_frames: 256,
            }],
            outputs: vec![],
        };
        let b = ChainStreamSignature {
            inputs: vec![InputStreamSignature {
                device_id: "dev2".to_string(),
                channels: vec![0],
                stream_channels: 1,
                sample_rate: 48_000,
                buffer_size_frames: 256,
            }],
            outputs: vec![],
        };
        assert_ne!(a, b);
    }

    // ── is_asio_host (non-Windows always returns false) ─────────────────────

    #[test]
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    fn is_asio_host_returns_false_on_non_windows() {
        use super::host::is_asio_host;
        let host = cpal::default_host();
        assert!(!is_asio_host(&host), "non-Windows host should not be ASIO");
    }

    // ── insert_return_as_input_entry ────────────────────────────────────────

    #[test]
    fn insert_return_as_input_entry_copies_return_fields() {
        use super::chain_resolve::insert_return_as_input_entry;
        use project::block::{InsertBlock, InsertEndpoint};
        use project::chain::ChainInputMode;
        use domain::ids::DeviceId;

        let insert = InsertBlock {
            model: "external_loop".into(),
            send: InsertEndpoint {
                device_id: DeviceId("send".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            },
            return_: InsertEndpoint {
                device_id: DeviceId("return".into()),
                mode: ChainInputMode::Stereo,
                channels: vec![2, 3],
            },
        };
        let entry = insert_return_as_input_entry(&insert);
        assert_eq!(entry.device_id.0, "return");
        assert_eq!(entry.channels, vec![2, 3]);
    }

    // ── insert_send_as_output_entry ─────────────────────────────────────────

    #[test]
    fn insert_send_as_output_entry_mono_becomes_mono() {
        use super::chain_resolve::insert_send_as_output_entry;
        use project::block::{InsertBlock, InsertEndpoint};
        use project::chain::{ChainInputMode, ChainOutputMode};
        use domain::ids::DeviceId;

        let insert = InsertBlock {
            model: "external_loop".into(),
            send: InsertEndpoint {
                device_id: DeviceId("send".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            },
            return_: InsertEndpoint {
                device_id: DeviceId("return".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            },
        };
        let entry = insert_send_as_output_entry(&insert);
        assert_eq!(entry.device_id.0, "send");
        assert!(matches!(entry.mode, ChainOutputMode::Mono));
    }

    #[test]
    fn insert_send_as_output_entry_stereo_becomes_stereo() {
        use super::chain_resolve::insert_send_as_output_entry;
        use project::block::{InsertBlock, InsertEndpoint};
        use project::chain::{ChainInputMode, ChainOutputMode};
        use domain::ids::DeviceId;

        let insert = InsertBlock {
            model: "external_loop".into(),
            send: InsertEndpoint {
                device_id: DeviceId("send".into()),
                mode: ChainInputMode::Stereo,
                channels: vec![0, 1],
            },
            return_: InsertEndpoint {
                device_id: DeviceId("return".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            },
        };
        let entry = insert_send_as_output_entry(&insert);
        assert!(matches!(entry.mode, ChainOutputMode::Stereo));
    }

    #[test]
    fn is_healthy_returns_true_when_no_chains_active() {
        let mut controller = ProjectRuntimeController {
            runtime_graph: super::RuntimeGraph {
                chains: std::collections::HashMap::new(),
            },
            active_chains: std::collections::HashMap::new(),
            #[cfg(all(target_os = "linux", feature = "jack"))]
            supervisor: super::jack_supervisor::JackSupervisor::new(
                super::jack_supervisor::LiveJackBackend::new(),
            ),
        };
        assert!(controller.is_healthy());
    }

    #[test]
    fn is_running_returns_false_when_no_chains() {
        let controller = ProjectRuntimeController {
            runtime_graph: super::RuntimeGraph {
                chains: std::collections::HashMap::new(),
            },
            active_chains: std::collections::HashMap::new(),
            #[cfg(all(target_os = "linux", feature = "jack"))]
            supervisor: super::jack_supervisor::JackSupervisor::new(
                super::jack_supervisor::LiveJackBackend::new(),
            ),
        };
        assert!(!controller.is_running());
    }

    // ── Regression tests for issue #294: stale JACK client on chain reconfigure ──
    //
    // Reconfiguring input channels on an active chain (e.g. unchecking a channel
    // in a stereo input) used to leave the previous JACK client alive while the
    // replacement client was being built, because HashMap::insert only dropped
    // the old ActiveChainRuntime AFTER constructing the new one. On JACK, the
    // new client would get a suffixed name while connect_ports_by_name still
    // used the literal (unsuffixed) name — so the connections bound to the
    // OLD client's ports, which then vanished when the old client was finally
    // dropped, leaving the new client orphaned and audio silent.
    //
    // The fix tears down the existing ActiveChainRuntime BEFORE building the
    // replacement (teardown_active_chain_for_rebuild), mirroring the pattern
    // in remove_chain. These tests cover the teardown helper directly; the
    // end-to-end "audio still flows after channel toggle" behavior is
    // verifiable only on real JACK hardware and is exercised manually on the
    // Orange Pi during regression testing.

    #[test]
    fn teardown_active_chain_for_rebuild_drops_entry_when_present() {
        let chain_id = super::ChainId("chain:0".into());
        let mut controller = ProjectRuntimeController {
            runtime_graph: super::RuntimeGraph {
                chains: std::collections::HashMap::new(),
            },
            active_chains: std::collections::HashMap::new(),
            #[cfg(all(target_os = "linux", feature = "jack"))]
            supervisor: super::jack_supervisor::JackSupervisor::new(
                super::jack_supervisor::LiveJackBackend::new(),
            ),
        };
        controller.active_chains.insert(chain_id.clone(), super::ActiveChainRuntime {
            stream_signature: super::ChainStreamSignature { inputs: vec![], outputs: vec![] },
            _input_streams: vec![],
            _output_streams: vec![],
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _jack_client: None,
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _dsp_worker: None,
        });
        assert!(controller.active_chains.contains_key(&chain_id));

        controller.teardown_active_chain_for_rebuild(&chain_id);

        assert!(!controller.active_chains.contains_key(&chain_id),
            "active_chains entry must be removed so the old JACK client/DSP worker are dropped \
             before a replacement is built");
    }

    #[test]
    fn teardown_active_chain_for_rebuild_is_noop_when_chain_absent() {
        let chain_id = super::ChainId("chain:missing".into());
        let mut controller = ProjectRuntimeController {
            runtime_graph: super::RuntimeGraph {
                chains: std::collections::HashMap::new(),
            },
            active_chains: std::collections::HashMap::new(),
            #[cfg(all(target_os = "linux", feature = "jack"))]
            supervisor: super::jack_supervisor::JackSupervisor::new(
                super::jack_supervisor::LiveJackBackend::new(),
            ),
        };

        controller.teardown_active_chain_for_rebuild(&chain_id);

        assert!(controller.active_chains.is_empty());
    }

    // ── Regression #316: teardown clears the draining flag for rebuild ──
    //
    // The JACK fix from #294 (this same `teardown_active_chain_for_rebuild`)
    // calls `set_draining(true)` on the live `Arc<ChainRuntimeState>` so the
    // audio callback bails out while the old CPAL/JACK streams are dropped.
    // The Arc stays alive in `runtime_graph` because the caller is about to
    // re-upsert it, and `RuntimeGraph::upsert_chain` reuses an existing
    // entry instead of rebuilding the state. Without a matching reset the
    // new streams' callbacks observe `is_draining()==true` from the very
    // first invocation and silence every segment on the chain — including
    // sibling InputEntries that were not touched by the channel edit. The
    // user-visible symptom is "remove a channel from one entry → audio of
    // the other entry on the same chain stops too" (issue #316). Toggling
    // the chain off then on works because `remove_chain` drops the Arc, so
    // the next enable rebuilds a fresh `ChainRuntimeState` with the flag
    // already initialized to `false`.
    #[test]
    fn teardown_active_chain_for_rebuild_clears_draining_so_rebuild_can_resume_audio() {
        use std::sync::Arc;
        let chain_id = super::ChainId("chain:316".into());
        let chain = project::chain::Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![],
        };
        let runtime_arc = Arc::new(
            engine::runtime::build_chain_runtime_state(&chain, 48_000.0, &[1024])
                .expect("empty chain runtime should build"),
        );

        let mut graph = super::RuntimeGraph {
            chains: std::collections::HashMap::new(),
        };
        graph.chains.insert(chain_id.clone(), Arc::clone(&runtime_arc));

        let mut active_chains = std::collections::HashMap::new();
        active_chains.insert(
            chain_id.clone(),
            super::ActiveChainRuntime {
                stream_signature: super::ChainStreamSignature {
                    inputs: vec![],
                    outputs: vec![],
                },
                _input_streams: vec![],
                _output_streams: vec![],
                #[cfg(all(target_os = "linux", feature = "jack"))]
                _jack_client: None,
                #[cfg(all(target_os = "linux", feature = "jack"))]
                _dsp_worker: None,
            },
        );

        let mut controller = ProjectRuntimeController {
            runtime_graph: graph,
            active_chains,
            #[cfg(all(target_os = "linux", feature = "jack"))]
            supervisor: super::jack_supervisor::JackSupervisor::new(
                super::jack_supervisor::LiveJackBackend::new(),
            ),
        };

        assert!(!runtime_arc.is_draining(), "freshly built runtime starts un-drained");

        controller.teardown_active_chain_for_rebuild(&chain_id);

        assert!(
            !runtime_arc.is_draining(),
            "teardown_active_chain_for_rebuild must clear the draining flag — \
             the Arc<ChainRuntimeState> is reused by the rebuild that follows, \
             and leaving the flag set silences every CPAL/JACK callback on the \
             chain (including sibling InputEntries) until the chain is fully \
             removed and re-added (#316)"
        );
    }

    // ── jack_config_for_card reads DeviceSettings (#308) ─────────────────
    //
    // Guarded to Linux+jack because that is the only cfg the function is
    // compiled for. On macOS/Windows these tests are compiled out — same
    // as the function itself.

    #[cfg(all(target_os = "linux", feature = "jack"))]
    fn test_card(device_id: &str) -> super::UsbAudioCard {
        super::UsbAudioCard {
            card_num: "4".into(),
            server_name: "openrig_hw4".into(),
            display_name: "test card".into(),
            device_id: device_id.into(),
            capture_channels: 2,
            playback_channels: 2,
        }
    }

    #[cfg(all(target_os = "linux", feature = "jack"))]
    fn empty_project() -> project::Project {
        project::Project {
            name: None,
            device_settings: Vec::new(),
            chains: Vec::new(),
        }
    }

    #[cfg(all(target_os = "linux", feature = "jack"))]
    #[test]
    fn jack_config_for_card_uses_device_settings_values() {
        use domain::ids::DeviceId;
        use project::device::DeviceSettings;

        let card = test_card("hw:4");
        let mut project = empty_project();
        project.device_settings.push(DeviceSettings {
            device_id: DeviceId("hw:4".into()),
            sample_rate: 48_000,
            buffer_size_frames: 64,
            bit_depth: 32,
            realtime: true,
            rt_priority: 80,
            nperiods: 2,
        });

        let config = ProjectRuntimeController::jack_config_for_card(&card, &project);

        assert!(config.realtime);
        assert_eq!(config.rt_priority, 80);
        assert_eq!(config.nperiods, 2);
        assert_eq!(config.sample_rate, 48_000);
        assert_eq!(config.buffer_size, 64);
    }

    #[cfg(all(target_os = "linux", feature = "jack"))]
    #[test]
    fn jack_config_for_card_falls_back_to_realtime_defaults_when_no_match() {
        let card = test_card("hw:4");
        // No matching device_settings — defaults are realtime + nperiods=3.
        // We ship nperiods=3 (not 2) because nperiods=2 triggered ALSA Broken
        // pipe on Q26 USB audio + RK3588 in hardware validation; the extra
        // period gives the USB driver enough slack without meaningfully
        // increasing latency (one period at 128 frames / 48kHz ≈ 2.7ms).
        let project = empty_project();

        let config = ProjectRuntimeController::jack_config_for_card(&card, &project);

        assert!(config.realtime);
        assert_eq!(config.rt_priority, 70);
        assert_eq!(config.nperiods, 3);
        assert_eq!(config.sample_rate, 48_000);
        assert_eq!(config.buffer_size, 64);
    }
}
