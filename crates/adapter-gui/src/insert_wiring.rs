//! Wiring for the chain Insert block editor window callbacks.
//!
//! Owns the 12 callbacks registered on `ChainInsertWindow` (send/return device
//! and channel pickers, mode selectors, enable toggle, delete, save, cancel).
//! Lives outside `lib.rs` so Insert-specific edits don't collide with other
//! features in parallel branches.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Model, VecModel};

use application::command::{BlockCommand, Command};
use application::dispatcher::CommandDispatcher;
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::audio_devices::{
    build_insert_return_channel_items, build_insert_send_channel_items, replace_channel_options,
};
use crate::chain_editor::insert_mode_from_index;
use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::state::{InsertDraft, ProjectSession};
use crate::{sync_block_toggle, sync_live_chain_runtime};
use crate::{AppWindow, ChainInsertWindow, ChannelOptionItem, ProjectChainItem};

/// State borrowed by the Insert window callbacks. Each `Rc` is cloned per
/// callback closure that needs it.
pub(crate) struct InsertWiringCtx {
    pub insert_draft: Rc<RefCell<Option<InsertDraft>>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub insert_send_channels: Rc<VecModel<ChannelOptionItem>>,
    pub insert_return_channels: Rc<VecModel<ChannelOptionItem>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub auto_save: bool,
}

