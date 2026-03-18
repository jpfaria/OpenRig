use anyhow::{Error, Result};
use stage_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use stage_core::{
    calculate_coefficient, db_to_lin, EnvelopeFollower, ModelChannelSupport, MonoProcessor,
};

pub const MODEL_ID: &str = "compressor_studio_clean";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompressorParams {
    pub threshold: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_gain_db: f32,
    pub mix: f32,
}

impl Default for CompressorParams {
    fn default() -> Self {
        Self {
            threshold: -18.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 80.0,
            makeup_gain_db: 0.0,
            mix: 1.0,
        }
    }
}

pub fn supports_model(model: &str) -> bool {
    matches!(model, MODEL_ID | "studio_clean" | "compressor" | "basic")
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "compressor".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Studio Clean Compressor".to_string(),
        channel_support: ModelChannelSupport::Mono,
        stereo_processing: None,
        parameters: vec![
            float_parameter(
                "threshold",
                "Threshold",
                None,
                Some(CompressorParams::default().threshold),
                -60.0,
                0.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            float_parameter(
                "ratio",
                "Ratio",
                None,
                Some(CompressorParams::default().ratio),
                1.0,
                20.0,
                0.1,
                ParameterUnit::Ratio,
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
                "makeup_gain_db",
                "Makeup Gain",
                None,
                Some(CompressorParams::default().makeup_gain_db),
                -24.0,
                24.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(CompressorParams::default().mix),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<CompressorParams> {
    Ok(CompressorParams {
        threshold: required_f32(params, "threshold").map_err(Error::msg)?,
        ratio: required_f32(params, "ratio").map_err(Error::msg)?,
        attack_ms: required_f32(params, "attack_ms").map_err(Error::msg)?,
        release_ms: required_f32(params, "release_ms").map_err(Error::msg)?,
        makeup_gain_db: required_f32(params, "makeup_gain_db").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)?,
    })
}

pub struct StudioCleanCompressor {
    attack_ms: f32,
    release_ms: f32,
    threshold: f32,
    ratio: f32,
    makeup: f32,
    mix: f32,
    envelope: EnvelopeFollower,
    sample_rate: f32,
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
            attack_ms,
            release_ms,
            threshold: db_to_lin(threshold_db),
            ratio,
            makeup: db_to_lin(makeup_gain_db),
            mix: mix.clamp(0.0, 1.0),
            envelope: EnvelopeFollower::from_ms(attack_ms, release_ms, sample_rate),
            sample_rate,
        }
    }

    pub fn set_attack_ms(&mut self, attack_ms: f32) {
        self.attack_ms = attack_ms;
        self.envelope
            .set_attack_coeff(calculate_coefficient(attack_ms, self.sample_rate));
    }

    pub fn set_release_ms(&mut self, release_ms: f32) {
        self.release_ms = release_ms;
        self.envelope
            .set_release_coeff(calculate_coefficient(release_ms, self.sample_rate));
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
        params.makeup_gain_db,
        params.mix,
        sample_rate,
    )))
}
