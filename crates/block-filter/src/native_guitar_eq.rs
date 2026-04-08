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
    low_shelf: BiquadFilter,
    high_shelf: BiquadFilter,
}

impl GuitarEq {
    pub fn new(low_cut_pct: f32, high_cut_pct: f32, sample_rate: f32) -> Self {
        let low_gain_db = -(low_cut_pct / 100.0) * 12.0;
        let high_gain_db = -(high_cut_pct / 100.0) * 12.0;
        Self {
            low_shelf: BiquadFilter::new(BiquadKind::LowShelf, 80.0, low_gain_db, 0.707, sample_rate),
            high_shelf: BiquadFilter::new(BiquadKind::HighShelf, 8000.0, high_gain_db, 0.707, sample_rate),
        }
    }
}

impl MonoProcessor for GuitarEq {
    fn process_sample(&mut self, input: f32) -> f32 {
        let x = self.low_shelf.process(input);
        self.high_shelf.process(x)
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
