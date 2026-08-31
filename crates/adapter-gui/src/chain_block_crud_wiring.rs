//! Responsibility: wires the chain block CRUD actions of the main window.
//! Wiring for chain-block CRUD callbacks on the main window.
//!
//! Three callbacks driving the per-block actions inside an open chain:
//!
//! - `on_clear_chain_block`        — clear all selected-block + drawer state
//!   (closes the standalone block editor too).
//! - `on_toggle_chain_block_enabled` — toggle one block's enabled flag, keep
//!   the editor draft in sync, and resync
//!   the live runtime.
//! - `on_reorder_chain_block`      — move a block, close any open block-editor
//!   windows for that chain (avoids stale
//!   index references), and resync runtime.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, Timer, VecModel};

use domain::AudioDeviceDescriptor;

use crate::helpers::{clear_status, set_status_error};
use crate::project_ops::sync_project_dirty;
use crate::project_view::set_selected_block;
use crate::state::{BlockEditorDraft, BlockWindow, ProjectSession, SelectedBlock};
use crate::{
    AppWindow, BlockModelPickerItem, BlockParameterItem, CurveEditorPoint, MultiSliderPoint,
    ProjectChainItem,
};
use slint::SharedString;

pub(crate) struct ChainBlockCrudCtx {
    pub selected_block: Rc<RefCell<Option<SelectedBlock>>>,
    pub block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    pub filtered_block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    pub block_model_option_labels: Rc<VecModel<SharedString>>,
    pub block_parameter_items: Rc<VecModel<BlockParameterItem>>,
    pub multi_slider_points: Rc<VecModel<MultiSliderPoint>>,
    pub curve_editor_points: Rc<VecModel<CurveEditorPoint>>,
    pub eq_band_curves: Rc<VecModel<SharedString>>,
    pub block_editor_persist_timer: Rc<Timer>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub toast_timer: Rc<Timer>,
    pub open_block_windows: Rc<RefCell<Vec<BlockWindow>>>,
    pub auto_save: bool,
}

