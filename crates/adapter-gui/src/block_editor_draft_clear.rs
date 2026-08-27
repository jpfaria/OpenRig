//! Responsibility: clears everything the block editor was holding.
//!
//! Split out of `block_picker_wiring` (#913). Hiding the drawer is screen work;
//! emptying the draft and every model it fed is not — a cancel that left the
//! parameter rows or the EQ curves behind would show the previous block's
//! knobs the next time the picker opened.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{SharedString, Timer, VecModel};

use crate::state::BlockEditorDraft;
use crate::{BlockModelPickerItem, BlockParameterItem, CurveEditorPoint, MultiSliderPoint};

/// Everything a cancelled block edit has to let go of.
pub(crate) struct BlockEditorModels {
    pub(crate) block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub(crate) block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    pub(crate) filtered_block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    pub(crate) block_model_option_labels: Rc<VecModel<SharedString>>,
    pub(crate) block_parameter_items: Rc<VecModel<BlockParameterItem>>,
    pub(crate) multi_slider_points: Rc<VecModel<MultiSliderPoint>>,
    pub(crate) curve_editor_points: Rc<VecModel<CurveEditorPoint>>,
    pub(crate) eq_band_curves: Rc<VecModel<SharedString>>,
}

/// Drop the draft and empty every model that was showing it.
///
/// The persist timer is stopped FIRST: it writes the draft back on its next
/// tick, so clearing the draft while it is still armed races a save of the edit
/// the user just cancelled.
pub(crate) fn clear_block_editor(models: &BlockEditorModels, persist_timer: &Timer) {
    persist_timer.stop();
    *models.block_editor_draft.borrow_mut() = None;
    models.block_model_options.set_vec(Vec::new());
    models.filtered_block_model_options.set_vec(Vec::new());
    models.block_model_option_labels.set_vec(Vec::new());
    models.block_parameter_items.set_vec(Vec::new());
    models.multi_slider_points.set_vec(Vec::new());
    models.curve_editor_points.set_vec(Vec::new());
    models.eq_band_curves.set_vec(Vec::new());
}

#[cfg(test)]
#[path = "block_editor_draft_clear_tests.rs"]
mod tests;
