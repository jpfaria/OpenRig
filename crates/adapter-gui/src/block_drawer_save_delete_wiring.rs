//! Wiring for the block drawer save + delete (open-confirm-dialog) callbacks.
//!
//! Save: persists the active block_editor_draft, clears all editor state, hides
//! the standalone editor, and refreshes the compact view if open.
//! Delete: validates the draft has a real block_index, then triggers the
//! confirm-delete dialog.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, Timer, VecModel, Weak};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::block_editor::persist_block_editor_draft;
use crate::project_view::{build_compact_blocks, set_selected_block};
use crate::state::{BlockEditorDraft, ProjectSession, SelectedBlock};
use crate::{
    AppWindow, BlockEditorWindow, BlockModelPickerItem, BlockParameterItem,
    CompactChainViewWindow, CurveEditorPoint, MultiSliderPoint, ProjectChainItem,
};

pub(crate) struct BlockDrawerSaveDeleteCtx {
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
    pub block_editor_persist_timer: Rc<Timer>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub open_compact_window: Rc<RefCell<Option<(usize, Weak<CompactChainViewWindow>)>>>,
    pub auto_save: bool,
}

pub(crate) fn wire(
    window: &AppWindow,
    block_editor_window: &BlockEditorWindow,
    ctx: BlockDrawerSaveDeleteCtx,
) {
    let BlockDrawerSaveDeleteCtx {
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
        block_editor_persist_timer,
        input_chain_devices,
        output_chain_devices,
        open_compact_window,
        auto_save,
    } = ctx;

    {
        let weak_window = window.as_weak();
        let block_editor_draft_save = block_editor_draft.clone();
        let block_parameter_items_save = block_parameter_items.clone();
        let project_session_save = project_session.clone();
        let project_session_compact = project_session.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        window.on_save_block_drawer(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            block_editor_persist_timer.stop();
            let Some(draft) = block_editor_draft_save.borrow().clone() else {
                return;
            };
            if let Err(error) = persist_block_editor_draft(
                &window,
                &draft,
                &block_parameter_items_save,
                &project_session_save,
                &project_chains,
                &project_runtime,
                &saved_project_snapshot,
                &project_dirty,
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                true,
                auto_save,
            ) {
                log::error!("[adapter-gui] block-drawer.save: {error}");
                window.set_block_drawer_status_message(error.to_string().into());
                return;
            }
            *selected_block.borrow_mut() = None;
            set_selected_block(&window, None, None);
            *block_editor_draft_save.borrow_mut() = None;
            block_model_options.set_vec(Vec::new());
            filtered_block_model_options.set_vec(Vec::new());
            block_model_option_labels.set_vec(Vec::new());
            block_parameter_items_save.set_vec(Vec::new());
            multi_slider_points.set_vec(Vec::new());
            curve_editor_points.set_vec(Vec::new());
            eq_band_curves.set_vec(Vec::new());
            window.set_eq_total_curve("".into());
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                let _ = block_editor_window.hide();
            }
            // Refresh compact chain view if open
            if let Some((ci, weak_cw)) = open_compact_window.borrow().as_ref() {
                if let Some(cw) = weak_cw.upgrade() {
                    let session_borrow = project_session_compact.borrow();
                    if let Some(session) = session_borrow.as_ref() {
                        let blocks = build_compact_blocks(&session.project, *ci);
                        cw.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
                    }
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        window.on_delete_block_drawer(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            block_editor_persist_timer.stop();
            let Some(draft) = block_editor_draft.borrow().clone() else {
                return;
            };
            if draft.block_index.is_none() {
                return;
            }
            window.set_confirm_delete_block_name(draft.model_id.into());
            window.set_show_confirm_delete_block(true);
        });
    }
}