pub(crate) fn wire(window: &AppWindow, ctx: ChainBlockCrudCtx) {
    let ChainBlockCrudCtx {
        selected_block,
        block_editor_draft,
        block_model_options,
        filtered_block_model_options,
        block_model_option_labels,
        block_parameter_items,
        multi_slider_points,
        curve_editor_points,
        eq_band_curves,
        block_editor_persist_timer,
        project_session,
        project_chains,
        saved_project_snapshot,
        project_dirty,
        input_chain_devices,
        output_chain_devices,
        toast_timer,
        open_block_windows,
        auto_save,
    } = ctx;

    {
        let weak_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let block_model_options = block_model_options.clone();
        let filtered_block_model_options = filtered_block_model_options.clone();
        let block_model_option_labels = block_model_option_labels.clone();
        let block_parameter_items = block_parameter_items.clone();
        let multi_slider_points = multi_slider_points.clone();
        let curve_editor_points = curve_editor_points.clone();
        let eq_band_curves = eq_band_curves.clone();
        window.on_clear_chain_block(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            *selected_block.borrow_mut() = None;
            crate::block_editor_draft_clear::clear_block_editor_models(
                &crate::block_editor_draft_clear::BlockEditorModels {
                    block_editor_draft: block_editor_draft.clone(),
                    block_model_options: block_model_options.clone(),
                    filtered_block_model_options: filtered_block_model_options.clone(),
                    block_model_option_labels: block_model_option_labels.clone(),
                    block_parameter_items: block_parameter_items.clone(),
                    multi_slider_points: multi_slider_points.clone(),
                    curve_editor_points: curve_editor_points.clone(),
                    eq_band_curves: eq_band_curves.clone(),
                },
            );
            crate::BlockEditorBridge::get(&window).set_eq_total_curve("".into());
            set_selected_block(&window, None, None);
            crate::BlockEditorBridge::get(&window).set_show_block_drawer(false);
            crate::BlockEditorBridge::get(&window).set_show_block_type_picker(false);
            crate::BlockEditorBridge::get(&window).set_block_drawer_status_message("".into());
        });
    }
    {
        let weak_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_toggle_chain_block_enabled(move |chain_index, ui_block_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            if project_session.borrow().is_none() {
                set_status_error(
                    &window,
                    &toast_timer,
                    &rust_i18n::t!("error-no-project-loaded"),
                );
                return;
            }
            let toggled = match crate::block_toggle::toggle_block_at_row(
                &project_session,
                chain_index as usize,
                ui_block_index as usize,
                &project_chains,
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            ) {
                Ok(toggled) => toggled,
                Err(crate::block_toggle::ToggleBlockError::NoSuchChain) => {
                    set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                    return;
                }
                Err(crate::block_toggle::ToggleBlockError::NoSuchBlock) => {
                    set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-block"));
                    return;
                }
                Err(crate::block_toggle::ToggleBlockError::Failed(message)) => {
                    set_status_error(&window, &toast_timer, &message);
                    return;
                }
                Err(crate::block_toggle::ToggleBlockError::NoProject) => return,
            };
            let block_index = toggled.block_index;
            let new_enabled = toggled.enabled;
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            // Keep block_editor_draft in sync to prevent stale persist from reverting
            if let Some(draft) = block_editor_draft.borrow_mut().as_mut() {
                if draft.chain_index == chain_index as usize
                    && draft.block_index == Some(block_index)
                {
                    draft.enabled = new_enabled;
                }
            }
            // Keep inline drawer UI in sync
            crate::BlockEditorBridge::get(&window).set_block_drawer_enabled(new_enabled);
            let selected = SelectedBlock {
                chain_index: chain_index as usize,
                block_index,
            };
            *selected_block.borrow_mut() = Some(selected);
            {
                let proj = session.project.borrow();
                let chain_ref = proj.chains.get(chain_index as usize);
                set_selected_block(&window, selected_block.borrow().as_ref(), chain_ref);
            }
            sync_project_dirty(
                &window,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
            clear_status(&window, &toast_timer);
        });
    }
    {
        let weak_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        let open_block_windows = open_block_windows.clone();
        window.on_reorder_chain_block(move |chain_index, ui_from_index, ui_before_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            if project_session.borrow().is_none() {
                set_status_error(
                    &window,
                    &toast_timer,
                    &rust_i18n::t!("error-no-project-loaded"),
                );
                return;
            }
            // Compute real indices and resolve block_id; the dispatcher handles the actual move.
            match crate::block_reorder::reorder_block(
                &project_session,
                chain_index as usize,
                ui_from_index as usize,
                ui_before_index as usize,
                &project_chains,
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            ) {
                Ok(_) => {}
                Err(crate::block_reorder::ReorderBlockError::NoSuchChain) => {
                    set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                    return;
                }
                Err(crate::block_reorder::ReorderBlockError::Failed(message)) => {
                    set_status_error(&window, &toast_timer, &message);
                    return;
                }
                Err(_) => return,
            }
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            // Close editor and clear all state — avoids stale index references
            block_editor_persist_timer.stop();
            *selected_block.borrow_mut() = None;
            *block_editor_draft.borrow_mut() = None;
            // Close all open block editor windows for this chain
            {
                let ci = chain_index as usize;
                for bw in open_block_windows.borrow().iter() {
                    if bw.chain_index == ci {
                        let _ = bw.window.hide();
                    }
                }
                open_block_windows
                    .borrow_mut()
                    .retain(|bw| bw.chain_index != ci);
            }
            crate::BlockEditorBridge::get(&window).set_show_block_drawer(false);
            crate::BlockEditorBridge::get(&window).set_show_block_type_picker(false);
            set_selected_block(&window, None, None);
            sync_project_dirty(
                &window,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
            clear_status(&window, &toast_timer);
        });
    }
}
