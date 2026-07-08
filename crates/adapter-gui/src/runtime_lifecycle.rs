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
use std::rc::Rc;

use anyhow::Result;

use application::validate::validate_project;
use domain::ids::{BlockId, ChainId};
use infra_cpal::ProjectRuntimeController;
use project::block::{AudioBlock, AudioBlockKind};
use project::chain::Chain;

use crate::state::ProjectSession;

pub(crate) fn stop_project_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) {
    if let Some(mut runtime) = project_runtime.borrow_mut().take() {
        runtime.stop();
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
            validate_project(&*proj)?;
            runtime.sync_project(&*proj)?;
        }
    }
    // #669: keep the dispatcher's engine sample rate in lock-step with the
    // (possibly rebuilt) runtime so DI loops resample to the live device rate.
    crate::di_loop_wiring::sync_engine_sr_from_runtime(project_runtime, &session.dispatcher);
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
                &*proj,
                session.io_bindings.borrow().clone(),
            )?;
            *borrow = Some(controller);
            drop(borrow);
            // #669: start() resolved the real device rate — push it to the
            // dispatcher so DI loops resample correctly (not stuck at 48000).
            crate::di_loop_wiring::sync_engine_sr_from_runtime(
                project_runtime,
                &session.dispatcher,
            );
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
            validate_project(&*proj)?;
            // #743: plan the action BEFORE resolving anything. A disable must
            // pause immediately (drain → output silent) and must NOT run
            // `chain_io_changed` — that synchronous CoreAudio resolve costs
            // hundreds of ms per device, so on a four-device rig the GUI stalls
            // ~750 ms while the still-live output starves and emits stale frames
            // at full level (the owner's "microfonia"/underrun flood on toggle
            // off). The IO-change re-bind check belongs only to an enable.
            let action = plan_live_sync(chain.is_some(), chain_enabled, || {
                let chain = chain.expect("io_changed is only queried for a present, enabled chain");
                runtime.chain_io_changed(&*proj, chain)
            })?;
            match action {
                LiveSyncAction::Remove => runtime.remove_chain(chain_id),
                LiveSyncAction::Pause => {
                    // upsert_chain's !enabled path pauses (keeps streams alive,
                    // drains to silence) in O(1) — no device queries.
                    let chain = chain.expect("Pause implies the chain is present");
                    runtime.upsert_chain(&*proj, chain)?;
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
                    if !runtime.schedule_chain_activation(&*proj, chain)?
                        && !runtime.request_offthread_rebuild_if_live(&*proj, chain)?
                    {
                        runtime.upsert_chain(&*proj, chain)?;
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
    crate::di_loop_wiring::sync_engine_sr_from_runtime(project_runtime, &session.dispatcher);
    Ok(())
}

pub(crate) fn remove_live_chain_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    chain_id: &ChainId,
) {
    if let Some(runtime) = project_runtime.borrow_mut().as_mut() {
        runtime.remove_chain(chain_id);
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
