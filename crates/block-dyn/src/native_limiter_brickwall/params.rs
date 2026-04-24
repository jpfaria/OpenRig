//! User-facing parameter schema and parsing.

use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::ModelAudioMode;

pub const MODEL_ID: &str = "limiter_brickwall";
pub const DISPLAY_NAME: &str = "Brick Wall Limiter";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimiterParams {
    pub threshold_db: f32,
    pub ceiling_db: f32,
    pub release_ms: f32,
    pub lookahead_ms: f32,
    pub knee_db: f32,
}

impl Default for LimiterParams {
    fn default() -> Self {
        Self {
            threshold_db: -1.0,
            ceiling_db: -0.1,
            release_ms: 100.0,
            lookahead_ms: 3.0,
            knee_db: 2.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    let d = LimiterParams::default();
    ModelParameterSchema {
        effect_type: "dynamics".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "threshold",
                "Threshold",
                None,
                Some(d.threshold_db),
                -30.0,
                0.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            float_parameter(
                "ceiling",
                "Ceiling",
                None,
                Some(d.ceiling_db),
                -6.0,
                0.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            float_parameter(
                "release_ms",
                "Release",
                None,
                Some(d.release_ms),
                10.0,
                1000.0,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "lookahead_ms",
                "Lookahead",
                None,
                Some(d.lookahead_ms),
                1.0,
                10.0,
                0.1,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "knee_db",
                "Knee",
                None,
                Some(d.knee_db),
                0.0,
                6.0,
                0.1,
                ParameterUnit::Decibels,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<LimiterParams> {
    Ok(LimiterParams {
        threshold_db: required_f32(params, "threshold").map_err(Error::msg)?,
        ceiling_db: required_f32(params, "ceiling").map_err(Error::msg)?,
        release_ms: required_f32(params, "release_ms").map_err(Error::msg)?,
        lookahead_ms: required_f32(params, "lookahead_ms").map_err(Error::msg)?,
        knee_db: required_f32(params, "knee_db").map_err(Error::msg)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_against_schema() {
        let schema = model_schema();
        let result = ParameterSet::default().normalized_against(&schema);
        assert!(
            result.is_ok(),
            "defaults must normalize: {:?}",
            result.err()
        );
    }

    #[test]
    fn params_from_set_reads_defaults() {
        let schema = model_schema();
        let ps = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults");
        let params = params_from_set(&ps).expect("parse");
        assert_eq!(params, LimiterParams::default());
    }

    #[test]
    fn schema_has_all_expected_params() {
        let schema = model_schema();
        let names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        for required in &[
            "threshold",
            "ceiling",
            "release_ms",
            "lookahead_ms",
            "knee_db",
        ] {
            assert!(names.contains(required), "missing param: {required}");
        }
    }
}
