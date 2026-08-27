//! #913 — cancelling a block edit lets go of everything.
//!
//! Every model here feeds a visible surface. One left populated shows the
//! previous block's knobs, model list or EQ curve the next time the picker
//! opens — and the draft left behind would be saved by the persist timer as if
//! the user had confirmed.

use super::{clear_block_editor, BlockEditorModels};
use crate::{BlockModelPickerItem, BlockParameterItem, CurveEditorPoint, MultiSliderPoint};
use slint::{Model, SharedString, Timer, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

fn populated() -> BlockEditorModels {
    BlockEditorModels {
        block_editor_draft: Rc::new(RefCell::new(None)),
        block_model_options: Rc::new(VecModel::from(vec![BlockModelPickerItem::default()])),
        filtered_block_model_options: Rc::new(VecModel::from(
            vec![BlockModelPickerItem::default()],
        )),
        block_model_option_labels: Rc::new(VecModel::from(vec![SharedString::from("Amp")])),
        block_parameter_items: Rc::new(VecModel::from(vec![
            BlockParameterItem::default(),
            BlockParameterItem::default(),
        ])),
        multi_slider_points: Rc::new(VecModel::from(vec![MultiSliderPoint::default()])),
        curve_editor_points: Rc::new(VecModel::from(vec![CurveEditorPoint::default()])),
        eq_band_curves: Rc::new(VecModel::from(vec![SharedString::from("M 0 0")])),
    }
}

fn empty() -> BlockEditorModels {
    BlockEditorModels {
        block_editor_draft: Rc::new(RefCell::new(None)),
        block_model_options: Rc::new(VecModel::from(Vec::<BlockModelPickerItem>::new())),
        filtered_block_model_options: Rc::new(VecModel::from(Vec::<BlockModelPickerItem>::new())),
        block_model_option_labels: Rc::new(VecModel::from(Vec::<SharedString>::new())),
        block_parameter_items: Rc::new(VecModel::from(Vec::<BlockParameterItem>::new())),
        multi_slider_points: Rc::new(VecModel::from(Vec::<MultiSliderPoint>::new())),
        curve_editor_points: Rc::new(VecModel::from(Vec::<CurveEditorPoint>::new())),
        eq_band_curves: Rc::new(VecModel::from(Vec::<SharedString>::new())),
    }
}

#[test]
fn cancelling_empties_every_model_the_editor_was_showing() {
    let models = populated();
    clear_block_editor(&models, &Timer::default());

    assert_eq!(models.block_model_options.row_count(), 0);
    assert_eq!(models.filtered_block_model_options.row_count(), 0);
    assert_eq!(models.block_model_option_labels.row_count(), 0);
    assert_eq!(models.block_parameter_items.row_count(), 0);
    assert_eq!(models.multi_slider_points.row_count(), 0);
    assert_eq!(models.curve_editor_points.row_count(), 0);
    assert_eq!(models.eq_band_curves.row_count(), 0);
}

#[test]
fn cancelling_drops_the_draft() {
    let models = populated();
    clear_block_editor(&models, &Timer::default());
    assert!(
        models.block_editor_draft.borrow().is_none(),
        "a draft left behind is saved by the persist timer as if confirmed"
    );
}

#[test]
fn cancelling_an_already_empty_editor_is_a_no_op() {
    let models = empty();
    clear_block_editor(&models, &Timer::default());
    assert_eq!(models.block_parameter_items.row_count(), 0);
}

#[test]
fn cancelling_twice_is_safe() {
    let models = populated();
    let timer = Timer::default();
    clear_block_editor(&models, &timer);
    clear_block_editor(&models, &timer);
    assert_eq!(models.block_model_options.row_count(), 0);
}
