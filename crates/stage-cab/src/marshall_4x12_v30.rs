use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use stage_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, ModelAudioMode, StageProcessor};
use std::path::{Path, PathBuf};

pub const MODEL_ID: &str = "marshall_4x12_v30";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marshall4x12V30Capture {
    pub capture: &'static str,
    pub ir_path: &'static str,
}

pub const CAPTURES: &[Marshall4x12V30Capture] = &[
    Marshall4x12V30Capture {
        capture: "ev_mix_b",
        ir_path: "captures/ir/cabs/marshall_4x12_v30/ev_mix_b.wav",
    },
    Marshall4x12V30Capture {
        capture: "ev_mix_d",
        ir_path: "captures/ir/cabs/marshall_4x12_v30/ev_mix_d.wav",
    },
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
) -> Result<StageProcessor> {
    match layout {
        AudioChannelLayout::Mono => {
            let capture = resolve_capture(params)?;
            let resolved_ir_path = resolve_ir_path(capture.ir_path);
            let ir = IrAsset::load_from_wav(&resolved_ir_path.to_string_lossy())?;
            if ir.channel_count() != 1 {
                bail!(
                    "cab model '{}' capture '{}' must be mono, got {} channels",
                    MODEL_ID,
                    capture.capture,
                    ir.channel_count()
                );
            }
            let processor = build_mono_ir_processor_from_wav(
                &resolved_ir_path.to_string_lossy(),
                sample_rate,
            )?;
            Ok(StageProcessor::Mono(processor))
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
    Ok(format!("ir='{}'", capture.ir_path))
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

fn resolve_ir_path(asset_path: &str) -> PathBuf {
    let raw = PathBuf::from(asset_path);
    if raw.is_absolute() || raw.exists() {
        return raw;
    }

    if let Ok(asset_root) = std::env::var("OPENRIG_ASSET_ROOT") {
        let candidate = Path::new(&asset_root).join(asset_path);
        if candidate.exists() {
            return candidate;
        }
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(asset_path)
}
