//! Builders for `Vec<BlockParameterItem>` — the rows that drive the block
//! editor's parameter grid in Slint.
//!
//! `block_parameter_items_for_editor` reads the editor's `BlockEditorData`
//! (which already resolved Select-block options) and prepends a synthetic
//! `enum` row for the active select option when applicable, then appends
//! the regular schema-driven rows from `block_parameter_items_for_model`.
//!
//! `block_parameter_items_for_model` materializes one row per parameter
//! spec: numeric range / enum option / file-path metadata / widget kind
//! (stepper vs slider) / unit text / current value text.

use std::rc::Rc;

use slint::{ModelRc, SharedString, VecModel};

use project::block::schema_for_block_model;
use project::param::{ParameterDomain, ParameterSet, ParameterWidget};

use crate::block_editor::numeric_widget_kind;
use crate::block_editor_values::unit_label;
use crate::state::BlockEditorData;
use crate::{BlockParameterItem, SELECT_SELECTED_BLOCK_ID};

pub(crate) fn block_parameter_items_for_editor(data: &BlockEditorData) -> Vec<BlockParameterItem> {
    let mut items = Vec::new();
    if !data.select_options.is_empty() {
        let option_labels = data
            .select_options
            .iter()
            .map(|option| SharedString::from(option.label.as_str()))
            .collect::<Vec<_>>();
        let option_values = data
            .select_options
            .iter()
            .map(|option| SharedString::from(option.block_id.as_str()))
            .collect::<Vec<_>>();
        let selected_option_index = data
            .selected_select_option_block_id
            .as_ref()
            .and_then(|selected| {
                data.select_options
                    .iter()
                    .position(|option| &option.block_id == selected)
            })
            .map(|index| index as i32)
            .unwrap_or(0);
        items.push(BlockParameterItem {
            path: SELECT_SELECTED_BLOCK_ID.into(),
            label: "Modelo ativo".into(),
            group: "Select".into(),
            widget_kind: "enum".into(),
            unit_text: "".into(),
            value_text: data
                .selected_select_option_block_id
                .clone()
                .unwrap_or_default()
                .into(),
            numeric_value: 0.0,
            numeric_min: 0.0,
            numeric_max: 1.0,
            numeric_step: 0.0,
            numeric_integer: false,
            bool_value: false,
            selected_option_index,
            option_labels: ModelRc::from(Rc::new(VecModel::from(option_labels))),
            option_values: ModelRc::from(Rc::new(VecModel::from(option_values))),
            file_extensions: ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
            optional: false,
            allow_empty: false,
        });
    }
    items.extend(block_parameter_items_for_model(
        &data.effect_type,
        &data.model_id,
        &data.params,
    ));
    items
}

pub(crate) fn block_parameter_items_for_model(
    effect_type: &str,
    model_id: &str,
    params: &ParameterSet,
) -> Vec<BlockParameterItem> {
    let Ok(schema) = schema_for_block_model(effect_type, model_id) else {
        return Vec::new();
    };
    schema
        .parameters
        .iter()
        .filter(|spec| spec.path != "enabled")
        .map(|spec| {
            let current = params
                .get(&spec.path)
                .cloned()
                .or_else(|| spec.default_value.clone())
                .unwrap_or(domain::value_objects::ParameterValue::Null);
            let (numeric_value, numeric_min, numeric_max, numeric_step) = match &spec.domain {
                ParameterDomain::IntRange { min, max, .. } => (
                    current.as_i64().unwrap_or(*min) as f32,
                    *min as f32,
                    *max as f32,
                    match &spec.domain {
                        ParameterDomain::IntRange { step, .. } => *step as f32,
                        _ => 1.0,
                    },
                ),
                ParameterDomain::FloatRange { min, max, .. } => (
                    current.as_f32().unwrap_or(*min),
                    *min,
                    *max,
                    match &spec.domain {
                        ParameterDomain::FloatRange { step, .. } => *step,
                        _ => 0.0,
                    },
                ),
                _ => (0.0, 0.0, 1.0, 0.0),
            };
            let (option_labels, option_values, selected_option_index, file_extensions) = match &spec
                .domain
            {
                ParameterDomain::Enum { options } => {
                    let labels = options
                        .iter()
                        .map(|option| SharedString::from(option.label.as_str()))
                        .collect::<Vec<_>>();
                    let values = options
                        .iter()
                        .map(|option| SharedString::from(option.value.as_str()))
                        .collect::<Vec<_>>();
                    let selected = current
                        .as_str()
                        .and_then(|value| options.iter().position(|option| option.value == value))
                        .map(|index| index as i32)
                        .unwrap_or(0);
                    (
                        ModelRc::from(Rc::new(VecModel::from(labels))),
                        ModelRc::from(Rc::new(VecModel::from(values))),
                        selected,
                        ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                    )
                }
                ParameterDomain::FilePath { extensions } => {
                    let values = extensions
                        .iter()
                        .map(|value| SharedString::from(value.as_str()))
                        .collect::<Vec<_>>();
                    (
                        ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                        ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                        -1,
                        ModelRc::from(Rc::new(VecModel::from(values))),
                    )
                }
                _ => (
                    ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                    ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                    -1,
                    ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                ),
            };
            BlockParameterItem {
                path: spec.path.clone().into(),
                label: spec.label.to_uppercase().into(),
                group: spec.group.clone().unwrap_or_default().into(),
                widget_kind: match &spec.widget {
                    ParameterWidget::MultiSlider | ParameterWidget::CurveEditor { .. } => "",
                    _ => match &spec.domain {
                        ParameterDomain::Bool => "bool",
                        ParameterDomain::IntRange { min, max, step } => {
                            numeric_widget_kind(*min as f32, *max as f32, *step as f32, true)
                        }
                        ParameterDomain::FloatRange { min, max, step } => {
                            numeric_widget_kind(*min, *max, *step, false)
                        }
                        ParameterDomain::Enum { .. } => "enum",
                        ParameterDomain::Text => "text",
                        ParameterDomain::FilePath { .. } => "path",
                    },
                }
                .into(),
                unit_text: unit_label(&spec.unit).into(),
                value_text: match current {
                    domain::value_objects::ParameterValue::String(ref value) => {
                        value.clone().into()
                    }
                    domain::value_objects::ParameterValue::Int(value) => value.to_string().into(),
                    domain::value_objects::ParameterValue::Float(value) => {
                        format!("{value:.2}").into()
                    }
                    domain::value_objects::ParameterValue::Bool(value) => {
                        if value {
                            "true".into()
                        } else {
                            "false".into()
                        }
                    }
                    domain::value_objects::ParameterValue::Null => "".into(),
                },
                numeric_value,
                numeric_min,
                numeric_max,
                numeric_step,
                numeric_integer: matches!(&spec.domain, ParameterDomain::IntRange { .. }),
                bool_value: current.as_bool().unwrap_or(false),
                selected_option_index,
                option_labels,
                option_values,
                file_extensions,
                optional: spec.optional,
                allow_empty: spec.allow_empty,
            }
        })
        .collect()
}
