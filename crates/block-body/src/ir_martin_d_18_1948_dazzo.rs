use anyhow::{anyhow, bail, Result};
use asset_runtime::{materialize, EmbeddedAsset};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "martin_d_18_1948_dazzo";
pub const DISPLAY_NAME: &str = "D-18 1948 Dazzo";
const BRAND: &str = "martin";

macro_rules! capture {
    ($voicing:literal, $asset_id:literal, $relative_path:literal) => {
        MartinD181948DazzoCapture {
            voicing: $voicing,
            asset: EmbeddedAsset::new($asset_id, $relative_path, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../", $relative_path))),
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MartinD181948DazzoCapture { pub voicing: &'static str, pub asset: EmbeddedAsset }

pub const CAPTURES: &[MartinD181948DazzoCapture] = &[
    capture!("ir_martin_d18_1948_dazzo_44100", "body.martin_d_18_1948_dazzo.ir_martin_d18_1948_dazzo_44100", "captures/ir/body/martin_d_18_1948_dazzo/ir_martin_d18_1948_dazzo_44100.wav"),
    capture!("ir_martin_d18_1948_dazzo_44100_match2", "body.martin_d_18_1948_dazzo.ir_martin_d18_1948_dazzo_44100_match2", "captures/ir/body/martin_d_18_1948_dazzo/ir_martin_d18_1948_dazzo_44100_match2.wav"),
    capture!("ir_martin_d18_1948_dazzo_44100_meq3", "body.martin_d_18_1948_dazzo.ir_martin_d18_1948_dazzo_44100_meq3", "captures/ir/body/martin_d_18_1948_dazzo/ir_martin_d18_1948_dazzo_44100_meq3.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_BODY.to_string(), model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(), audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter("voicing", "Voicing", Some("Body"), Some("ir_martin_d18_1948_dazzo_44100"), &[
            ("ir_martin_d18_1948_dazzo_44100", "44100"),
            ("ir_martin_d18_1948_dazzo_44100_match2", "44100 Match 2"),
            ("ir_martin_d18_1948_dazzo_44100_meq3", "44100 MEQ 3"),
        ])],
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
pub fn asset_summary(params: &ParameterSet) -> Result<String> { let capture = resolve_capture(params)?; Ok(format!("asset_id='{}'", capture.asset.id)) }

fn resolve_capture(params: &ParameterSet) -> Result<&'static MartinD181948DazzoCapture> {
    let requested = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    CAPTURES.iter().find(|c| c.voicing == requested).ok_or_else(|| anyhow!("body model '{}' does not support voicing '{}'", MODEL_ID, requested))
}
