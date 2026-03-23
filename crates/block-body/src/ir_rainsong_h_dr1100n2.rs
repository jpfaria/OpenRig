use anyhow::{anyhow, bail, Result};
use asset_runtime::{materialize, EmbeddedAsset};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "rainsong_h_dr1100n2";
pub const DISPLAY_NAME: &str = "H-DR1100N2";
const BRAND: &str = "rainsong";

macro_rules! capture {
    ($flavor:literal, $asset_id:literal, $relative_path:literal) => {
        RainsongHDr1100n2Capture {
            flavor: $flavor,
            asset: EmbeddedAsset::new(
                $asset_id,
                $relative_path,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../",
                    $relative_path
                )),
            ),
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RainsongHDr1100n2Capture {
    pub flavor: &'static str,
    pub asset: EmbeddedAsset,
}

pub const CAPTURES: &[RainsongHDr1100n2Capture] = &[
    capture!("standard", "body.rainsong_h_dr1100n2.standard", "captures/ir/body/rainsong_h_dr1100n2/jf_rainsong_h_dr1100n2_hfn_44100.wav"),
    capture!("blend", "body.rainsong_h_dr1100n2.blend", "captures/ir/body/rainsong_h_dr1100n2/jf_rainsong_h_dr1100n2_hfn_44100_bld.wav"),
    capture!("match", "body.rainsong_h_dr1100n2.match", "captures/ir/body/rainsong_h_dr1100n2/rainsong_dread_44100_matcheq.wav"),
    capture!("jf", "body.rainsong_h_dr1100n2.jf", "captures/ir/body/rainsong_h_dr1100n2/jf_rainsong_h_dr1100n2_hfn_44100_jf_flavor.wav"),
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
            Some("standard"),
            &[
                ("standard", "Standard"),
                ("blend", "Blend"),
                ("match", "Match"),
                ("jf", "JF Flavor"),
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
            let materialized_path = materialize(&capture.asset)?;
            let materialized_path_str = materialized_path.to_string_lossy();
            let ir = IrAsset::load_from_wav(&materialized_path_str)?;
            if ir.channel_count() != 1 {
                bail!(
                    "body model '{}' capture must be mono, got {} channels",
                    MODEL_ID,
                    ir.channel_count()
                );
            }
            let processor = build_mono_ir_processor_from_wav(&materialized_path_str, sample_rate)?;
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
    Ok(format!("asset_id='{}'", capture.asset.id))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static RainsongHDr1100n2Capture> {
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
