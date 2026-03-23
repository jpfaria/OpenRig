use anyhow::{anyhow, bail, Result};
use asset_runtime::{materialize, EmbeddedAsset};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "martin_custom_shop_000_28md_k_k";
pub const DISPLAY_NAME: &str = "Custom Shop 000-28MD K&K";
const BRAND: &str = "martin";

macro_rules! capture {
    ($voicing:literal, $asset_id:literal, $relative_path:literal) => {
        Capture { voicing: $voicing, asset: EmbeddedAsset::new($asset_id, $relative_path, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../", $relative_path))) }
    };
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capture { pub voicing: &'static str, pub asset: EmbeddedAsset }

pub const CAPTURES: &[Capture] = &[
    capture!("00028md_knk_44100", "body.martin_custom_shop_000_28md_k_k.00028md_knk_44100", "captures/ir/body/martin_custom_shop_000_28md_k_k/00028md_knk_44100.wav"),
    capture!("00028md_knk_44100_match1", "body.martin_custom_shop_000_28md_k_k.00028md_knk_44100_match1", "captures/ir/body/martin_custom_shop_000_28md_k_k/00028md_knk_44100_match1.wav"),
    capture!("jf45ir00028jf", "body.martin_custom_shop_000_28md_k_k.jf45ir00028jf", "captures/ir/body/martin_custom_shop_000_28md_k_k/jf45ir00028jf.wav"),
    capture!("jf45ir102400028jf", "body.martin_custom_shop_000_28md_k_k.jf45ir102400028jf", "captures/ir/body/martin_custom_shop_000_28md_k_k/jf45ir102400028jf.wav"),
    capture!("jf45ir505000028jf", "body.martin_custom_shop_000_28md_k_k.jf45ir505000028jf", "captures/ir/body/martin_custom_shop_000_28md_k_k/jf45ir505000028jf.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_BODY.to_string(), model: MODEL_ID.to_string(), display_name: DISPLAY_NAME.to_string(), audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter("voicing", "Voicing", Some("Body"), Some("00028md_knk_44100"), &[
            ("00028md_knk_44100", "Standard"), ("00028md_knk_44100_match1", "Match 1"),
            ("jf45ir00028jf", "JF45 IR"), ("jf45ir102400028jf", "JF45 IR 1024"), ("jf45ir505000028jf", "JF45 IR 5050"),
        ])],
    }
}

pub fn build_processor_for_model(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => { let c = resolve_capture(params)?; let p = materialize(&c.asset)?; let ps = p.to_string_lossy(); let ir = IrAsset::load_from_wav(&ps)?; if ir.channel_count() != 1 { bail!("body model '{}' capture must be mono, got {} channels", MODEL_ID, ir.channel_count()); } Ok(BlockProcessor::Mono(build_mono_ir_processor_from_wav(&ps, sample_rate)?)) }
        AudioChannelLayout::Stereo => bail!("body model '{}' currently expects mono processor layout", MODEL_ID),
    }
}
fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }
fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> { build_processor_for_model(params, sample_rate, layout) }
pub const MODEL_DEFINITION: BodyModelDefinition = BodyModelDefinition { id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND, backend_kind: BodyBackendKind::Ir, schema, validate: validate_params, asset_summary, build, supported_instruments: &[block_core::INST_ACOUSTIC_GUITAR], knob_layout: &[] };
pub fn validate_params(params: &ParameterSet) -> Result<()> { resolve_capture(params).map(|_| ()) }
pub fn asset_summary(params: &ParameterSet) -> Result<String> { let c = resolve_capture(params)?; Ok(format!("asset_id='{}'", c.asset.id)) }
fn resolve_capture(params: &ParameterSet) -> Result<&'static Capture> {
    let requested = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    CAPTURES.iter().find(|c| c.voicing == requested).ok_or_else(|| anyhow!("body model '{}' does not support voicing '{}'", MODEL_ID, requested))
}
#[cfg(test)] mod tests { use super::*; use block_core::param::ParameterSet; use block_core::{AudioChannelLayout, BlockProcessor}; use domain::value_objects::ParameterValue;
    #[test] fn schema_ok() { let s = model_schema(); assert_eq!(s.parameters.len(), 1); assert_eq!(s.parameters[0].path, "voicing"); }
    #[test] fn rejects_unknown() { let mut p = ParameterSet::default(); p.insert("voicing", ParameterValue::String("x".into())); assert!(validate_params(&p).is_err()); }
    #[test] fn builds_mono() { let mut p = ParameterSet::default(); p.insert("voicing", ParameterValue::String("00028md_knk_44100".into())); match build_processor_for_model(&p, 48_000.0, AudioChannelLayout::Mono).unwrap() { BlockProcessor::Mono(_) => {} BlockProcessor::Stereo(_) => panic!("expected mono") } }
}
