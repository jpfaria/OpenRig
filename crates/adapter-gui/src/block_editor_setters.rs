//! Mutators for individual `BlockParameterItem` rows by parameter path.
//!
//! Each setter walks the `VecModel<BlockParameterItem>` until it finds the
//! row whose `path` matches, mutates the relevant field, and writes the row
//! back. `set_block_parameter_number` also quantizes against the spec's
//! min/max/step/integer constraints before formatting `value_text`.

use std::rc::Rc;

use slint::{Model, VecModel};

use crate::block_editor::quantize_numeric_value;
use crate::BlockParameterItem;

pub(crate) fn set_block_parameter_text(
    model: &Rc<VecModel<BlockParameterItem>>,
    path: &str,
    value: &str,
) {
    for index in 0..model.row_count() {
        if let Some(mut row) = model.row_data(index) {
            if row.path.as_str() == path {
                row.value_text = value.into();
                model.set_row_data(index, row);
                break;
            }
        }
    }
}

pub(crate) fn set_block_parameter_bool(
    model: &Rc<VecModel<BlockParameterItem>>,
    path: &str,
    value: bool,
) {
    for index in 0..model.row_count() {
        if let Some(mut row) = model.row_data(index) {
            if row.path.as_str() == path {
                row.bool_value = value;
                model.set_row_data(index, row);
                break;
            }
        }
    }
}

pub(crate) fn set_block_parameter_number(
    model: &Rc<VecModel<BlockParameterItem>>,
    path: &str,
    value: f32,
) {
    for index in 0..model.row_count() {
        if let Some(mut row) = model.row_data(index) {
            if row.path.as_str() == path {
                let quantized = quantize_numeric_value(
                    value,
                    row.numeric_min,
                    row.numeric_max,
                    row.numeric_step,
                    row.numeric_integer,
                );
                row.numeric_value = quantized;
                row.value_text = if row.numeric_integer {
                    format!("{:.0}", quantized.round()).into()
                } else {
                    format!("{quantized:.2}").into()
                };
                model.set_row_data(index, row);
                break;
            }
        }
    }
}

pub(crate) fn set_block_parameter_option(
    model: &Rc<VecModel<BlockParameterItem>>,
    path: &str,
    selected_index: i32,
) {
    for index in 0..model.row_count() {
        if let Some(mut row) = model.row_data(index) {
            if row.path.as_str() == path {
                row.selected_option_index = selected_index;
                if selected_index >= 0 {
                    if let Some(value) = row.option_values.row_data(selected_index as usize) {
                        row.value_text = value;
                    }
                }
                model.set_row_data(index, row);
                break;
            }
        }
    }
}