pub(crate) fn wire(
    window: &AppWindow,
    chain_insert_window: &ChainInsertWindow,
    ctx: InsertWiringCtx,
) {
    let InsertWiringCtx {
        insert_draft,
        input_chain_devices,
        output_chain_devices,
        insert_send_channels,
        insert_return_channels,
        project_session,
        project_runtime,
        project_chains,
        saved_project_snapshot,
        project_dirty,
        auto_save,
    } = ctx;

    {
        let insert_draft = insert_draft.clone();
        let output_chain_devices = output_chain_devices.clone();
        let insert_send_channels = insert_send_channels.clone();
        chain_insert_window.on_select_send_device(move |index| {
            let devs_out = output_chain_devices.borrow();
            let Some(device) = devs_out.get(index as usize) else {
                return;
            };
            let mut draft_borrow = insert_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            draft.send_device_id = Some(device.id.clone());
            draft.send_channels.clear();
            let items = build_insert_send_channel_items(draft, &devs_out);
            replace_channel_options(&insert_send_channels, items);
        });
    }
    {
        let insert_draft = insert_draft.clone();
        let insert_send_channels = insert_send_channels.clone();
        chain_insert_window.on_toggle_send_channel(move |index, selected| {
            let mut draft_borrow = insert_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let ch = index as usize;
            if selected {
                if !draft.send_channels.contains(&ch) {
                    draft.send_channels.push(ch);
                }
            } else {
                draft.send_channels.retain(|&c| c != ch);
            }
            if let Some(mut row) = insert_send_channels.row_data(index as usize) {
                row.selected = selected;
                insert_send_channels.set_row_data(index as usize, row);
            }
        });
    }
    {
        let insert_draft = insert_draft.clone();
        chain_insert_window.on_select_send_mode(move |index| {
            let mut draft_borrow = insert_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            draft.send_mode = insert_mode_from_index(index);
            log::debug!(
                "[select_send_mode] index={}, mode={:?}",
                index,
                draft.send_mode
            );
        });
    }
    {
        let insert_draft = insert_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let insert_return_channels = insert_return_channels.clone();
        chain_insert_window.on_select_return_device(move |index| {
            let devs_in = input_chain_devices.borrow();
            let Some(device) = devs_in.get(index as usize) else {
                return;
            };
            let mut draft_borrow = insert_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            draft.return_device_id = Some(device.id.clone());
            draft.return_channels.clear();
            let items = build_insert_return_channel_items(draft, &devs_in);
            replace_channel_options(&insert_return_channels, items);
        });
    }
    {
        let insert_draft = insert_draft.clone();
        let insert_return_channels = insert_return_channels.clone();
        chain_insert_window.on_toggle_return_channel(move |index, selected| {
            let mut draft_borrow = insert_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let ch = index as usize;
            if selected {
                if !draft.return_channels.contains(&ch) {
                    draft.return_channels.push(ch);
                }
            } else {
                draft.return_channels.retain(|&c| c != ch);
            }
            if let Some(mut row) = insert_return_channels.row_data(index as usize) {
                row.selected = selected;
                insert_return_channels.set_row_data(index as usize, row);
            }
        });
    }
    {
        let insert_draft = insert_draft.clone();
        chain_insert_window.on_select_return_mode(move |index| {
            let mut draft_borrow = insert_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            draft.return_mode = insert_mode_from_index(index);
            log::debug!(
                "[select_return_mode] index={}, mode={:?}",
                index,
                draft.return_mode
            );
        });
    }
    {
        let insert_draft = insert_draft.clone();
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_window = window.as_weak();
        let weak_insert_window = chain_insert_window.as_weak();
        chain_insert_window.on_toggle_enabled(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(iw) = weak_insert_window.upgrade() else {
                return;
            };
            let draft_borrow = insert_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            let chain_idx = draft.chain_index;
            let block_idx = draft.block_index;
            drop(draft_borrow);
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            // Resolve positional indices to IDs before dispatching.
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
            match session.dispatcher.dispatch(Command::Block(
                BlockCommand::ToggleBlockEnabled {
                    chain: chain_id.clone(),
                    block: block_id.clone(),
                },
            )) {
                Ok(_) => {}
                Err(e) => {
                    log::error!("toggle insert block enabled: {e}");
                    return;
                }
            }
            let block_enabled = session
                .project
                .borrow()
                .chains
                .iter()
                .find(|c| c.id == chain_id)
                .and_then(|c| c.blocks.get(block_idx))
                .map(|b| b.enabled)
                .unwrap_or(false);
            iw.set_block_enabled(block_enabled);
            if let Err(e) = sync_block_toggle(
                &project_runtime,
                session,
                &chain_id,
                &block_id,
                block_enabled,
            ) {
                log::error!("toggle insert block enabled runtime sync: {e}");
            }
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                &[],
            );
            sync_project_dirty(
                &window,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
        });
    }
    {
        let insert_draft = insert_draft.clone();
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_window = window.as_weak();
        let weak_insert_window = chain_insert_window.as_weak();
        chain_insert_window.on_delete_block(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(iw) = weak_insert_window.upgrade() else {
                return;
            };
            let draft_borrow = insert_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            let chain_idx = draft.chain_index;
            let block_idx = draft.block_index;
            drop(draft_borrow);
            *insert_draft.borrow_mut() = None;
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
            if let Err(e) = session
                .dispatcher
                .dispatch(Command::Block(BlockCommand::RemoveBlock {
                    chain: chain_id.clone(),
                    block: block_id,
                })) {
                log::error!("delete insert block: {e}");
                return;
            }
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("delete insert block: {e}");
            }
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                &[],
            );
            sync_project_dirty(
                &window,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
            let _ = iw.hide();
        });
    }
    {
        let insert_draft = insert_draft.clone();
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_window = window.as_weak();
        let weak_insert_window = chain_insert_window.as_weak();
        chain_insert_window.on_save(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(iw) = weak_insert_window.upgrade() else {
                return;
            };
            let draft_borrow = insert_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            // TODO(#716): the insert editor Slint UI still exposes send/return
            // device + channel pickers, but model A persists a single I/O binding
            // id (`io`) — the send resolves to the binding's output and the
            // return to its input via the per-machine registry. The picker
            // widgets are placeholder no-ops; this saves the draft's `io`. The
            // Slint window should be reworked to pick a binding directly.
            let chain_idx = draft.chain_index;
            let block_idx = draft.block_index;
            let io = draft.io.clone();
            drop(draft_borrow);
            *insert_draft.borrow_mut() = None;
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                let _ = iw.hide();
                return;
            };
            // Resolve positional indices to IDs before dispatching.
            let (chain_id, block_id) = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_idx) else {
                    let _ = iw.hide();
                    return;
                };
                let Some(block) = chain.blocks.get(block_idx) else {
                    let _ = iw.hide();
                    return;
                };
                (chain.id.clone(), block.id.clone())
            };
            if let Err(e) = session
                .dispatcher
                .dispatch(Command::Block(BlockCommand::SaveInsertBlock {
                    chain: chain_id.clone(),
                    block: block_id,
                    io,
                })) {
                log::error!("insert save error: {e}");
                let _ = iw.hide();
                return;
            }
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("insert save runtime sync error: {e}");
            }
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                &[],
            );
            sync_project_dirty(
                &window,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
            iw.set_status_message("".into());
            let _ = iw.hide();
        });
    }
    {
        let insert_draft = insert_draft.clone();
        let weak_insert_window = chain_insert_window.as_weak();
        chain_insert_window.on_cancel(move || {
            *insert_draft.borrow_mut() = None;
            if let Some(iw) = weak_insert_window.upgrade() {
                iw.set_status_message("".into());
                let _ = iw.hide();
            }
        });
    }
}
