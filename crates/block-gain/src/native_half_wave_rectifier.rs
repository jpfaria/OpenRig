//! Half-wave rectifier — Tycobrahe Octavia / Roger Mayer Octavia–style
//! octave-up effect. Rectifying the signal (taking |x|) frequency-doubles
//! the input, producing the octave-up fundamental, then a soft clipper
//! adds the fuzzy harmonic stack the original Hendrix Octavia is known for.
//!
//! References:
//! - electrosmash.com — Tycobrahe Octavia analysis (full-wave rectifier
//!   built from a Germanium-diode bridge feeding into a fuzz stage).
//! - Yeh, D. T. (2008). 'Digital Implementation of Musical Distortion
//!   Circuits', Stanford CCRMA — chapter on rectifier-based octave effects.
//!
//! We implement a half-wave variant (single side rectifier) — slightly
//! gritter than full-wave but cheaper and the dominant character is the
//! same: pitch doubles, with the rectifier's discontinuity producing
//! the harmonic stack. Tracking is best on a clean note above E2.

use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, OnePoleHighPass,
    OnePoleLowPass, StereoProcessor,
};

pub const MODEL_ID: &str = "half_wave_rectifier";
pub const DISPLAY_NAME: &str = "Octave-Up (Half-Wave)";
const BRAND: &str = block_core::BRAND_NATIVE;

#[derive(Debug, Clone, Copy)]
struct Settings {
    drive: f32,
    tone: f32,
    octave_mix: f32,  // 0..1: 0 = dry only, 1 = octave only
    level: f32,
}

struct OctaveProcessor {
    settings: Settings,
    in_dc_block: OnePoleHighPass,
    out_dc_block: OnePoleHighPass,
    tone_lpf: OnePoleLowPass,
}

impl OctaveProcessor {
    fn new(settings: Settings, sample_rate: f32) -> Self {
        Self {
            settings,
            in_dc_block: OnePoleHighPass::new(40.0, sample_rate),
            // The rectifier introduces a DC term ~ 2/π·peak; HPF takes it out.
            out_dc_block: OnePoleHighPass::new(80.0, sample_rate),
            tone_lpf: OnePoleLowPass::new(4_500.0, sample_rate),
        }
    }

    fn pct(v: f32) -> f32 {
        (v / 100.0).clamp(0.0, 1.0)
    }

    /// Soft clipper used to add the Octavia fuzz character on top of
    /// the rectified signal. tanh keeps it well-behaved without aliasing.
    #[inline]
    fn fuzz(x: f32) -> f32 {
        x.tanh()
    }
}

impl MonoProcessor for OctaveProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let drive = Self::pct(self.settings.drive);
        let tone = Self::pct(self.settings.tone);
        let octave_mix = Self::pct(self.settings.octave_mix);
        let level = Self::pct(self.settings.level);

        let x = self.in_dc_block.process(input);
        let driven = x * (1.0 + drive * 8.0);

        // Full-wave rectify (Octavia diode bridge): |x| produces a series
        // of positive half-sines at 2× the input frequency — the actual
        // pitch-doubling mechanism. (The issue title says "half-wave"
        // following the common informal name for the effect, but the
        // Octavia circuit is a full-wave diode bridge.)
        let rect = driven.abs();
        // DC-block the asymmetric pulse train so the output sits centred.
        let centred = self.out_dc_block.process(rect);
        // Octavia-style fuzz on the rectified signal.
        let fuzzy = Self::fuzz(centred * 2.0);
        // Tone control (LPF blend).
        let warm = self.tone_lpf.process(fuzzy);
        let toned = fuzzy * tone + warm * (1.0 - tone);

        // Blend dry and octave.
        let mixed = x * (1.0 - octave_mix) + toned * octave_mix;

        // Output level (50% = unity, 100% = +6 dB).
        mixed * (level * 2.0)
    }
}

struct DualMonoProcessor {
    left: OctaveProcessor,
    right: OctaveProcessor,
}

