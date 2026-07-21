//! Compact-chain block delete + reorder callback wiring (issue #792 split
//! from compact_chain_block_handlers.rs).

use std::cell::RefCell;
use std::rc::Rc;
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use application::command::Command;
use application::dispatcher::CommandDispatcher;
use crate::compact_block_view::build_compact_blocks;
use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::sync_live_chain_runtime;
use crate::{AppWindow, CompactChainViewWindow};

use crate::compact_chain_block_handlers::CompactChainBlockHandlersCtx;

pub(crate) fn wire_block_delete(
    main_window: &AppWindow,
    compact_win: &CompactChainViewWindow,
    ctx: &CompactChainBlockHandlersCtx,
) {
    let project_session = &ctx.project_session;
    let project_runtime = &ctx.project_runtime;
    let project_chains = &ctx.project_chains;
    let input_chain_devices = &ctx.input_chain_devices;
    let output_chain_devices = &ctx.output_chain_devices;
    let saved_project_snapshot = &ctx.saved_project_snapshot;
    let project_dirty = &ctx.project_dirty;
    let auto_save = ctx.auto_save;

    // Wire remove-block — issue #360: open the in-window overlay; the
    // real dispatch lives in confirm-delete-block below. Closures keep
    // the heap state on `pending_compact_delete_block` so the confirm
    // handler can consume the chain+block ids the user is acting on.
    let pending_compact_delete_block: Rc<RefCell<Option<(usize, usize)>>> =
        Rc::new(RefCell::new(None));
    {
        let project_session = project_session.clone();
        let weak_compact = compact_win.as_weak();
        let pending = pending_compact_delete_block.clone();
        compact_win.on_remove_block(move |ci, bi| {
            log::info!("[compact] remove-block: chain={}, block={}", ci, bi);
            let Some(cw) = weak_compact.upgrade() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            let chain_idx = ci as usize;
            let block_idx = bi as usize;
            // Resolve a human label for the dialog body and ensure the
            // indices are valid before opening the overlay.
            let model_id = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_idx) else {
                    return;
                };
                let Some(block) = chain.blocks.get(block_idx) else {
                    return;
                };
                match block.model_ref() {
                    Some(m) => {
                        project::catalog::model_display_name(m.effect_type, m.model).to_string()
                    }
                    None => String::new(),
                }
            };
            *pending.borrow_mut() = Some((chain_idx, block_idx));
            cw.set_confirm_delete_block_name(model_id.into());
            cw.set_show_confirm_delete_block(true);
        });
    }
    // Wire cancel-delete-block — just hide the overlay and forget the
    // captured ids.
    {
        let weak_compact = compact_win.as_weak();
        let pending = pending_compact_delete_block.clone();
        compact_win.on_cancel_delete_block(move || {
            *pending.borrow_mut() = None;
            if let Some(cw) = weak_compact.upgrade() {
                cw.set_show_confirm_delete_block(false);
            }
        });
    }
    // Wire confirm-delete-block — run the dispatch the overlay gated.
    {
        let project_session = project_session.clone();
        let weak_main = main_window.as_weak();
        let weak_compact = compact_win.as_weak();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let pending = pending_compact_delete_block.clone();
        compact_win.on_confirm_delete_block(move || {
            let Some(cw) = weak_compact.upgrade() else {
                return;
            };
            cw.set_show_confirm_delete_block(false);
            let Some((chain_idx, block_idx)) = pending.borrow_mut().take() else {
                return;
            };
            let Some(main_win) = weak_main.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            // Resolve IDs (read-only) before dispatching.
            let (chain_id, block_id) = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_idx) else {
                    return;
                };
                let Some(block) = chain.blocks.get(block_idx) else {
                    return;
                };
                (chain.id.clone(), block.id.clone())
            };
            if let Err(e) = session.dispatcher.dispatch(Command::RemoveBlock {
                chain: chain_id.clone(),
                block: block_id,
            }) {
                log::error!("[compact] remove-block dispatch: {e}");
                return;
            }
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("[compact] remove-block runtime sync: {}", e);
            }
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                &[],
            );
            let blocks = build_compact_blocks(&session.project.borrow(), chain_idx);
            cw.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
            sync_project_dirty(
                &main_win,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
        });
    }


}

pub(crate) fn wire_block_reorder(
    main_window: &AppWindow,
    compact_win: &CompactChainViewWindow,
    ctx: &CompactChainBlockHandlersCtx,
) {
    let project_session = &ctx.project_session;
    let project_runtime = &ctx.project_runtime;
    let project_chains = &ctx.project_chains;
    let input_chain_devices = &ctx.input_chain_devices;
    let output_chain_devices = &ctx.output_chain_devices;
    let saved_project_snapshot = &ctx.saved_project_snapshot;
    let project_dirty = &ctx.project_dirty;
    let auto_save = ctx.auto_save;

    // Wire reorder-block — resolve real indices from CompactBlockItem.block_index
    {
        let project_session = project_session.clone();
        let weak_main = main_window.as_weak();
        let weak_compact = compact_win.as_weak();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        compact_win.on_reorder_block(move |ci, compact_from, compact_before| {
            let Some(main_win) = weak_main.upgrade() else { return; };
            let Some(cw) = weak_compact.upgrade() else { return; };
            // Look up real chain.blocks indices from the Slint compact model
            let compact_model = cw.get_compact_blocks();
            let compact_len = compact_model.row_count();
            let from_pos = compact_from as usize;
            if from_pos >= compact_len { return; }
            let from_index = compact_model.row_data(from_pos)
                .map(|item| item.block_index as usize)
                .unwrap_or(0);
            let before_pos = compact_before as usize;
            let real_before = if before_pos < compact_len {
                compact_model.row_data(before_pos)
                    .map(|item| item.block_index as usize)
                    .unwrap_or(0)
            } else {
                // "after last compact block" → one position after last compact block's real index
                compact_model.row_data(compact_len - 1)
                    .map(|item| item.block_index as usize + 1)
                    .unwrap_or(0)
            };
            log::info!("[compact] reorder-block: compact_from={}, compact_before={}, real_from={}, real_before={}", compact_from, compact_before, from_index, real_before);
            if real_before == from_index || real_before == from_index + 1 { return; }
            let chain_idx = ci as usize;
            // Resolve block_id and compute insert_at before dispatching.
            let (chain_id, block_id, insert_at) = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else { return; };
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_idx) else { return; };
                let block_count = chain.blocks.len();
                if from_index >= block_count { return; }
                let block_id = chain.blocks[from_index].id.clone();
                let mut normalized_before = real_before;
                if normalized_before > from_index { normalized_before -= 1; }
                let insert_at = normalized_before.min(block_count.saturating_sub(1));
                (chain.id.clone(), block_id, insert_at)
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else { return; };
            // Dispatch Command::MoveBlock — mutates project via shared Rc.
            if let Err(e) = session.dispatcher.dispatch(Command::MoveBlock {
                chain: chain_id.clone(),
                block: block_id,
                new_position: insert_at,
            }) {
                log::error!("[compact] reorder-block dispatch: {}", e);
                return;
            }
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("[compact] reorder-block runtime sync: {}", e);
            }
            replace_project_chains(&project_chains, &session.project.borrow(), &input_chain_devices.borrow(), &output_chain_devices.borrow(),
            &[]);
            let blocks = build_compact_blocks(&session.project.borrow(), chain_idx);
            cw.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
            sync_project_dirty(&main_win, session, &saved_project_snapshot, &project_dirty, auto_save);
        });
    }

}

