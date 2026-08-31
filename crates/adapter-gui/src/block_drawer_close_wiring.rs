//! Responsibility: wires the block drawer close action.
//! Wiring for the block drawer close callback.
//!
//! Stops the persist timer + inline stream timer, clears all selected-block
//! / draft state, resets all VecModels feeding the drawer UI, and hides the
//! standalone block editor window.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, SharedString, Timer, VecModel};

use crate::project_view::set_selected_block;
use crate::state::{BlockEditorDraft, SelectedBlock};
use crate::{
    AppWindow, BlockModelPickerItem, BlockParameterItem, CurveEditorPoint, MultiSliderPoint,
};

pub(crate) struct BlockDrawerCloseCtx {
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
    pub inline_stream_timer: Rc<RefCell<Option<Timer>>>,
}

pub(crate) fn wire(window: &AppWindow, ctx: BlockDrawerCloseCtx) {
    let BlockDrawerCloseCtx {
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
        inline_stream_timer,
    } = ctx;
    let weak_window = window.as_weak();
    crate::BlockEditorBridge::get(window).on_close_block_drawer(move || {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        crate::block_editor_draft_clear::close_block_drawer(
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
            &selected_block,
            &inline_stream_timer,
        );
        crate::BlockEditorBridge::get(&window).set_eq_total_curve("".into());
        crate::BlockEditorBridge::get(&window).set_block_drawer_selected_model_index(-1);
        crate::BlockEditorBridge::get(&window).set_block_drawer_selected_type_index(-1);
        set_selected_block(&window, None, None);
        crate::BlockEditorBridge::get(&window).set_show_block_type_picker(false);
        crate::BlockEditorBridge::get(&window).set_show_block_drawer(false);
        crate::BlockEditorBridge::get(&window).set_block_drawer_status_message("".into());
    });
}
