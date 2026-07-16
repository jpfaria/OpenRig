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

use anyhow::Result;
use std::cell::RefCell;
use std::collections::HashMap;

use std::sync::mpsc::Receiver;
use std::sync::Arc;

use domain::ids::ChainId;
use engine::runtime::{ChainRuntimeState, RuntimeGraph};
use project::chain::Chain;
use project::project::Project;

use crate::active_runtime::ActiveChainRuntime;
use crate::elastic::compute_elastic_targets_for_chain;
use crate::resolved::ResolvedChainAudioConfig;
use crate::{build_chain_runtime, BuildRequest, ControlWorker, LiveRuntimeSlot};

#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::host::using_jack_direct;
#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::jack_supervisor;
#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::resolved::stream_signatures_require_client_rebuild;
#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::usb_proc::{detect_all_usb_audio_cards, UsbAudioCard};
#[cfg(all(target_os = "linux", feature = "jack"))]
use anyhow::bail;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::chain_resolve::{resolve_chain_audio_config, resolve_enabled_chain_audio_configs};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::host::get_host;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::validation::{
    validate_chain_channels_against_devices, validate_channels_against_devices,
};

pub struct ProjectRuntimeController {
    pub(crate) runtime_graph: RuntimeGraph,
    pub(crate) active_chains: HashMap<ChainId, ActiveChainRuntime>,
    /// Issue #672 — per-`(chain, group)` swap point. The control worker
    /// publishes a rebuilt runtime here; once the stream callbacks read through
    /// these slots the swap is observed live, with no stream rebuild.
    pub(crate) chain_slots: HashMap<(ChainId, usize), LiveRuntimeSlot>,
    /// Issue #672 — dedicated thread that builds chain runtimes off the
    /// frontend thread so heavy commands never block the UI.
    pub(crate) worker: ControlWorker,
    /// Issue #672 — in-flight off-thread rebuilds. The worker only *builds*;
    /// `poll_pending_rebuilds` (called on the frontend tick) applies a finished
    /// build by swapping the slots and `runtime_graph` in lock-step so they
    /// stay consistent, and drops the superseded runtimes back on the worker.
    /// Issue #703: a build yields one runtime per input-entry group, each
    /// published into its own `(chain, group)` slot.
    #[allow(clippy::type_complexity)]
    pub(crate) pending_rebuilds: Vec<(
        ChainId,
        Receiver<Result<Vec<(usize, Arc<ChainRuntimeState>)>>>,
    )>,
    /// Issue #672 — in-flight cold activations (single-device chains). The
    /// worker builds the runtimes off-thread; since #693 it also validates
    /// and resolves the device config there (CoreAudio property queries cost
    /// hundreds of ms and must not hold the frontend). The resolved config
    /// comes back with the runtimes (issue #703: one per input-entry group);
    /// `poll_pending_rebuilds` then creates the cpal streams on the frontend
    /// (they are `!Send`) and installs the chain.
    #[allow(clippy::type_complexity)]
    pub(crate) pending_activations: Vec<(
        ChainId,
        Chain,
        Receiver<
            Result<(
                Vec<(usize, Arc<ChainRuntimeState>)>,
                ResolvedChainAudioConfig,
            )>,
        >,
    )>,
    /// Sample rate (Hz) the live streams were built at, captured from the last
    /// resolved chain config. The DI-loop loader reads this (via the
    /// dispatcher's `engine_sr`) to resample loops to the device rate; a stale
    /// value plays them at the wrong speed (#669). Defaults to 48000 until the
    /// first chain is built.
    pub(crate) sample_rate: u32,
    /// Model A (#716): the per-machine I/O binding registry. Device endpoints
    /// for every chain resolve from this (via
    /// [`engine::runtime_endpoints::resolve_chain_io`]), never from block
    /// `entries`. Set by the owner via [`Self::set_io_bindings`] before
    /// syncing/activating; defaults to empty until then.
    pub(crate) io_bindings: Vec<domain::io_binding::IoBinding>,
    /// Issue #717: per-chain dedicated DI-loop runtimes, alive only while the
    /// DI is armed. Each is a fully isolated copy of the chain's block graph
    /// fed by the loop — never the guitar runtime. `&self` arm/disarm mutate
    /// this, so it needs interior mutability; the controller is frontend-thread
    /// owned (cpal `Stream` is `!Send`), so a `RefCell` suffices.
    pub(crate) di_streams: RefCell<HashMap<ChainId, crate::di_stream::DiStreamHandle>>,
    /// Issue #771: one playback cell per (chain, flat output index). The
    /// output stream's callback clones its cell at build time and mixes
    /// whatever playback is parked there (wait-free load); arming parks the
    /// pre-rendered loop on the CHOSEN output's cell only. Entries are
    /// created on demand and survive stream rebuilds.
    pub(crate) di_playback_cells:
        RefCell<HashMap<(ChainId, usize), crate::di_playback::DiPlaybackCell>>,
    /// Issue #771: playbacks swapped out by disarm, freed on a LATER cycle so
    /// the audio callback is never the last owner of a multi-MB render
    /// buffer (invariant #8).
    pub(crate) di_retired: RefCell<Vec<std::sync::Arc<crate::di_playback::DiPlayback>>>,
    /// Single owner of every jackd process openrig controls on Linux. Replaces
    /// the former ensure_jack_running / stop_jackd_for / jack_meta_for set of
    /// free functions with an explicit state machine (issue #308).
    #[cfg(all(target_os = "linux", feature = "jack"))]
    pub(crate) supervisor: jack_supervisor::JackSupervisor<jack_supervisor::LiveJackBackend>,
}

