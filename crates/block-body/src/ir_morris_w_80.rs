use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "morris_w_80";
pub const DISPLAY_NAME: &str = "W-80";
const BRAND: &str = "";

macro_rules! capture {
    ($voicing:literal, $ir_file:literal) => {
        Capture {
            voicing: $voicing,
            ir_file: $ir_file,
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capture { pub voicing: &'static str, pub ir_file: &'static str }

pub const CAPTURES: &[Capture] = &[
    capture!("morris_2nd_attempt_ir44100", "body/morris_w_80/morris_2nd_attempt_ir44100.wav"),
    capture!("morris_2nd_attempt_ir44100_bp", "body/morris_w_80/morris_2nd_attempt_ir44100_bp.wav"),
    capture!("morris_2nd_attempt_ir44100_bld", "body/morris_w_80/morris_2nd_attempt_ir44100_bld.wav"),
    capture!("morris_2nd_attempt_ir44100_match1", "body/morris_w_80/morris_2nd_attempt_ir44100_match1.wav"),
    capture!("morris_2nd_attempt_ir44100_match2", "body/morris_w_80/morris_2nd_attempt_ir44100_match2.wav"),
    capture!("morris_2nd_attempt_ir44100_match3", "body/morris_w_80/morris_2nd_attempt_ir44100_match3.wav"),
    capture!("morris_2nd_attempt_ir44100_jf_flavor", "body/morris_w_80/morris_2nd_attempt_ir44100_jf_flavor.wav"),
    capture!("ir_morris_w80_44100", "body/morris_w_80/ir_morris_w80_44100.wav"),
    capture!("ir_morris_w80_44100_match", "body/morris_w_80/ir_morris_w80_44100_match.wav"),
    capture!("ir_morris_w80_44100_match2", "body/morris_w_80/ir_morris_w80_44100_match2.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_BODY.to_string(), model: MODEL_ID.to_string(), display_name: DISPLAY_NAME.to_string(), audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter("voicing", "Voicing", Some("Body"), Some("morris_2nd_attempt_ir44100"), &[
            ("morris_2nd_attempt_ir44100", "Standard"),
            ("morris_2nd_attempt_ir44100_bp", "Bandpass"),
            ("morris_2nd_attempt_ir44100_bld", "Blend"),
            ("morris_2nd_attempt_ir44100_match1", "Match 1"),
            ("morris_2nd_attempt_ir44100_match2", "Match 2"),
            ("morris_2nd_attempt_ir44100_match3", "Match 3"),
            ("morris_2nd_attempt_ir44100_jf_flavor", "JF Flavor"),
            ("ir_morris_w80_44100", "W80 Standard"),
            ("ir_morris_w80_44100_match", "W80 Match"),
            ("ir_morris_w80_44100_match2", "W80 Match 2"),
        ])],
    }
}

pub fn build_processor_for_model(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => {
            let capture = resolve_capture(params)?;
            let wav_path = ir::resolve_ir_capture(capture.ir_file)?;
            let ir = IrAsset::load_from_wav(&wav_path)?;
            if ir.channel_count() != 1 { bail!("body model '{}' capture must be mono, got {} channels", MODEL_ID, ir.channel_count()); }
            Ok(BlockProcessor::Mono(build_mono_ir_processor_from_wav(&wav_path, sample_rate)?))
        }
        AudioChannelLayout::Stereo => bail!("body model '{}' currently expects mono processor layout", MODEL_ID),
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }
fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> { build_processor_for_model(params, sample_rate, layout) }

pub const MODEL_DEFINITION: BodyModelDefinition = BodyModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND, backend_kind: BodyBackendKind::Ir,
    schema, validate: validate_params, asset_summary, build,
    supported_instruments: &[block_core::INST_ACOUSTIC_GUITAR], knob_layout: &[],
};

pub fn validate_params(params: &ParameterSet) -> Result<()> { resolve_capture(params).map(|_| ()) }
pub fn asset_summary(params: &ParameterSet) -> Result<String> { let c = resolve_capture(params)?; Ok(format!("asset_id='{}'", c.ir_file)) }

fn resolve_capture(params: &ParameterSet) -> Result<&'static Capture> {
    let requested = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    CAPTURES.iter().find(|c| c.voicing == requested)
        .ok_or_else(|| anyhow!("body model '{}' does not support voicing '{}'", MODEL_ID, requested))
}

#[cfg(test)]
mod tests {
    use super::*; use block_core::param::ParameterSet; use block_core::{AudioChannelLayout, BlockProcessor}; use domain::value_objects::ParameterValue;
    #[test] fn schema_exposes_voicing_parameter() { let s = model_schema(); assert_eq!(s.parameters.len(), 1); assert_eq!(s.parameters[0].path, "voicing"); }
    #[test] fn rejects_unknown_voicing() { let mut p = ParameterSet::default(); p.insert("voicing", ParameterValue::String("unknown".into())); assert!(validate_params(&p).is_err()); }
    #[test] fn builds_mono_processor() {
        let mut p = ParameterSet::default(); p.insert("voicing", ParameterValue::String("morris_2nd_attempt_ir44100".into()));
        match build_processor_for_model(&p, 48_000.0, AudioChannelLayout::Mono).expect("should build") { BlockProcessor::Mono(_) => {} BlockProcessor::Stereo(_) => panic!("expected mono") }
    }
}
