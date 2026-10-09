//! Responsibility: implements the half wave rectifier gain model.
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
use block_core::dsp::IirOversampler;
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, OnePoleHighPass,
    OnePoleLowPass, StereoProcessor,
};

pub const MODEL_ID: &str = "half_wave_rectifier";
pub const DISPLAY_NAME: &str = "Octave-Up (Half-Wave)";
const BRAND: &str = block_core::BRAND_NATIVE;

/// Rate multiple the rectifier and fuzz run at (#1106).
const OVERSAMPLING: usize = 32;

#[derive(Debug, Clone, Copy)]
struct Settings {
    drive: f32,
    tone: f32,
    octave_mix: f32, // 0..1: 0 = dry only, 1 = octave only
    level: f32,
}

struct OctaveProcessor {
    settings: Settings,
    in_dc_block: OnePoleHighPass,
    /// Runs at the oversampled rate, between the rectifier and the fuzz.
    rect_dc_block: OnePoleHighPass,
    out_dc_block: OnePoleHighPass,
    tone_lpf: OnePoleLowPass,
    oversampler: IirOversampler,
}

impl OctaveProcessor {
    fn new(settings: Settings, sample_rate: f32) -> Self {
        Self {
            settings,
            in_dc_block: OnePoleHighPass::new(40.0, sample_rate),
            // The rectifier introduces a DC term ~ 2/π·peak; HPF takes it out.
            rect_dc_block: OnePoleHighPass::new(80.0, sample_rate * OVERSAMPLING as f32),
            // The fuzz bends the centred pulse train asymmetrically.
            out_dc_block: OnePoleHighPass::new(20.0, sample_rate),
            tone_lpf: OnePoleLowPass::new(4_500.0, sample_rate),
            oversampler: IirOversampler::new(sample_rate, OVERSAMPLING),
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
        // Rectifier, DC block and fuzz all run oversampled: the
        // rectifier's corner has harmonics far past Nyquist.
        let rect_dc_block = &mut self.rect_dc_block;
        let shaped = self.oversampler.process(driven, |s| {
            // DC-block the asymmetric pulse train so it sits centred.
            let centred = rect_dc_block.process(s.abs());
            // Octavia-style fuzz on the rectified signal.
            Self::fuzz(centred * 2.0)
        });
        let fuzzy = self.out_dc_block.process(shaped);
        // Tone control (LPF blend).
        let warm = self.tone_lpf.process(fuzzy);
        let toned = fuzzy * tone + warm * (1.0 - tone);

        // Blend dry and octave.
        let mixed = x * (1.0 - octave_mix) + toned * octave_mix;

        // Output level (50% = unity, 100% = +6 dB).
        mixed * (level * 2.0)
    }

    fn latency_samples(&self) -> usize {
        self.oversampler.latency_samples()
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

    fn latency_samples(&self) -> usize {
        self.left.latency_samples()
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "drive",
                "Drive",
                Some("Gain"),
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "tone",
                "Tone",
                Some("EQ"),
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "octave_mix",
                "Octave",
                Some("Mix"),
                Some(70.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "level",
                "Level",
                Some("Output"),
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
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
    Ok("native='half_wave_rectifier' algorithm='|x| + DC-block + tanh fuzz 32x oversampled'".to_string())
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(p: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
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
#[path = "native_half_wave_rectifier_tests.rs"]
mod tests;
