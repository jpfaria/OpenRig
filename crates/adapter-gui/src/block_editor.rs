//! Core block-editor primitives shared by the desktop and touch flows:
//! knob-overlay layout, the editor data lift from `AudioBlock`, and the
//! numeric quantization / widget-kind heuristics.
//!
//! Heavier responsibilities are split out:
//!   - `block_editor_param_items` — `Vec<BlockParameterItem>` builders
//!   - `block_editor_setters`     — per-row mutators by parameter path
//!   - `block_editor_values`      — `ParameterSet` + value reads
//!   - `block_editor_persist`     — synchronous + debounced commit flow
//!
//! Each split module is re-exported here so existing call sites
//! (`use crate::block_editor::...`) keep working.

use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind};

use crate::state::{BlockEditorData, SelectOptionEditorItem};
use crate::{BlockKnobOverlay, BlockParameterItem};

pub(crate) use crate::block_editor_param_items::{
    block_parameter_items_for_editor, block_parameter_items_for_model,
};
pub(crate) use crate::block_editor_persist::{
    persist_block_editor_draft, schedule_block_editor_persist,
    schedule_block_editor_persist_for_block_win,
};
pub(crate) use crate::block_editor_setters::{
    set_block_parameter_bool, set_block_parameter_number, set_block_parameter_option,
    set_block_parameter_text,
};
pub(crate) use crate::block_editor_values::{
    block_parameter_extensions, build_params_from_items, internal_block_parameter_value,
};

pub(crate) fn build_knob_overlays(
    knob_layout: &[block_core::KnobLayoutEntry],
    param_items: &[BlockParameterItem],
) -> Vec<BlockKnobOverlay> {
    knob_layout
        .iter()
        .map(|info| {
            let found = param_items
                .iter()
                .find(|p| p.path.as_str() == info.param_key);
            let value = found.map(|p| p.numeric_value).unwrap_or(info.min);
            let label = found
                .map(|p| p.label.to_string().to_uppercase())
                .unwrap_or_else(|| info.param_key.to_uppercase());
            BlockKnobOverlay {
                path: info.param_key.into(),
                label: label.into(),
                svg_cx: info.svg_cx,
                svg_cy: info.svg_cy,
                svg_r: info.svg_r,
                value,
                min_val: info.min,
                max_val: info.max,
                step: info.step,
            }
        })
        .collect()
}

pub(crate) fn block_editor_data(block: &AudioBlock) -> Option<BlockEditorData> {
    block_editor_data_with_selected(block, None)
}

pub(crate) fn block_editor_data_with_selected(
    block: &AudioBlock,
    selected_option_block_id: Option<&str>,
) -> Option<BlockEditorData> {
    match &block.kind {
        AudioBlockKind::Select(select) => {
            let selected = selected_option_block_id
                .and_then(|selected_id| {
                    select
                        .options
                        .iter()
                        .find(|option| option.id.0 == selected_id)
                })
                .or_else(|| select.selected_option())?;
            let model = selected.model_ref()?;
            Some(BlockEditorData {
                effect_type: model.effect_type.to_string(),
                model_id: model.model.to_string(),
                params: model.params.clone(),
                enabled: block.enabled,
                is_select: true,
                select_options: select
                    .options
                    .iter()
                    .filter_map(|option| {
                        let model = option.model_ref()?;
                        let label = schema_for_block_model(model.effect_type, model.model)
                            .map(|schema| schema.display_name)
                            .unwrap_or_else(|_| model.model.to_string());
                        Some(SelectOptionEditorItem {
                            block_id: option.id.0.clone(),
                            label,
                        })
                    })
                    .collect(),
                selected_select_option_block_id: Some(select.selected_block_id.0.clone()),
            })
        }
        _ => block.model_ref().map(|model| BlockEditorData {
            effect_type: model.effect_type.to_string(),
            model_id: model.model.to_string(),
            params: model.params.clone(),
            enabled: block.enabled,
            is_select: false,
            select_options: Vec::new(),
            selected_select_option_block_id: None,
        }),
    }
}

pub(crate) fn quantize_numeric_value(
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    integer: bool,
) -> f32 {
    let mut clamped = value.clamp(min, max);
    if step > 0.0 {
        let snapped_steps = ((clamped - min) / step).round();
        clamped = min + (snapped_steps * step);
        clamped = clamped.clamp(min, max);
    }
    if integer {
        clamped.round()
    } else {
        clamped
    }
}

pub(crate) fn numeric_widget_kind(min: f32, max: f32, step: f32, integer: bool) -> &'static str {
    if step > 0.0 && max > min {
        let steps = ((max - min) / step).round();
        if steps <= 24.0 {
            return "stepper";
        }
    }
    let _ = integer;
    "slider"
}

#[cfg(test)]
#[path = "block_editor_tests.rs"]
mod tests;
