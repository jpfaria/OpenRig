use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_earthquaker_hizumitas";
pub const DISPLAY_NAME: &str = "EarthQuaker Hizumitas";
const BRAND: &str = "earthquaker";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "10_hizumitas_vol_5_sustain_0_tone_5",   model_path: "pedals/earthquaker_hizumitas/10_hizumitas_vol_5_sustain_0_tone_5.nam" },
    NamCapture { tone: "11_hizumitas_vol_5_sustain_3_tone_5",   model_path: "pedals/earthquaker_hizumitas/11_hizumitas_vol_5_sustain_3_tone_5.nam" },
    NamCapture { tone: "12_hizumitas_vol_5_sustain_5_tone_5",   model_path: "pedals/earthquaker_hizumitas/12_hizumitas_vol_5_sustain_5_tone_5.nam" },
    NamCapture { tone: "13_hizumitas_vol_5_sustain_7_tone_5",   model_path: "pedals/earthquaker_hizumitas/13_hizumitas_vol_5_sustain_7_tone_5.nam" },
    NamCapture { tone: "14_hizumitas_vol_5_sustain_10_tone_5",  model_path: "pedals/earthquaker_hizumitas/14_hizumitas_vol_5_sustain_10_tone_5.nam" },
    NamCapture { tone: "15_hizumitas_vol_5_sustain_0_tone_7",   model_path: "pedals/earthquaker_hizumitas/15_hizumitas_vol_5_sustain_0_tone_7.nam" },
    NamCapture { tone: "16_hizumitas_vol_5_sustain_3_tone_7",   model_path: "pedals/earthquaker_hizumitas/16_hizumitas_vol_5_sustain_3_tone_7.nam" },
    NamCapture { tone: "17_hizumitas_vol_5_sustain_5_tone_7",   model_path: "pedals/earthquaker_hizumitas/17_hizumitas_vol_5_sustain_5_tone_7.nam" },
    NamCapture { tone: "18_hizumitas_vol_5_sustain_7_tone_7",   model_path: "pedals/earthquaker_hizumitas/18_hizumitas_vol_5_sustain_7_tone_7.nam" },
    NamCapture { tone: "19_hizumitas_vol_5_sustain_10_tone_7",  model_path: "pedals/earthquaker_hizumitas/19_hizumitas_vol_5_sustain_10_tone_7.nam" },
    NamCapture { tone: "1_hizumitas_vol_5_sustain_0_tone_0",    model_path: "pedals/earthquaker_hizumitas/1_hizumitas_vol_5_sustain_0_tone_0.nam" },
    NamCapture { tone: "20_hizumitas_vol_5_sustain_0_tone_10",  model_path: "pedals/earthquaker_hizumitas/20_hizumitas_vol_5_sustain_0_tone_10.nam" },
    NamCapture { tone: "21_hizumitas_vol_5_sustain_3_tone_10",  model_path: "pedals/earthquaker_hizumitas/21_hizumitas_vol_5_sustain_3_tone_10.nam" },
    NamCapture { tone: "22_hizumitas_vol_5_sustain_5_tone_10",  model_path: "pedals/earthquaker_hizumitas/22_hizumitas_vol_5_sustain_5_tone_10.nam" },
    NamCapture { tone: "23_hizumitas_vol_5_sustain_7_tone_10",  model_path: "pedals/earthquaker_hizumitas/23_hizumitas_vol_5_sustain_7_tone_10.nam" },
    NamCapture { tone: "24_hizumitas_vol_5_sustain_10_tone_10", model_path: "pedals/earthquaker_hizumitas/24_hizumitas_vol_5_sustain_10_tone_10.nam" },
    NamCapture { tone: "2_hizumitas_vol_5_sustain_3_tone_0",    model_path: "pedals/earthquaker_hizumitas/2_hizumitas_vol_5_sustain_3_tone_0.nam" },
    NamCapture { tone: "3_hizumitas_vol_5_sustain_5_tone_0",    model_path: "pedals/earthquaker_hizumitas/3_hizumitas_vol_5_sustain_5_tone_0.nam" },
    NamCapture { tone: "4_hizumitas_vol_5_sustain_7_tone_0",    model_path: "pedals/earthquaker_hizumitas/4_hizumitas_vol_5_sustain_7_tone_0.nam" },
    NamCapture { tone: "5_hizumitas_vol_5_sustain_10_tone_0",   model_path: "pedals/earthquaker_hizumitas/5_hizumitas_vol_5_sustain_10_tone_0.nam" },
    NamCapture { tone: "6_hizumitas_vol_5_sustain_3_tone_3",    model_path: "pedals/earthquaker_hizumitas/6_hizumitas_vol_5_sustain_3_tone_3.nam" },
    NamCapture { tone: "7_hizumitas_vol_5_sustain_5_tone_3",    model_path: "pedals/earthquaker_hizumitas/7_hizumitas_vol_5_sustain_5_tone_3.nam" },
    NamCapture { tone: "8_hizumitas_vol_5_sustain_7_tone_3",    model_path: "pedals/earthquaker_hizumitas/8_hizumitas_vol_5_sustain_7_tone_3.nam" },
    NamCapture { tone: "9_hizumitas_vol_5_sustain_10_tone_3",   model_path: "pedals/earthquaker_hizumitas/9_hizumitas_vol_5_sustain_10_tone_3.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("10_hizumitas_vol_5_sustain_0_tone_5"),
        &[
            ("10_hizumitas_vol_5_sustain_0_tone_5",   "10 Hizumitas Vol 5 Sustain 0 Tone 5"),
            ("11_hizumitas_vol_5_sustain_3_tone_5",   "11 Hizumitas Vol 5 Sustain 3 Tone 5"),
            ("12_hizumitas_vol_5_sustain_5_tone_5",   "12 Hizumitas Vol 5 Sustain 5 Tone 5"),
            ("13_hizumitas_vol_5_sustain_7_tone_5",   "13 Hizumitas Vol 5 Sustain 7 Tone 5"),
            ("14_hizumitas_vol_5_sustain_10_tone_5",  "14 Hizumitas Vol 5 Sustain 10 Tone 5"),
            ("15_hizumitas_vol_5_sustain_0_tone_7",   "15 Hizumitas Vol 5 Sustain 0 Tone 7"),
            ("16_hizumitas_vol_5_sustain_3_tone_7",   "16 Hizumitas Vol 5 Sustain 3 Tone 7"),
            ("17_hizumitas_vol_5_sustain_5_tone_7",   "17 Hizumitas Vol 5 Sustain 5 Tone 7"),
            ("18_hizumitas_vol_5_sustain_7_tone_7",   "18 Hizumitas Vol 5 Sustain 7 Tone 7"),
            ("19_hizumitas_vol_5_sustain_10_tone_7",  "19 Hizumitas Vol 5 Sustain 10 Tone 7"),
            ("1_hizumitas_vol_5_sustain_0_tone_0",    "1 Hizumitas Vol 5 Sustain 0 Tone 0"),
            ("20_hizumitas_vol_5_sustain_0_tone_10",  "20 Hizumitas Vol 5 Sustain 0 Tone 10"),
            ("21_hizumitas_vol_5_sustain_3_tone_10",  "21 Hizumitas Vol 5 Sustain 3 Tone 10"),
            ("22_hizumitas_vol_5_sustain_5_tone_10",  "22 Hizumitas Vol 5 Sustain 5 Tone 10"),
            ("23_hizumitas_vol_5_sustain_7_tone_10",  "23 Hizumitas Vol 5 Sustain 7 Tone 10"),
            ("24_hizumitas_vol_5_sustain_10_tone_10", "24 Hizumitas Vol 5 Sustain 10 Tone 10"),
            ("2_hizumitas_vol_5_sustain_3_tone_0",    "2 Hizumitas Vol 5 Sustain 3 Tone 0"),
            ("3_hizumitas_vol_5_sustain_5_tone_0",    "3 Hizumitas Vol 5 Sustain 5 Tone 0"),
            ("4_hizumitas_vol_5_sustain_7_tone_0",    "4 Hizumitas Vol 5 Sustain 7 Tone 0"),
            ("5_hizumitas_vol_5_sustain_10_tone_0",   "5 Hizumitas Vol 5 Sustain 10 Tone 0"),
            ("6_hizumitas_vol_5_sustain_3_tone_3",    "6 Hizumitas Vol 5 Sustain 3 Tone 3"),
            ("7_hizumitas_vol_5_sustain_5_tone_3",    "7 Hizumitas Vol 5 Sustain 5 Tone 3"),
            ("8_hizumitas_vol_5_sustain_7_tone_3",    "8 Hizumitas Vol 5 Sustain 7 Tone 3"),
            ("9_hizumitas_vol_5_sustain_10_tone_3",   "9 Hizumitas Vol 5 Sustain 10 Tone 3"),
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
