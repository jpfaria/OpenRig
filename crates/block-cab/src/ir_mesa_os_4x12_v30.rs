use anyhow::{anyhow, bail, Result};
use asset_runtime::{materialize, EmbeddedAsset};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "mesa_os_4x12_v30";
pub const DISPLAY_NAME: &str = "Oversized 4x12 V30";
const BRAND: &str = "mesa";

macro_rules! capture {
    ($capture:literal, $asset_id:literal, $relative_path:literal) => {
        MesaOs4x12V30Capture {
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
pub struct MesaOs4x12V30Capture {
    pub capture: &'static str,
    pub asset: EmbeddedAsset,
}

pub const CAPTURES: &[MesaOs4x12V30Capture] = &[
    capture!(
        "at2020_1",
        "cab.mesa_os_4x12_v30.at2020_1",
        "captures/ir/cabs/mesa_os_4x12_v30/at2020_1.wav"
    ),
    capture!(
        "at2020_2",
        "cab.mesa_os_4x12_v30.at2020_2",
        "captures/ir/cabs/mesa_os_4x12_v30/at2020_2.wav"
    ),
    capture!(
        "room_left_at2020",
        "cab.mesa_os_4x12_v30.room_left_at2020",
        "captures/ir/cabs/mesa_os_4x12_v30/room_left_at2020.wav"
    ),
    capture!(
        "room_right_at2020",
        "cab.mesa_os_4x12_v30.room_right_at2020",
        "captures/ir/cabs/mesa_os_4x12_v30/room_right_at2020.wav"
    ),
    capture!(
        "sm57_1",
        "cab.mesa_os_4x12_v30.sm57_1",
        "captures/ir/cabs/mesa_os_4x12_v30/sm57_1.wav"
    ),
    capture!(
        "sm57_2",
        "cab.mesa_os_4x12_v30.sm57_2",
        "captures/ir/cabs/mesa_os_4x12_v30/sm57_2.wav"
    ),
    capture!(
        "sm58_1",
        "cab.mesa_os_4x12_v30.sm58_1",
        "captures/ir/cabs/mesa_os_4x12_v30/sm58_1.wav"
    ),
    capture!(
        "sm58_2",
        "cab.mesa_os_4x12_v30.sm58_2",
        "captures/ir/cabs/mesa_os_4x12_v30/sm58_2.wav"
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
            Some("at2020_1"),
            &[
                ("at2020_1", "AT2020 1"),
                ("at2020_2", "AT2020 2"),
                ("room_left_at2020", "Room Left AT2020"),
                ("room_right_at2020", "Room Right AT2020"),
                ("sm57_1", "SM57 1"),
                ("sm57_2", "SM57 2"),
                ("sm58_1", "SM58 1"),
                ("sm58_2", "SM58 2"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static MesaOs4x12V30Capture> {
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
        params.insert("mic", ParameterValue::String("at2020_1".into()));

        let processor = build_processor_for_model(&params, 48_000.0, AudioChannelLayout::Mono)
            .expect("cab processor should build");

        match processor {
            BlockProcessor::Mono(_) => {}
            BlockProcessor::Stereo(_) => panic!("expected mono processor"),
        }

        let summary = asset_summary(&params).expect("asset summary should resolve");
        assert!(summary.contains("cab.mesa_os_4x12_v30.at2020_1"));
    }
}
