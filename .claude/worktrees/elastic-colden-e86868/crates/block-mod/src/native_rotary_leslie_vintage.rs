//! Vintage-gentle rotary variant — lower crossover, lighter AM/
//! Doppler, longer motor inertia. The "chorale jazz organ" voice
//! that breathes rather than warbles. Same engine as
//! `native_rotary_leslie.rs`; only the hidden tuning differs.

use crate::registry::native_rotary_leslie::{LeslieMono, LeslieRotary, LeslieTuning};
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::ModelAudioMode;

pub const MODEL_ID: &str = "rotary_leslie_vintage";
pub const DISPLAY_NAME: &str = "Rotary Leslie (Vintage)";

const TUNING: LeslieTuning = LeslieTuning {
    crossover_hz: 600.0,
    horn_delay_base_ms: 0.85,
    horn_delay_depth_ms: 0.25,
    drum_delay_base_ms: 1.8,
    drum_delay_depth_ms: 0.6,
    horn_am_depth: 0.20,
    drum_am_depth: 0.12,
    horn_rate_slow_hz: 0.65,
    horn_rate_fast_hz: 5.40,
    drum_rate_slow_hz: 0.55,
    drum_rate_fast_hz: 4.80,
    motor_tau_seconds: 0.9,
};

#[derive(Debug, Clone, Copy)]
struct VintageParams {
    speed: f32,
    mix: f32,
}

impl Default for VintageParams {
    fn default() -> Self {
        Self { speed: 0.0, mix: 100.0 }
    }
}

fn parse(params: &ParameterSet) -> Result<VintageParams> {
    Ok(VintageParams {
        speed: required_f32(params, "speed").map_err(Error::msg)? / 100.0,
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
                "speed",
                "Speed",
                None,
                Some(VintageParams::default().speed * 100.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(VintageParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    })
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: block_core::AudioChannelLayout,
) -> Result<block_core::BlockProcessor> {
    let p = parse(params)?;
    match layout {
        block_core::AudioChannelLayout::Mono => Ok(block_core::BlockProcessor::Mono(Box::new(
            LeslieMono::new(LeslieRotary::with_tuning(p.speed, p.mix, sample_rate, TUNING)),
        ))),
        block_core::AudioChannelLayout::Stereo => {
            struct LeslieStereoProc {
                inner: LeslieRotary,
            }

            impl block_core::StereoProcessor for LeslieStereoProc {
                fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
                    let mono_in = 0.5 * (input[0] + input[1]);
                    self.inner.process_stereo(mono_in)
                }
            }

            Ok(block_core::BlockProcessor::Stereo(Box::new(
                LeslieStereoProc {
                    inner: LeslieRotary::with_tuning(p.speed, p.mix, sample_rate, TUNING),
                },
            )))
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
