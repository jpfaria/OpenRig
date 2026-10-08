//! Responsibility: wires the detached block editor deletion.
//! Block-editor drawer delete + plugin-info/close callback wiring
//! (issue #792 split from block_editor_window_lifecycle.rs).

use slint::{ComponentHandle, Global};

use application::command::{BlockCommand, Command};

use crate::plugin_info_window_open::open_plugin_info_window;
use crate::project_ops::sync_project_dirty;
use crate::project_view::{replace_project_chains, set_selected_block};
use crate::runtime_sync_policy::request_chain_sync;
use crate::{AppWindow, BlockEditorWindow};

use crate::block_editor_window_lifecycle::BlockEditorWindowLifecycleCtx;

pub(crate) fn wire_block_delete(
    win: &BlockEditorWindow,
    weak_main_window: &slint::Weak<AppWindow>,
    ctx: &BlockEditorWindowLifecycleCtx,
) {
    let win_draft = &ctx.win_draft;
    let win_timer = &ctx.win_timer;
    let project_session = &ctx.project_session;
    let project_chains = &ctx.project_chains;
    let saved_project_snapshot = &ctx.saved_project_snapshot;
    let project_dirty = &ctx.project_dirty;
    let input_chain_devices = &ctx.input_chain_devices;
    let output_chain_devices = &ctx.output_chain_devices;
    let selected_block = &ctx.selected_block;
    let open_block_windows = &ctx.open_block_windows;
    // on_delete_block_drawer (trash icon) — opens the in-window overlay.
    // Issue #360: the actual delete moved to on_confirm_delete_block below;
    // the previous native-dialog path is gone (native popup did not suit
    // Orange Pi touch sessions and stole focus on macOS).
    {
        let win_draft = win_draft.clone();
        let win_timer = win_timer.clone();
        let weak_win = win.as_weak();
        crate::BlockEditorBridge::get(win).on_delete_block_drawer(move || {
            let Some(win) = weak_win.upgrade() else {
                return;
            };
            win_timer.stop();
            let Some(draft) = win_draft.borrow().clone() else {
                return;
            };
            if draft.block_index.is_none() {
                return;
            }
            crate::OverlayBridge::get(&win).set_confirm_delete_block_name(draft.model_id.into());
            crate::OverlayBridge::get(&win).set_show_confirm_delete_block(true);
        });
    }

    // on_cancel_delete_block — just hide the overlay.
    {
        let weak_win = win.as_weak();
        crate::OverlayBridge::get(win).on_cancel_delete_block(move || {
            if let Some(win) = weak_win.upgrade() {
                crate::OverlayBridge::get(&win).set_show_confirm_delete_block(false);
            }
        });
    }

    // on_confirm_delete_block — execute the deletion the overlay just gated.
    {
        let win_draft = win_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let selected_block_delete = selected_block.clone();
        let open_block_windows_delete = open_block_windows.clone();
        let weak_main = weak_main_window.clone();
        let weak_win = win.as_weak();
        crate::OverlayBridge::get(win).on_confirm_delete_block(move || {
            let Some(win) = weak_win.upgrade() else {
                return;
            };
            // Hide overlay first so any error toast renders on the
            // window, not behind the modal backdrop.
            crate::OverlayBridge::get(&win).set_show_confirm_delete_block(false);
            let Some(main) = weak_main.upgrade() else {
                return;
            };
            let Some(draft) = win_draft.borrow().clone() else {
                return;
            };
            let Some(block_index) = draft.block_index else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            // Resolve chain_id and block_id before dispatching.
            let (chain_id, block_id) = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(draft.chain_index) else {
                    return;
                };
                // #328: an index inside a split path counts in that path.
                let Some(block) =
                    crate::chain_block_lists::block_at(chain, block_index, draft.path.as_ref())
                else {
                    return;
                };
                (chain.id.clone(), block.id.clone())
            };
            // Dispatch BlockCommand::RemoveBlock — mutates project via shared Rc.
            if let Err(e) = session
                .dispatcher
                .dispatch(Command::Block(BlockCommand::RemoveBlock {
                    chain: chain_id.clone(),
                    block: block_id,
                }))
            {
                log::error!("[adapter-gui] block-window.delete dispatch: {e}");
                if let Some(w) = weak_main.upgrade() {
                    crate::BlockEditorBridge::get(&w)
                        .set_block_drawer_status_message(e.to_string().into());
                }
                return;
            }
            if let Err(e) = request_chain_sync(session, &chain_id) {
                log::error!("[adapter-gui] block-window.delete: {e}");
                if let Some(w) = weak_main.upgrade() {
                    crate::BlockEditorBridge::get(&w)
                        .set_block_drawer_status_message(e.to_string().into());
                }
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                &session.io_bindings.borrow(),
            );
            sync_project_dirty(&main, session, &saved_project_snapshot, &project_dirty);
            drop(session_borrow);
            *selected_block_delete.borrow_mut() = None;
            set_selected_block(&main, None);
            open_block_windows_delete
                .borrow_mut()
                .retain(|bw| bw.chain_index != draft.chain_index || bw.block_index != block_index);
            let _ = win.hide();
        });
    }
}

pub(crate) fn wire_plugin_info_close(
    win: &BlockEditorWindow,
    weak_main_window: &slint::Weak<AppWindow>,
    ctx: &BlockEditorWindowLifecycleCtx,
) {
    let win_draft = &ctx.win_draft;
    let selected_block = &ctx.selected_block;
    let open_block_windows = &ctx.open_block_windows;
    let plugin_info_window = &ctx.plugin_info_window;
    let chain_index = ctx.chain_index;
    let block_index = ctx.block_index;

    // on_show_plugin_info
    {
        let weak_main = weak_main_window.clone();
        let plugin_info_window = plugin_info_window.clone();
        win.on_show_plugin_info(move |effect_type, model_id| {
            if let Some(window) = weak_main.upgrade() {
                open_plugin_info_window(&window, &plugin_info_window, &effect_type, &model_id);
            }
        });
    }

    // on_close_block_drawer (close without saving)
    {
        let win_draft = win_draft.clone();
        let open_block_windows_close = open_block_windows.clone();
        let selected_block_close = selected_block.clone();
        let weak_main = weak_main_window.clone();
        let weak_win = win.as_weak();
        crate::BlockEditorBridge::get(win).on_close_block_drawer(move || {
            let Some(win) = weak_win.upgrade() else {
                return;
            };
            let Some(main) = weak_main.upgrade() else {
                return;
            };
            let draft_borrow = win_draft.borrow();
            if let Some(draft) = draft_borrow.as_ref() {
                open_block_windows_close.borrow_mut().retain(|bw| {
                    bw.chain_index != draft.chain_index || Some(bw.block_index) != draft.block_index
                });
            }
            drop(draft_borrow);
            *selected_block_close.borrow_mut() = None;
            set_selected_block(&main, None);
            let _ = win.hide();
        });
    }

    // Clean up stream timer when block editor is closed via the window X button.
    {
        let open_block_windows_close = open_block_windows.clone();
        let ci = chain_index;
        let bi = block_index;
        win.window().on_close_requested(move || {
            open_block_windows_close
                .borrow_mut()
                .retain(|bw| bw.chain_index != ci || bw.block_index != bi);
            slint::CloseRequestResponse::HideWindow
        });
    }
}
