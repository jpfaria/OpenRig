//! Wiring for the block drawer close callback.
//!
//! Stops the persist timer + inline stream timer, clears all selected-block
//! / draft state, resets all VecModels feeding the drawer UI, and hides the
//! standalone block editor window.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, SharedString, Timer, VecModel};

use crate::project_view::set_selected_block;
use crate::state::{BlockEditorDraft, SelectedBlock};
use crate::{
    AppWindow, BlockEditorWindow, BlockModelPickerItem, BlockParameterItem, CurveEditorPoint,
    MultiSliderPoint,
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

pub(crate) fn wire(
    window: &AppWindow,
    block_editor_window: &BlockEditorWindow,
    ctx: BlockDrawerCloseCtx,
) {
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
    let weak_block_editor_window = block_editor_window.as_weak();
    window.on_close_block_drawer(move || {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        block_editor_persist_timer.stop();
        *inline_stream_timer.borrow_mut() = None;
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
        window.set_block_drawer_selected_model_index(-1);
        window.set_block_drawer_selected_type_index(-1);
        set_selected_block(&window, None, None);
        window.set_show_block_type_picker(false);
        window.set_show_block_drawer(false);
        window.set_block_drawer_status_message("".into());
        if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
            let _ = block_editor_window.hide();
        }
    });
}
