//! Wiring for chain-block CRUD callbacks on the main window.
//!
//! Three callbacks driving the per-block actions inside an open chain:
//!
//! - `on_clear_chain_block`        — clear all selected-block + drawer state
//!                                   (closes the standalone block editor too).
//! - `on_toggle_chain_block_enabled` — toggle one block's enabled flag, keep
//!                                     the editor draft in sync, and resync
//!                                     the live runtime.
//! - `on_reorder_chain_block`      — move a block, close any open block-editor
//!                                   windows for that chain (avoids stale
//!                                   index references), and resync runtime.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::helpers::{clear_status, set_status_error};
use crate::project_ops::sync_project_dirty;
use crate::project_view::{replace_project_chains, set_selected_block};
use crate::state::{BlockEditorDraft, BlockWindow, ProjectSession, SelectedBlock};
use crate::sync_live_chain_runtime;
use crate::ui_index_to_real_block_index;
use crate::{
    AppWindow, BlockEditorWindow, BlockModelPickerItem, BlockParameterItem, CurveEditorPoint,
    MultiSliderPoint, ProjectChainItem,
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
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub toast_timer: Rc<Timer>,
    pub open_block_windows: Rc<RefCell<Vec<BlockWindow>>>,
    pub auto_save: bool,
}

pub(crate) fn wire(
    window: &AppWindow,
    block_editor_window: &BlockEditorWindow,
    ctx: ChainBlockCrudCtx,
) {
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
        project_runtime,
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
        let weak_block_editor_window = block_editor_window.as_weak();
        window.on_clear_chain_block(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
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
            window.set_show_block_type_picker(false);
            window.set_block_drawer_status_message("".into());
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                let _ = block_editor_window.hide();
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_toggle_chain_block_enabled(move |chain_index, ui_block_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                set_status_error(
                    &window,
                    &toast_timer,
                    &rust_i18n::t!("error-no-project-loaded"),
                );
                return;
            };
            let Some(chain) = session.project.chains.get_mut(chain_index as usize) else {
                set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                return;
            };
            // Convert UI index to real block index from current chain state
            let block_index = ui_index_to_real_block_index(chain, ui_block_index as usize);
            log::info!(
                "on_toggle_chain_block_enabled: chain_index={}, ui_index={}, real_index={}",
                chain_index,
                ui_block_index,
                block_index
            );
            let Some(block) = chain.blocks.get_mut(block_index) else {
                set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-block"));
                return;
            };
            block.enabled = !block.enabled;
            let new_enabled = block.enabled;
            let chain_id = chain.id.clone();
            // Keep block_editor_draft in sync to prevent stale persist from reverting
            if let Some(draft) = block_editor_draft.borrow_mut().as_mut() {
                if draft.chain_index == chain_index as usize
                    && draft.block_index == Some(block_index)
                {
                    draft.enabled = new_enabled;
                }
            }
            // Keep inline drawer UI in sync
            window.set_block_drawer_enabled(new_enabled);
            if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                set_status_error(&window, &toast_timer, &error.to_string());
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project,
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            );
            let chain_ref = session.project.chains.get(chain_index as usize);
            *selected_block.borrow_mut() = Some(SelectedBlock {
                chain_index: chain_index as usize,
                block_index,
            });
            set_selected_block(&window, selected_block.borrow().as_ref(), chain_ref);
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
        let project_runtime = project_runtime.clone();
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
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                set_status_error(&window, &toast_timer, &rust_i18n::t!("error-no-project-loaded"));
                return;
            };
            let (chain_id, _insert_at) = {
                let Some(chain) = session.project.chains.get_mut(chain_index as usize) else {
                    set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                    return;
                };
                // Both from_index and before_index are in UI space — convert to real indices
                let from_index = ui_index_to_real_block_index(chain, ui_from_index as usize) as i32;
                let real_before =
                    ui_index_to_real_block_index(chain, ui_before_index as usize) as i32;
                log::info!(
                    "[reorder_chain_block] chain_index={}, ui_from={} → real_from={}, ui_before={} → real_before={}",
                    chain_index,
                    ui_from_index,
                    from_index,
                    ui_before_index,
                    real_before
                );
                let block_count = chain.blocks.len() as i32;
                if from_index < 0 || from_index >= block_count {
                    return;
                }
                if real_before == from_index || real_before == from_index + 1 {
                    return;
                }
                let block = chain.blocks.remove(from_index as usize);
                let mut normalized_before = real_before;
                if normalized_before > from_index {
                    normalized_before -= 1;
                }
                let insert_at = normalized_before.clamp(0, chain.blocks.len() as i32) as usize;
                chain.blocks.insert(insert_at, block);
                (chain.id.clone(), insert_at)
            };
            if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                set_status_error(&window, &toast_timer, &error.to_string());
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project,
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            );
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
            window.set_show_block_drawer(false);
            window.set_show_block_type_picker(false);
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
