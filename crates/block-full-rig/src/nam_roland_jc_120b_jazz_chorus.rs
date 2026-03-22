use anyhow::{anyhow, Result};
use crate::registry::FullRigModelDefinition;
use crate::FullRigBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{bool_parameter, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "roland_jc_120b_jazz_chorus";
pub const DISPLAY_NAME: &str = "JC-120B Jazz Chorus";

pub const NAM_PLUGIN_DEFAULTS: NamPluginParams = NamPluginParams {
    input_level_db: 0.0,
    output_level_db: 0.0,
    noise_gate_threshold_db: -80.0,
    noise_gate_enabled: true,
    eq_enabled: true,
    bass: 5.0,
    middle: 5.0,
    treble: 5.0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RolandParams {
    pub bright_enabled: bool,
    pub royer_101_enabled: bool,
    pub sm57_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RolandCapture {
    pub params: RolandParams,
    pub model_path: &'static str,
}

pub const CAPTURES: &[RolandCapture] = &[
    capture(
        false,
        true,
        false,
        "captures/nam/full_rigs/roland_jc_120b_jazz_chorus/roland_jc_120b_jazz_chorus_bright_off_royer_101.nam",
    ),
    capture(
        false,
        false,
        true,
        "captures/nam/full_rigs/roland_jc_120b_jazz_chorus/roland_jc_120b_jazz_chorus_bright_off_sm57.nam",
    ),
    capture(
        false,
        true,
        true,
        "captures/nam/full_rigs/roland_jc_120b_jazz_chorus/roland_jc_120b_jazz_chorus_bright_off_sm57_and_royer_101.nam",
    ),
    capture(
        true,
        true,
        false,
        "captures/nam/full_rigs/roland_jc_120b_jazz_chorus/roland_jc_120b_jazz_chorus_bright_on_royer_r_101.nam",
    ),
    capture(
        true,
        false,
        true,
        "captures/nam/full_rigs/roland_jc_120b_jazz_chorus/roland_jc_120b_jazz_chorus_bright_on_sm57.nam",
    ),
    capture(
        true,
        true,
        true,
        "captures/nam/full_rigs/roland_jc_120b_jazz_chorus/roland_jc_120b_jazz_chorus_bright_on_royer_r_101_and_sm57.nam",
    ),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("full_rig", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        bool_parameter("bright_enabled", "Bright", Some("Rig"), Some(false)),
        bool_parameter("royer_101_enabled", "Royer 101", Some("Rig"), Some(true)),
        bool_parameter("sm57_enabled", "SM57", Some("Rig"), Some(false)),
    ];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let capture = resolve_capture(params)?;
    let plugin_params = plugin_params_from_set_with_defaults(params, NAM_PLUGIN_DEFAULTS)?;
    build_processor_with_assets_for_layout(capture.model_path, None, plugin_params, layout)
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("model='{}'", capture.model_path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static RolandCapture> {
    let requested = RolandParams {
        bright_enabled: params.get_bool("bright_enabled").unwrap_or(false),
        royer_101_enabled: params.get_bool("royer_101_enabled").unwrap_or(true),
        sm57_enabled: params.get_bool("sm57_enabled").unwrap_or(false),
    };

    CAPTURES
        .iter()
        .find(|capture| capture.params == requested)
        .or_else(|| CAPTURES.first())
        .ok_or_else(|| anyhow!("no captures available for model '{}'", MODEL_ID))
}

const fn capture(
    bright_enabled: bool,
    royer_101_enabled: bool,
    sm57_enabled: bool,
    model_path: &'static str,
) -> RolandCapture {
    RolandCapture {
        params: RolandParams {
            bright_enabled,
            royer_101_enabled,
            sm57_enabled,
        },
        model_path,
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    build_processor_for_model(params, layout)
}

pub const MODEL_DEFINITION: FullRigModelDefinition = FullRigModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "roland",
    backend_kind: FullRigBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
};
