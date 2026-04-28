use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "mesa_os_4x12_v30";
pub const DISPLAY_NAME: &str = "Oversized 4x12 V30";
const BRAND: &str = "mesa";

// Two-axis pack: mic × take.
// AT2020 has close (take_1/2) and ambient (room_left/right) takes.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (mic, take, file)
    ("at2020", "take_1",     "cabs/mesa_os_4x12_v30/at2020_1.wav"),
    ("at2020", "take_2",     "cabs/mesa_os_4x12_v30/at2020_2.wav"),
    ("at2020", "room_left",  "cabs/mesa_os_4x12_v30/room_left_at2020.wav"),
    ("at2020", "room_right", "cabs/mesa_os_4x12_v30/room_right_at2020.wav"),
    ("sm57",   "take_1",     "cabs/mesa_os_4x12_v30/sm57_1.wav"),
    ("sm57",   "take_2",     "cabs/mesa_os_4x12_v30/sm57_2.wav"),
    ("sm57",   "m160_mix",   "cabs/mesa_os_4x12_v30/sm57_m160_mix.wav"),
    ("sm58",   "take_1",     "cabs/mesa_os_4x12_v30/sm58_1.wav"),
    ("sm58",   "take_2",     "cabs/mesa_os_4x12_v30/sm58_2.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "cab".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            enum_parameter(
                "mic",
                "Mic",
                Some("Cab"),
                Some("sm57"),
                &[
                    ("at2020", "AT2020"),
                    ("sm57",   "SM57"),
                    ("sm58",   "SM58"),
                ],
            ),
            enum_parameter(
                "take",
                "Take",
                Some("Cab"),
                Some("take_1"),
                &[
                    ("take_1",     "Take 1"),
                    ("take_2",     "Take 2"),
                    ("room_left",  "Room Left"),
                    ("room_right", "Room Right"),
                    ("m160_mix",   "M160 Mix"),
                ],
            ),
        ],
    }
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => {
            let path = resolve_capture(params)?;
            let wav_path = ir::resolve_ir_capture(path)?;
            let ir = IrAsset::load_from_wav(&wav_path)?;
            if ir.channel_count() != 1 {
                bail!(
                    "cab model '{}' capture must be mono, got {} channels",
                    MODEL_ID,
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
    let path = resolve_capture(params)?;
    Ok(format!("asset_id='{}'", path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let mic = required_string(params, "mic").map_err(anyhow::Error::msg)?;
    let take = required_string(params, "take").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(m, t, _)| *m == mic && *t == take)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "cab '{}' has no capture for mic={} take={}",
                MODEL_ID, mic, take
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{build_processor_for_model, model_schema, validate_params};
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, BlockProcessor};
    use domain::value_objects::ParameterValue;

    #[test]
    fn schema_exposes_two_axes() {
        let schema = model_schema();

        assert_eq!(schema.parameters.len(), 2);
        assert_eq!(schema.parameters[0].path, "mic");
        assert_eq!(schema.parameters[1].path, "take");
    }

    #[test]
    fn rejects_unknown_mic() {
        let mut params = ParameterSet::default();
        params.insert("mic", ParameterValue::String("unknown".into()));
        params.insert("take", ParameterValue::String("take_1".into()));

        let error = validate_params(&params).expect_err("unknown mic should fail");
        assert!(error.to_string().contains("mic"));
    }

    #[test]
    #[ignore]
    fn builds_mono_processor_for_curated_capture() {
        let mut params = ParameterSet::default();
        params.insert("mic", ParameterValue::String("at2020".into()));
        params.insert("take", ParameterValue::String("take_1".into()));

        let processor = build_processor_for_model(&params, 48_000.0, AudioChannelLayout::Mono)
            .expect("cab processor should build");

        match processor {
            BlockProcessor::Mono(_) => {}
            BlockProcessor::Stereo(_) => panic!("expected mono processor"),
        }
    }
}
