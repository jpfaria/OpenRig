use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "martin_d_35";
pub const DISPLAY_NAME: &str = "D-35";
const BRAND: &str = "martin";

macro_rules! capture {
    ($p1:literal, $ir_file:literal) => {
        MartinD35Capture {
            voicing: $p1,
            ir_file: $ir_file,
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MartinD35Capture {
    pub voicing: &'static str,
    pub ir_file: &'static str,
}

pub const CAPTURES: &[MartinD35Capture] = &[
    capture!("exp", "body/martin_d_35/exp.wav"),
    capture!("piezo_1", "body/martin_d_35/piezo_1.wav"),
    capture!("piezo_2", "body/martin_d_35/piezo_2.wav"),
    capture!("piezo_3", "body/martin_d_35/piezo_3.wav"),
    capture!("soundboard_1", "body/martin_d_35/soundboard_1.wav"),
    capture!("soundboard_2", "body/martin_d_35/soundboard_2.wav"),
    capture!("blend", "body/martin_d_35/blend.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_BODY.to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "voicing",
            "Voicing",
            Some("Body"),
            Some("blend"),
            &[
                ("exp", "Expression"),
                ("piezo_1", "Piezo 1"),
                ("piezo_2", "Piezo 2"),
                ("piezo_3", "Piezo 3"),
                ("soundboard_1", "Soundboard 1"),
                ("soundboard_2", "Soundboard 2"),
                ("blend", "Blend"),
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
                    "body model '{}' capture must be mono, got {} channels",
                    MODEL_ID,
                    ir.channel_count()
                );
            }
            let processor = build_mono_ir_processor_from_wav(&wav_path, sample_rate)?;
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
    Ok(format!("asset_id='{}'", capture.ir_file))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static MartinD35Capture> {
    let requested = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.voicing == requested)
        .ok_or_else(|| {
            anyhow!(
                "body model '{}' does not support voicing '{}'",
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
    fn schema_exposes_voicing_parameter() {
        let schema = model_schema();
        assert_eq!(schema.parameters.len(), 1);
        assert_eq!(schema.parameters[0].path, "voicing");
    }

    #[test]
    fn rejects_unknown_voicing() {
        let mut params = ParameterSet::default();
        params.insert("voicing", ParameterValue::String("unknown".into()));

        let error = validate_params(&params).expect_err("unknown voicing should fail");
        assert!(error.to_string().contains("voicing"));
    }

    #[test]
    #[ignore]
    fn builds_mono_processor() {
        let mut params = ParameterSet::default();
        params.insert("voicing", ParameterValue::String("blend".into()));

        let processor = build_processor_for_model(&params, 48_000.0, AudioChannelLayout::Mono)
            .expect("body processor should build");

        match processor {
            BlockProcessor::Mono(_) => {}
            BlockProcessor::Stereo(_) => panic!("expected mono processor"),
        }

        let summary = asset_summary(&params).expect("asset summary should resolve");
        assert!(summary.contains("body.martin_d_35.blend"));
    }
}
