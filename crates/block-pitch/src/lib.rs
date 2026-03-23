use anyhow::{anyhow, Result};
use block_core::param::{float_parameter, ModelParameterSchema, ParameterSet, ParameterUnit};
use block_core::{ModelAudioMode, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PitchBackendKind {
    Native,
    Nam,
    Ir,
}

const MODEL_ID: &str = "octave_simple";
#[allow(dead_code)]
const DISPLAY_NAME: &str = "Simple Octave";
const SUPPORTED_MODELS: &[&str] = &[MODEL_ID];

pub fn supported_models() -> &'static [&'static str] {
    SUPPORTED_MODELS
}

pub fn pitch_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if model != MODEL_ID {
        return Err(anyhow!("unsupported pitch model '{model}'"));
    }

    Ok(ModelParameterSchema {
        effect_type: "pitch".into(),
        model: MODEL_ID.into(),
        display_name: "Simple Octave".into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "semitones",
                "Semitones",
                None,
                Some(12.0),
                -12.0,
                12.0,
                12.0,
                ParameterUnit::Semitones,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    })
}

pub fn pitch_model_visual(model_id: &str) -> Option<ModelVisualData> {
    if model_id != MODEL_ID {
        return None;
    }
    Some(ModelVisualData {
        brand: "",
        type_label: "NATIVE",
        supported_instruments: block_core::ALL_INSTRUMENTS,
        knob_layout: &[],
    })
}

pub fn validate_pitch_params(model: &str, params: &ParameterSet) -> Result<()> {
    let schema = pitch_model_schema(model)?;
    params
        .normalized_against(&schema)
        .map(|_| ())
        .map_err(|error| anyhow!(error))
}

#[cfg(test)]
mod tests {
    use super::{pitch_model_schema, supported_models, validate_pitch_params};
    use block_core::param::ParameterSet;

    #[test]
    fn exposes_example_pitch_model() {
        assert_eq!(supported_models(), &["octave_simple"]);
        let schema = pitch_model_schema("octave_simple").expect("schema");
        assert_eq!(schema.effect_type, "pitch");
        assert_eq!(schema.model, "octave_simple");
        assert_eq!(schema.parameters.len(), 2);
    }

    #[test]
    fn defaults_normalize() {
        let params = ParameterSet::default();
        validate_pitch_params("octave_simple", &params).expect("defaults should normalize");
    }
}
