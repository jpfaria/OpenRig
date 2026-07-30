//! Lifecycle helpers for the project audio runtime and chain-block bookkeeping.
//!
//! Six small utilities pulled out of `lib.rs` so the main file stops growing
//! with every new chain-manipulation flow:
//!
//! * `stop_project_runtime` — drop the active `ProjectRuntimeController`.
//! * `sync_project_runtime` — rebuild the running graph from a session
//!   (validates first; failure leaves the runtime unchanged).
//! * `sync_live_chain_runtime` — incremental sync for one chain: starts the
//!   runtime if a chain is being enabled and none exists, otherwise upserts
//!   or removes that single chain. Tears down the runtime when no chain
//!   remains running. This is the hot path called from every block edit.
//! * `remove_live_chain_runtime` — drop one chain from the live graph.
//! * `assign_new_block_ids` — reassigns IDs across a chain's blocks
//!   (recursive into `Select` block options) when cloning a chain so two
//!   live chains never share a block id.
//! * `system_language` — best-effort BCP-47-ish locale tag from `LANG`
//!   (`pt_BR.UTF-8` → `pt-BR`), defaulting to `en-US` when the env is
//!   missing/POSIX/empty.
//! * `ui_index_to_real_block_index` — translate a UI-visible block position
//!   (which hides the first Input and last Output) into the real index in
//!   `chain.blocks`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use anyhow::Result;

use application::dispatcher::CommandDispatcher;
use application::runtime_control::RuntimeControl;
use application::validate::validate_project;
use domain::ids::{BlockId, ChainId};
use domain::io_binding::IoBinding;
use infra_cpal::ProjectRuntimeController;
use project::block::{AudioBlock, AudioBlockKind};
use project::chain::Chain;
use project::project::Project;
use project::rig::RigProject;

use crate::state::ProjectSession;

/// #127: the GUI's `RuntimeControl` — how a command handler reaches THIS
/// frontend's audio runtime. Holds the same `Rc` the whole app shares, so it
/// always addresses the current controller (or none, when the rig is stopped).
///
/// Lives here because `runtime_lifecycle` is the module that owns the
/// controller; every other wiring module dispatches a `Command` instead.
struct GuiRuntimeControl {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    session: SessionHandle,
}

/// The open session, as seen from inside a command handler.
///
/// `sync_live_chain_runtime` and `sync_block_toggle` need the whole
/// `ProjectSession`, but this control cannot hold the app's
/// `Rc<RefCell<Option<ProjectSession>>>`: every GUI callback holds that cell
/// `borrow_mut()` while it dispatches, so borrowing it from a handler would
/// panic. The session's own fields are cheap shared handles, so they are
/// mirrored instead — with the dispatcher held **weakly**, because the
/// dispatcher OWNS this control and an `Rc` back to it would be a reference
/// cycle that leaks the session (project data, DI loop PCM) on every project
/// switch.
struct SessionHandle {
    project: Rc<RefCell<Project>>,
    dispatcher: Weak<dyn CommandDispatcher>,
    project_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
    presets_path: PathBuf,
    rig: Option<Rc<RefCell<RigProject>>>,
    io_bindings: Rc<RefCell<Vec<IoBinding>>>,
}

impl SessionHandle {
    fn mirror(session: &ProjectSession) -> Self {
        Self {
            project: Rc::clone(&session.project),
            dispatcher: Rc::downgrade(&session.dispatcher),
            project_path: session.project_path.clone(),
            config_path: session.config_path.clone(),
            presets_path: session.presets_path.clone(),
            rig: session.rig.clone(),
            io_bindings: Rc::clone(&session.io_bindings),
        }
    }

    /// Rebuild the session the sync helpers take. `None` once the project has
    /// been closed — and then there is no runtime left to sync either.
    fn session(&self) -> Option<ProjectSession> {
        Some(ProjectSession {
            project: Rc::clone(&self.project),
            dispatcher: self.dispatcher.upgrade()?,
            project_path: self.project_path.clone(),
            config_path: self.config_path.clone(),
            presets_path: self.presets_path.clone(),
            rig: self.rig.clone(),
            io_bindings: Rc::clone(&self.io_bindings),
        })
    }
}

impl RuntimeControl for GuiRuntimeControl {
    fn set_output_muted(&self, muted: bool) {
        if let Some(runtime) = self.runtime.borrow().as_ref() {
            runtime.set_output_muted(muted);
        }
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
        sync_block_toggle(&self.runtime, &session, chain, block, enabled)
    }

