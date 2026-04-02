use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "mdf_eastman_ac422ce";
pub const DISPLAY_NAME: &str = "MDF Eastman AC422CE";
const BRAND: &str = "";

macro_rules! capture {
    ($p1:literal, $ir_file:literal) => {
        MdfEastmanAc422ceCapture {
            flavor: $p1,
            ir_file: $ir_file,
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MdfEastmanAc422ceCapture {
    pub flavor: &'static str,
    pub ir_file: &'static str,
}

pub const CAPTURES: &[MdfEastmanAc422ceCapture] = &[
    capture!("eastman_ac422ce_44100", "body/mdf_eastman_ac422ce/eastman_ac422ce_44100.wav"),
    capture!("eastman_ac422ce_44100b", "body/mdf_eastman_ac422ce/eastman_ac422ce_44100b.wav"),
    capture!("eastman_ac422ce_44100b_match1", "body/mdf_eastman_ac422ce/eastman_ac422ce_44100b_match1.wav"),
    capture!("eastman_ac422ce_44100_match2", "body/mdf_eastman_ac422ce/eastman_ac422ce_44100_match2.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_BODY.to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "flavor",
            "Flavor",
            Some("Body"),
            Some("eastman_ac422ce_44100"),
            &[
                ("eastman_ac422ce_44100", "Eastman AC422CE 44100"),
                ("eastman_ac422ce_44100b", "Eastman AC422CE 44100B"),
                ("eastman_ac422ce_44100b_match1", "Eastman AC422CE 44100B Match 1"),
                ("eastman_ac422ce_44100_match2", "Eastman AC422CE 44100 Match 2"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static MdfEastmanAc422ceCapture> {
    let requested = required_string(params, "flavor").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.flavor == requested)
        .ok_or_else(|| {
            anyhow!(
                "body model '{}' does not support flavor '{}'",
                MODEL_ID,
                requested
            )
        })
}
