//! Responsibility: owns the live runtime state of one project.
//!
//! `ProjectRuntimeController` is the long-lived owner the application layer
//! (`adapter-gui`, `vst3`, ...) drives. It holds:
//!
//! - `runtime_graph: RuntimeGraph` — the engine-side per-chain
//!   `Arc<ChainRuntimeState>`s. Each chain's audio thread reads from here.
//! - `active_chains: HashMap<ChainId, ActiveChainRuntime>` — the cpal
//!   `Stream`s plus, on Linux+JACK, the live JACK `AsyncClient` and DSP
//!   worker handle.
//! - `supervisor` (Linux+JACK only) — `JackSupervisor<LiveJackBackend>`,
//!   the single owner of every `jackd` process.
//!
//! What the controller DOES with that state lives in a sibling file per job:
//! `controller_sync` (project sync), `controller_upsert` (one chain),
//! `controller_rebuild_queue` (off-thread rebuilds), `controller_health`
//! (backend loss), `controller_jack_servers` (jackd config), plus the
//! pre-existing taps / loopers / activation splits.

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
use crate::resolved::ResolvedChainAudioConfig;
use crate::{ControlWorker, LiveRuntimeSlot};

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
    /// #323: keyed by `(chain, source)` — the DI loop and each looper are
    /// independent isolated streams sharing one lifecycle (`IsolatedSource`).
    pub(crate) di_streams:
        RefCell<HashMap<crate::di_stream::IsolatedKey, crate::di_stream::DiStreamHandle>>,
    /// Issue #771: one playback cell per (chain, source, flat output index). The
    /// output stream's callback clones its cell at build time and mixes
    /// whatever playback is parked there (wait-free load); arming parks the
    /// pre-rendered loop on the CHOSEN output's cell only. Entries are
    /// created on demand and survive stream rebuilds.
    pub(crate) di_playback_cells: RefCell<
        HashMap<(crate::di_stream::IsolatedKey, usize), crate::di_playback::DiPlaybackCell>,
    >,
    /// Issue #771/#785: playbacks swapped out by a disarm or a gapless hand-off,
    /// freed on a LATER cycle so the audio callback is never the last owner of a
    /// multi-MB render buffer (invariant #8).
    pub(crate) di_retired: crate::di_playback::DiRetired,
    /// #323: the loop content `(len_frames, content_rev, playback_rev)` each
    /// looper's isolated stream was last armed with, so `sync_looper_streams`
    /// re-arms only when the recording OR the linked-preset blocks actually
    /// changed — not every meter tick.
    pub(crate) looper_armed: RefCell<HashMap<(ChainId, u64), (u64, u64, u64)>>,
    /// #323: controller-owned looper state — the recorded material and transport
    /// of every loop live HERE (reusing `LooperSlot`), off the volatile chain
    /// runtime. Recording drains an input tap into it off the audio thread;
    /// stop/clear/undo mutate it directly, so control is deterministic and a
    /// loop survives a chain rebuild/toggle. Replaces the bank-in-runtime and
    /// the suppression band-aid it forced.
    pub(crate) looper_store: RefCell<crate::looper_store::LooperStore>,
    /// Issue #14: the metronome's OWN output stream, alive only while the
    /// metronome is on. Never shares a chain stream — the backend sums it on
    /// the device (invariant #4), so a chain rebuild cannot chop the click and
    /// the click can never reach the guitar's buffers.
    pub(crate) metronome_stream: RefCell<Option<crate::metronome_stream::MetronomeStreamHandle>>,
    /// Issue #14: lock-free settings/position shared with that stream's
    /// callback. Outlives the stream so settings survive a stop/start.
    pub(crate) metronome_shared: engine::metronome_state::MetronomeCell,
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
    /// that `set_chain_di_loop` / `chain_has_di_loop` work without cpal I/O).
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
            di_retired: Default::default(),
            looper_armed: RefCell::new(HashMap::new()),
            looper_store: RefCell::new(crate::looper_store::LooperStore::default()),
            metronome_stream: RefCell::new(None),
            metronome_shared: std::sync::Arc::new(engine::metronome_state::MetronomeShared::new(
                Default::default(),
            )),
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
            di_retired: Default::default(),
            looper_armed: RefCell::new(HashMap::new()),
            looper_store: RefCell::new(crate::looper_store::LooperStore::default()),
            metronome_stream: RefCell::new(None),
            metronome_shared: std::sync::Arc::new(engine::metronome_state::MetronomeShared::new(
                Default::default(),
            )),
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

    /// Sample rate (Hz) the live streams are running at. The DI-loop loader
    /// resamples to this so loops play at the correct speed on any device rate
    /// (#669).
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    // `schedule_chain_activation` (issue #672 cold activation + #808 DI re-arm)
    // lives in `controller_chain_activation.rs` (line-cap split).

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
        // #323: drop the looper stream bookkeeping too.
        self.forget_chain_looper_streams(chain_id);
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

    // `is_running` — the one question every teardown door asks — lives in
    // `controller_liveness.rs` (line-cap split).

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