    fn sync_chain(&self, chain: &ChainId) -> Result<()> {
        let Some(session) = self.session.session() else {
            return Ok(());
        };
        sync_live_chain_runtime(&self.runtime, &session, chain)
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
    session: &ProjectSession,
) {
    session
        .dispatcher
        .attach_runtime_control(Rc::new(GuiRuntimeControl {
            runtime: project_runtime.clone(),
            session: SessionHandle::mirror(session),
        }));
}

/// Drop the active controller, and tell the session's dispatcher the streams
/// are gone.
///
/// #127: the engine rate is part of the teardown. It was only ever pushed
/// forward, so a stopped 44.1 kHz rig left the dispatcher reporting 44 100 and
/// every consumer of the live rate — the block editor's EQ curve first among
/// them — kept drawing against a device that was no longer open. Resetting it
/// here means "nothing running" reads as the reference rate again, which is
/// what it is (issue #723's sanctioned no-device value, never a live-path
/// assumption).
pub(crate) fn stop_project_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) {
    if let Some(mut runtime) = project_runtime.borrow_mut().take() {
        runtime.stop();
    }
    if let Some(session) = project_session.borrow().as_ref() {
        crate::di_loop_wiring::sync_engine_sr_from_runtime(
            project_runtime,
            session.dispatcher.as_ref(),
        );
    }
}

pub(crate) fn sync_project_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
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
    crate::di_loop_wiring::sync_engine_sr_from_runtime(
        project_runtime,
        session.dispatcher.as_ref(),
    );
    attach_runtime_control(project_runtime, session);
    Ok(())
}

/// #743: the planned action for a one-chain live sync. Modelled as data so the
/// decision — crucially, WHETHER a device-IO resolve runs — is unit-testable
/// without audio hardware.
pub enum LiveSyncAction {
    /// The chain is gone from the project: drop it from the live graph.
    Remove,
    /// The chain is present but disabled: pause it (drain → silence) in O(1).
    /// No device-IO resolve — that synchronous CoreAudio query (hundreds of ms
    /// per device) would stall the GUI while the live output starves into a
    /// feedback howl (#743). A disable never re-binds, so the check is moot.
    Pause,
    /// The chain is present and enabled: (re)activate it. `io_changed` is the
    /// re-bind check — only an enable consults it.
    Enable { io_changed: bool },
}

/// Decide the live-sync action for a toggled chain. The `io_changed` closure
/// (the device-IO resolve) is invoked ONLY for an enable; a disable or a
/// removal must never touch it — that resolve is the ~750 ms CoreAudio stall
/// that starves the live output into feedback on a four-device toggle (#743).
pub fn plan_live_sync(
    chain_present: bool,
    chain_enabled: bool,
    io_changed: impl FnOnce() -> Result<bool>,
) -> Result<LiveSyncAction> {
    if !chain_present {
        return Ok(LiveSyncAction::Remove);
    }
    if !chain_enabled {
        return Ok(LiveSyncAction::Pause);
    }
    Ok(LiveSyncAction::Enable {
        io_changed: io_changed()?,
    })
}

pub(crate) fn sync_live_chain_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
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
            crate::di_loop_wiring::sync_engine_sr_from_runtime(
                project_runtime,
                session.dispatcher.as_ref(),
            );
            attach_runtime_control(project_runtime, session);
            // #323: the runtimes were just born empty — give them back the
            // loopers the project carries, with whatever audio they saved.
            restore_project_loops(project_runtime, session);
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
    crate::di_loop_wiring::sync_engine_sr_from_runtime(
        project_runtime,
        session.dispatcher.as_ref(),
    );
    attach_runtime_control(project_runtime, session);
    Ok(())
}

/// #808: ensure a runtime controller exists so the DI can play WITHOUT any
/// chain being enabled. The DI is an independent pipeline (invariant #4) — it
/// must not depend on a guitar stream even existing. `sync_live_chain_runtime`
/// only lazily creates the controller when a chain is being ENABLED, so a user
/// who opens a project and hits ▶ on the DI (no chain active) had no controller
/// at all — the play was a silent no-op until a chain toggle created one. This
/// mirrors that lazy creation but is NOT gated on `enabled`. No-op when a
/// controller already exists.
pub(crate) fn ensure_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    session: &ProjectSession,
) -> Result<()> {
    {
        let borrow = project_runtime.borrow();
        if borrow.is_some() {
            return Ok(());
        }
    }
    // #716 (AUDIO-CRITICAL): hand the I/O binding registry to the controller
    // BEFORE start() runs its initial sync (same reason as the enable path).
    let controller = ProjectRuntimeController::start_with_io_bindings(
        &session.project.borrow(),
        session.io_bindings.borrow().clone(),
    )?;
    *project_runtime.borrow_mut() = Some(controller);
    // #669: keep the dispatcher's engine rate in lock-step with the real device
    // rate start() resolved, so a DI resamples correctly.
    crate::di_loop_wiring::sync_engine_sr_from_runtime(
        project_runtime,
        session.dispatcher.as_ref(),
    );
    attach_runtime_control(project_runtime, session);
    // #323: same as the enable path — the fresh runtimes get the project's
    // loopers back.
    restore_project_loops(project_runtime, session);
    Ok(())
}

