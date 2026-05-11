//! Read-side helpers for `BlockParameterItem` rows.
//!
//! These functions traverse the parameter model to:
//!   - extract per-row metadata (file extensions),
//!   - reconstruct a typed `ParameterSet` validated against the block schema,
//!   - read raw text/option values for internal flow control,
//!   - build a quick float-only `ParameterSet` from a model snapshot.
//!
//! `unit_label` is co-located here as the human-readable formatter for
//! `ParameterUnit`, which is used by parameter-item builders.

use std::rc::Rc;

use anyhow::{anyhow, Result};
use slint::{Model, VecModel};

use project::block::schema_for_block_model;
use project::param::{ParameterSet, ParameterUnit};

use crate::{BlockParameterItem, SELECT_PATH_PREFIX};

pub(crate) fn block_parameter_extensions(
    model: &Rc<VecModel<BlockParameterItem>>,
    path: &str,
) -> Vec<String> {
    for index in 0..model.row_count() {
        if let Some(row) = model.row_data(index) {
            if row.path.as_str() == path {
                let mut values = Vec::new();
                for ext_index in 0..row.file_extensions.row_count() {
                    if let Some(ext) = row.file_extensions.row_data(ext_index) {
                        values.push(ext.to_string());
                    }
                }
                return values;
            }
        }
    }
    Vec::new()
}

pub(crate) fn block_parameter_values(
    model: &Rc<VecModel<BlockParameterItem>>,
    effect_type: &str,
    model_id: &str,
) -> Result<ParameterSet> {
    let schema = schema_for_block_model(effect_type, model_id).map_err(|error| anyhow!(error))?;
    let mut params = ParameterSet::default();
    for index in 0..model.row_count() {
        let Some(row) = model.row_data(index) else {
            continue;
        };
        if row.path.as_str().starts_with(SELECT_PATH_PREFIX) {
            continue;
        }
        let value = match row.widget_kind.as_str() {
            "bool" => domain::value_objects::ParameterValue::Bool(row.bool_value),
            "int" => domain::value_objects::ParameterValue::Int(row.numeric_value.round() as i64),
            "float" => domain::value_objects::ParameterValue::Float(row.numeric_value),
            "slider" => {
                if row.numeric_integer {
                    domain::value_objects::ParameterValue::Int(row.numeric_value.round() as i64)
                } else {
                    domain::value_objects::ParameterValue::Float(row.numeric_value)
                }
            }
            "stepper" => {
                if row.numeric_integer {
                    domain::value_objects::ParameterValue::Int(row.numeric_value.round() as i64)
                } else {
                    domain::value_objects::ParameterValue::Float(row.numeric_value)
                }
            }
            "enum" => {
                if row.selected_option_index < 0 {
                    return Err(anyhow!(
                        "{}",
                        rust_i18n::t!("error-select-option-required-for", label = row.label)
                    ));
                }
                let selected = row
                    .option_values
                    .row_data(row.selected_option_index as usize)
                    .ok_or_else(|| {
                        anyhow!(
                            "{}",
                            rust_i18n::t!("error-select-invalid-for", label = row.label)
                        )
                    })?;
                domain::value_objects::ParameterValue::String(selected.to_string())
            }
            "text" | "path" => {
                let value = row.value_text.to_string();
                if row.optional && value.trim().is_empty() {
                    domain::value_objects::ParameterValue::Null
                } else {
                    domain::value_objects::ParameterValue::String(value)
                }
            }
            // CurveEditor / MultiSlider params use widget_kind="" and store their
            // value in numeric_value — persist as Float.
            "" => domain::value_objects::ParameterValue::Float(row.numeric_value),
            _ => domain::value_objects::ParameterValue::Null,
        };
        params.insert(row.path.as_str(), value);
    }
    params
        .normalized_against(&schema)
        .map_err(|error| anyhow!(error))
}

pub(crate) fn internal_block_parameter_value(
    model: &Rc<VecModel<BlockParameterItem>>,
    path: &str,
) -> Option<String> {
    for index in 0..model.row_count() {
        let Some(row) = model.row_data(index) else {
            continue;
        };
        if row.path.as_str() != path {
            continue;
        }
        if row.selected_option_index >= 0 {
            if let Some(value) = row
                .option_values
                .row_data(row.selected_option_index as usize)
            {
                return Some(value.to_string());
            }
        }
        return Some(row.value_text.to_string());
    }
    None
}

pub(crate) fn build_params_from_items(items: &Rc<VecModel<BlockParameterItem>>) -> ParameterSet {
    let mut params = ParameterSet::default();
    for i in 0..items.row_count() {
        if let Some(item) = items.row_data(i) {
            if !item.path.is_empty() {
                params.insert(
                    item.path.to_string(),
                    domain::value_objects::ParameterValue::Float(item.numeric_value),
                );
            }
        }
    }
    params
}

pub(crate) fn unit_label(unit: &ParameterUnit) -> &'static str {
    match unit {
        ParameterUnit::None => "",
        ParameterUnit::Decibels => "dB",
        ParameterUnit::Hertz => "Hz",
        ParameterUnit::Milliseconds => "ms",
        ParameterUnit::Percent => "%",
        ParameterUnit::Ratio => "Ratio",
        ParameterUnit::Semitones => "st",
    }
}
