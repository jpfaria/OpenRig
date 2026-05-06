use anyhow::{Error, Result};
use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, BiquadFilter, BiquadKind, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "native_guitar_hpf_lpf";
pub const DISPLAY_NAME: &str = "Guitar HPF/LPF";

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

/// 4th-order HPF/LPF pair using cascaded biquad stages (24 dB/oct roll-off).
/// Two cascaded 2nd-order Butterworth stages with Q values chosen for a maximally
/// flat Butterworth response: Q1 = 0.5412, Q2 = 1.3066.
///
/// Used to be called "Guitar EQ", but it never had band gains — only HPF/LPF
/// cuts at the extremes — so it was renamed in #303 to free the "Guitar EQ"
/// name for the actual 4-band tone shaper.
pub struct GuitarHpfLpf {
    hpf1: BiquadFilter,
    hpf2: BiquadFilter,
    lpf1: BiquadFilter,
    lpf2: BiquadFilter,
}

const BUTTERWORTH_Q1: f32 = 0.5412;
const BUTTERWORTH_Q2: f32 = 1.3066;

impl GuitarHpfLpf {
    pub fn new(low_cut: f32, high_cut: f32, sample_rate: f32) -> Self {
        let hpf_freq = 20.0 + (low_cut / 100.0) * 80.0;
        let lpf_freq = 20000.0 - (high_cut / 100.0) * 13000.0;
        Self {
            hpf1: BiquadFilter::new(BiquadKind::HighPass, hpf_freq, 0.0, BUTTERWORTH_Q1, sample_rate),
            hpf2: BiquadFilter::new(BiquadKind::HighPass, hpf_freq, 0.0, BUTTERWORTH_Q2, sample_rate),
            lpf1: BiquadFilter::new(BiquadKind::LowPass,  lpf_freq, 0.0, BUTTERWORTH_Q1, sample_rate),
            lpf2: BiquadFilter::new(BiquadKind::LowPass,  lpf_freq, 0.0, BUTTERWORTH_Q2, sample_rate),
        }
    }
}

impl MonoProcessor for GuitarHpfLpf {
    fn process_sample(&mut self, input: f32) -> f32 {
        let x = self.hpf1.process(input);
        let x = self.hpf2.process(x);
        let x = self.lpf1.process(x);
        self.lpf2.process(x)
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let low_cut = required_f32(params, "low_cut").map_err(Error::msg)?;
    let high_cut = required_f32(params, "high_cut").map_err(Error::msg)?;
    Ok(Box::new(GuitarHpfLpf::new(low_cut, high_cut, sample_rate)))
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
    brand: block_core::BRAND_NATIVE,
    backend_kind: FilterBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::GUITAR_ACOUSTIC_BASS,
    knob_layout: &[],
};

#[cfg(test)]
#[path = "native_guitar_hpf_lpf_tests.rs"]
mod tests;
