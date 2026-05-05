//! 4-band tone-shaping EQ tuned for electric guitar / acoustic / bass.
//! Issue #303 — replaces the prior `native_guitar_eq` (an HPF+LPF cleanup
//! filter, now `native_guitar_hpf_lpf`) with a real boost+cut tone shaper.

use anyhow::{Error, Result};
use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterSpec,
    ParameterUnit,
};
use block_core::{
    AudioChannelLayout, BiquadFilter, BiquadKind, BlockProcessor, ModelAudioMode, MonoProcessor,
};

pub const MODEL_ID: &str = "native_guitar_eq";
pub const DISPLAY_NAME: &str = "Guitar EQ";

const LOW_SHELF_FREQ_HZ: f32 = 150.0;
const LOW_MID_FREQ_HZ: f32 = 500.0;
const HIGH_MID_FREQ_HZ: f32 = 2500.0;
const HIGH_SHELF_FREQ_HZ: f32 = 6000.0;
const PEAK_Q: f32 = 0.7;
const SHELF_Q: f32 = 0.707;
const GAIN_MIN_DB: f32 = -12.0;
const GAIN_MAX_DB: f32 = 12.0;
const GAIN_STEP_DB: f32 = 0.1;

fn band_gain(name: &'static str, label: &'static str) -> ParameterSpec {
    float_parameter(
        name,
        label,
        None,
        Some(0.0),
        GAIN_MIN_DB,
        GAIN_MAX_DB,
        GAIN_STEP_DB,
        ParameterUnit::Decibels,
    )
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "filter".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            band_gain("low", "Low"),
            band_gain("low_mid", "Low Mid"),
            band_gain("high_mid", "High Mid"),
            band_gain("high", "High"),
        ],
    }
}

pub struct GuitarEq {
    low_shelf: BiquadFilter,
    low_mid_peak: BiquadFilter,
    high_mid_peak: BiquadFilter,
    high_shelf: BiquadFilter,
}

impl GuitarEq {
    pub fn new(
        low_db: f32,
        low_mid_db: f32,
        high_mid_db: f32,
        high_db: f32,
        sample_rate: f32,
    ) -> Self {
        Self {
            low_shelf: BiquadFilter::new(
                BiquadKind::LowShelf,
                LOW_SHELF_FREQ_HZ,
                low_db,
                SHELF_Q,
                sample_rate,
            ),
            low_mid_peak: BiquadFilter::new(
                BiquadKind::Peak,
                LOW_MID_FREQ_HZ,
                low_mid_db,
                PEAK_Q,
                sample_rate,
            ),
            high_mid_peak: BiquadFilter::new(
                BiquadKind::Peak,
                HIGH_MID_FREQ_HZ,
                high_mid_db,
                PEAK_Q,
                sample_rate,
            ),
            high_shelf: BiquadFilter::new(
                BiquadKind::HighShelf,
                HIGH_SHELF_FREQ_HZ,
                high_db,
                SHELF_Q,
                sample_rate,
            ),
        }
    }
}

impl MonoProcessor for GuitarEq {
    fn process_sample(&mut self, input: f32) -> f32 {
        let x = self.low_shelf.process(input);
        let x = self.low_mid_peak.process(x);
        let x = self.high_mid_peak.process(x);
        self.high_shelf.process(x)
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let low = required_f32(params, "low").map_err(Error::msg)?;
    let low_mid = required_f32(params, "low_mid").map_err(Error::msg)?;
    let high_mid = required_f32(params, "high_mid").map_err(Error::msg)?;
    let high = required_f32(params, "high").map_err(Error::msg)?;
    Ok(Box::new(GuitarEq::new(
        low,
        low_mid,
        high_mid,
        high,
        sample_rate,
    )))
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
#[path = "native_guitar_eq_tests.rs"]
mod tests;