impl ProjectRuntimeController {
    /// Construct a controller that owns a pre-built [`RuntimeGraph`] but has
    /// no live audio streams.  Intended for integration tests that need a real
    /// `ProjectRuntimeController` without opening audio devices (e.g. to verify
    /// chain runtime state without cpal I/O).
    pub fn for_testing(graph: RuntimeGraph) -> Self {
        Self::for_testing_with_sample_rate(graph, 48_000)
    }

    /// Like [`Self::for_testing`] but reports `sample_rate` Hz, so tests can
    /// exercise rate-dependent wiring (e.g. DI-loop resampling, #669) without
    /// opening audio devices.
    pub fn for_testing_with_sample_rate(graph: RuntimeGraph, sample_rate: u32) -> Self {
        let chain_slots = graph
            .chains
            .iter()
            .map(|(key, runtime)| (key.clone(), LiveRuntimeSlot::new(Arc::clone(runtime))))
            .collect();
        Self {
            runtime_graph: graph,
            active_chains: HashMap::new(),
            chain_slots,
            worker: ControlWorker::new(),
            pending_rebuilds: Vec::new(),
            pending_activations: Vec::new(),
            sample_rate,
            io_bindings: Vec::new(),
            di_streams: RefCell::new(HashMap::new()),
            di_playback_cells: RefCell::new(HashMap::new()),
            di_retired: RefCell::new(Vec::new()),
            #[cfg(all(target_os = "linux", feature = "jack"))]
            supervisor: jack_supervisor::JackSupervisor::new(
                jack_supervisor::LiveJackBackend::new(),
            ),
        }
    }

    pub fn start(project: &Project) -> Result<Self> {
        Self::start_with_io_bindings(project, Vec::new())
    }

    /// Like [`Self::start`] but installs the per-machine I/O binding registry
    /// BEFORE the initial `sync_project` schedules its cold-start activations.
    /// #716 (AUDIO-CRITICAL): `schedule_chain_activation` snapshots
    /// `self.io_bindings` into its worker job, so a binding-bound chain whose
    /// registry is installed only AFTER `start()` resolves zero inputs and
    /// bails "no input blocks". The owner must hand the registry here.
    pub fn start_with_io_bindings(
        project: &Project,
        io_bindings: Vec<domain::io_binding::IoBinding>,
    ) -> Result<Self> {
        log::info!("starting project runtime controller");
        let mut controller = Self {
            runtime_graph: RuntimeGraph {
                chains: HashMap::new(),
            },
            active_chains: HashMap::new(),
            chain_slots: HashMap::new(),
            worker: ControlWorker::new(),
            pending_rebuilds: Vec::new(),
            pending_activations: Vec::new(),
            // Updated to the real device rate by `upsert_chain_with_resolved`
            // as each chain is built below (#669).
            sample_rate: 48_000,
            io_bindings,
            di_streams: RefCell::new(HashMap::new()),
            di_playback_cells: RefCell::new(HashMap::new()),
            di_retired: RefCell::new(Vec::new()),
            #[cfg(all(target_os = "linux", feature = "jack"))]
            supervisor: jack_supervisor::JackSupervisor::new(
                jack_supervisor::LiveJackBackend::new(),
            ),
        };
        controller.sync_project(project)?;
        Ok(controller)
    }

