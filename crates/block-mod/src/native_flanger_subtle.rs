//! "Subtle" flanger variant — chorus-like, longer base delay, modest
//! feedback. Sits behind a clean tone without dominating. Same engine
//! as `native_flanger.rs`; only the hidden tuning differs.

use crate::registry::native_flanger::{Flanger, FlangerTuning};
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "flanger_subtle";
pub const DISPLAY_NAME: &str = "Subtle Flanger";

const TUNING: FlangerTuning = FlangerTuning {
    base_ms: 5.0,
    max_ms: 11.0,
    feedback_clamp: 0.50,
};

#[derive(Debug, Clone, Copy)]
struct SubtleParams {
    rate_hz: f32,
    depth: f32,
    feedback: f32,
    mix: f32,
}

impl Default for SubtleParams {
    fn default() -> Self {
        Self {
            rate_hz: 0.7,
            depth: 50.0,
            feedback: 25.0,
            mix: 35.0,
        }
    }
}

fn parse(params: &ParameterSet) -> Result<SubtleParams> {
    Ok(SubtleParams {
        rate_hz: required_f32(params, "rate_hz").map_err(Error::msg)?,
        depth: required_f32(params, "depth").map_err(Error::msg)? / 100.0,
        feedback: required_f32(params, "feedback").map_err(Error::msg)? / 100.0,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: "modulation".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "rate_hz",
                "Rate",
                None,
                Some(SubtleParams::default().rate_hz),
                0.05,
                5.0,
                0.05,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(SubtleParams::default().depth),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(SubtleParams::default().feedback),
                -50.0,
                50.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(SubtleParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    })
}

fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let p = parse(params)?;
    Ok(Box::new(Flanger::with_tuning(
        p.rate_hz,
        p.depth,
        p.feedback,
        p.mix,
        sample_rate,
        TUNING,
    )))
}

fn build_processor_with_phase(
    params: &ParameterSet,
    sample_rate: f32,
    phase_offset: f32,
) -> Result<Box<dyn MonoProcessor>> {
    let p = parse(params)?;
    let mut f = Flanger::with_tuning(
        p.rate_hz,
        p.depth,
        p.feedback,
        p.mix,
        sample_rate,
        TUNING,
    );
    f.set_lfo_phase(phase_offset / std::f32::consts::TAU);
    Ok(Box::new(f))
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: block_core::AudioChannelLayout,
) -> Result<block_core::BlockProcessor> {
    match layout {
        block_core::AudioChannelLayout::Mono => Ok(block_core::BlockProcessor::Mono(
            build_processor(params, sample_rate)?,
        )),
        block_core::AudioChannelLayout::Stereo => {
            struct StereoFlanger {
                left: Box<dyn block_core::MonoProcessor>,
                right: Box<dyn block_core::MonoProcessor>,
            }

            impl block_core::StereoProcessor for StereoFlanger {
                fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
                    [
                        self.left.process_sample(input[0]),
                        self.right.process_sample(input[1]),
                    ]
                }
            }

            Ok(block_core::BlockProcessor::Stereo(Box::new(StereoFlanger {
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
