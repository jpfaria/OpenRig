use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "fender_deluxe_reverb_oxford";
pub const DISPLAY_NAME: &str = "Deluxe Reverb Oxford";
const BRAND: &str = "fender";

macro_rules! capture {
    ($p1:literal, $ir_file:literal) => {
        FenderDeluxeReverbOxfordCapture {
            capture: $p1,
            ir_file: $ir_file,
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FenderDeluxeReverbOxfordCapture {
    pub capture: &'static str,
    pub ir_file: &'static str,
}

pub const CAPTURES: &[FenderDeluxeReverbOxfordCapture] = &[
    capture!("big", "cabs/fender_deluxe_reverb_oxford/big.wav"),
    capture!("lean", "cabs/fender_deluxe_reverb_oxford/lean.wav"),
    capture!("big_48_24", "cabs/fender_deluxe_reverb_oxford/big_48_24.wav"),
    capture!("lean_48_24", "cabs/fender_deluxe_reverb_oxford/lean_48_24.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "cab".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "flavor",
            "Flavor",
            Some("Cab"),
            Some("big"),
            &[
                ("big", "Big"),
                ("lean", "Lean"),
                ("big_48_24", "Big 48/24"),
                ("lean_48_24", "Lean 48/24"),
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
            let wav_path = ir::resolve_ir_capture(capture.ir_file)?;
            
            let ir = IrAsset::load_from_wav(&wav_path)?;
            if ir.channel_count() != 1 {
                bail!(
                    "cab model '{}' capture '{}' must be mono, got {} channels",
                    MODEL_ID,
                    capture.capture,
                    ir.channel_count()
                );
            }
            let processor = build_mono_ir_processor_from_wav(&wav_path, sample_rate)?;
            Ok(BlockProcessor::Mono(processor))
        }
        AudioChannelLayout::Stereo => bail!(
            "cab model '{}' currently expects mono processor layout",
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

pub const MODEL_DEFINITION: CabModelDefinition = CabModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: CabBackendKind::Ir,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("asset_id='{}'", capture.ir_file))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static FenderDeluxeReverbOxfordCapture> {
    let requested = required_string(params, "flavor").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|capture| capture.capture == requested)
        .ok_or_else(|| {
            anyhow!(
                "cab model '{}' does not support flavor '{}'",
                MODEL_ID,
                requested
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{asset_summary, build_processor_for_model, model_schema, validate_params};
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, BlockProcessor};
    use domain::value_objects::ParameterValue;

    #[test]
    fn schema_exposes_flavor_select() {
        let schema = model_schema();

        assert_eq!(schema.parameters.len(), 1);
        assert_eq!(schema.parameters[0].path, "flavor");
    }

    #[test]
    fn rejects_unknown_flavor() {
        let mut params = ParameterSet::default();
        params.insert("flavor", ParameterValue::String("unknown".into()));

        let error = validate_params(&params).expect_err("unknown flavor should fail");
        assert!(error.to_string().contains("flavor"));
    }

    #[test]
    fn builds_mono_processor_for_curated_capture() {
        let mut params = ParameterSet::default();
        params.insert("flavor", ParameterValue::String("big".into()));

        let processor = build_processor_for_model(&params, 48_000.0, AudioChannelLayout::Mono)
            .expect("cab processor should build");

        match processor {
            BlockProcessor::Mono(_) => {}
            BlockProcessor::Stereo(_) => panic!("expected mono processor"),
        }

        let summary = asset_summary(&params).expect("asset summary should resolve");
        assert!(summary.contains("cab.fender_deluxe_reverb_oxford.big"));
    }
}
