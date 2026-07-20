//! The NAM block's parameter schema — the engine controls every NAM has
//! (input / output makeup, noise gate, EQ, and the A2-only slim lever) plus
//! the generic `neural_amp_modeler` loader's file pickers.
//!
//! A spec's `group` IS the tab the block editor renders it in (#786). These
//! controls are identical for every NAM — a plugin package or the generic
//! loader — so the tab split lives here and not in each plugin's manifest;
//! a manifest only declares its own capture axes, which the project layer
//! tags as the "Capture" tab.

use crate::processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS, SLIM_PERCENT_FULL};
use crate::GENERIC_NAM_MODEL_ID;
use block_core::param::{
    bool_parameter, file_path_parameter, float_parameter, ModelParameterSchema, ParameterSpec,
    ParameterUnit,
};
use block_core::ModelAudioMode;
use domain::value_objects::ParameterValue;

/// Editor tab for the NAM engine's level controls (input, output, slim).
pub const AMP_GROUP: &str = "Amp";
/// Editor tab for the noise gate (toggle + threshold).
pub const NOISE_GATE_GROUP: &str = "Noise Gate";
/// Editor tab for the tone stack (toggle + bass / middle / treble).
pub const EQ_GROUP: &str = "EQ";

pub fn model_schema(include_file_params: bool) -> ModelParameterSchema {
    let mut parameters = Vec::new();

    if include_file_params {
        parameters.push(file_path_parameter(
            "model_path",
            "Model",
            None,
            &["nam"],
            false,
        ));
        parameters.push(file_path_parameter(
            "ir_path",
            "Impulse Response",
            Some(ParameterValue::Null),
            &["wav"],
            true,
        ));
    }

    parameters.extend(plugin_parameter_specs());

    ModelParameterSchema {
        effect_type: "nam".to_string(),
        model: GENERIC_NAM_MODEL_ID.to_string(),
        display_name: "Neural Amp Modeler".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters,
    }
}

pub fn plugin_parameter_specs() -> Vec<ParameterSpec> {
    plugin_parameter_specs_with_defaults(DEFAULT_PLUGIN_PARAMS)
}

/// User-facing `slim` knob for A2 SlimmableContainer models (issue #657):
/// a 0..100 % size, where 100 % is the full model and lower values pick a
/// smaller submodel via `SetSlimmableSize` (trading fidelity for CPU).
///
/// Exposed ONLY for NAM/A2 packages — A1 models are not slimmable, so the
/// knob would be inert. The caller (`synthesize_parameters_from_manifest`)
/// appends it based on the manifest's declared architecture.
pub fn slim_parameter_spec() -> ParameterSpec {
    float_parameter(
        "slim",
        "Slim",
        Some(AMP_GROUP),
        Some(SLIM_PERCENT_FULL),
        0.0,
        SLIM_PERCENT_FULL,
        1.0,
        ParameterUnit::Percent,
    )
}

pub fn plugin_parameter_specs_with_defaults(defaults: NamPluginParams) -> Vec<ParameterSpec> {
    vec![
        float_parameter(
            "input_db",
            "Input",
            Some(AMP_GROUP),
            Some(defaults.input_level_db),
            -24.0,
            24.0,
            0.1,
            ParameterUnit::Decibels,
        ),
        float_parameter(
            "output_db",
            "Output",
            Some(AMP_GROUP),
            Some(defaults.output_level_db),
            -24.0,
            24.0,
            0.1,
            ParameterUnit::Decibels,
        ),
        bool_parameter(
            "noise_gate.enabled",
            "Noise Gate",
            Some(NOISE_GATE_GROUP),
            Some(defaults.noise_gate_enabled),
        ),
        float_parameter(
            "noise_gate.threshold_db",
            "Threshold",
            Some(NOISE_GATE_GROUP),
            Some(defaults.noise_gate_threshold_db),
            -96.0,
            0.0,
            0.1,
            ParameterUnit::Decibels,
        ),
        bool_parameter(
            "eq.enabled",
            "EQ",
            Some(EQ_GROUP),
            Some(defaults.eq_enabled),
        ),
        float_parameter(
            "eq.bass",
            "Bass",
            Some(EQ_GROUP),
            Some(defaults.bass),
            0.0,
            10.0,
            0.1,
            ParameterUnit::None,
        ),
        float_parameter(
            "eq.middle",
            "Middle",
            Some(EQ_GROUP),
            Some(defaults.middle),
            0.0,
            10.0,
            0.1,
            ParameterUnit::None,
        ),
        float_parameter(
            "eq.treble",
            "Treble",
            Some(EQ_GROUP),
            Some(defaults.treble),
            0.0,
            10.0,
            0.1,
            ParameterUnit::None,
        ),
    ]
}