    /// Model A (#716): install the per-machine I/O binding registry. Every
    /// chain's device endpoints resolve from this; the owner must set it
    /// before `sync_project`/`upsert_chain` so the resolved endpoints are
    /// non-empty.
    pub fn set_io_bindings(&mut self, io_bindings: Vec<domain::io_binding::IoBinding>) {
        self.io_bindings = io_bindings;
    }

    /// Issue #672 — read a chain's current live runtime (group 0), reflecting
    /// any runtime the control worker has published into the live slot.
    #[must_use]
    pub fn chain_runtime(&self, chain_id: &ChainId) -> Option<Arc<ChainRuntimeState>> {
        if let Some(slot) = self.chain_slots.get(&(chain_id.clone(), 0)) {
            return Some(slot.load());
        }
        self.runtime_graph
            .chains
            .get(&(chain_id.clone(), 0))
            .map(Arc::clone)
    }

    /// Issue #672 — enqueue an off-thread rebuild of a chain's runtime.
    ///
    /// The heavy build (NAM loads, route assembly) runs on the control worker;
    /// the caller (frontend thread) returns immediately. The finished build is
    /// applied later by [`ProjectRuntimeController::poll_pending_rebuilds`].
    /// Applies to chains that already have a runtime (the freeze case — editing
    /// a live chain whose IO is unchanged); a brand-new chain or an IO-topology
    /// change is still built synchronously by `upsert_chain`.
    pub fn schedule_chain_rebuild(
        &mut self,
        chain: &Chain,
        sample_rate: f32,
        device_sample_rates: std::collections::HashMap<domain::ids::DeviceId, f32>,
        buffer_sizes: Vec<usize>,
    ) {
        // Seed the slots from the current graph runtimes if they have not
        // been created yet (chain built before its slots were wired). Issue
        // #703: a chain owns one slot per input-entry group.
        let groups: Vec<usize> = self
            .runtime_graph
            .chains
            .keys()
            .filter(|(cid, _)| cid == &chain.id)
            .map(|(_, g)| *g)
            .collect();
        for group in groups {
            let key = (chain.id.clone(), group);
            if !self.chain_slots.contains_key(&key) {
                if let Some(runtime) = self.runtime_graph.chains.get(&key).map(Arc::clone) {
                    self.chain_slots.insert(key, LiveRuntimeSlot::new(runtime));
                }
            }
        }

        let request = BuildRequest {
            chain: chain.clone(),
            sample_rate,
            device_sample_rates,
            buffer_sizes,
            io_bindings: self.io_bindings.clone(),
        };
        let rx = self.worker.submit(move || build_chain_runtime(&request));
        self.pending_rebuilds.push((chain.id.clone(), rx));
    }