/// #323: claim a slot for every looper the project carries and reload the
/// audio each one saved. Called right after a controller is created — the
/// runtimes are empty until it runs.
fn restore_project_loops(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    session: &ProjectSession,
) {
    let Some(project_path) = session.project_path.clone() else {
        return;
    };
    crate::looper_persist::restore_chain_loops(session, project_runtime, &project_path);
}

/// Drop one chain from the live graph — the chain-delete teardown.
///
/// #127: this is the THIRD way a rig stops, and the one that used to leak a
/// dead rate. Deleting the last chain removed its streams but left the
/// controller alive, so nothing ever re-synced and every reader of the live
/// rate — the block editor's EQ curve first — kept the rate of a device that
/// was no longer open. It now mirrors `sync_live_chain_runtime`'s own
/// teardown: with nothing left running (no chain, no pending activation, no
/// armed DI — `is_running` counts all three, so a DI playing without any chain
/// keeps its controller, #808) the controller goes, and the engine rate is
/// re-synced either way. Two chains in, one deleted, the survivor's rate is
/// re-pushed unchanged.
pub(crate) fn remove_live_chain_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    chain_id: &ChainId,
) {
    {
        let mut borrow = project_runtime.borrow_mut();
        if let Some(runtime) = borrow.as_mut() {
            runtime.remove_chain(chain_id);
            if !runtime.is_running() {
                *borrow = None;
            }
        }
    }
    if let Some(session) = project_session.borrow().as_ref() {
        crate::di_loop_wiring::sync_engine_sr_from_runtime(
            project_runtime,
            session.dispatcher.as_ref(),
        );
    }
}

/// Issue #522: fast path for `Command::ToggleBlockEnabled`. Flips the
/// block's fade state in place on the live chain runtime — no CPAL
/// re-resolve, no chain rebuild. Falls back to `sync_live_chain_runtime`
/// only when the fast path can't take the change (chain not yet running,
/// or the block is a `Bypass` that needs a real processor rebuild).
pub(crate) fn sync_block_toggle(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
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
    sync_live_chain_runtime(project_runtime, session, chain_id)
}

pub(crate) fn assign_new_block_ids(chain: &mut Chain) {
    for block in &mut chain.blocks {
        assign_new_block_ids_recursive(block, &chain.id);
    }
}

fn assign_new_block_ids_recursive(block: &mut AudioBlock, chain_id: &ChainId) {
    block.id = BlockId::generate_for_chain(chain_id);
    if let AudioBlockKind::Select(select) = &mut block.kind {
        for option in &mut select.options {
            assign_new_block_ids_recursive(option, chain_id);
        }
    }
}

pub(crate) fn system_language() -> String {
    let lang = std::env::var("LANG").unwrap_or_default();
    let base = lang.split('.').next().unwrap_or("");
    // "C", "POSIX", empty, or too short = not a real locale → fall back to English
    if base.is_empty() || base.len() < 2 || matches!(base, "C" | "POSIX") {
        return "en-US".to_string();
    }
    base.replace('_', "-")
}

/// Map a UI block index (which excludes hidden first Input and last Output) to the real chain.blocks index.
pub(crate) fn ui_index_to_real_block_index(chain: &Chain, ui_index: usize) -> usize {
    let first_input_idx = chain
        .blocks
        .iter()
        .position(|b| matches!(&b.kind, AudioBlockKind::Input(_)));
    let last_output_idx = chain
        .blocks
        .iter()
        .rposition(|b| matches!(&b.kind, AudioBlockKind::Output(_)));
    let mut visible_count = 0;
    for (real_idx, _) in chain.blocks.iter().enumerate() {
        if Some(real_idx) == first_input_idx || Some(real_idx) == last_output_idx {
            continue; // hidden
        }
        if visible_count == ui_index {
            return real_idx;
        }
        visible_count += 1;
    }
    // If ui_index is past all visible blocks, return end (before last output)
    last_output_idx.unwrap_or(chain.blocks.len())
}

#[cfg(test)]
#[path = "runtime_lifecycle_di_808_tests.rs"]
mod di_808_tests;

#[cfg(test)]
#[path = "runtime_lifecycle_control_tests.rs"]
mod control_tests;
