//! Lifecycle of the project audio runtime — the ONE module that creates,
//! syncs and drops the `ProjectRuntimeController`, and the home of
//! `GuiRuntimeControl`, the [`RuntimeControl`] every command handler reaches
//! this frontend's audio through.
//!
//! * `stop_project_runtime` — drop the active `ProjectRuntimeController`.
//! * `sync_project_runtime` — rebuild the running graph from a session
//!   (validates first; failure leaves the runtime unchanged).
//! * `sync_live_chain_runtime` — incremental sync for one chain: starts the
//!   runtime if a chain is being enabled and none exists, otherwise upserts
//!   or removes that single chain. Tears down the runtime when no chain
//!   remains running. This is the hot path called from every block edit.
//! * `remove_live_chain_runtime` — drop one chain from the live graph.
//! * `sync_engine_sr_from_runtime` — publish the live device rate to the
//!   dispatcher (and the reference rate once nothing is running).
//!
//! The doors for the INDEPENDENT pipelines (the DI loop, the metronome click)
//! keep their bodies in `runtime_pipelines` — including `ensure_runtime`, the
//! #808 lazy creation their starts are the only doors allowed to run — the
//! looper's in `runtime_loopers`, and the analyzers' (tuner, spectrum) in
//! `runtime_analyzers`: same seam, different rules. The session mirror the
//! doors read the project through is `runtime_session_handle`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::Result;

use application::command::{LooperAction, LooperParam};
use application::dispatcher::CommandDispatcher;
use application::runtime_control::RuntimeControl;
use application::validate::validate_project;
use domain::ids::{BlockId, ChainId};
use domain::io_binding::IoBinding;
use engine::metronome_state::MetronomeSettings;
use engine::{DiPcm, LoopPcm};
use infra_cpal::ProjectRuntimeController;
use project::chain::{Chain, EndpointRef};

use crate::live_sync_plan::{plan_live_sync, LiveSyncAction};
use crate::runtime_analyzers::AnalyzerSessions;
use crate::runtime_session_handle::SessionHandle;
use crate::state::ProjectSession;
use crate::{runtime_devices, runtime_loopers, runtime_pipelines, runtime_teardown};

/// #127: the GUI's `RuntimeControl` — how a command handler reaches THIS
/// frontend's audio runtime. Holds the same `Rc` the whole app shares, so it
/// always addresses the current controller (or none, when the rig is stopped).
///
/// Lives here because `runtime_lifecycle` is the module that owns the
/// controller; every other wiring module dispatches a `Command` instead.
struct GuiRuntimeControl {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    /// The tuner / spectrum sessions (#544/#546). Not part of the audio
    /// runtime: an analyzer is a READER of it, which is why powering one on
    /// never creates a controller.
    analyzers: AnalyzerSessions,
    session: SessionHandle,
}

impl RuntimeControl for GuiRuntimeControl {
    fn set_output_muted(&self, muted: bool) {
        if let Some(runtime) = self.runtime.borrow().as_ref() {
            runtime.set_output_muted(muted);
        }
    }

    /// The machine's devices, not the graph — body in `runtime_devices`. The
    /// same handler calls `sync_project` right after, in that order.
    fn apply_device_settings(&self, settings: &[project::device::DeviceSettings]) {
        runtime_devices::apply_device_settings(settings);
    }

    fn set_io_bindings(&self, bindings: Vec<IoBinding>) {
        if let Some(runtime) = self.runtime.borrow_mut().as_mut() {
            runtime.set_io_bindings(bindings);
        }
    }

    /// #522 stays #522: the in-place fade toggle, with the same fallback the
    /// GUI callback used to run itself. No stream restart, no added latency.
    fn set_block_enabled(&self, chain: &ChainId, block: &BlockId, enabled: bool) -> Result<()> {
        let Some(session) = self.session.session() else {
            return Ok(());
        };
        sync_block_toggle(
            &self.runtime,
            &self.analyzers,
            &session,
            chain,
            block,
            enabled,
        )
    }

    fn sync_chain(&self, chain: &ChainId) -> Result<()> {
        let Some(session) = self.session.session() else {
            return Ok(());
        };
        sync_live_chain_runtime(&self.runtime, &self.analyzers, &session, chain)
    }

