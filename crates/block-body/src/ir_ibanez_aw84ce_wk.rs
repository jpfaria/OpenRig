use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "ibanez_aw84ce_wk";
pub const DISPLAY_NAME: &str = "AW84CE WK";
const BRAND: &str = "ibanez";

macro_rules! capture {
    ($p1:literal, $ir_file:literal) => {
        IbanezAw84ceWkCapture {
            voicing: $p1,
            ir_file: $ir_file,
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IbanezAw84ceWkCapture {
    pub voicing: &'static str,
    pub ir_file: &'static str,
}

pub const CAPTURES: &[IbanezAw84ceWkCapture] = &[
    capture!("ibanez_aw84ce_wk_48000", "body/ibanez_aw84ce_wk/ibanez_aw84ce_wk_48000.wav"),
    capture!("ibanez_aw84ce_wk_48000_bld", "body/ibanez_aw84ce_wk/ibanez_aw84ce_wk_48000_bld.wav"),
    capture!("ibanez_aw84ce_wk_48000_jf_flavor", "body/ibanez_aw84ce_wk/ibanez_aw84ce_wk_48000_jf_flavor.wav"),
    capture!("ibanez_aw84ce_wk_48000_match", "body/ibanez_aw84ce_wk/ibanez_aw84ce_wk_48000_match.wav"),
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
            Some("ibanez_aw84ce_wk_48000_bld"),
            &[
                ("ibanez_aw84ce_wk_48000", "48000"),
                ("ibanez_aw84ce_wk_48000_bld", "48000 Bld"),
                ("ibanez_aw84ce_wk_48000_jf_flavor", "48000 JF Flavor"),
                ("ibanez_aw84ce_wk_48000_match", "48000 Match"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static IbanezAw84ceWkCapture> {
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
