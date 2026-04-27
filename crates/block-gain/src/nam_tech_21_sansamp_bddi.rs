use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_tech_21_sansamp_bddi";
pub const DISPLAY_NAME: &str = "Tech 21 SansAmp BDDI";
const BRAND: &str = "sansamp";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "midfocus_clean", model_path: "pedals/tech_21_sansamp_bddi/sans_midfocus_clean.nam" },
    NamCapture { tone: "midfocus_drive_high", model_path: "pedals/tech_21_sansamp_bddi/sans_midfocus_drive_high.nam" },
    NamCapture { tone: "midfocus_drive_low", model_path: "pedals/tech_21_sansamp_bddi/sans_midfocus_drive_low.nam" },
    NamCapture { tone: "midfocus_drive_mid", model_path: "pedals/tech_21_sansamp_bddi/sans_midfocus_drive_mid.nam" },
    NamCapture { tone: "midfocus_maxed", model_path: "pedals/tech_21_sansamp_bddi/sans_midfocus_maxed.nam" },
    NamCapture { tone: "neutral_clean", model_path: "pedals/tech_21_sansamp_bddi/sans_neutral_clean.nam" },
    NamCapture { tone: "neutral_drive_high", model_path: "pedals/tech_21_sansamp_bddi/sans_neutral_drive_high.nam" },
    NamCapture { tone: "neutral_drive_low", model_path: "pedals/tech_21_sansamp_bddi/sans_neutral_drive_low.nam" },
    NamCapture { tone: "neutral_drive_mid", model_path: "pedals/tech_21_sansamp_bddi/sans_neutral_drive_mid.nam" },
    NamCapture { tone: "neutral_maxed", model_path: "pedals/tech_21_sansamp_bddi/sans_neutral_maxed.nam" },
    NamCapture { tone: "scooped_clean", model_path: "pedals/tech_21_sansamp_bddi/sans_scooped_clean.nam" },
    NamCapture { tone: "scooped_drive_high", model_path: "pedals/tech_21_sansamp_bddi/sans_scooped_drive_high.nam" },
    NamCapture { tone: "scooped_drive_low", model_path: "pedals/tech_21_sansamp_bddi/sans_scooped_drive_low.nam" },
    NamCapture { tone: "scooped_drive_mid", model_path: "pedals/tech_21_sansamp_bddi/sans_scooped_drive_mid.nam" },
    NamCapture { tone: "scooped_maxed", model_path: "pedals/tech_21_sansamp_bddi/sans_scooped_maxed.nam" },
    NamCapture { tone: "amp_high_drive", model_path: "pedals/tech_21_sansamp_bddi/sansamp_high_drive.nam" },
    NamCapture { tone: "amp_low_drive", model_path: "pedals/tech_21_sansamp_bddi/sansamp_low_drive.nam" },
    NamCapture { tone: "amp_max_drive", model_path: "pedals/tech_21_sansamp_bddi/sansamp_max_drive.nam" },
    NamCapture { tone: "amp_max_drive_less_blend", model_path: "pedals/tech_21_sansamp_bddi/sansamp_max_drive_less_blend.nam" },
    NamCapture { tone: "amp_mid_drive", model_path: "pedals/tech_21_sansamp_bddi/sansamp_mid_drive.nam" },
    NamCapture { tone: "amp_mid_drive_less_blend", model_path: "pedals/tech_21_sansamp_bddi/sansamp_mid_drive_less_blend.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("midfocus_clean"),
        &[
            ("midfocus_clean", "Midfocus Clean"),
            ("midfocus_drive_high", "Midfocus Drive High"),
            ("midfocus_drive_low", "Midfocus Drive Low"),
            ("midfocus_drive_mid", "Midfocus Drive Mid"),
            ("midfocus_maxed", "Midfocus Maxed"),
            ("neutral_clean", "Neutral Clean"),
            ("neutral_drive_high", "Neutral Drive High"),
            ("neutral_drive_low", "Neutral Drive Low"),
            ("neutral_drive_mid", "Neutral Drive Mid"),
            ("neutral_maxed", "Neutral Maxed"),
            ("scooped_clean", "Scooped Clean"),
            ("scooped_drive_high", "Scooped Drive High"),
            ("scooped_drive_low", "Scooped Drive Low"),
            ("scooped_drive_mid", "Scooped Drive Mid"),
            ("scooped_maxed", "Scooped Maxed"),
            ("amp_high_drive", "Amp High Drive"),
            ("amp_low_drive", "Amp Low Drive"),
            ("amp_max_drive", "Amp Max Drive"),
            ("amp_max_drive_less_blend", "Amp Max Drive Less Blend"),
            ("amp_mid_drive", "Amp Mid Drive"),
            ("amp_mid_drive_less_blend", "Amp Mid Drive Less Blend"),
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