    // The teardowns (`runtime_teardown`): the whole rig, and one chain. A
    // closed project leaves no dispatcher to re-publish the rate to, but the
    // streams still go — which is why both take the session as an `Option`.

    fn stop_project_runtime(&self) {
        runtime_teardown::stop_project_runtime(&self.runtime, self.session.session().as_ref());
    }

    fn remove_chain(&self, chain: &ChainId) {
        runtime_teardown::remove_live_chain_runtime(
            &self.runtime,
            self.session.session().as_ref(),
            chain,
        );
    }

    /// The device-settings rebuild: every chain the PROJECT names, re-opened
    /// against the rate/buffer that just changed. Never selects runtimes by
    /// rate (`CLAUDE.md` LAW) — `sync_project` walks the project's own chain
    /// list — and never creates a controller, so a save on a stopped rig
    /// leaves it stopped.
    fn sync_project(&self) -> Result<()> {
        let Some(session) = self.session.session() else {
            return Ok(());
        };
        sync_project_runtime(&self.runtime, &self.analyzers, &session)
    }

    // The independent pipelines (invariant #4) — the DI loop and the
    // metronome click — have their bodies in `runtime_pipelines`, where the
    // rules they share are written down: a start may wake audio, a follow
    // never may, a stop is never an error.

    fn arm_di_stream(&self, chain: &Chain, pcm: Arc<DiPcm>) -> Result<()> {
        let Some(session) = self.session.session() else {
            return Ok(());
        };
        runtime_pipelines::arm_di(&self.runtime, &self.analyzers, &session, chain, pcm)
    }

    fn disarm_di_stream(&self, chain: &ChainId) {
        runtime_pipelines::disarm_di(&self.runtime, chain);
    }

    fn refresh_di_stream(&self, chain: &Chain, pcm: Arc<DiPcm>) -> Result<()> {
        runtime_pipelines::refresh_di(&self.runtime, chain, pcm)
    }

    fn start_metronome(&self, settings: MetronomeSettings, output_key: Option<&str>) -> Result<()> {
        let Some(session) = self.session.session() else {
            return Ok(());
        };
        runtime_pipelines::start_metronome(
            &self.runtime,
            &self.analyzers,
            &session,
            settings,
            output_key,
        )
    }

    fn stop_metronome(&self) {
        runtime_pipelines::stop_metronome(&self.runtime);
    }

    fn set_metronome_settings(&self, settings: MetronomeSettings) {
        runtime_pipelines::push_metronome_settings(&self.runtime, settings);
    }

    fn refresh_metronome_output(&self, output_key: Option<&str>) -> Result<()> {
        let Some(session) = self.session.session() else {
            return Ok(());
        };
        runtime_pipelines::refresh_metronome_output(&self.runtime, &session, output_key)
    }

    // The analyzers (#544/#546). Powering one on is what makes it SUBSCRIBE
    // and start reading; the bodies are in `runtime_analyzers`. Neither may
    // wake audio: an analyzer reads what is already sounding.

    fn set_tuner_running(&self, running: bool) {
        self.analyzers
            .set_tuner_running(self.session.session().as_ref(), running);
    }

    fn set_spectrum_running(&self, running: bool) {
        self.analyzers
            .set_spectrum_running(self.session.session().as_ref(), running);
    }

    // The looper doors' bodies live in `runtime_loopers`, where the rules they
    // share are written down: one chain each, the playback reconcile belongs
    // to the door, and only an add or a transport START may wake audio (#808).

    /// #808: the store slot is claimed on a LIVE runtime — the panel arms REC
    /// only against one, so a looper added with nothing running would be a
    /// looper the user cannot record into.
    fn create_looper(&self, chain: &Chain, looper: u64) -> Result<()> {
        if let Some(session) = self.session.session() {
            runtime_pipelines::ensure_runtime(&self.runtime, &self.analyzers, &session)?;
        }
        runtime_loopers::create(&self.runtime, chain, looper);
        Ok(())
    }

    fn remove_looper(&self, chain: &Chain, looper: u64) {
        runtime_loopers::remove(&self.runtime, chain, looper);
    }

