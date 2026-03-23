use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};

use crate::registry::{build_dual_mono_delay_processor, DelayModelDefinition};
use crate::DelayBackendKind;
use crate::shared::{
    clamp_feedback, clamp_mix, clamp_time_ms, process_simple_delay, DelayLine, MAX_DELAY_MS,
    MAX_FEEDBACK, MIN_DELAY_MS,
};

pub const MODEL_ID: &str = "slapback";
pub const DISPLAY_NAME: &str = "Slapback Delay";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlapbackParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
}

impl Default for SlapbackParams {
    fn default() -> Self {
        Self {
            time_ms: 110.0,
            feedback: 0.18,
            mix: 0.28,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "delay".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Slapback Delay".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "time_ms",
                "Time",
                None,
                Some(SlapbackParams::default().time_ms),
                MIN_DELAY_MS,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(SlapbackParams::default().feedback),
                0.0,
                MAX_FEEDBACK,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(SlapbackParams::default().mix),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<SlapbackParams> {
    Ok(SlapbackParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: required_f32(params, "feedback").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)?,
    })
}

pub struct SlapbackDelay {
    params: SlapbackParams,
    line: DelayLine,
}

impl SlapbackDelay {
    pub fn new(params: SlapbackParams, sample_rate: f32) -> Self {
        let params = SlapbackParams {
            time_ms: clamp_time_ms(params.time_ms),
            feedback: clamp_feedback(params.feedback),
            mix: clamp_mix(params.mix),
        };
        Self {
            line: DelayLine::new(params.time_ms, sample_rate),
            params,
        }
    }
}

impl MonoProcessor for SlapbackDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        process_simple_delay(&mut self.line, input, self.params.feedback, self.params.mix)
    }
}

pub fn build_mono_processor(
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    Ok(Box::new(SlapbackDelay::new(
        params_from_set(params)?,
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
    build_dual_mono_delay_processor(layout, || build_mono_processor(params, sample_rate))
}

pub const MODEL_DEFINITION: DelayModelDefinition = DelayModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "",
    backend_kind: DelayBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

#[cfg(test)]
mod tests {
    use super::*;
    use block_core::MonoProcessor;

    #[test]
    fn slapback_outputs_finite_values() {
        let mut delay = SlapbackDelay::new(SlapbackParams::default(), 48_000.0);
        for _ in 0..10_000 {
            let output = delay.process_sample(0.2);
            assert!(output.is_finite());
        }
    }
}
