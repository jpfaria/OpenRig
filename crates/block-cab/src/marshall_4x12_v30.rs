use anyhow::{anyhow, bail, Result};
use asset_runtime::{materialize, EmbeddedAsset};
use audio_ir::{build_mono_ir_processor_from_wav, IrAsset};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "marshall_4x12_v30";

macro_rules! capture {
    ($capture:literal, $asset_id:literal, $relative_path:literal) => {
        Marshall4x12V30Capture {
            capture: $capture,
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
pub struct Marshall4x12V30Capture {
    pub capture: &'static str,
    pub asset: EmbeddedAsset,
}

pub const CAPTURES: &[Marshall4x12V30Capture] = &[
    capture!(
        "ev_mix_b",
        "cab.marshall_4x12_v30.ev_mix_b",
        "captures/ir/cabs/marshall_4x12_v30/ev_mix_b.wav"
    ),
    capture!(
        "ev_mix_d",
        "cab.marshall_4x12_v30.ev_mix_d",
        "captures/ir/cabs/marshall_4x12_v30/ev_mix_d.wav"
    ),
];

pub fn supports_model(model: &str) -> bool {
    model == MODEL_ID
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "cab".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Marshall 4x12 V30".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "capture",
            "Capture",
            Some("Cab"),
            Some("ev_mix_b"),
            &[("ev_mix_b", "EV Mix B"), ("ev_mix_d", "EV Mix D")],
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
                    "cab model '{}' capture '{}' must be mono, got {} channels",
                    MODEL_ID,
                    capture.capture,
                    ir.channel_count()
                );
            }
            let processor = build_mono_ir_processor_from_wav(&materialized_path_str, sample_rate)?;
            Ok(BlockProcessor::Mono(processor))
        }
        AudioChannelLayout::Stereo => bail!(
            "cab model '{}' currently expects mono processor layout",
            MODEL_ID
        ),
    }
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("asset_id='{}'", capture.asset.id))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static Marshall4x12V30Capture> {
    let requested = required_string(params, "capture").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|capture| capture.capture == requested)
        .ok_or_else(|| {
            anyhow!(
                "cab model '{}' does not support capture '{}'",
                MODEL_ID,
                requested
            )
        })
}
