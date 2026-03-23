use anyhow::{anyhow, bail, Result};
use asset_runtime::{materialize, EmbeddedAsset};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "guild";
pub const DISPLAY_NAME: &str = "Guild";
const BRAND: &str = "guild";

macro_rules! capture {
    ($voicing:literal, $asset_id:literal, $relative_path:literal) => {
        Capture { voicing: $voicing, asset: EmbeddedAsset::new($asset_id, $relative_path, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../", $relative_path))) }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capture { pub voicing: &'static str, pub asset: EmbeddedAsset }

pub const CAPTURES: &[Capture] = &[
    capture!("001_dtar_parlor", "body.guild.001_dtar_parlor", "captures/ir/body/guild/001_dtar_parlor.wav"),
    capture!("002_dtar_finger", "body.guild.002_dtar_finger", "captures/ir/body/guild/002_dtar_finger.wav"),
    capture!("003_dtar_blues", "body.guild.003_dtar_blues", "captures/ir/body/guild/003_dtar_blues.wav"),
    capture!("004_dtar_mag", "body.guild.004_dtar_mag", "captures/ir/body/guild/004_dtar_mag.wav"),
    capture!("005_dtar_ros", "body.guild.005_dtar_ros", "captures/ir/body/guild/005_dtar_ros.wav"),
    capture!("006_dtar_btq_finger", "body.guild.006_dtar_btq_finger", "captures/ir/body/guild/006_dtar_btq_finger.wav"),
    capture!("007_dtar_dread", "body.guild.007_dtar_dread", "captures/ir/body/guild/007_dtar_dread.wav"),
    capture!("008_dtar_graud", "body.guild.008_dtar_graud", "captures/ir/body/guild/008_dtar_graud.wav"),
    capture!("009_dtar_slope_jumbo", "body.guild.009_dtar_slope_jumbo", "captures/ir/body/guild/009_dtar_slope_jumbo.wav"),
    capture!("010_dtar_mah_dread", "body.guild.010_dtar_mah_dread", "captures/ir/body/guild/010_dtar_mah_dread.wav"),
    capture!("011_dtar_ros_dread", "body.guild.011_dtar_ros_dread", "captures/ir/body/guild/011_dtar_ros_dread.wav"),
    capture!("012_dtar_sup_jumbo", "body.guild.012_dtar_sup_jumbo", "captures/ir/body/guild/012_dtar_sup_jumbo.wav"),
    capture!("013_dtar_hollow_arch_jazz", "body.guild.013_dtar_hollow_arch_jazz", "captures/ir/body/guild/013_dtar_hollow_arch_jazz.wav"),
    capture!("014_dtar_gypsy_jazz", "body.guild.014_dtar_gypsy_jazz", "captures/ir/body/guild/014_dtar_gypsy_jazz.wav"),
    capture!("015_dtar_blues_res", "body.guild.015_dtar_blues_res", "captures/ir/body/guild/015_dtar_blues_res.wav"),
    capture!("016_dtar_tricone_res", "body.guild.016_dtar_tricone_res", "captures/ir/body/guild/016_dtar_tricone_res.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_BODY.to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "voicing", "Voicing", Some("Body"), Some("001_dtar_parlor"),
            &[
                ("001_dtar_parlor", "Parlor"),
                ("002_dtar_finger", "Finger"),
                ("003_dtar_blues", "Blues"),
                ("004_dtar_mag", "Mag"),
                ("005_dtar_ros", "Rosewood"),
                ("006_dtar_btq_finger", "Boutique Finger"),
                ("007_dtar_dread", "Dreadnought"),
                ("008_dtar_graud", "Grand Auditorium"),
                ("009_dtar_slope_jumbo", "Slope Jumbo"),
                ("010_dtar_mah_dread", "Mahogany Dreadnought"),
                ("011_dtar_ros_dread", "Rosewood Dreadnought"),
                ("012_dtar_sup_jumbo", "Super Jumbo"),
                ("013_dtar_hollow_arch_jazz", "Hollow Arch Jazz"),
                ("014_dtar_gypsy_jazz", "Gypsy Jazz"),
                ("015_dtar_blues_res", "Blues Resonator"),
                ("016_dtar_tricone_res", "Tricone Resonator"),
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

    #[test] fn schema_exposes_voicing_parameter() { let s = model_schema(); assert_eq!(s.parameters.len(), 1); assert_eq!(s.parameters[0].path, "voicing"); }
    #[test] fn rejects_unknown_voicing() { let mut p = ParameterSet::default(); p.insert("voicing", ParameterValue::String("unknown".into())); assert!(validate_params(&p).is_err()); }
    #[test] fn builds_mono_processor() {
        let mut p = ParameterSet::default(); p.insert("voicing", ParameterValue::String("001_dtar_parlor".into()));
        match build_processor_for_model(&p, 48_000.0, AudioChannelLayout::Mono).expect("should build") { BlockProcessor::Mono(_) => {} BlockProcessor::Stereo(_) => panic!("expected mono") }
    }
}
