use anyhow::{Error, Result};
use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use block_core::param::{
    curve_editor_parameter, required_f32, CurveEditorRole, ModelParameterSchema, ParameterSet,
    ParameterUnit,
};
use block_core::{BiquadFilter, BiquadKind, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "eq_three_band_basic";
pub const DISPLAY_NAME: &str = "Three Band EQ";

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "filter".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            // Low shelf
            curve_editor_parameter(
                "low_gain",
                "Gain",
                Some("Low"),
                CurveEditorRole::Y,
                Some(0.0),
                -12.0,
                12.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            curve_editor_parameter(
                "low_freq",
                "Freq",
                Some("Low"),
                CurveEditorRole::X,
                Some(200.0),
                80.0,
                320.0,
                1.0,
                ParameterUnit::Hertz,
            ),
            // Mid peak
            curve_editor_parameter(
                "mid_gain",
                "Gain",
                Some("Mid"),
                CurveEditorRole::Y,
                Some(0.0),
                -12.0,
                12.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            curve_editor_parameter(
                "mid_freq",
                "Freq",
                Some("Mid"),
                CurveEditorRole::X,
                Some(1000.0),
                200.0,
                5000.0,
                1.0,
                ParameterUnit::Hertz,
            ),
            curve_editor_parameter(
                "mid_q",
                "Q",
                Some("Mid"),
                CurveEditorRole::Width,
                Some(1.0),
                0.1,
                6.0,
                0.01,
                ParameterUnit::None,
            ),
            // High shelf
            curve_editor_parameter(
                "high_gain",
                "Gain",
                Some("High"),
                CurveEditorRole::Y,
                Some(0.0),
                -12.0,
                12.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            curve_editor_parameter(
                "high_freq",
                "Freq",
                Some("High"),
                CurveEditorRole::X,
                Some(6000.0),
                3000.0,
                12000.0,
                1.0,
                ParameterUnit::Hertz,
            ),
        ],
    }
}

pub struct ThreeBandEq {
    low_shelf: BiquadFilter,
    mid_peak: BiquadFilter,
    high_shelf: BiquadFilter,
}

impl ThreeBandEq {
    pub fn new(
        low_gain_db: f32,
        low_freq: f32,
        mid_gain_db: f32,
        mid_freq: f32,
        mid_q: f32,
        high_gain_db: f32,
        high_freq: f32,
        sample_rate: f32,
    ) -> Self {
        Self {
            low_shelf: BiquadFilter::new(
                BiquadKind::LowShelf,
                low_freq,
                low_gain_db,
                0.707,
                sample_rate,
            ),
            mid_peak: BiquadFilter::new(
                BiquadKind::Peak,
                mid_freq,
                mid_gain_db,
                mid_q,
                sample_rate,
            ),
            high_shelf: BiquadFilter::new(
                BiquadKind::HighShelf,
                high_freq,
                high_gain_db,
                0.707,
                sample_rate,
            ),
        }
    }
}

impl MonoProcessor for ThreeBandEq {
    fn process_sample(&mut self, input: f32) -> f32 {
        let x = self.low_shelf.process(input);
        let x = self.mid_peak.process(x);
        self.high_shelf.process(x)
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let low_gain = required_f32(params, "low_gain").map_err(Error::msg)?;
    let low_freq = required_f32(params, "low_freq").map_err(Error::msg)?;
    let mid_gain = required_f32(params, "mid_gain").map_err(Error::msg)?;
    let mid_freq = required_f32(params, "mid_freq").map_err(Error::msg)?;
    let mid_q = required_f32(params, "mid_q").map_err(Error::msg)?;
    let high_gain = required_f32(params, "high_gain").map_err(Error::msg)?;
    let high_freq = required_f32(params, "high_freq").map_err(Error::msg)?;
    Ok(Box::new(ThreeBandEq::new(
        low_gain, low_freq, mid_gain, mid_freq, mid_q, high_gain, high_freq, sample_rate,
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
    match layout {
        block_core::AudioChannelLayout::Mono => {
            Ok(block_core::BlockProcessor::Mono(build_processor(params, sample_rate)?))
        }
        block_core::AudioChannelLayout::Stereo => anyhow::bail!(
            "eq model '{}' is mono-only and cannot build native stereo processing",
            MODEL_ID
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_sample_silence_output_finite() {
        let mut eq = ThreeBandEq::new(0.0, 200.0, 0.0, 1000.0, 1.0, 0.0, 6000.0, 44_100.0);
        for i in 0..1024 {
            let out = MonoProcessor::process_sample(&mut eq, 0.0);
            assert!(out.is_finite(), "output not finite at sample {i}");
        }
    }

    #[test]
    fn process_sample_silence_produces_zero() {
        let mut eq = ThreeBandEq::new(0.0, 200.0, 0.0, 1000.0, 1.0, 0.0, 6000.0, 44_100.0);
        for i in 0..1024 {
            let out = MonoProcessor::process_sample(&mut eq, 0.0);
            assert!(out.abs() < 1e-10, "flat EQ should not add energy to silence at sample {i}");
        }
    }

    #[test]
    fn process_sample_sine_output_finite_and_nonzero() {
        let mut eq = ThreeBandEq::new(0.0, 200.0, 0.0, 1000.0, 1.0, 0.0, 6000.0, 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..1024 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let out = MonoProcessor::process_sample(&mut eq, input);
            assert!(out.is_finite(), "output not finite at sample {i}");
            if out.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output for sine input");
    }

    #[test]
    fn process_block_all_finite() {
        let mut eq = ThreeBandEq::new(3.0, 200.0, -2.0, 1000.0, 1.5, 1.0, 6000.0, 44_100.0);
        let sr = 44_100.0_f32;
        let mut buffer: Vec<f32> = (0..1024)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin())
            .collect();
        MonoProcessor::process_block(&mut eq, &mut buffer);
        for (i, s) in buffer.iter().enumerate() {
            assert!(s.is_finite(), "output not finite at frame {i}");
        }
    }

    #[test]
    fn process_sample_with_boost_increases_energy() {
        let sr = 44_100.0_f32;
        // Flat EQ
        let mut eq_flat = ThreeBandEq::new(0.0, 200.0, 0.0, 1000.0, 1.0, 0.0, 6000.0, sr);
        // Mid boost +12dB
        let mut eq_boost = ThreeBandEq::new(0.0, 200.0, 12.0, 1000.0, 1.0, 0.0, 6000.0, sr);

        let samples: Vec<f32> = (0..4096)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / sr).sin())
            .collect();

        // Warm up
        for &s in &samples[..2048] {
            eq_flat.process_sample(s);
            eq_boost.process_sample(s);
        }

        // Measure RMS
        let rms_flat: f32 = (samples[2048..].iter()
            .map(|&s| { let o = eq_flat.process_sample(s); o * o })
            .sum::<f32>() / 2048.0).sqrt();
        let rms_boost: f32 = (samples[2048..].iter()
            .map(|&s| { let o = eq_boost.process_sample(s); o * o })
            .sum::<f32>() / 2048.0).sqrt();

        assert!(rms_boost > rms_flat * 2.0,
            "boosted EQ should be significantly louder: flat={rms_flat}, boost={rms_boost}");
    }
}

pub const MODEL_DEFINITION: FilterModelDefinition = FilterModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "",
    backend_kind: FilterBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
