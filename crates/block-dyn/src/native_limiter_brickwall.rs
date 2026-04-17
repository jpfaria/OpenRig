use anyhow::{Error, Result};
use crate::registry::DynModelDefinition;
use crate::DynBackendKind;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{db_to_lin, EnvelopeFollower, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "limiter_brickwall";
pub const DISPLAY_NAME: &str = "Brick Wall Limiter";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimiterParams {
    pub threshold: f32,  // dBFS
    pub release_ms: f32,
    pub ceiling: f32,    // dBFS
}

impl Default for LimiterParams {
    fn default() -> Self {
        Self {
            threshold: -1.0,
            release_ms: 50.0,
            ceiling: -0.1,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    let defaults = LimiterParams::default();
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
                Some(defaults.threshold),
                -20.0,
                0.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            float_parameter(
                "release_ms",
                "Release",
                None,
                Some(defaults.release_ms),
                1.0,
                500.0,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "ceiling",
                "Ceiling",
                None,
                Some(defaults.ceiling),
                -6.0,
                0.0,
                0.1,
                ParameterUnit::Decibels,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<LimiterParams> {
    Ok(LimiterParams {
        threshold: required_f32(params, "threshold").map_err(Error::msg)?,
        release_ms: required_f32(params, "release_ms").map_err(Error::msg)?,
        ceiling: required_f32(params, "ceiling").map_err(Error::msg)?,
    })
}

pub struct BrickWallLimiter {
    threshold: f32,  // linear
    ceiling: f32,    // linear
    // Attack is near-instantaneous (1 sample) for brickwall behavior.
    // EnvelopeFollower used with 0.01ms attack and configurable release.
    envelope: EnvelopeFollower,
}

impl BrickWallLimiter {
    pub fn new(threshold_db: f32, release_ms: f32, ceiling_db: f32, sample_rate: f32) -> Self {
        Self {
            threshold: db_to_lin(threshold_db),
            ceiling: db_to_lin(ceiling_db),
            envelope: EnvelopeFollower::from_ms(0.01, release_ms, sample_rate),
        }
    }
}

impl MonoProcessor for BrickWallLimiter {
    fn process_sample(&mut self, input: f32) -> f32 {
        let level = input.abs().max(1e-10);
        self.envelope.process(level);
        let env = self.envelope.value();

        // Gain reduction: bring any peak above threshold down to threshold
        let gain = if env > self.threshold {
            self.threshold / env
        } else {
            1.0
        };

        // Apply gain then hard clip to ceiling (safety net)
        (input * gain).clamp(-self.ceiling, self.ceiling)
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let p = params_from_set(params)?;
    Ok(Box::new(BrickWallLimiter::new(
        p.threshold,
        p.release_ms,
        p.ceiling,
        sample_rate,
    )))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: block_core::AudioChannelLayout,
) -> Result<block_core::BlockProcessor> {
    match layout {
        block_core::AudioChannelLayout::Mono => {
            Ok(block_core::BlockProcessor::Mono(build_processor(params, sample_rate)?))
        }
        block_core::AudioChannelLayout::Stereo => anyhow::bail!(
            "limiter model '{}' is mono-only and cannot build native stereo processing",
            MODEL_ID
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn default_params() -> block_core::param::ParameterSet {
        let schema = model_schema();
        block_core::param::ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize")
    }

    #[test]
    fn process_frame_silence_output_is_finite() {
        let params = default_params();
        let mut proc = build_processor(&params, 44100.0).unwrap();
        for i in 0..1024 {
            let out = proc.process_sample(0.0);
            assert!(out.is_finite(), "non-finite at sample {i}: {out}");
        }
    }

    #[test]
    fn process_frame_sine_output_is_finite() {
        let params = default_params();
        let mut proc = build_processor(&params, 44100.0).unwrap();
        for i in 0..1024 {
            let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            let out = proc.process_sample(input);
            assert!(out.is_finite(), "non-finite at sample {i}: {out}");
        }
    }

    #[test]
    fn process_block_1024_frames_all_finite() {
        let params = default_params();
        let mut proc = build_processor(&params, 44100.0).unwrap();
        let mut buf: Vec<f32> = (0..1024)
            .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
            .collect();
        proc.process_block(&mut buf);
        for (i, &s) in buf.iter().enumerate() {
            assert!(s.is_finite(), "non-finite at index {i}: {s}");
        }
    }

    #[test]
    fn process_limiter_clips_hot_signal_below_ceiling() {
        let params = default_params();
        let mut proc = build_processor(&params, 44100.0).unwrap();
        // Feed a hot signal (amplitude 2.0) that should be limited
        for _ in 0..1024 {
            let out = proc.process_sample(2.0);
            assert!(out.is_finite(), "output should be finite");
            assert!(out.abs() <= 1.0, "output {out} should be limited below ceiling");
        }
    }
}

pub const MODEL_DEFINITION: DynModelDefinition = DynModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: DynBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