    /// #808: a Record / Play / PlayStop is a request to hear something and may
    /// bring the runtime up; a Stop / Clear / Undo / Redo never may.
    fn looper_transport(&self, chain: &Chain, looper: u64, action: LooperAction) -> Result<()> {
        if runtime_loopers::transport_may_start_audio(action) {
            if let Some(session) = self.session.session() {
                runtime_pipelines::ensure_runtime(&self.runtime, &self.analyzers, &session)?;
            }
        }
        runtime_loopers::transport(&self.runtime, chain, looper, action);
        Ok(())
    }

    fn set_looper_param(&self, chain: &Chain, looper: u64, param: LooperParam) {
        runtime_loopers::set_param(&self.runtime, chain, looper, param);
    }

    fn set_looper_input(&self, chain: &Chain, looper: u64, input: Option<EndpointRef>) {
        runtime_loopers::set_input(&self.runtime, chain, looper, input);
    }

    fn set_looper_output(&self, chain: &Chain, looper: u64, output: Option<EndpointRef>) {
        runtime_loopers::set_output(&self.runtime, chain, looper, output);
    }

    fn export_chain_loops(&self, chain: &Chain) -> Option<Vec<(u64, Arc<LoopPcm>)>> {
        runtime_loopers::export_chain_loops(&self.runtime, chain)
    }
}

/// #127: the ONE capability a project-open path needs from the audio runtime —
/// "give this freshly built session's dispatcher the frontend's audio runtime".
///
/// It exists so the modules that open, create and switch projects do not have
/// to hold `Rc<RefCell<Option<ProjectRuntimeController>>>` just to perform an
/// installation step. A module holding one of these cannot start, stop, sync or
/// read the audio: the handle is private and the only thing it can do with it
/// is hand it to a dispatcher. That is the difference the guard in
/// `no_infra_cpal_in_wiring_tests` is about — reaching the engine directly
/// versus wiring the seam up.
#[derive(Clone)]
pub(crate) struct RuntimeAttach {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    analyzers: AnalyzerSessions,
}

impl RuntimeAttach {
    pub(crate) fn new(
        project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
        analyzers: &AnalyzerSessions,
    ) -> Self {
        Self {
            runtime: project_runtime.clone(),
            analyzers: analyzers.clone(),
        }
    }

    /// Hand `session`'s dispatcher this frontend's runtime control. Idempotent
    /// and cheap — it clones a handful of `Rc`s.
    pub(crate) fn to_session(&self, session: &ProjectSession) {
        attach_runtime_control(&self.runtime, &self.analyzers, session);
    }
}

/// #127: give this session's dispatcher a handle on the audio runtime, so
/// runtime-control commands apply their effect from the dispatcher instead of
/// from a UI callback (which left MCP/MIDI dispatching into the void).
///
/// Called wherever the runtime is created or re-synced, and wherever the
/// session's paths change (Save As): the dispatcher belongs to the open
/// session, so a newly opened project re-attaches on its first sync.
/// Idempotent and cheap — it clones a handful of `Rc`s.
pub(crate) fn attach_runtime_control(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    analyzers: &AnalyzerSessions,
    session: &ProjectSession,
) {
    session
        .dispatcher
        .attach_runtime_control(Rc::new(GuiRuntimeControl {
            runtime: project_runtime.clone(),
            analyzers: analyzers.clone(),
            session: SessionHandle::mirror(session),
        }));
}

pub(crate) fn sync_project_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    analyzers: &AnalyzerSessions,
    session: &ProjectSession,
) -> Result<()> {
    let proj = session.project.borrow();
    {
        let mut borrow = project_runtime.borrow_mut();
        if let Some(runtime) = borrow.as_mut() {
            validate_project(&proj)?;
            runtime.sync_project(&proj)?;
        }
    }
    // #669: keep the dispatcher's engine sample rate in lock-step with the
    // (possibly rebuilt) runtime so DI loops resample to the live device rate.
    sync_engine_sr_from_runtime(project_runtime, session.dispatcher.as_ref());
    attach_runtime_control(project_runtime, analyzers, session);
    Ok(())
}

