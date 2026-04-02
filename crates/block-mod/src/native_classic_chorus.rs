use anyhow::{Error, Result};
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};
use std::f32::consts::TAU;

pub const MODEL_ID: &str = "classic_chorus";
pub const DISPLAY_NAME: &str = "Classic Chorus";

const CENTER_DELAY_SECS: f32 = 0.020;
const DEPTH_DELAY_SECS: f32 = 0.008;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "modulation".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "rate_hz",
                "Rate",
                None,
                Some(0.5),
                0.1,
                5.0,
                0.1,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
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
    }
}

pub struct ClassicChorus {
    rate_hz: f32,
    depth: f32,
    mix: f32,
    sample_rate: f32,
    phase: f32,
    delay_buf: Vec<f32>,
    write_pos: usize,
}

impl ClassicChorus {
    pub fn new(rate_hz: f32, depth: f32, mix: f32, sample_rate: f32) -> Self {
        let max_delay_samples = ((CENTER_DELAY_SECS + DEPTH_DELAY_SECS) * sample_rate) as usize + 2;
        Self {
            rate_hz,
            depth: depth.clamp(0.0, 1.0),
            mix: mix.clamp(0.0, 1.0),
            sample_rate,
            phase: 0.0,
            delay_buf: vec![0.0; max_delay_samples],
            write_pos: 0,
        }
    }
}

impl MonoProcessor for ClassicChorus {
    fn process_sample(&mut self, input: f32) -> f32 {
        let lfo = self.phase.sin();
        let delay_secs = CENTER_DELAY_SECS + self.depth * DEPTH_DELAY_SECS * lfo;
        let delay_samples = delay_secs * self.sample_rate;

        let buf_len = self.delay_buf.len();
        self.delay_buf[self.write_pos] = input;

        let read_pos_f = self.write_pos as f32 - delay_samples;
        let read_pos_f = read_pos_f.rem_euclid(buf_len as f32);
        let read_pos_floor = read_pos_f as usize;
        let frac = read_pos_f - read_pos_floor as f32;
        let s0 = self.delay_buf[read_pos_floor % buf_len];
        let s1 = self.delay_buf[(read_pos_floor + 1) % buf_len];
        let wet = s0 + frac * (s1 - s0);

        self.write_pos = (self.write_pos + 1) % buf_len;
        self.phase = (self.phase + TAU * self.rate_hz / self.sample_rate).rem_euclid(TAU);

        input * (1.0 - self.mix) + wet * self.mix
    }
}

fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let rate_hz = required_f32(params, "rate_hz").map_err(Error::msg)?;
    let depth = required_f32(params, "depth").map_err(Error::msg)? / 100.0;
    let mix = required_f32(params, "mix").map_err(Error::msg)? / 100.0;
    Ok(Box::new(ClassicChorus::new(rate_hz, depth, mix, sample_rate)))
}

fn build_processor_with_phase(params: &ParameterSet, sample_rate: f32, phase_offset: f32) -> Result<Box<dyn MonoProcessor>> {
    let rate_hz = required_f32(params, "rate_hz").map_err(Error::msg)?;
    let depth = required_f32(params, "depth").map_err(Error::msg)? / 100.0;
    let mix = required_f32(params, "mix").map_err(Error::msg)? / 100.0;
    let mut c = ClassicChorus::new(rate_hz, depth, mix, sample_rate);
    c.phase = phase_offset;
    Ok(Box::new(c))
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
            struct StereoChorus {
                left: Box<dyn block_core::MonoProcessor>,
                right: Box<dyn block_core::MonoProcessor>,
            }

            impl block_core::StereoProcessor for StereoChorus {
                fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
                    [
                        self.left.process_sample(input[0]),
                        self.right.process_sample(input[1]),
                    ]
                }
            }

            Ok(block_core::BlockProcessor::Stereo(Box::new(StereoChorus {
                left: build_processor(params, sample_rate)?,
                right: build_processor_with_phase(params, sample_rate, std::f32::consts::PI)?,
            })))
        }
    }
}

pub const MODEL_DEFINITION: ModModelDefinition = ModModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "",
    backend_kind: ModBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

#[cfg(test)]
mod tests {
    use super::*;
    use block_core::param::ParameterDomain;
    use domain::value_objects::ParameterValue;

    fn make_params(rate_hz: f32, depth: f32, mix: f32) -> ParameterSet {
        let mut ps = ParameterSet::default();
        ps.insert("rate_hz", ParameterValue::Float(rate_hz));
        ps.insert("depth", ParameterValue::Float(depth));
        ps.insert("mix", ParameterValue::Float(mix));
        ps
    }

    #[test]
    fn schema_has_rate_depth_mix_parameters() {
        let schema = model_schema();
        let paths: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(paths.contains(&"rate_hz"), "missing rate_hz");
        assert!(paths.contains(&"depth"), "missing depth");
        assert!(paths.contains(&"mix"), "missing mix");
    }

    #[test]
    fn schema_rate_range_is_0_1_to_5_hz() {
        let schema = model_schema();
        let rate = schema.parameters.iter().find(|p| p.path == "rate_hz").unwrap();
        let ParameterDomain::FloatRange { min, max, .. } = rate.domain else {
            panic!("expected FloatRange");
        };
        assert_eq!(min, 0.1);
        assert_eq!(max, 5.0);
    }

    #[test]
    fn dry_signal_passes_through_when_mix_is_zero() {
        let mut chorus = ClassicChorus::new(0.5, 0.5, 0.0, 44100.0);
        // With mix=0, output should equal input (no wet signal)
        let input = 1.0_f32;
        let output = chorus.process_sample(input);
        assert_eq!(output, input);
    }

    #[test]
    fn output_is_zero_for_silent_input() {
        let mut chorus = ClassicChorus::new(0.5, 0.5, 0.5, 44100.0);
        // Process many samples of silence
        for _ in 0..2000 {
            let out = chorus.process_sample(0.0);
            assert_eq!(out, 0.0, "silent input should produce silent output");
        }
    }

    #[test]
    fn model_definition_has_correct_id_and_display_name() {
        assert_eq!(MODEL_DEFINITION.id, "classic_chorus");
        assert_eq!(MODEL_DEFINITION.display_name, "Classic Chorus");
    }

    #[test]
    fn build_succeeds_with_valid_params() {
        let params = make_params(0.5, 50.0, 50.0);
        let result = build_processor(&params, 44100.0);
        assert!(result.is_ok());
    }

    #[test]
    fn schema_model_id_matches_constant() {
        let schema = model_schema();
        assert_eq!(schema.model, MODEL_ID);
    }
}
