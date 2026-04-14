use anyhow::{Error, Result};
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};
use std::f32::consts::TAU;

pub const MODEL_ID: &str = "tremolo_sine";
pub const DISPLAY_NAME: &str = "Sine Tremolo";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TremoloParams {
    pub rate_hz: f32,
    pub depth: f32,
}

impl Default for TremoloParams {
    fn default() -> Self {
        Self {
            rate_hz: 4.0,
            depth: 50.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "modulation".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Sine Tremolo".to_string(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "rate_hz",
                "Rate",
                None,
                Some(TremoloParams::default().rate_hz),
                0.1,
                20.0,
                0.1,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(TremoloParams::default().depth),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<TremoloParams> {
    Ok(TremoloParams {
        rate_hz: required_f32(params, "rate_hz").map_err(Error::msg)?,
        depth: required_f32(params, "depth").map_err(Error::msg)? / 100.0,
    })
}

pub struct SineTremolo {
    rate_hz: f32,
    depth: f32,
    sample_rate: f32,
    phase: f32,
}

impl SineTremolo {
    pub fn new(rate_hz: f32, depth: f32, sample_rate: f32) -> Self {
        Self {
            rate_hz,
            depth: depth.clamp(0.0, 1.0),
            sample_rate,
            phase: 0.0,
        }
    }
}

impl MonoProcessor for SineTremolo {
    fn process_sample(&mut self, input: f32) -> f32 {
        let lfo = 0.5 * (1.0 + self.phase.sin());
        let gain = 1.0 - (self.depth * lfo);
        self.phase = (self.phase + (TAU * self.rate_hz / self.sample_rate)).rem_euclid(TAU);
        input * gain
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let params = params_from_set(params)?;
    Ok(Box::new(SineTremolo::new(
        params.rate_hz,
        params.depth,
        sample_rate,
    )))
}

pub fn build_processor_with_phase(params: &ParameterSet, sample_rate: f32, phase_offset: f32) -> Result<Box<dyn MonoProcessor>> {
    let params = params_from_set(params)?;
    let mut t = SineTremolo::new(params.rate_hz, params.depth, sample_rate);
    t.phase = phase_offset;
    Ok(Box::new(t))
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
        block_core::AudioChannelLayout::Stereo => {
            struct StereoTremolo {
                left: Box<dyn block_core::MonoProcessor>,
                right: Box<dyn block_core::MonoProcessor>,
            }

            impl block_core::StereoProcessor for StereoTremolo {
                fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
                    [
                        self.left.process_sample(input[0]),
                        self.right.process_sample(input[1]),
                    ]
                }
            }

            Ok(block_core::BlockProcessor::Stereo(Box::new(StereoTremolo {
                left: build_processor(params, sample_rate)?,
                right: build_processor_with_phase(params, sample_rate, std::f32::consts::PI)?,
            })))
        }
    }
}

pub const MODEL_DEFINITION: ModModelDefinition = ModModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: ModBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_sample_silence_output_finite() {
        let mut trem = SineTremolo::new(4.0, 0.5, 44_100.0);
        for i in 0..1024 {
            let out = MonoProcessor::process_sample(&mut trem, 0.0);
            assert!(out.is_finite(), "output not finite at sample {i}");
        }
    }

    #[test]
    fn process_sample_silence_is_zero() {
        let mut trem = SineTremolo::new(4.0, 0.5, 44_100.0);
        for _ in 0..1024 {
            let out = MonoProcessor::process_sample(&mut trem, 0.0);
            assert_eq!(out, 0.0, "tremolo of silence should be silence");
        }
    }

    #[test]
    fn process_sample_sine_output_finite_and_nonzero() {
        let mut trem = SineTremolo::new(4.0, 0.5, 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..1024 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let out = MonoProcessor::process_sample(&mut trem, input);
            assert!(out.is_finite(), "output not finite at sample {i}");
            if out.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output for sine input");
    }

    #[test]
    fn process_block_all_finite() {
        let mut trem = SineTremolo::new(4.0, 0.5, 44_100.0);
        let sr = 44_100.0_f32;
        let mut buffer: Vec<f32> = (0..1024)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin())
            .collect();
        MonoProcessor::process_block(&mut trem, &mut buffer);
        for (i, s) in buffer.iter().enumerate() {
            assert!(s.is_finite(), "output not finite at frame {i}");
        }
    }

    #[test]
    fn process_sample_output_bounded_by_input() {
        let mut trem = SineTremolo::new(4.0, 1.0, 44_100.0);
        for _ in 0..1024 {
            let out = MonoProcessor::process_sample(&mut trem, 1.0);
            assert!(out >= 0.0 && out <= 1.0,
                "tremolo output {out} should be in [0,1] for unit input with full depth");
        }
    }
}
