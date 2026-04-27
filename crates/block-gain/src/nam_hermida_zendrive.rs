use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_hermida_zendrive";
pub const DISPLAY_NAME: &str = "Hermida Zendrive";
const BRAND: &str = "zendrive";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "voice1030_gain1030", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1030_gain1030.nam" },
    NamCapture { tone: "voice1030_gain1200", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1030_gain1200.nam" },
    NamCapture { tone: "voice1030_gain130", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1030_gain130.nam" },
    NamCapture { tone: "voice1030_gain300", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1030_gain300.nam" },
    NamCapture { tone: "voice1030_gain500", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1030_gain500.nam" },
    NamCapture { tone: "voice1030_gain700", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1030_gain700.nam" },
    NamCapture { tone: "voice1030_gain900", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1030_gain900.nam" },
    NamCapture { tone: "voice1200_gain1030", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1200_gain1030.nam" },
    NamCapture { tone: "voice1200_gain1200", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1200_gain1200.nam" },
    NamCapture { tone: "voice1200_gain130", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1200_gain130.nam" },
    NamCapture { tone: "voice1200_gain300", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1200_gain300.nam" },
    NamCapture { tone: "voice1200_gain500", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1200_gain500.nam" },
    NamCapture { tone: "voice1200_gain700", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1200_gain700.nam" },
    NamCapture { tone: "voice1200_gain900", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice1200_gain900.nam" },
    NamCapture { tone: "voice130_gain1030", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice130_gain1030.nam" },
    NamCapture { tone: "voice130_gain1200", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice130_gain1200.nam" },
    NamCapture { tone: "voice130_gain1300", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice130_gain1300.nam" },
    NamCapture { tone: "voice130_gain300", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice130_gain300.nam" },
    NamCapture { tone: "voice130_gain500", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice130_gain500.nam" },
    NamCapture { tone: "voice130_gain700", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice130_gain700.nam" },
    NamCapture { tone: "voice130_gain900", model_path: "pedals/hermida_zendrive/zendrive_vol100_tone130_voice130_gain900.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("voice1030_gain1030"),
        &[
            ("voice1030_gain1030", "Voice1030 Gain1030"),
            ("voice1030_gain1200", "Voice1030 Gain1200"),
            ("voice1030_gain130", "Voice1030 Gain130"),
            ("voice1030_gain300", "Voice1030 Gain300"),
            ("voice1030_gain500", "Voice1030 Gain500"),
            ("voice1030_gain700", "Voice1030 Gain700"),
            ("voice1030_gain900", "Voice1030 Gain900"),
            ("voice1200_gain1030", "Voice1200 Gain1030"),
            ("voice1200_gain1200", "Voice1200 Gain1200"),
            ("voice1200_gain130", "Voice1200 Gain130"),
            ("voice1200_gain300", "Voice1200 Gain300"),
            ("voice1200_gain500", "Voice1200 Gain500"),
            ("voice1200_gain700", "Voice1200 Gain700"),
            ("voice1200_gain900", "Voice1200 Gain900"),
            ("voice130_gain1030", "Voice130 Gain1030"),
            ("voice130_gain1200", "Voice130 Gain1200"),
            ("voice130_gain1300", "Voice130 Gain1300"),
            ("voice130_gain300", "Voice130 Gain300"),
            ("voice130_gain500", "Voice130 Gain500"),
            ("voice130_gain700", "Voice130 Gain700"),
            ("voice130_gain900", "Voice130 Gain900"),
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