pub(crate) fn sync_live_chain_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    analyzers: &AnalyzerSessions,
    session: &ProjectSession,
    chain_id: &ChainId,
) -> Result<()> {
    log::debug!("sync_live_chain_runtime: chain_id='{}'", chain_id.0);
    let proj = session.project.borrow();
    let chain = proj.chains.iter().find(|c| &c.id == chain_id);
    let chain_enabled = chain.map(|c| c.enabled).unwrap_or(false);
    // If chain is being enabled and no runtime exists, create one
    if chain_enabled {
        let mut borrow = project_runtime.borrow_mut();
        if borrow.is_none() {
            // #716 (AUDIO-CRITICAL): hand the per-machine I/O binding registry
            // to the controller BEFORE `start()` runs its initial sync — the
            // cold-start activation snapshots the registry into its worker job,
            // so installing it AFTER start is too late and the binding-bound
            // chain bails "no input blocks". Sourced from the session's mirror
            // of `AppConfig.io_bindings`.
            let controller = ProjectRuntimeController::start_with_io_bindings(
                &proj,
                session.io_bindings.borrow().clone(),
            )?;
            *borrow = Some(controller);
            drop(borrow);
            // #669: start() resolved the real device rate — push it to the
            // dispatcher so DI loops resample correctly (not stuck at 48000).
            sync_engine_sr_from_runtime(project_runtime, session.dispatcher.as_ref());
            attach_runtime_control(project_runtime, analyzers, session);
            // #323: the runtimes were just born empty — give them back the
            // loopers the project carries, with whatever audio they saved.
            runtime_loopers::restore_project_loops(project_runtime, session);
            return Ok(()); // start() already processes all enabled chains via sync_project
        }
        drop(borrow);
    }
    // Normal sync
    {
        let mut borrow = project_runtime.borrow_mut();
        if let Some(runtime) = borrow.as_mut() {
            // #716 (AUDIO-CRITICAL): a controller created earlier (before the
            // user added/related an I/O binding) holds a STALE registry, so a
            // newly-bound chain resolves to zero inputs ("chain '...' has no
            // input blocks configured"). Refresh the controller's registry from
            // the session's live mirror of `AppConfig.io_bindings` on EVERY
            // sync, not just at start, so a just-created binding takes effect.
            runtime.set_io_bindings(session.io_bindings.borrow().clone());
            validate_project(&proj)?;
            // #743: plan the action BEFORE resolving anything. A disable must
            // pause immediately (drain → output silent) and must NOT run
            // `chain_io_changed` — that synchronous CoreAudio resolve costs
            // hundreds of ms per device, so on a four-device rig the GUI stalls
            // ~750 ms while the still-live output starves and emits stale frames
            // at full level (the owner's "microfonia"/underrun flood on toggle
            // off). The IO-change re-bind check belongs only to an enable.
            let action = plan_live_sync(chain.is_some(), chain_enabled, || {
                let chain = chain.expect("io_changed is only queried for a present, enabled chain");
                runtime.chain_io_changed(&proj, chain)
            })?;
            match action {
                LiveSyncAction::Remove => runtime.remove_chain(chain_id),
                LiveSyncAction::Pause => {
                    // upsert_chain's !enabled path pauses (keeps streams alive,
                    // drains to silence) in O(1) — no device queries.
                    let chain = chain.expect("Pause implies the chain is present");
                    runtime.upsert_chain(&proj, chain)?;
                }
                LiveSyncAction::Enable { io_changed } => {
                    // Issue #672/#693: a cold activation builds the runtime off the
                    // control worker and installs it on the poll tick.
                    // #716: a re-bind changes stream topology, so REBUILD (drop the
                    // streams) when the resolved I/O differs from what's live.
                    // #740: a LIVE edit (preset switch, block toggle, param change)
                    // on an ALREADY-RUNNING chain must NOT go through the
                    // synchronous `upsert_chain` — that resolves the devices AND
                    // reloads the NAM/IR models on the GUI thread (measured ~5.7 s
                    // on the owner's two-interface rig, the freeze on every edit).
                    // With unchanged I/O it reuses the live stream config and
                    // rebuilds the DSP off-thread; the GUI returns immediately.
                    let chain = chain.expect("Enable implies the chain is present");
                    if io_changed {
                        runtime.remove_chain(&chain.id);
                    }
                    if !runtime.schedule_chain_activation(&proj, chain)?
                        && !runtime.request_offthread_rebuild_if_live(&proj, chain)?
                    {
                        runtime.upsert_chain(&proj, chain)?;
                    }
                }
            }
            // If no chains are running (and none are activating), destroy runtime.
            if !runtime.is_running() {
                *borrow = None;
            }
        }
    }
    // #669: an upsert may have rebuilt the stream at a new device rate; keep
    // the dispatcher's engine sample rate in lock-step.
    sync_engine_sr_from_runtime(project_runtime, session.dispatcher.as_ref());
    attach_runtime_control(project_runtime, analyzers, session);
    Ok(())
}

