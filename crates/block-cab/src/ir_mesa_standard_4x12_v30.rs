use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "mesa_standard_4x12_v30";
pub const DISPLAY_NAME: &str = "Standard 4x12 V30";
const BRAND: &str = "mesa";

macro_rules! capture {
    ($p1:literal, $ir_file:literal) => {
        MesaStandard4x12V30Capture { capture: $p1, ir_file: $ir_file }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MesaStandard4x12V30Capture {
    pub capture: &'static str,
    pub ir_file: &'static str,
}

pub const CAPTURES: &[MesaStandard4x12V30Capture] = &[
    capture!("sm57","cabs/mesa_standard_4x12_v30/sm57.wav"),
    capture!("sm58","cabs/mesa_standard_4x12_v30/sm58.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "cab".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "mic",
            "Mic",
            Some("Cab"),
            Some("sm57"),
            &[
                ("sm57","SM57"),
                ("sm58","SM58"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static MesaStandard4x12V30Capture> {
    let requested = required_string(params, "mic").map_err(anyhow::Error::msg)?;
    CAPTURES.iter()
        .find(|c| c.capture == requested)
        .ok_or_else(|| anyhow!("cab model '{}' does not support mic '{}'", MODEL_ID, requested))
}
