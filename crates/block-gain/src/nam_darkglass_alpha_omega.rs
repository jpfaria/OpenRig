use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "darkglass_alpha_omega";
pub const DISPLAY_NAME: &str = "Alpha Omega Ultra";
const BRAND: &str = "darkglass";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlphaOmegaParams {
    pub channel: &'static str,
    pub gain: &'static str,
}

struct AlphaOmegaCapture {
    params: AlphaOmegaParams,
    model_path: &'static str,
}

const CAPTURES: &[AlphaOmegaCapture] = &[
    AlphaOmegaCapture { params: AlphaOmegaParams { channel: "alpha", gain: "2"  }, model_path: "pedals/darkglass_alpha_omega/alpha_omega_alpha_g2.nam" },
    AlphaOmegaCapture { params: AlphaOmegaParams { channel: "alpha", gain: "5"  }, model_path: "pedals/darkglass_alpha_omega/alpha_omega_alpha_g5.nam" },
    AlphaOmegaCapture { params: AlphaOmegaParams { channel: "alpha", gain: "8"  }, model_path: "pedals/darkglass_alpha_omega/alpha_omega_alpha_g8.nam" },
    AlphaOmegaCapture { params: AlphaOmegaParams { channel: "alpha", gain: "10" }, model_path: "pedals/darkglass_alpha_omega/alpha_omega_alpha_g10.nam" },
    AlphaOmegaCapture { params: AlphaOmegaParams { channel: "omega", gain: "2"  }, model_path: "pedals/darkglass_alpha_omega/alpha_omega_omega_g2.nam" },
    AlphaOmegaCapture { params: AlphaOmegaParams { channel: "omega", gain: "5"  }, model_path: "pedals/darkglass_alpha_omega/alpha_omega_omega_g5.nam" },
    AlphaOmegaCapture { params: AlphaOmegaParams { channel: "omega", gain: "8"  }, model_path: "pedals/darkglass_alpha_omega/alpha_omega_omega_g8.nam" },
    AlphaOmegaCapture { params: AlphaOmegaParams { channel: "omega", gain: "10" }, model_path: "pedals/darkglass_alpha_omega/alpha_omega_omega_g10.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Pedal"),
            Some("omega"),
            &[("alpha", "Alpha"), ("omega", "Omega")],
        ),
        enum_parameter(
            "gain",
            "Gain",
            Some("Pedal"),
            Some("5"),
            &[("2", "2"), ("5", "5"), ("8", "8"), ("10", "10")],
        ),
    ];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let capture = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(capture.model_path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("model='{}'", capture.model_path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static AlphaOmegaCapture> {
    let channel = required_string(params, "channel").map_err(anyhow::Error::msg)?;
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.params.channel == channel && c.params.gain == gain)
        .ok_or_else(|| {
            anyhow!(
                "gain model '{}' does not support channel='{}' gain='{}'",
                MODEL_ID, channel, gain
            )
        })
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
