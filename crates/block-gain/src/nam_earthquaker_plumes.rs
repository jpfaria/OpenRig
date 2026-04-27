use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_earthquaker_plumes";
pub const DISPLAY_NAME: &str = "EarthQuaker Plumes";
const BRAND: &str = "earthquaker_devices";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "100_gain_0_tone_0", model_path: "pedals/earthquaker_plumes/plumes_switch_3_level_100_gain_0_tone_0.nam" },
    NamCapture { tone: "100_gain_0_tone_100", model_path: "pedals/earthquaker_plumes/plumes_switch_3_level_100_gain_0_tone_100.nam" },
    NamCapture { tone: "100_gain_0_tone_25", model_path: "pedals/earthquaker_plumes/plumes_switch_3_level_100_gain_0_tone_25.nam" },
    NamCapture { tone: "100_gain_0_tone_50", model_path: "pedals/earthquaker_plumes/plumes_switch_3_level_100_gain_0_tone_50.nam" },
    NamCapture { tone: "100_gain_0_tone_75", model_path: "pedals/earthquaker_plumes/plumes_switch_3_level_100_gain_0_tone_75.nam" },
    NamCapture { tone: "25_gain_75_tone_0", model_path: "pedals/earthquaker_plumes/plumes_switch_3_level_25_gain_75_tone_0.nam" },
    NamCapture { tone: "25_gain_75_tone_100", model_path: "pedals/earthquaker_plumes/plumes_switch_3_level_25_gain_75_tone_100.nam" },
    NamCapture { tone: "25_gain_75_tone_25", model_path: "pedals/earthquaker_plumes/plumes_switch_3_level_25_gain_75_tone_25.nam" },
    NamCapture { tone: "25_gain_75_tone_50", model_path: "pedals/earthquaker_plumes/plumes_switch_3_level_25_gain_75_tone_50.nam" },
    NamCapture { tone: "25_gain_75_tone_75", model_path: "pedals/earthquaker_plumes/plumes_switch_3_level_25_gain_75_tone_75.nam" },
    NamCapture { tone: "50_gain_50_tone_0", model_path: "pedals/earthquaker_plumes/plumes_switch_3_level_50_gain_50_tone_0.nam" },
    NamCapture { tone: "50_gain_50_tone_100", model_path: "pedals/earthquaker_plumes/plumes_switch_3_level_50_gain_50_tone_100.nam" },
    NamCapture { tone: "50_gain_50_tone_25", model_path: "pedals/earthquaker_plumes/plumes_switch_3_level_50_gain_50_tone_25.nam" },
    NamCapture { tone: "50_gain_50_tone_50", model_path: "pedals/earthquaker_plumes/plumes_switch_3_level_50_gain_50_tone_50.nam" },
    NamCapture { tone: "50_gain_50_tone_75", model_path: "pedals/earthquaker_plumes/plumes_switch_3_level_50_gain_50_tone_75.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("100_gain_0_tone_0"),
        &[
            ("100_gain_0_tone_0", "100 Gain 0 Tone 0"),
            ("100_gain_0_tone_100", "100 Gain 0 Tone 100"),
            ("100_gain_0_tone_25", "100 Gain 0 Tone 25"),
            ("100_gain_0_tone_50", "100 Gain 0 Tone 50"),
            ("100_gain_0_tone_75", "100 Gain 0 Tone 75"),
            ("25_gain_75_tone_0", "25 Gain 75 Tone 0"),
            ("25_gain_75_tone_100", "25 Gain 75 Tone 100"),
            ("25_gain_75_tone_25", "25 Gain 75 Tone 25"),
            ("25_gain_75_tone_50", "25 Gain 75 Tone 50"),
            ("25_gain_75_tone_75", "25 Gain 75 Tone 75"),
            ("50_gain_50_tone_0", "50 Gain 50 Tone 0"),
            ("50_gain_50_tone_100", "50 Gain 50 Tone 100"),
            ("50_gain_50_tone_25", "50 Gain 50 Tone 25"),
            ("50_gain_50_tone_50", "50 Gain 50 Tone 50"),
            ("50_gain_50_tone_75", "50 Gain 50 Tone 75"),
        ],
    )];
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static NamCapture> {
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.tone == tone)
        .ok_or_else(|| anyhow!("gain model '{}' does not support tone='{}'", MODEL_ID, tone))
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
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
