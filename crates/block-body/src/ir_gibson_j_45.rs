use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "gibson_j_45";
pub const DISPLAY_NAME: &str = "J-45";
const BRAND: &str = "gibson";

macro_rules! capture {
    ($position:literal, $flavor:literal, $ir_file:literal) => {
        GibsonJ45Capture {
            position: $position,
            flavor: $flavor,
            ir_file: $ir_file,
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GibsonJ45Capture {
    pub position: &'static str,
    pub flavor: &'static str,
    pub ir_file: &'static str,
}

pub const CAPTURES: &[GibsonJ45Capture] = &[
    // Position 1
    capture!("1", "standard", "body/gibson_j_45/position_1.wav"),
    capture!("1", "blend", "body/gibson_j_45/position_1_blend.wav"),
    capture!("1", "match", "body/gibson_j_45/position_1_match.wav"),
    capture!("1", "jf", "body/gibson_j_45/position_1_jf.wav"),
    // Position 2
    capture!("2", "standard", "body/gibson_j_45/position_2.wav"),
    capture!("2", "blend", "body/gibson_j_45/position_2_blend.wav"),
    capture!("2", "match", "body/gibson_j_45/position_2_match.wav"),
    capture!("2", "jf", "body/gibson_j_45/position_2_jf.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_BODY.to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            enum_parameter(
                "position",
                "Position",
                Some("Body"),
                Some("1"),
                &[("1", "Position 1"), ("2", "Position 2")],
            ),
            enum_parameter(
                "flavor",
                "Flavor",
                Some("Body"),
                Some("standard"),
                &[("standard", "Standard"), ("blend", "Blend"), ("match", "Match"), ("jf", "JF Flavor")],
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static GibsonJ45Capture> {
    let position = required_string(params, "position").map_err(anyhow::Error::msg)?;
    let flavor = required_string(params, "flavor").map_err(anyhow::Error::msg)?;

    CAPTURES
        .iter()
        .find(|c| c.position == position && c.flavor == flavor)
        .ok_or_else(|| {
            anyhow!(
                "body model '{}' does not support position='{}' flavor='{}'",
                MODEL_ID,
                position,
                flavor
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
    fn schema_exposes_parameters() {
        let schema = model_schema();
        assert_eq!(schema.parameters.len(), 2);
        assert_eq!(schema.parameters[0].path, "position");
        assert_eq!(schema.parameters[1].path, "flavor");
    }

    #[test]
    fn rejects_unknown_position() {
        let mut params = ParameterSet::default();
        params.insert("position", ParameterValue::String("9".into()));
        params.insert("flavor", ParameterValue::String("standard".into()));

        let error = validate_params(&params).expect_err("unknown position should fail");
        assert!(error.to_string().contains("position"));
    }

    #[test]
    #[ignore]
    fn builds_mono_processor() {
        let mut params = ParameterSet::default();
        params.insert("position", ParameterValue::String("1".into()));
        params.insert("flavor", ParameterValue::String("standard".into()));

        let processor = build_processor_for_model(&params, 48_000.0, AudioChannelLayout::Mono)
            .expect("body processor should build");

        match processor {
            BlockProcessor::Mono(_) => {}
            BlockProcessor::Stereo(_) => panic!("expected mono processor"),
        }

        let summary = asset_summary(&params).expect("asset summary should resolve");
        assert!(summary.contains("body.gibson_j_45.1_standard"));
    }
}
