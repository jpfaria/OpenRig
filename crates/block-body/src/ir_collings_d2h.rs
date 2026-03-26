use anyhow::{anyhow, bail, Result};
use asset_runtime::{materialize, EmbeddedAsset};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "collings_d2h";
pub const DISPLAY_NAME: &str = "D2H";
const BRAND: &str = "collings";

macro_rules! capture {
    ($voicing:literal, $asset_id:literal, $relative_path:literal) => {
        Capture { voicing: $voicing, asset: EmbeddedAsset::new($asset_id, $relative_path, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../", $relative_path))) }
    };
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capture { pub voicing: &'static str, pub asset: EmbeddedAsset }

pub const CAPTURES: &[Capture] = &[
    capture!("collings_d2h_dpa_4011", "body.collings_d2h.collings_d2h_dpa_4011", "captures/ir/body/collings_d2h/collings_d2h_dpa_4011.wav"),
    capture!("collings_d2h_neumann_m147", "body.collings_d2h.collings_d2h_neumann_m147", "captures/ir/body/collings_d2h/collings_d2h_neumann_m147.wav"),
    capture!("collings_d2h_neumann_km84", "body.collings_d2h.collings_d2h_neumann_km84", "captures/ir/body/collings_d2h/collings_d2h_neumann_km84.wav"),
    capture!("collings_d2h_sanken_cu31", "body.collings_d2h.collings_d2h_sanken_cu31", "captures/ir/body/collings_d2h/collings_d2h_sanken_cu31.wav"),
    capture!("collings_om2ha_akg_cs1000", "body.collings_d2h.collings_om2ha_akg_cs1000", "captures/ir/body/collings_d2h/collings_om2ha_akg_cs1000.wav"),
    capture!("collings_om2ha_dpa_4011", "body.collings_d2h.collings_om2ha_dpa_4011", "captures/ir/body/collings_d2h/collings_om2ha_dpa_4011.wav"),
    capture!("collings_om2ha_schoeps_cmc64g", "body.collings_d2h.collings_om2ha_schoeps_cmc64g", "captures/ir/body/collings_d2h/collings_om2ha_schoeps_cmc64g.wav"),
    capture!("collings_om2ha_schoeps_cmc64g_xy", "body.collings_d2h.collings_om2ha_schoeps_cmc64g_xy", "captures/ir/body/collings_d2h/collings_om2ha_schoeps_cmc64g_xy.wav"),
    capture!("collings_om2ha_soundelux_e47", "body.collings_d2h.collings_om2ha_soundelux_e47", "captures/ir/body/collings_d2h/collings_om2ha_soundelux_e47.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_BODY.to_string(), model: MODEL_ID.to_string(), display_name: DISPLAY_NAME.to_string(), audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter("voicing", "Voicing", Some("Body"), Some("collings_d2h_dpa_4011"), &[
            ("collings_d2h_dpa_4011", "D2H DPA 4011"),
            ("collings_d2h_neumann_m147", "D2H Neumann M147"),
            ("collings_d2h_neumann_km84", "D2H Neumann KM84"),
            ("collings_d2h_sanken_cu31", "D2H Sanken CU31"),
            ("collings_om2ha_akg_cs1000", "OM2HA AKG CS1000"),
            ("collings_om2ha_dpa_4011", "OM2HA DPA 4011"),
            ("collings_om2ha_schoeps_cmc64g", "OM2HA Schoeps CMC64G"),
            ("collings_om2ha_schoeps_cmc64g_xy", "OM2HA Schoeps CMC64G XY"),
            ("collings_om2ha_soundelux_e47", "OM2HA Soundelux E47"),
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
    #[test] fn builds_mono() { let mut p = ParameterSet::default(); p.insert("voicing", ParameterValue::String("collings_d2h_dpa_4011".into())); match build_processor_for_model(&p, 48_000.0, AudioChannelLayout::Mono).unwrap() { BlockProcessor::Mono(_) => {} BlockProcessor::Stereo(_) => panic!("expected mono") } }
}
