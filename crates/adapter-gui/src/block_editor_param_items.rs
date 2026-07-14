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

/// Fallback tab label for parameters that declare no group.
pub(crate) const DEFAULT_PARAM_GROUP: &str = "Main";

/// Ordered, de-duplicated tab labels across `items`, in first-appearance
/// order. Parameters with an empty group collapse under [`DEFAULT_PARAM_GROUP`].
/// One label (or none) means the block needs no tab bar (#780).
pub(crate) fn parameter_groups(items: &[BlockParameterItem]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for it in items {
        let g = it.group.as_str();
        let label = if g.is_empty() { DEFAULT_PARAM_GROUP } else { g };
        if !out.iter().any(|existing| existing == label) {
            out.push(label.to_string());
        }
    }
    out
}

// Per-tab filtering now lives in `block_editor_param_tabs::retag_for_group`,
// which tags each row's `tab_slot` instead of dropping rows — the model must
// stay full so a save never loses a non-active tab's params (#780).

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
            tab_slot: 0,
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
                tab_slot: 0,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(label: &str, group: &str) -> BlockParameterItem {
        BlockParameterItem {
            label: label.into(),
            group: group.into(),
            ..Default::default()
        }
    }

    #[test]
    fn groups_are_distinct_first_appearance_with_default_fallback() {
        let items = vec![
            item("Gain", "Tone"),
            item("Level", "Tone"),
            item("Mode", "Voicing"),
            item("Mix", ""), // ungrouped → Main
        ];
        assert_eq!(
            parameter_groups(&items),
            vec![
                "Tone".to_string(),
                "Voicing".to_string(),
                "Main".to_string()
            ]
        );
    }

    #[test]
    fn vst3_block_yields_params_via_the_compact_build_path() {
        // #780 repro: the compact view showed a VST3 block with zero params.
        // This drives the SAME build the compact view uses — build the block,
        // resolve its editor data, build the param items — with a real plugin.
        use std::path::PathBuf;
        let Some(dir) = std::env::var_os("OPENRIG_TEST_VST3_DIR").map(PathBuf::from) else {
            return;
        };
        project::vst3_editor::init_vst3_catalog(48_000.0, &[dir]);
        let Some(model) = project::catalog::supported_block_models(block_core::EFFECT_TYPE_VST3)
            .ok()
            .and_then(|models| {
                models
                    .into_iter()
                    .find(|m| m.model_id.to_lowercase().contains("chowcentaur"))
                    .map(|m| m.model_id)
            })
        else {
            return;
        };
        let kind = project::block::build_audio_block_kind(
            block_core::EFFECT_TYPE_VST3,
            &model,
            project::param::ParameterSet::default(),
        )
        .expect("build vst3 block kind");
        let block = project::block::AudioBlock {
            id: domain::ids::BlockId("v1".into()),
            enabled: true,
            kind,
        };
        let data =
            crate::block_editor::block_editor_data(&block).expect("editor data for vst3 block");
        let params = block_parameter_items_for_editor(&data);
        assert!(
            !params.is_empty(),
            "compact build path produced ZERO params for VST3 model {}",
            model
        );
    }
}
