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
    let mut borrow = project_runtime.borrow_mut();
    if let Some(runtime) = borrow.as_mut() {
        validate_project(&session.project)?;
        runtime.sync_project(&session.project)?;
    }
    Ok(())
}

pub(crate) fn sync_live_chain_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    session: &ProjectSession,
    chain_id: &ChainId,
) -> Result<()> {
    log::debug!("sync_live_chain_runtime: chain_id='{}'", chain_id.0);
    let chain = session.project.chains.iter().find(|c| &c.id == chain_id);
    let chain_enabled = chain.map(|c| c.enabled).unwrap_or(false);
    // If chain is being enabled and no runtime exists, create one
    if chain_enabled {
        let mut borrow = project_runtime.borrow_mut();
        if borrow.is_none() {
            *borrow = Some(ProjectRuntimeController::start(&session.project)?);
            return Ok(()); // start() already processes all enabled chains via sync_project
        }
        drop(borrow);
    }
    // Normal sync
    let mut borrow = project_runtime.borrow_mut();
    if let Some(runtime) = borrow.as_mut() {
        validate_project(&session.project)?;
        if let Some(chain) = chain {
            runtime.upsert_chain(&session.project, chain)?;
        } else {
            runtime.remove_chain(chain_id);
        }
        // If no chains are running, destroy runtime
        if !runtime.is_running() {
            *borrow = None;
        }
    }
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
