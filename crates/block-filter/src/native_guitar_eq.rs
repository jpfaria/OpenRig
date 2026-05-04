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

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_eq(sr: f32) -> GuitarEq {
        GuitarEq::new(0.0, 0.0, 0.0, 0.0, sr)
    }

    fn sine_rms(eq: &mut GuitarEq, freq_hz: f32, sample_rate: f32) -> f32 {
        let samples: Vec<f32> = (0..4096)
            .map(|i| (2.0 * std::f32::consts::PI * freq_hz * i as f32 / sample_rate).sin())
            .collect();
        for &s in &samples[..2048] {
            eq.process_sample(s);
        }
        let out: Vec<f32> = samples[2048..]
            .iter()
            .map(|&s| eq.process_sample(s))
            .collect();
        (out.iter().map(|x| x * x).sum::<f32>() / out.len() as f32).sqrt()
    }

    fn db(ratio: f32) -> f32 {
        20.0 * ratio.log10()
    }

    #[test]
    fn flat_silence_in_silence_out() {
        let mut eq = flat_eq(48_000.0);
        for i in 0..2048 {
            let out = eq.process_sample(0.0);
            assert!(
                out.abs() < 1e-10,
                "flat EQ should not add energy to silence at sample {i}"
            );
        }
    }

    #[test]
    fn flat_passes_sine_unchanged() {
        let sr = 48_000.0;
        let mut eq = flat_eq(sr);
        let rms_eq = sine_rms(&mut eq, 1000.0, sr);
        let rms_unit = 1.0_f32 / 2.0_f32.sqrt();
        let delta_db = db(rms_eq / rms_unit);
        assert!(
            delta_db.abs() < 0.1,
            "Expected < 0.1dB change with flat EQ at 1kHz, got {:.4} dB",
            delta_db
        );
    }

    #[test]
    fn output_is_finite_for_extreme_boost() {
        let sr = 44_100.0;
        let mut eq = GuitarEq::new(GAIN_MAX_DB, GAIN_MAX_DB, GAIN_MAX_DB, GAIN_MAX_DB, sr);
        for i in 0..1024 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let out = eq.process_sample(input);
            assert!(out.is_finite(), "output not finite at sample {i}");
        }
    }

    /// A boost on a band must measurably raise the RMS at the band's center
    /// frequency. Shelves only reach half their max gain at the corner
    /// frequency (so ~6 dB at +12 dB shelf), peaks reach ~full gain at the
    /// peak frequency. The 5 dB threshold catches both shapes while still
    /// being well above measurement noise.
    fn assert_band_boost_audible(name: &str, freq_hz: f32, eq_make: impl Fn(f32) -> GuitarEq) {
        let sr = 48_000.0;
        let mut flat = flat_eq(sr);
        let mut boosted = eq_make(sr);
        let rms_flat = sine_rms(&mut flat, freq_hz, sr);
        let rms_boost = sine_rms(&mut boosted, freq_hz, sr);
        let delta_db = db(rms_boost / rms_flat);
        assert!(
            delta_db > 5.0,
            "{name} band: expected >5dB boost at {freq_hz}Hz with +12dB gain, got {:.2}dB",
            delta_db
        );
    }

    #[test]
    fn low_band_boost_audible_at_center_freq() {
        assert_band_boost_audible("low", LOW_SHELF_FREQ_HZ, |sr| {
            GuitarEq::new(GAIN_MAX_DB, 0.0, 0.0, 0.0, sr)
        });
    }

    #[test]
    fn low_mid_band_boost_audible_at_center_freq() {
        assert_band_boost_audible("low-mid", LOW_MID_FREQ_HZ, |sr| {
            GuitarEq::new(0.0, GAIN_MAX_DB, 0.0, 0.0, sr)
        });
    }

    #[test]
    fn high_mid_band_boost_audible_at_center_freq() {
        assert_band_boost_audible("high-mid", HIGH_MID_FREQ_HZ, |sr| {
            GuitarEq::new(0.0, 0.0, GAIN_MAX_DB, 0.0, sr)
        });
    }

    #[test]
    fn high_band_boost_audible_at_center_freq() {
        assert_band_boost_audible("high", HIGH_SHELF_FREQ_HZ, |sr| {
            GuitarEq::new(0.0, 0.0, 0.0, GAIN_MAX_DB, sr)
        });
    }

    #[test]
    fn cut_attenuates_at_center_freq() {
        let sr = 48_000.0;
        let mut flat = flat_eq(sr);
        let mut cut = GuitarEq::new(0.0, GAIN_MIN_DB, 0.0, 0.0, sr);
        let rms_flat = sine_rms(&mut flat, LOW_MID_FREQ_HZ, sr);
        let rms_cut = sine_rms(&mut cut, LOW_MID_FREQ_HZ, sr);
        let delta_db = db(rms_cut / rms_flat);
        assert!(
            delta_db < -6.0,
            "low-mid cut: expected <-6dB attenuation at {}Hz with -12dB gain, got {:.2}dB",
            LOW_MID_FREQ_HZ,
            delta_db
        );
    }

    #[test]
    fn schema_lists_four_band_gains() {
        use block_core::param::ParameterDomain;
        use domain::value_objects::ParameterValue;
        let schema = model_schema();
        let names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(names, vec!["low", "low_mid", "high_mid", "high"]);
        for p in &schema.parameters {
            assert_eq!(p.unit, ParameterUnit::Decibels);
            match p.domain {
                ParameterDomain::FloatRange { min, max, step } => {
                    assert_eq!(min, GAIN_MIN_DB);
                    assert_eq!(max, GAIN_MAX_DB);
                    assert_eq!(step, GAIN_STEP_DB);
                }
                _ => panic!("expected FloatRange domain"),
            }
            assert!(matches!(p.default_value, Some(ParameterValue::Float(v)) if v == 0.0));
        }
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
