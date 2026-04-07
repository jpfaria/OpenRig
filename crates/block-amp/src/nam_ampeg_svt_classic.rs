use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};

use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "ampeg_svt_classic";
pub const DISPLAY_NAME: &str = "SVT Classic";
const BRAND: &str = "ampeg";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AmpegSvtParams {
    pub tone: &'static str,
    pub mic: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AmpegSvtCapture {
    pub params: AmpegSvtParams,
    pub model_path: &'static str,
}

pub const CAPTURES: &[AmpegSvtCapture] = &[
    capture("standard", "md421", "full_rigs/ampeg_svt_classic/ampeg_svt_md421.nam"),
    capture("standard", "sm57",  "full_rigs/ampeg_svt_classic/ampeg_svt_sm57.nam"),
    capture("ultra_hi", "md421", "full_rigs/ampeg_svt_classic/ampeg_svt_ultra_hi_md421.nam"),
    capture("ultra_hi", "sm57",  "full_rigs/ampeg_svt_classic/ampeg_svt_ultra_hi_sm57.nam"),
    capture("ultra_lo", "md421", "full_rigs/ampeg_svt_classic/ampeg_svt_ultra_lo_md421.nam"),
    capture("ultra_lo", "sm57",  "full_rigs/ampeg_svt_classic/ampeg_svt_ultra_lo_sm57.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "tone",
            "Tone",
            Some("Amp"),
            Some("standard"),
            &[
                ("standard", "Standard"),
                ("ultra_hi", "Ultra Hi"),
                ("ultra_lo", "Ultra Lo"),
            ],
        ),
        enum_parameter(
            "mic",
            "Mic",
            Some("Cab"),
            Some("md421"),
            &[("md421", "MD 421"), ("sm57", "SM57")],
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static AmpegSvtCapture> {
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    let mic = required_string(params, "mic").map_err(anyhow::Error::msg)?;

    CAPTURES
        .iter()
        .find(|c| c.params.tone == tone && c.params.mic == mic)
        .ok_or_else(|| {
            anyhow!(
                "amp model '{}' does not support tone='{}' mic='{}'",
                MODEL_ID,
                tone,
                mic
            )
        })
}

const fn capture(tone: &'static str, mic: &'static str, model_path: &'static str) -> AmpegSvtCapture {
    AmpegSvtCapture {
        params: AmpegSvtParams { tone, mic },
        model_path,
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: AmpModelDefinition = AmpModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: AmpBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_ACOUSTIC_BASS,
    knob_layout: &[],
};
