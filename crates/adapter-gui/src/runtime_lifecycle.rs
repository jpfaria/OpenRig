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
            let mut controller = ProjectRuntimeController::start(&*proj)?;
            // #716 (AUDIO-CRITICAL): hand the per-machine I/O binding registry
            // to the freshly started controller BEFORE its deferred cold-start
            // activations resolve on the poll tick. `start()` only schedules
            // pending activations (cold start); they resolve their device
            // endpoints from the controller's stored registry — so a
            // binding-bound chain produces no sound unless we install the
            // registry here. Sourced from the session's mirror of
            // `AppConfig.io_bindings`.
            controller.set_io_bindings(session.io_bindings.borrow().clone());
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
            validate_project(&*proj)?;
            if let Some(chain) = chain {
                // Issue #672: cold activation of a single-input chain builds the
                // runtime off the control worker and installs it on the poll tick
                // (no live state to preserve). A LIVE edit (model swap, param,
                // block) must NOT be routed off-thread: replacing the whole
                // runtime discards the SPSC ring continuity and runtime-only
                // state (DI loop, #614) — it stops the sound. Live edits keep the
                // engine's in-place lock-free update via upsert_chain.
                if !runtime.schedule_chain_activation(&*proj, chain)? {
                    runtime.upsert_chain(&*proj, chain)?;
                }
            } else {
                runtime.remove_chain(chain_id);
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
            Some(runtime) => runtime.set_block_enabled(chain_id, block_id, enabled),
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
