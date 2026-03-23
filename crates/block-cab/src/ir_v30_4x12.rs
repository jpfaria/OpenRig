use anyhow::{anyhow, bail, Result};
use asset_runtime::{materialize, EmbeddedAsset};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "v30_4x12";
pub const DISPLAY_NAME: &str = "V30 4x12";
const BRAND: &str = "";

macro_rules! capture {
    ($capture:literal, $asset_id:literal, $relative_path:literal) => {
        V304x12Capture {
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
pub struct V304x12Capture {
    pub capture: &'static str,
    pub asset: EmbeddedAsset,
}

pub const CAPTURES: &[V304x12Capture] = &[
    capture!(
        "sm57_balanced",
        "cab.v30_4x12.sm57_balanced",
        "captures/ir/cabs/v30_4x12/sm57_balanced.wav"
    ),
    capture!(
        "sm57_dark",
        "cab.v30_4x12.sm57_dark",
        "captures/ir/cabs/v30_4x12/sm57_dark.wav"
    ),
    capture!(
        "sm57_fat",
        "cab.v30_4x12.sm57_fat",
        "captures/ir/cabs/v30_4x12/sm57_fat.wav"
    ),
    capture!(
        "lo_gn_all_room_mono",
        "cab.v30_4x12.lo_gn_all_room_mono",
        "captures/ir/cabs/v30_4x12/lo_gn_all_room_mono.wav"
    ),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "cab".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "mic",
            "Mic",
            Some("Cab"),
            Some("sm57_balanced"),
            &[
                ("sm57_balanced", "SM57 Balanced"),
                ("sm57_dark", "SM57 Dark"),
                ("sm57_fat", "SM57 Fat"),
                ("lo_gn_all_room_mono", "Lo Gn All Room Mono"),
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
    Ok(format!("asset_id='{}'", capture.asset.id))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static V304x12Capture> {
    let requested = required_string(params, "mic").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|capture| capture.capture == requested)
        .ok_or_else(|| {
            anyhow!(
                "cab model '{}' does not support mic '{}'",
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
    fn schema_exposes_mic_select() {
        let schema = model_schema();

        assert_eq!(schema.parameters.len(), 1);
        assert_eq!(schema.parameters[0].path, "mic");
    }

    #[test]
    fn rejects_unknown_mic() {
        let mut params = ParameterSet::default();
        params.insert("mic", ParameterValue::String("unknown".into()));

        let error = validate_params(&params).expect_err("unknown mic should fail");
        assert!(error.to_string().contains("mic"));
    }

    #[test]
    fn builds_mono_processor_for_curated_capture() {
        let mut params = ParameterSet::default();
        params.insert("mic", ParameterValue::String("sm57_balanced".into()));

        let processor = build_processor_for_model(&params, 48_000.0, AudioChannelLayout::Mono)
            .expect("cab processor should build");

        match processor {
            BlockProcessor::Mono(_) => {}
            BlockProcessor::Stereo(_) => panic!("expected mono processor"),
        }

        let summary = asset_summary(&params).expect("asset summary should resolve");
        assert!(summary.contains("cab.v30_4x12.sm57_balanced"));
    }
}
