use anyhow::{Error, Result};
use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, BiquadFilter, BiquadKind, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "native_guitar_eq";
pub const DISPLAY_NAME: &str = "Guitar EQ";

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "filter".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "low_cut",
                "Low Cut",
                None,
                Some(100.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "high_cut",
                "High Cut",
                None,
                Some(100.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub struct GuitarEq {
    hpf: BiquadFilter,
    lpf: BiquadFilter,
}

impl GuitarEq {
    pub fn new(low_cut: f32, high_cut: f32, sample_rate: f32) -> Self {
        let hpf_freq = 20.0 + (low_cut / 100.0) * 80.0;
        let lpf_freq = 20000.0 - (high_cut / 100.0) * 13000.0;
        Self {
            hpf: BiquadFilter::new(BiquadKind::HighPass, hpf_freq, 0.0, 0.707, sample_rate),
            lpf: BiquadFilter::new(BiquadKind::LowPass,  lpf_freq, 0.0, 0.707, sample_rate),
        }
    }
}

impl MonoProcessor for GuitarEq {
    fn process_sample(&mut self, input: f32) -> f32 {
        let x = self.hpf.process(input);
        self.lpf.process(x)
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let low_cut = required_f32(params, "low_cut").map_err(Error::msg)?;
    let high_cut = required_f32(params, "high_cut").map_err(Error::msg)?;
    Ok(Box::new(GuitarEq::new(low_cut, high_cut, sample_rate)))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => {
            Ok(BlockProcessor::Mono(build_processor(params, sample_rate)?))
        }
        AudioChannelLayout::Stereo => anyhow::bail!(
            "filter model '{}' is mono-only and cannot build native stereo processing",
            MODEL_ID
        ),
    }
}

pub const MODEL_DEFINITION: FilterModelDefinition = FilterModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "",
    backend_kind: FilterBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::GUITAR_ACOUSTIC_BASS,
    knob_layout: &[],
};
