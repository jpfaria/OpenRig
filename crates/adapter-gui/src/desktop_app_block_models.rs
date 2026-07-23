//! Block-editor `VecModel`s bound to the main `AppWindow` (the inline editor
//! and pickers). The detached editor builds its own per-window models in
//! `block_editor_window_setup::create_and_wire` (#819).
//!
//! The 8 models cover:
//!
//! * `block_type_options` — top-level type picker, seeded with the generic
//!   instrument list (re-populated when a chain is selected).
//! * `block_model_options` / `filtered_block_model_options` — full model
//!   list and the search-filtered subset.
//! * `block_model_option_labels` — pre-rendered label strings.
//! * `block_parameter_items` — the active block's parameter rows.
//! * `multi_slider_points` / `curve_editor_points` — points for the EQ
//!   shape editors.
//! * `eq_band_curves` — per-band curve strings rendered behind the points.
//!
use std::rc::Rc;

use slint::{ModelRc, SharedString, VecModel};

use crate::project_view::block_type_picker_items;
use crate::{
    AppWindow, BlockModelPickerItem, BlockParameterItem, BlockTypePickerItem, CurveEditorPoint,
    MultiSliderPoint,
};

pub(crate) struct BlockEditorModels {
    pub block_type_options: Rc<VecModel<BlockTypePickerItem>>,
    pub block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    pub filtered_block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    pub block_model_option_labels: Rc<VecModel<SharedString>>,
    pub block_parameter_items: Rc<VecModel<BlockParameterItem>>,
    pub multi_slider_points: Rc<VecModel<MultiSliderPoint>>,
    pub curve_editor_points: Rc<VecModel<CurveEditorPoint>>,
    pub eq_band_curves: Rc<VecModel<SharedString>>,
}

pub(crate) fn init(window: &AppWindow) -> BlockEditorModels {
    let block_type_options = Rc::new(VecModel::from(block_type_picker_items(
        block_core::INST_GENERIC,
    )));
    let block_model_options = Rc::new(VecModel::from(Vec::<BlockModelPickerItem>::new()));
    let filtered_block_model_options = Rc::new(VecModel::from(Vec::<BlockModelPickerItem>::new()));
    let block_model_option_labels = Rc::new(VecModel::from(Vec::<SharedString>::new()));
    let block_parameter_items = Rc::new(VecModel::from(Vec::<BlockParameterItem>::new()));
    let multi_slider_points = Rc::new(VecModel::from(Vec::<MultiSliderPoint>::new()));
    let curve_editor_points = Rc::new(VecModel::from(Vec::<CurveEditorPoint>::new()));
    let eq_band_curves = Rc::new(VecModel::from(Vec::<SharedString>::new()));

    window.set_block_type_options(ModelRc::from(block_type_options.clone()));
    window.set_block_model_options(ModelRc::from(block_model_options.clone()));
    window.set_filtered_block_model_options(ModelRc::from(filtered_block_model_options.clone()));
    window.set_block_model_option_labels(ModelRc::from(block_model_option_labels.clone()));
    window.set_block_parameter_items(ModelRc::from(block_parameter_items.clone()));
    window.set_multi_slider_points(ModelRc::from(multi_slider_points.clone()));
    window.set_curve_editor_points(ModelRc::from(curve_editor_points.clone()));
    window.set_eq_band_curves(ModelRc::from(eq_band_curves.clone()));

    BlockEditorModels {
        block_type_options,
        block_model_options,
        filtered_block_model_options,
        block_model_option_labels,
        block_parameter_items,
        multi_slider_points,
        curve_editor_points,
        eq_band_curves,
    }
}
