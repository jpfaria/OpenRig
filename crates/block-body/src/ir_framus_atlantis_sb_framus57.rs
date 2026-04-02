use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "framus_atlantis_sb_framus57";
pub const DISPLAY_NAME: &str = "Atlantis SB Framus57";
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
    capture!("framus57_44100", "body/framus_atlantis_sb_framus57/framus57_44100.wav"),
    capture!("framus57_44100_match1", "body/framus_atlantis_sb_framus57/framus57_44100_match1.wav"),
    capture!("framus57_44100_match2", "body/framus_atlantis_sb_framus57/framus57_44100_match2.wav"),
    capture!("framus57_44100_1024", "body/framus_atlantis_sb_framus57/framus57_44100_1024.wav"),
    capture!("framus57_44100_1024_match1", "body/framus_atlantis_sb_framus57/framus57_44100_1024_match1.wav"),
    capture!("framus57_44100_1024_match2", "body/framus_atlantis_sb_framus57/framus57_44100_1024_match2.wav"),
    capture!("framus57_44100_1024_match3", "body/framus_atlantis_sb_framus57/framus57_44100_1024_match3.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_BODY.to_string(), model: MODEL_ID.to_string(), display_name: DISPLAY_NAME.to_string(), audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter("voicing", "Voicing", Some("Body"), Some("framus57_44100"), &[
            ("framus57_44100", "Standard"), ("framus57_44100_match1", "Match 1"), ("framus57_44100_match2", "Match 2"),
            ("framus57_44100_1024", "1024 Standard"), ("framus57_44100_1024_match1", "1024 Match 1"), ("framus57_44100_1024_match2", "1024 Match 2"), ("framus57_44100_1024_match3", "1024 Match 3"),
        ])],
    }
}

pub fn build_processor_for_model(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => { let c = resolve_capture(params)?; let wav_path = ir::resolve_ir_capture(c.ir_file)?; let ir = IrAsset::load_from_wav(&wav_path)?; if ir.channel_count() != 1 { bail!("body model '{}' capture must be mono, got {} channels", MODEL_ID, ir.channel_count()); } Ok(BlockProcessor::Mono(build_mono_ir_processor_from_wav(&wav_path, sample_rate)?)) }
        AudioChannelLayout::Stereo => bail!("body model '{}' currently expects mono processor layout", MODEL_ID),
    }
}
fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }
fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> { build_processor_for_model(params, sample_rate, layout) }
pub const MODEL_DEFINITION: BodyModelDefinition = BodyModelDefinition { id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND, backend_kind: BodyBackendKind::Ir, schema, validate: validate_params, asset_summary, build, supported_instruments: &[block_core::INST_ACOUSTIC_GUITAR], knob_layout: &[] };
pub fn validate_params(params: &ParameterSet) -> Result<()> { resolve_capture(params).map(|_| ()) }
pub fn asset_summary(params: &ParameterSet) -> Result<String> { let c = resolve_capture(params)?; Ok(format!("asset_id='{}'", c.ir_file)) }
fn resolve_capture(params: &ParameterSet) -> Result<&'static Capture> {
    let requested = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    CAPTURES.iter().find(|c| c.voicing == requested).ok_or_else(|| anyhow!("body model '{}' does not support voicing '{}'", MODEL_ID, requested))
}
#[cfg(test)] mod tests { use super::*; use block_core::param::ParameterSet; use block_core::{AudioChannelLayout, BlockProcessor}; use domain::value_objects::ParameterValue;
    #[test] fn schema_ok() { let s = model_schema(); assert_eq!(s.parameters.len(), 1); assert_eq!(s.parameters[0].path, "voicing"); }
    #[test] fn rejects_unknown() { let mut p = ParameterSet::default(); p.insert("voicing", ParameterValue::String("x".into())); assert!(validate_params(&p).is_err()); }
    #[test] fn builds_mono() { let mut p = ParameterSet::default(); p.insert("voicing", ParameterValue::String("framus57_44100".into())); match build_processor_for_model(&p, 48_000.0, AudioChannelLayout::Mono).unwrap() { BlockProcessor::Mono(_) => {} BlockProcessor::Stereo(_) => panic!("expected mono") } }
}
