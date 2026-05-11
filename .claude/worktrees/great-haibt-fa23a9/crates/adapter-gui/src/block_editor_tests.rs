//! Tests for `adapter-gui::block_editor`. Lifted from `block_editor.rs` so
//! the production file moves toward the size cap. Re-attached via
//! `#[cfg(test)] #[path] mod tests;`.

use super::{
    block_editor_data, block_parameter_items_for_editor, numeric_widget_kind,
    quantize_numeric_value,
};
use crate::SELECT_SELECTED_BLOCK_ID;
use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, SelectBlock};
use project::catalog::supported_block_models;
use project::param::ParameterSet;
use slint::Model;

#[test]
fn quantize_numeric_value_respects_float_step_and_bounds() {
    assert_eq!(quantize_numeric_value(19.64, 0.0, 100.0, 0.5, false), 19.5);
    assert_eq!(quantize_numeric_value(101.0, 0.0, 100.0, 0.5, false), 100.0);
    assert_eq!(quantize_numeric_value(-1.0, 0.0, 100.0, 0.5, false), 0.0);
}

#[test]
fn quantize_numeric_value_respects_integer_step() {
    assert_eq!(
        quantize_numeric_value(243.0, 64.0, 1024.0, 64.0, true),
        256.0
    );
    assert_eq!(
        quantize_numeric_value(96.0, 64.0, 1024.0, 64.0, true),
        128.0
    );
}

#[test]
fn numeric_widget_kind_prefers_stepper_for_sparse_ranges() {
    assert_eq!(numeric_widget_kind(50.0, 70.0, 10.0, false), "stepper");
    assert_eq!(numeric_widget_kind(10.0, 100.0, 10.0, false), "stepper");
}

#[test]
fn numeric_widget_kind_uses_slider_for_dense_ranges() {
    assert_eq!(numeric_widget_kind(0.0, 5.0, 0.01, false), "slider");
    assert_eq!(numeric_widget_kind(1.0, 10.0, 0.1, false), "slider");
}

#[test]
fn numeric_widget_kind_prefers_slider_for_large_ranges() {
    assert_eq!(numeric_widget_kind(0.0, 100.0, 0.5, false), "slider");
    assert_eq!(numeric_widget_kind(20.0, 20000.0, 1.0, false), "slider");
}

#[test]
fn select_block_editor_uses_selected_option_model() {
    let delay_models = delay_model_ids();
    let first_model = delay_models
        .first()
        .expect("delay catalog must not be empty");
    let second_model = delay_models.get(1).unwrap_or(first_model);
    let block = select_delay_block(
        "chain:0:block:0",
        first_model.as_str(),
        second_model.as_str(),
    );
    let editor_data = block_editor_data(&block).expect("select should expose editor data");
    assert!(editor_data.is_select);
    assert_eq!(editor_data.effect_type, "delay");
    assert_eq!(editor_data.model_id, second_model.as_str());
    assert_eq!(editor_data.select_options.len(), 2);
    assert_eq!(
        editor_data.selected_select_option_block_id.as_deref(),
        Some("chain:0:block:0::delay_b")
    );
}

#[test]
fn select_block_editor_includes_active_option_picker() {
    let delay_models = delay_model_ids();
    let first_model = delay_models
        .first()
        .expect("delay catalog must not be empty");
    let second_model = delay_models.get(1).unwrap_or(first_model);
    let block = select_delay_block(
        "chain:0:block:0",
        first_model.as_str(),
        second_model.as_str(),
    );
    let editor_data = block_editor_data(&block).expect("select should expose editor data");
    let items = block_parameter_items_for_editor(&editor_data);
    let selector = items
        .iter()
        .find(|item| item.path.as_str() == SELECT_SELECTED_BLOCK_ID)
        .expect("select editor should expose active option picker");
    assert_eq!(selector.option_values.row_count(), 2);
    assert_eq!(selector.selected_option_index, 1);
}

fn select_delay_block(id: &str, first_model: &str, second_model: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId(format!("{id}::delay_b")),
            options: vec![
                delay_block(format!("{id}::delay_a"), first_model, 120.0),
                delay_block(format!("{id}::delay_b"), second_model, 240.0),
            ],
        }),
    }
}

fn delay_block(id: impl Into<String>, model: &str, time_ms: f32) -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert(
        "time_ms",
        domain::value_objects::ParameterValue::Float(time_ms),
    );
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: model.to_string(),
            params,
        }),
    }
}

fn delay_model_ids() -> Vec<String> {
    supported_block_models("delay")
        .expect("delay catalog should exist")
        .into_iter()
        .map(|entry| entry.model_id)
        .collect()
}

// --- quantize_numeric_value edge cases ---

#[test]
fn quantize_numeric_value_zero_step_only_clamps() {
    assert_eq!(quantize_numeric_value(50.0, 0.0, 100.0, 0.0, false), 50.0);
    assert_eq!(quantize_numeric_value(150.0, 0.0, 100.0, 0.0, false), 100.0);
}

#[test]
fn quantize_numeric_value_exact_boundary_stays() {
    assert_eq!(quantize_numeric_value(0.0, 0.0, 100.0, 10.0, false), 0.0);
    assert_eq!(
        quantize_numeric_value(100.0, 0.0, 100.0, 10.0, false),
        100.0
    );
}

#[test]
fn quantize_numeric_value_integer_flag_rounds() {
    assert_eq!(quantize_numeric_value(3.7, 0.0, 10.0, 0.0, true), 4.0);
    assert_eq!(quantize_numeric_value(3.2, 0.0, 10.0, 0.0, true), 3.0);
}

// --- numeric_widget_kind edge cases ---

#[test]
fn numeric_widget_kind_step_zero_returns_slider() {
    assert_eq!(numeric_widget_kind(0.0, 100.0, 0.0, false), "slider");
}

#[test]
fn numeric_widget_kind_boundary_24_steps_is_stepper() {
    // exactly 24 steps: (24.0 - 0.0) / 1.0 = 24
    assert_eq!(numeric_widget_kind(0.0, 24.0, 1.0, false), "stepper");
}

#[test]
fn numeric_widget_kind_25_steps_is_slider() {
    assert_eq!(numeric_widget_kind(0.0, 25.0, 1.0, false), "slider");
}

#[test]
fn numeric_widget_kind_equal_min_max_returns_slider() {
    assert_eq!(numeric_widget_kind(5.0, 5.0, 1.0, false), "slider");
}