    /// Issue #672 — apply any finished off-thread rebuilds (call on the frontend
    /// tick). For each completed build, swap the live slot AND the
    /// `runtime_graph` entry in lock-step so the audio path and every other
    /// reader stay consistent, and drop the superseded runtime back on the
    /// worker (its NAM C++ destructors never run on the audio/frontend thread).
    ///
    /// Returns the number of rebuilds applied this tick.
    pub fn poll_pending_rebuilds(&mut self) -> usize {
        let mut applied = 0;
        let mut still_pending = Vec::new();
        for (chain_id, rx) in std::mem::take(&mut self.pending_rebuilds) {
            match rx.try_recv() {
                Ok(Ok(runtimes)) => {
                    // Issue #703: publish each per-entry runtime into ITS
                    // OWN (chain, group) slot. Publishing a single runtime
                    // into group 0 (the old shape) would leave sibling
                    // entries stale — or, on a shared device, double-process
                    // the buffer (the stream feeds every bound slot).
                    for (group, runtime) in runtimes {
                        let key = (chain_id.clone(), group);
                        if let Some(slot) = self.chain_slots.get(&key) {
                            // #740: carry the live meter/spectrum/tuner taps over
                            // to the rebuilt runtime BEFORE it goes live, or the
                            // graph freezes after a preset switch / live edit (the
                            // UI's tap rings were subscribed on the old runtime).
                            runtime.adopt_taps_from(&slot.load());
                            let graph_runtime = Arc::clone(&runtime);
                            let superseded = slot.publish(runtime);
                            self.runtime_graph.chains.insert(key, graph_runtime);
                            // Drop the old runtime off the audio/frontend thread.
                            let _ = self.worker.submit(move || drop(superseded));
                            applied += 1;
                        } else {
                            log::error!(
                                "chain '{}' rebuild produced entry group {} with no live \
                                 slot — the edit needs a stream rebuild to be heard",
                                chain_id.0,
                                group
                            );
                        }
                    }
                }
                Ok(Err(e)) => {
                    log::error!("chain '{}' off-thread rebuild failed: {e}", chain_id.0);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => still_pending.push((chain_id, rx)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    log::error!("chain '{}' rebuild worker disconnected", chain_id.0);
                }
            }
        }
        self.pending_rebuilds = still_pending;

        // Cold activations: once the runtime is built off-thread, create the
        // cpal streams on THIS (frontend) thread — cpal `Stream` is `!Send` — and
        // install the chain.
        let mut still_activating = Vec::new();
        for (chain_id, chain, rx) in std::mem::take(&mut self.pending_activations) {
            match rx.try_recv() {
                Ok(Ok((runtimes, resolved))) => {
                    // Issue #703: install every per-entry runtime — a
                    // single-device chain may own N isolated runtimes (one
                    // per input entry) all fed by the one device stream.
                    for (group, runtime) in &runtimes {
                        self.runtime_graph
                            .chains
                            .insert((chain_id.clone(), *group), Arc::clone(runtime));
                    }
                    let slots = crate::build_chain_slots(&runtimes);
                    for (group, slot) in &slots {
                        self.chain_slots
                            .insert((chain_id.clone(), *group), slot.handle());
                    }
                    // #669/#693: the worker resolved the real device rate —
                    // mirror it like the synchronous upsert path does, so DI
                    // loops resample to the live rate.
                    self.sample_rate = resolved.sample_rate as u32;
                    // #771: one DI playback cell per output stream, shared
                    // with arm_di_stream through the controller map.
                    let di_cells: Vec<_> = (0..resolved.outputs.len())
                        .map(|j| self.di_playback_cell(&chain_id, j))
                        .collect();
                    match crate::build_active_chain_runtime(
                        &chain_id,
                        &chain,
                        resolved,
                        slots,
                        &self.io_bindings,
                        &di_cells,
                    ) {
                        Ok(active) => {
                            self.active_chains.insert(chain_id, active);
                            // #771: an armed DI re-renders against the fresh
                            // streams (output index/rate/dest may have moved).
                            self.rearm_di_stream_after_rebuild(&chain);
                            applied += 1;
                        }
                        Err(e) => {
                            log::error!("chain '{}' stream build failed: {e}", chain_id.0);
                            self.runtime_graph.remove_chain(&chain_id);
                        }
                    }
                }
                Ok(Err(e)) => log::error!("chain '{}' activation build failed: {e}", chain_id.0),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    still_activating.push((chain_id, chain, rx))
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    log::error!("chain '{}' activation worker disconnected", chain_id.0)
                }
            }
        }
        self.pending_activations = still_activating;
        applied
    }

