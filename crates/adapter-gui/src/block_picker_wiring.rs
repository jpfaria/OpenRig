//! Responsibility: wires the block picker cancel action.
//! Wiring for the block picker cancel callback.
//!
//! Clears the entire block-editor draft state (model options, parameters,
//! EQ curves, persist timer) and hides the standalone block editor window.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, SharedString, Timer, VecModel};

use crate::state::BlockEditorDraft;
use crate::{
    AppWindow, BlockModelPickerItem, BlockParameterItem, CurveEditorPoint, MultiSliderPoint,
};

pub(crate) struct BlockPickerCtx {
    pub block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    pub filtered_block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    pub block_model_option_labels: Rc<VecModel<SharedString>>,
    pub block_parameter_items: Rc<VecModel<BlockParameterItem>>,
    pub multi_slider_points: Rc<VecModel<MultiSliderPoint>>,
    pub curve_editor_points: Rc<VecModel<CurveEditorPoint>>,
    pub eq_band_curves: Rc<VecModel<SharedString>>,
    pub block_editor_persist_timer: Rc<Timer>,
}

pub(crate) fn wire(window: &AppWindow, ctx: BlockPickerCtx) {
    let BlockPickerCtx {
        block_editor_draft,
        block_model_options,
        filtered_block_model_options,
        block_model_option_labels,
        block_parameter_items,
        multi_slider_points,
        curve_editor_points,
        eq_band_curves,
        block_editor_persist_timer,
    } = ctx;
    let weak_window = window.as_weak();
    crate::BlockEditorBridge::get(window).on_cancel_block_picker(move || {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        crate::block_editor_draft_clear::clear_block_editor(
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
            &block_editor_persist_timer,
        );
        crate::BlockEditorBridge::get(&window).set_eq_total_curve("".into());
        crate::BlockEditorBridge::get(&window).set_block_drawer_selected_model_index(-1);
        crate::BlockEditorBridge::get(&window).set_block_drawer_selected_type_index(-1);
        crate::BlockEditorBridge::get(&window).set_show_block_type_picker(false);
        crate::BlockEditorBridge::get(&window).set_show_block_drawer(false);
        crate::BlockEditorBridge::get(&window).set_block_drawer_status_message("".into());
    });
}
