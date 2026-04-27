use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_ibanez_tube_screamer";
pub const DISPLAY_NAME: &str = "Ibanez Tube Screamer";
const BRAND: &str = "ibanez";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "d_00_t_05_l_10", model_path: "pedals/ibanez_tube_screamer/d_00_t_05_l_10.nam" },
    NamCapture { tone: "d_00_t_06_l_08", model_path: "pedals/ibanez_tube_screamer/d_00_t_06_l_08.nam" },
    NamCapture { tone: "d_00_t_08_l_10", model_path: "pedals/ibanez_tube_screamer/d_00_t_08_l_10.nam" },
    NamCapture { tone: "d_00_t_10_l_10", model_path: "pedals/ibanez_tube_screamer/d_00_t_10_l_10.nam" },
    NamCapture { tone: "d_02_t_08_l_08", model_path: "pedals/ibanez_tube_screamer/d_02_t_08_l_08.nam" },
    NamCapture { tone: "d_02_t_10_l_08", model_path: "pedals/ibanez_tube_screamer/d_02_t_10_l_08.nam" },
    NamCapture { tone: "d_05_t_08_l_05", model_path: "pedals/ibanez_tube_screamer/d_05_t_08_l_05.nam" },
    NamCapture { tone: "d_05_t_10_l_05", model_path: "pedals/ibanez_tube_screamer/d_05_t_10_l_05.nam" },
    NamCapture { tone: "d_10_t_10_l_10", model_path: "pedals/ibanez_tube_screamer/d_10_t_10_l_10.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("d_00_t_05_l_10"),
        &[
            ("d_00_t_05_l_10", "D 00 T 05 L 10"),
            ("d_00_t_06_l_08", "D 00 T 06 L 08"),
            ("d_00_t_08_l_10", "D 00 T 08 L 10"),
            ("d_00_t_10_l_10", "D 00 T 10 L 10"),
            ("d_02_t_08_l_08", "D 02 T 08 L 08"),
            ("d_02_t_10_l_08", "D 02 T 10 L 08"),
            ("d_05_t_08_l_05", "D 05 T 08 L 05"),
            ("d_05_t_10_l_05", "D 05 T 10 L 05"),
            ("d_10_t_10_l_10", "D 10 T 10 L 10"),
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
