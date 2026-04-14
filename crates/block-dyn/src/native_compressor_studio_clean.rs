use anyhow::{Error, Result};
use crate::registry::DynModelDefinition;
use crate::DynBackendKind;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{db_to_lin, EnvelopeFollower, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "compressor_studio_clean";
pub const DISPLAY_NAME: &str = "Studio Clean Compressor";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompressorParams {
    pub threshold: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_gain: f32,
    pub mix: f32,
}

impl Default for CompressorParams {
    fn default() -> Self {
        Self {
            threshold: 70.0,
            ratio: 16.0,
            attack_ms: 10.0,
            release_ms: 80.0,
            makeup_gain: 50.0,
            mix: 100.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "dynamics".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Studio Clean Compressor".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "threshold",
                "Threshold",
                None,
                Some(CompressorParams::default().threshold),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "ratio",
                "Ratio",
                None,
                Some(CompressorParams::default().ratio),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "attack_ms",
                "Attack",
                None,
                Some(CompressorParams::default().attack_ms),
                0.1,
                200.0,
                0.1,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "release_ms",
                "Release",
                None,
                Some(CompressorParams::default().release_ms),
                1.0,
                500.0,
                0.1,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "makeup_gain",
                "Makeup Gain",
                None,
                Some(CompressorParams::default().makeup_gain),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(CompressorParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<CompressorParams> {
    let threshold_pct = required_f32(params, "threshold").map_err(Error::msg)?;
    let ratio_pct = required_f32(params, "ratio").map_err(Error::msg)?;
    let makeup_pct = required_f32(params, "makeup_gain").map_err(Error::msg)?;
    let mix_pct = required_f32(params, "mix").map_err(Error::msg)?;
    Ok(CompressorParams {
        threshold: -60.0 + (threshold_pct / 100.0) * 60.0,
        ratio: 1.0 + (ratio_pct / 100.0) * 19.0,
        attack_ms: required_f32(params, "attack_ms").map_err(Error::msg)?,
        release_ms: required_f32(params, "release_ms").map_err(Error::msg)?,
        makeup_gain: -24.0 + (makeup_pct / 100.0) * 48.0,
        mix: mix_pct / 100.0,
    })
}

pub struct StudioCleanCompressor {
    threshold: f32,
    ratio: f32,
    makeup: f32,
    mix: f32,
    envelope: EnvelopeFollower,
}

impl StudioCleanCompressor {
    pub fn new(
        threshold_db: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
        makeup_gain_db: f32,
        mix: f32,
        sample_rate: f32,
    ) -> Self {
        Self {
            threshold: db_to_lin(threshold_db),
            ratio,
            makeup: db_to_lin(makeup_gain_db),
            mix: mix.clamp(0.0, 1.0),
            envelope: EnvelopeFollower::from_ms(attack_ms, release_ms, sample_rate),
        }
    }
}

impl MonoProcessor for StudioCleanCompressor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let level_in = input.abs().max(1e-10);
        self.envelope.process(level_in);
        let env = self.envelope.value();
        let over_threshold = (env / self.threshold).max(1.0);
        let gain_reduction = if over_threshold > 1.0 {
            over_threshold.powf((1.0 / self.ratio) - 1.0)
        } else {
            1.0
        };
        let compressed = input * gain_reduction * self.makeup;
        (1.0 - self.mix).mul_add(input, self.mix * compressed)
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let params = params_from_set(params)?;
    Ok(Box::new(StudioCleanCompressor::new(
        params.threshold,
        params.ratio,
        params.attack_ms,
        params.release_ms,
        params.makeup_gain,
        params.mix,
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
            "compressor model '{}' is mono-only and cannot build native stereo processing",
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
