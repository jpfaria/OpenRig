use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "leo_gs_mini";
pub const DISPLAY_NAME: &str = "GS Mini";
const BRAND: &str = "taylor";

macro_rules! capture {
    ($p1:literal, $ir_file:literal) => {
        LeoGsMiniCapture {
            voicing: $p1,
            ir_file: $ir_file,
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeoGsMiniCapture {
    pub voicing: &'static str,
    pub ir_file: &'static str,
}

pub const CAPTURES: &[LeoGsMiniCapture] = &[
    capture!("leo_gs_mini_es2_44100", "body/leo_gs_mini/leo_gs_mini_es2_44100.wav"),
    capture!("leo_gs_mini_es2_44100_bld", "body/leo_gs_mini/leo_gs_mini_es2_44100_bld.wav"),
    capture!("leo_gs_mini_es2_44100_jf_flavor", "body/leo_gs_mini/leo_gs_mini_es2_44100_jf_flavor.wav"),
    capture!("leo_gs_mini_es2_44100_match", "body/leo_gs_mini/leo_gs_mini_es2_44100_match.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_BODY.to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "voicing",
            "Voicing",
            Some("Body"),
            Some("leo_gs_mini_es2_44100_bld"),
            &[
                ("leo_gs_mini_es2_44100", "ES2 44100"),
                ("leo_gs_mini_es2_44100_bld", "ES2 44100 Bld"),
                ("leo_gs_mini_es2_44100_jf_flavor", "ES2 44100 JF Flavor"),
                ("leo_gs_mini_es2_44100_match", "ES2 44100 Match"),
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
                bail!(
                    "body model '{}' capture must be mono, got {} channels",
                    MODEL_ID,
                    ir.channel_count()
                );
            }
            let processor = build_mono_ir_processor_from_wav(&wav_path, sample_rate)?;
            Ok(BlockProcessor::Mono(processor))
        }
        AudioChannelLayout::Stereo => bail!(
            "body model '{}' currently expects mono processor layout",
            MODEL_ID
        ),
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: BodyModelDefinition = BodyModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: BodyBackendKind::Ir,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: &[block_core::INST_ACOUSTIC_GUITAR],
    knob_layout: &[],
};

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("asset_id='{}'", capture.ir_file))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static LeoGsMiniCapture> {
    let requested = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.voicing == requested)
        .ok_or_else(|| {
            anyhow!(
                "body model '{}' does not support voicing '{}'",
                MODEL_ID,
                requested
            )
        })
}