    /// Sample rate (Hz) the live streams are running at. The DI-loop loader
    /// resamples to this so loops play at the correct speed on any device rate
    /// (#669).
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
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
        // Unconfigured-device fallback. 64 frames continuously xruns on a
        // generic (non-RT) desktop kernel with USB audio — the stream
        // smears into a dull "muffled / in a bag" sound (NOT clicks),
        // which is exactly what users reported (#479). 256 is the safe
        // USB minimum. Configured setups carry an explicit value so this
        // is never hit (Orange Pi unaffected). THIS is the path that
        // builds the live JackConfig — device_settings.rs has a twin
        // fallback (kept in sync; both 256).
        let buffer_size = matched.map(|s| s.buffer_size_frames).unwrap_or(256);
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
            // Issue #522: pause instead of teardown. Keeps CPAL streams +
            // every block processor alive so re-enable resumes in O(1).
            self.pause_chain(&chain.id);
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

    /// Issue #672 — cold activation. If `chain` is not yet streaming, build its
    /// per-input runtimes (the heavy NAM/IR load) on the control worker and
    /// return `true`; the next poll creates the cpal streams on the frontend
    /// (they are `!Send`) and installs the chain. Returns `false` only for an
    /// already-streaming chain, which the caller then rebuilds synchronously.
    ///
    /// Issue #740: this used to defer ANY multi-device chain to the synchronous
    /// build (`input_devices.len() != 1`), so a rig bound to several interfaces
    /// (the owner's four-binding, two-interface boot) brought every stream up
    /// serially on the calling thread — the first streams ran their callback,
    /// counting underruns, while the remaining NAM/IR builds still blocked. The
    /// off-thread path already builds one runtime per input group
    /// (`build_per_input_runtime_states`) and installs one stream per device
    /// (`build_chain_streams`), so multi-device chains take it too: every
    /// runtime is built off-thread, then ALL streams are created and played
    /// together in one poll tick — no sibling starves another's bring-up.
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    pub fn schedule_chain_activation(&mut self, project: &Project, chain: &Chain) -> Result<bool> {
        if self.active_chains.contains_key(&chain.id) {
            return Ok(false); // already streaming — not a cold activation
        }

        // #693: validation + device resolution are CoreAudio property
        // queries costing hundreds of ms — they run on the control worker
        // together with the heavy build, never on the calling thread.
        let project_for_build = project.clone();
        let chain_for_build = chain.clone();
        let registry_for_build = self.io_bindings.clone();
        let rx = self.worker.submit(move || {
            let host = get_host();
            validate_chain_channels_against_devices(host, &chain_for_build, &registry_for_build)?;
            let resolved = resolve_chain_audio_config(
                host,
                &project_for_build,
                &chain_for_build,
                &registry_for_build,
            )?;
            let elastic_targets =
                compute_elastic_targets_for_chain(&chain_for_build, &resolved, &registry_for_build);
            let request = BuildRequest {
                chain: chain_for_build,
                sample_rate: resolved.sample_rate,
                device_sample_rates: resolved.by_device.clone(),
                buffer_sizes: elastic_targets,
                io_bindings: registry_for_build,
            };
            Ok((build_chain_runtime(&request)?, resolved))
        });
        self.pending_activations
            .push((chain.id.clone(), chain.clone(), rx));
        Ok(true)
    }

    /// JACK build keeps cold activation synchronous (issue #672 wires cpal first).
    #[cfg(all(target_os = "linux", feature = "jack"))]
    pub fn schedule_chain_activation(
        &mut self,
        _project: &Project,
        _chain: &Chain,
    ) -> Result<bool> {
        Ok(false)
    }

    pub fn remove_chain(&mut self, chain_id: &ChainId) {
        log::info!("removing chain '{}' from runtime", chain_id.0);
        if let Some(runtime) = self.runtime_graph.runtime_for_chain(chain_id) {
            runtime.set_draining();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        self.active_chains.remove(chain_id);
        self.runtime_graph.remove_chain(chain_id);
        // #771: never leak a parked render buffer past its chain.
        self.drop_di_state_for_chain(chain_id);
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
        // Issue #672: a chain whose runtime is still building off-thread (cold
        // activation) counts as running so the controller is not torn down
        // before poll_pending_rebuilds installs its streams.
        !self.active_chains.is_empty() || !self.pending_activations.is_empty()
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

    fn upsert_chain_with_resolved(
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
