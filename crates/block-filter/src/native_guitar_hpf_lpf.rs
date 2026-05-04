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

#[cfg(test)]
mod tests {
    use super::*;
    use block_core::MonoProcessor;

    fn sine_rms(freq_hz: f32, sample_rate: f32, low_cut: f32, high_cut: f32) -> f32 {
        let mut eq = GuitarHpfLpf::new(low_cut, high_cut, sample_rate);
        let samples: Vec<f32> = (0..4096)
            .map(|i| (2.0 * std::f32::consts::PI * freq_hz * i as f32 / sample_rate).sin())
            .collect();
        for &s in &samples[..2048] {
            eq.process_sample(s);
        }
        let out: Vec<f32> = samples[2048..].iter().map(|&s| eq.process_sample(s)).collect();
        (out.iter().map(|x| x * x).sum::<f32>() / out.len() as f32).sqrt()
    }

    fn db(ratio: f32) -> f32 {
        20.0 * ratio.log10()
    }

    #[test]
    fn hpf_attenuates_50hz() {
        let sr = 48000.0;
        let rms_filtered = sine_rms(50.0, sr, 100.0, 0.0);
        let rms_passthrough = sine_rms(50.0, sr, 0.0, 0.0);
        let attenuation_db = db(rms_filtered / rms_passthrough);
        assert!(
            attenuation_db <= -20.0,
            "Expected >= 20dB attenuation at 50Hz with HPF@100Hz, got {:.2} dB",
            attenuation_db
        );
    }

    #[test]
    fn lpf_attenuates_15khz() {
        let sr = 48000.0;
        let rms_filtered = sine_rms(15000.0, sr, 0.0, 100.0);
        let rms_passthrough = sine_rms(15000.0, sr, 0.0, 0.0);
        let attenuation_db = db(rms_filtered / rms_passthrough);
        assert!(
            attenuation_db <= -20.0,
            "Expected >= 20dB attenuation at 15kHz with LPF@7kHz, got {:.2} dB",
            attenuation_db
        );
    }

    #[test]
    fn passthrough_at_zero_percent() {
        let sr = 48000.0;
        let rms_filtered = sine_rms(1000.0, sr, 0.0, 0.0);
        let rms_passthrough = 1.0_f32 / 2.0_f32.sqrt();
        let attenuation_db = db(rms_filtered / rms_passthrough);
        assert!(
            attenuation_db.abs() < 0.1,
            "Expected < 0.1dB change at 1kHz with no filtering, got {:.4} dB",
            attenuation_db
        );
    }

    #[test]
    fn process_sample_silence_output_finite() {
        let mut eq = GuitarHpfLpf::new(50.0, 50.0, 44_100.0);
        for i in 0..1024 {
            let out = eq.process_sample(0.0);
            assert!(out.is_finite(), "output not finite at sample {i}");
        }
    }

    #[test]
    fn process_sample_sine_output_finite_and_nonzero() {
        let mut eq = GuitarHpfLpf::new(50.0, 50.0, 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..1024 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let out = eq.process_sample(input);
            assert!(out.is_finite(), "output not finite at sample {i}");
            if out.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output for sine input");
    }

    #[test]
    fn process_block_all_finite() {
        let mut eq = GuitarHpfLpf::new(50.0, 50.0, 44_100.0);
        let sr = 44_100.0_f32;
        let mut buffer: Vec<f32> = (0..1024)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin())
            .collect();
        eq.process_block(&mut buffer);
        for (i, s) in buffer.iter().enumerate() {
            assert!(s.is_finite(), "output not finite at frame {i}");
        }
    }

    #[test]
    fn mid_frequencies_pass_at_full_percent() {
        let sr = 48000.0;
        let rms_filtered = sine_rms(1000.0, sr, 100.0, 100.0);
        let rms_passthrough = sine_rms(1000.0, sr, 0.0, 0.0);
        let attenuation_db = db(rms_filtered / rms_passthrough);
        assert!(
            attenuation_db.abs() < 0.1,
            "Expected < 0.1dB change at 1kHz inside passband (HPF@100Hz, LPF@7kHz), got {:.4} dB",
            attenuation_db
        );
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
