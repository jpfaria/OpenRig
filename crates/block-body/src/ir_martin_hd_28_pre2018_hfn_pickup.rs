use anyhow::{anyhow, bail, Result};
use asset_runtime::{materialize, EmbeddedAsset};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "martin_hd_28_pre2018_hfn_pickup";
pub const DISPLAY_NAME: &str = "HD-28 Pre-2018 HFN Pickup";
const BRAND: &str = "martin";

macro_rules! capture {
    ($voicing:literal, $asset_id:literal, $relative_path:literal) => {
        Capture {
            voicing: $voicing,
            asset: EmbeddedAsset::new(
                $asset_id,
                $relative_path,
                include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../", $relative_path)),
            ),
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capture { pub voicing: &'static str, pub asset: EmbeddedAsset }

pub const CAPTURES: &[Capture] = &[
    capture!("martinhd28_hfn1_44100", "body.martin_hd_28_pre2018_hfn_pickup.martinhd28_hfn1_44100", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/martinhd28_hfn1_44100.wav"),
    capture!("martinhd28_hfn1_44100_bld", "body.martin_hd_28_pre2018_hfn_pickup.martinhd28_hfn1_44100_bld", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/martinhd28_hfn1_44100_bld.wav"),
    capture!("martinhd28_hfn1_44100_match", "body.martin_hd_28_pre2018_hfn_pickup.martinhd28_hfn1_44100_match", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/martinhd28_hfn1_44100_match.wav"),
    capture!("martinhd28_hfn1_44100_jf_flavor", "body.martin_hd_28_pre2018_hfn_pickup.martinhd28_hfn1_44100_jf_flavor", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/martinhd28_hfn1_44100_jf_flavor.wav"),
    capture!("martinhd28_hfn2_44100", "body.martin_hd_28_pre2018_hfn_pickup.martinhd28_hfn2_44100", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/martinhd28_hfn2_44100.wav"),
    capture!("martinhd28_hfn2_44100_bld", "body.martin_hd_28_pre2018_hfn_pickup.martinhd28_hfn2_44100_bld", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/martinhd28_hfn2_44100_bld.wav"),
    capture!("martinhd28_hfn2_44100_match", "body.martin_hd_28_pre2018_hfn_pickup.martinhd28_hfn2_44100_match", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/martinhd28_hfn2_44100_match.wav"),
    capture!("martinhd28_hfn2_44100_jf_flavor", "body.martin_hd_28_pre2018_hfn_pickup.martinhd28_hfn2_44100_jf_flavor", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/martinhd28_hfn2_44100_jf_flavor.wav"),
    capture!("ir_martin_hd28_8096st_matcheq", "body.martin_hd_28_pre2018_hfn_pickup.ir_martin_hd28_8096st_matcheq", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/ir_martin_hd28_8096st_matcheq.wav"),
    capture!("ir_martin_hd28_8096l_matcheq", "body.martin_hd_28_pre2018_hfn_pickup.ir_martin_hd28_8096l_matcheq", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/ir_martin_hd28_8096l_matcheq.wav"),
    capture!("ir_martin_hd28_8096r_matcheq", "body.martin_hd_28_pre2018_hfn_pickup.ir_martin_hd28_8096r_matcheq", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/ir_martin_hd28_8096r_matcheq.wav"),
    capture!("ir_martin_hd28_2048st_matcheq", "body.martin_hd_28_pre2018_hfn_pickup.ir_martin_hd28_2048st_matcheq", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/ir_martin_hd28_2048st_matcheq.wav"),
    capture!("ir_martin_hd28_2048l_matcheq", "body.martin_hd_28_pre2018_hfn_pickup.ir_martin_hd28_2048l_matcheq", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/ir_martin_hd28_2048l_matcheq.wav"),
    capture!("ir_martin_hd28_2048r_matcheq", "body.martin_hd_28_pre2018_hfn_pickup.ir_martin_hd28_2048r_matcheq", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/ir_martin_hd28_2048r_matcheq.wav"),
    capture!("irzrec1db65s4ss17", "body.martin_hd_28_pre2018_hfn_pickup.irzrec1db65s4ss17", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/irzrec1db65s4ss17.wav"),
    capture!("irzrec2db65s4ss17", "body.martin_hd_28_pre2018_hfn_pickup.irzrec2db65s4ss17", "captures/ir/body/martin_hd_28_pre2018_hfn_pickup/irzrec2db65s4ss17.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_BODY.to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "voicing", "Voicing", Some("Body"), Some("martinhd28_hfn1_44100"),
            &[
                ("martinhd28_hfn1_44100", "HFN1"),
                ("martinhd28_hfn1_44100_bld", "HFN1 Blend"),
                ("martinhd28_hfn1_44100_match", "HFN1 Match"),
                ("martinhd28_hfn1_44100_jf_flavor", "HFN1 JF Flavor"),
                ("martinhd28_hfn2_44100", "HFN2"),
                ("martinhd28_hfn2_44100_bld", "HFN2 Blend"),
                ("martinhd28_hfn2_44100_match", "HFN2 Match"),
                ("martinhd28_hfn2_44100_jf_flavor", "HFN2 JF Flavor"),
                ("ir_martin_hd28_8096st_matcheq", "8096 Stereo Match EQ"),
                ("ir_martin_hd28_8096l_matcheq", "8096 Left Match EQ"),
                ("ir_martin_hd28_8096r_matcheq", "8096 Right Match EQ"),
                ("ir_martin_hd28_2048st_matcheq", "2048 Stereo Match EQ"),
                ("ir_martin_hd28_2048l_matcheq", "2048 Left Match EQ"),
                ("ir_martin_hd28_2048r_matcheq", "2048 Right Match EQ"),
                ("irzrec1db65s4ss17", "ZRec 1"),
                ("irzrec2db65s4ss17", "ZRec 2"),
            ],
        )],
    }
}

pub fn build_processor_for_model(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => {
            let capture = resolve_capture(params)?;
            let materialized_path = materialize(&capture.asset)?;
            let materialized_path_str = materialized_path.to_string_lossy();
            let ir = IrAsset::load_from_wav(&materialized_path_str)?;
            if ir.channel_count() != 1 { bail!("body model '{}' capture must be mono, got {} channels", MODEL_ID, ir.channel_count()); }
            let processor = build_mono_ir_processor_from_wav(&materialized_path_str, sample_rate)?;
            Ok(BlockProcessor::Mono(processor))
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
pub fn asset_summary(params: &ParameterSet) -> Result<String> { let c = resolve_capture(params)?; Ok(format!("asset_id='{}'", c.asset.id)) }

fn resolve_capture(params: &ParameterSet) -> Result<&'static Capture> {
    let requested = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    CAPTURES.iter().find(|c| c.voicing == requested)
        .ok_or_else(|| anyhow!("body model '{}' does not support voicing '{}'", MODEL_ID, requested))
}

#[cfg(test)]
mod tests {
    use super::*;
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, BlockProcessor};
    use domain::value_objects::ParameterValue;

    #[test]
    fn schema_exposes_voicing_parameter() { let s = model_schema(); assert_eq!(s.parameters.len(), 1); assert_eq!(s.parameters[0].path, "voicing"); }
    #[test]
    fn rejects_unknown_voicing() { let mut p = ParameterSet::default(); p.insert("voicing", ParameterValue::String("unknown".into())); assert!(validate_params(&p).is_err()); }
    #[test]
    fn builds_mono_processor() {
        let mut p = ParameterSet::default(); p.insert("voicing", ParameterValue::String("martinhd28_hfn1_44100".into()));
        match build_processor_for_model(&p, 48_000.0, AudioChannelLayout::Mono).expect("should build") { BlockProcessor::Mono(_) => {} BlockProcessor::Stereo(_) => panic!("expected mono") }
    }
}