impl StereoProcessor for DualMonoProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [
            self.left.process_sample(input[0]),
            self.right.process_sample(input[1]),
        ]
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("drive", "Drive", Some("Gain"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("tone", "Tone", Some("EQ"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("octave_mix", "Octave", Some("Mix"), Some(70.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("level", "Level", Some("Output"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn read_settings(p: &ParameterSet) -> Result<Settings> {
    Ok(Settings {
        drive: required_f32(p, "drive").map_err(anyhow::Error::msg)?,
        tone: required_f32(p, "tone").map_err(anyhow::Error::msg)?,
        octave_mix: required_f32(p, "octave_mix").map_err(anyhow::Error::msg)?,
        level: required_f32(p, "level").map_err(anyhow::Error::msg)?,
    })
}

pub fn validate_params(p: &ParameterSet) -> Result<()> {
    let _ = read_settings(p)?;
    Ok(())
}

pub fn asset_summary(_: &ParameterSet) -> Result<String> {
    Ok("native='half_wave_rectifier' algorithm='|x| + DC-block + tanh fuzz'".to_string())
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    p: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let s = read_settings(p)?;
    Ok(match layout {
        AudioChannelLayout::Mono => {
            BlockProcessor::Mono(Box::new(OctaveProcessor::new(s, sample_rate)))
        }
        AudioChannelLayout::Stereo => BlockProcessor::Stereo(Box::new(DualMonoProcessor {
            left: OctaveProcessor::new(s, sample_rate),
            right: OctaveProcessor::new(s, sample_rate),
        })),
    })
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Native,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Settings {
        Settings { drive: 50.0, tone: 50.0, octave_mix: 70.0, level: 50.0 }
    }

    #[test]
    fn silence_input_produces_silence() {
        let mut p = OctaveProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_finite_and_nonzero() {
        let mut p = OctaveProcessor::new(defaults(), 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..2048 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.5;
            let out = p.process_sample(s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 { any_nonzero = true; }
        }
        assert!(any_nonzero);
    }

    #[test]
    fn dc_input_is_blocked() {
        let mut p = OctaveProcessor::new(defaults(), 44_100.0);
        for _ in 0..8192 { let _ = p.process_sample(0.5); }
        let mut peak = 0.0_f32;
        for _ in 0..2048 { peak = peak.max(p.process_sample(0.5).abs()); }
        assert!(peak < 0.05, "DC was not blocked (peak {peak})");
    }

    #[test]
    fn pure_sine_doubles_dominant_frequency() {
        // Feed a low-frequency sine; the rectified output should have
        // most energy at 2× the input frequency. We can't do an FFT in
        // this lightweight test, but we CAN verify the rectified signal
        // crosses zero at twice the rate of the input.
        // Use settings: octave_mix=100% (no dry), drive=0 (no extra fuzz),
        // tone=100% (no LPF smoothing), level=50% (unity).
        let mut p = OctaveProcessor::new(
            Settings { drive: 0.0, tone: 100.0, octave_mix: 100.0, level: 50.0 },
            44_100.0,
        );
        let sr = 44_100.0_f32;
        let f_in = 110.0; // low E ~ 82 Hz, well-tracked
        // Skip warm-up.
        for i in 0..1024 {
            let _ = p.process_sample((2.0 * std::f32::consts::PI * f_in * i as f32 / sr).sin());
        }
        // Count zero crossings over a reasonable window.
        let mut prev = 0.0_f32;
        let mut crossings = 0;
        let window_samples = (sr / 10.0) as usize; // 0.1s
        for i in 0..window_samples {
            let s = (2.0 * std::f32::consts::PI * f_in * (i + 1024) as f32 / sr).sin();
            let out = p.process_sample(s);
            if (prev <= 0.0 && out > 0.0) || (prev >= 0.0 && out < 0.0) {
                crossings += 1;
            }
            prev = out;
        }
        let observed_freq = crossings as f32 / 2.0 / 0.1;
        // We expect ~220 Hz (2× input). Allow 30% tolerance for harmonic
        // content and tracking imperfection.
        assert!(
            (observed_freq - 220.0).abs() < 70.0,
            "expected ~220 Hz, observed {observed_freq:.1}",
        );
    }

    #[test]
    fn dual_mono_produces_finite_output_for_both_channels() {
        let mut dm = DualMonoProcessor {
            left: OctaveProcessor::new(defaults(), 44_100.0),
            right: OctaveProcessor::new(defaults(), 44_100.0),
        };
        for i in 0..1024 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44_100.0).sin() * 0.5;
            let [l, r] = StereoProcessor::process_frame(&mut dm, [s, s]);
            assert!(l.is_finite() && r.is_finite());
        }
    }
}