/// Issue #522: fast path for `Command::ToggleBlockEnabled`. Flips the
/// block's fade state in place on the live chain runtime — no CPAL
/// re-resolve, no chain rebuild. Falls back to `sync_live_chain_runtime`
/// only when the fast path can't take the change (chain not yet running,
/// or the block is a `Bypass` that needs a real processor rebuild).
pub(crate) fn sync_block_toggle(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    analyzers: &AnalyzerSessions,
    session: &ProjectSession,
    chain_id: &ChainId,
    block_id: &BlockId,
    enabled: bool,
) -> Result<()> {
    let fast_path = {
        let borrow = project_runtime.borrow();
        match borrow.as_ref() {
            // #522 fast toggle + re-render the monitored DI (issue #717/#771): the
            // fast path only flips the guitar runtime, so a block disabled while
            // monitoring the DI would keep sounding without the re-arm.
            Some(runtime) => {
                let project = session.project.borrow();
                match project.chains.iter().find(|c| &c.id == chain_id) {
                    Some(chain) => runtime.toggle_block_enabled_live(chain, block_id, enabled),
                    None => runtime.set_block_enabled(chain_id, block_id, enabled),
                }
            }
            None => Err(anyhow::anyhow!("runtime not started")),
        }
    };
    if fast_path.is_ok() {
        return Ok(());
    }
    log::debug!(
        "sync_block_toggle: fast path declined ({:?}) — falling back to upsert",
        fast_path.err()
    );
    sync_live_chain_runtime(project_runtime, analyzers, session, chain_id)
}

/// #669/#749: publish the running controller's real device sample rate to the
/// dispatcher's `engine_sr` — the authoritative-rate fallback for consumers
/// that would otherwise assume 48000.
///
/// Called wherever the controller is created, re-synced (a sample-rate change
/// rebuilds the runtime) or torn down.
///
/// #127: with no controller the rate goes back to
/// `local_dispatcher::REFERENCE_SAMPLE_RATE`. It was previously left at
/// whatever the last stream negotiated, so after a 44.1 kHz rig stopped the
/// dispatcher still reported 44 100 — a rate nothing was running at, which the
/// block editor then drew its EQ curve against. "Nothing running" is not the
/// last device; it is no device.
///
/// The DI loops that were PLAYING at the old rate are re-armed by the
/// dispatcher itself, through `RuntimeControl::refresh_di_stream` — otherwise
/// a loop that was sounding when the device rate changed drags in slow motion
/// against its rebuilt runtime (#669).
pub(crate) fn sync_engine_sr_from_runtime(
    project_runtime: &RefCell<Option<ProjectRuntimeController>>,
    dispatcher: &dyn CommandDispatcher,
) {
    let rate = match project_runtime.borrow().as_ref() {
        Some(runtime) => runtime.sample_rate(),
        None => application::local_dispatcher::REFERENCE_SAMPLE_RATE,
    };
    dispatcher.attach_engine_sr(rate);
}

#[cfg(test)]
#[path = "runtime_lifecycle_di_808_tests.rs"]
mod di_808_tests;

#[cfg(test)]
#[path = "runtime_lifecycle_control_tests.rs"]
mod control_tests;

#[cfg(test)]
#[path = "metronome_runtime_tests.rs"]
mod metronome_tests;
