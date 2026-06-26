//! Wiring for the block-delete confirm/cancel callbacks.
//!
//! Confirm: removes the block at the selected chain/block index, resyncs
//! the live runtime, refreshes the chain rows, clears all editor state,
//! and hides the standalone block editor window.
//! Cancel: just hides the confirm dialog.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, SharedString, Timer, VecModel};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::helpers::{clear_status, log_gui_message, set_status_error};
use crate::project_ops::sync_project_dirty;
use crate::project_view::{replace_project_chains, set_selected_block};
use crate::state::{BlockEditorDraft, ProjectSession, SelectedBlock};
use crate::sync_live_chain_runtime;
use crate::{
    AppWindow, BlockEditorWindow, BlockModelPickerItem, BlockParameterItem, CurveEditorPoint,
    MultiSliderPoint, ProjectChainItem,
};

pub(crate) struct BlockDeleteCtx {
    pub selected_block: Rc<RefCell<Option<SelectedBlock>>>,
    pub block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    pub filtered_block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    pub block_model_option_labels: Rc<VecModel<SharedString>>,
    pub block_parameter_items: Rc<VecModel<BlockParameterItem>>,
    pub multi_slider_points: Rc<VecModel<MultiSliderPoint>>,
    pub curve_editor_points: Rc<VecModel<CurveEditorPoint>>,
    pub eq_band_curves: Rc<VecModel<SharedString>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub toast_timer: Rc<Timer>,
    pub auto_save: bool,
}

pub(crate) fn wire(
    window: &AppWindow,
    block_editor_window: &BlockEditorWindow,
    ctx: BlockDeleteCtx,
) {
    let BlockDeleteCtx {
        selected_block,
        block_editor_draft,
        block_model_options,
        filtered_block_model_options,
        block_model_option_labels,
        block_parameter_items,
        multi_slider_points,
        curve_editor_points,
        eq_band_curves,
        project_session,
        project_chains,
        project_runtime,
        saved_project_snapshot,
        project_dirty,
        input_chain_devices,
        output_chain_devices,
        toast_timer,
        auto_save,
    } = ctx;
    {
        let weak_window = window.as_weak();
        let weak_block_editor_window = block_editor_window.as_weak();
        window.on_confirm_delete_block(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            window.set_show_confirm_delete_block(false);
            let Some(draft) = block_editor_draft.borrow().clone() else {
                return;
            };
            let Some(block_index) = draft.block_index else {
                return;
            };
            log::info!(
                "on_delete_block: chain_index={}, block_index={}, effect_type='{}', model_id='{}'",
                draft.chain_index,
                block_index,
                draft.effect_type,
                draft.model_id
            );
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                log_gui_message("block-drawer.delete", "Nenhum projeto carregado.");
                return;
            };
            // Resolve chain_id and block_id before dispatching.
            let (chain_id, block_id) = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(draft.chain_index) else {
                    log_gui_message("block-drawer.delete", "Chain inválida.");
                    return;
                };
                let Some(block) = chain.blocks.get(block_index) else {
                    log_gui_message("block-drawer.delete", "Block inválido.");
                    return;
                };
                (chain.id.clone(), block.id.clone())
            };
            // Dispatch Command::RemoveBlock — mutates project via shared Rc.
            if let Err(error) = session.dispatcher.dispatch(Command::RemoveBlock {
                chain: chain_id.clone(),
                block: block_id,
            }) {
                log::error!("[adapter-gui] block-drawer.delete dispatch: {error}");
                set_status_error(&window, &toast_timer, &error.to_string());
                return;
            }
            if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("[adapter-gui] block-drawer.delete: {error}");
                set_status_error(&window, &toast_timer, &error.to_string());
                return;
            }
            replace_project_chains(
                &project_chains,
                &*session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            &[]
            );
            sync_project_dirty(
                &window,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
            *selected_block.borrow_mut() = None;
            *block_editor_draft.borrow_mut() = None;
            block_model_options.set_vec(Vec::new());
            filtered_block_model_options.set_vec(Vec::new());
            block_model_option_labels.set_vec(Vec::new());
            block_parameter_items.set_vec(Vec::new());
            multi_slider_points.set_vec(Vec::new());
            curve_editor_points.set_vec(Vec::new());
            eq_band_curves.set_vec(Vec::new());
            window.set_eq_total_curve("".into());
            set_selected_block(&window, None, None);
            window.set_show_block_drawer(false);
            window.set_block_drawer_status_message("".into());
            clear_status(&window, &toast_timer);
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                let _ = block_editor_window.hide();
            }
        });
    }
    {
        let weak_window = window.as_weak();
        window.on_cancel_delete_block(move || {
            if let Some(window) = weak_window.upgrade() {
                window.set_show_confirm_delete_block(false);
            }
        });
    }
}
