use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "evil_chug_blackstar_prs";
pub const DISPLAY_NAME: &str = "Evil Chug (Blackstar + PRS)";
const BRAND: &str = "blackstar";

macro_rules! capture {
    ($p1:literal, $ir_file:literal) => {
        EvilChugCapture { capture: $p1, ir_file: $ir_file }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvilChugCapture {
    pub capture: &'static str,
    pub ir_file: &'static str,
}

pub const CAPTURES: &[EvilChugCapture] = &[
    capture!("evil_chug_1","cabs/evil_chug_blackstar_prs/evil_chug_1.wav"),
    capture!("evil_chug_2","cabs/evil_chug_blackstar_prs/evil_chug_2.wav"),
    capture!("evil_chug_3","cabs/evil_chug_blackstar_prs/evil_chug_3.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "cab".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "capture",
            "Capture",
            Some("Cab"),
            Some("evil_chug_1"),
            &[
                ("evil_chug_1","Evil Chug 1"),
                ("evil_chug_2","Evil Chug 2"),
                ("evil_chug_3","Evil Chug 3"),
            ],
        )],
    }
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => {
            let capture = resolve_capture(params)?;
            let wav_path = ir::resolve_ir_capture(capture.ir_file)?;
            let ir = IrAsset::load_from_wav(&wav_path)?;
            if ir.channel_count() != 1 {
                bail!("cab model '{}' capture '{}' must be mono, got {} channels", MODEL_ID, capture.capture, ir.channel_count());
            }
            Ok(BlockProcessor::Mono(build_mono_ir_processor_from_wav(&wav_path, sample_rate)?))
        }
        AudioChannelLayout::Stereo => bail!("cab model '{}' currently expects mono processor layout", MODEL_ID),
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: CabModelDefinition = CabModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: CabBackendKind::Ir,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("asset_id='{}'", capture.ir_file))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static EvilChugCapture> {
    let requested = required_string(params, "capture").map_err(anyhow::Error::msg)?;
    CAPTURES.iter()
        .find(|c| c.capture == requested)
        .ok_or_else(|| anyhow!("cab model '{}' does not support capture '{}'", MODEL_ID, requested))
}
