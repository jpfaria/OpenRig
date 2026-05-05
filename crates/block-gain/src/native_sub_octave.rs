//! Sub-octave (octave down) — Boss OC-2 SUB1-style. Detects positive
//! zero crossings of the input and toggles a flip-flop, producing a
//! square wave at exactly half the input frequency. The square is then
//! amplitude-modulated by the input envelope so it tracks dynamics
//! and decays naturally with the original note.
//!
//! References:
//! - electrosmash.com — Boss OC-2 schematic analysis (CD4013 flip-flop
//!   on the zero crossings of the comparator output).
//! - Yeh, D. T. (2008). 'Digital Implementation of Musical Distortion
//!   Circuits', Stanford CCRMA — chapter on sub-harmonic generators.
//!
//! Tracking is best on a clean monophonic note above E2. Polyphonic
//! input or noisy/distorted signal will trigger erratically — that's
//! the same limitation as the real OC-2.

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

pub const MODEL_ID: &str = "sub_octave";
pub const DISPLAY_NAME: &str = "Sub-Octave (-1 oct)";
const BRAND: &str = block_core::BRAND_NATIVE;

#[derive(Debug, Clone, Copy)]
struct Settings {
    sub_mix: f32,
    tone: f32,
    level: f32,
}

struct SubProcessor {
    settings: Settings,
    in_hpf: OnePoleHighPass,
    out_lpf: OnePoleLowPass,
    /// Flip-flop state (-1 or +1) — toggled on every positive zero crossing.
    flip: f32,
    /// Last input sample, for crossing detection.
    last_in: f32,
    /// Envelope follower (peak with separate attack/release).
    env: f32,
    env_attack: f32,
    env_release: f32,
    /// Hysteresis threshold so noise around zero doesn't false-trigger.
    hysteresis: f32,
    /// Schmitt-trigger latched state — must cross +hysteresis after going
    /// below -hysteresis before another flip is allowed.
    armed: bool,
}

impl SubProcessor {
    fn new(settings: Settings, sample_rate: f32) -> Self {
        Self {
            settings,
            in_hpf: OnePoleHighPass::new(60.0, sample_rate),
            out_lpf: OnePoleLowPass::new(2_500.0, sample_rate),
            flip: 1.0,
            last_in: 0.0,
            env: 0.0,
            // ~10ms attack, ~150ms release at 44.1kHz — preserves transients
            // and lets sustained notes hold the sub-octave through the decay.
            env_attack: 0.001_f32.powf(1.0 / (0.010 * sample_rate)),
            env_release: 0.001_f32.powf(1.0 / (0.150 * sample_rate)),
            hysteresis: 0.02,
            armed: false,
        }
    }

    fn pct(v: f32) -> f32 { (v / 100.0).clamp(0.0, 1.0) }
}

impl MonoProcessor for SubProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let sub_mix = Self::pct(self.settings.sub_mix);
        let tone = Self::pct(self.settings.tone);
        let level = Self::pct(self.settings.level);

        let x = self.in_hpf.process(input);

        // Envelope follower: peak with attack/release.
        let abs = x.abs();
        if abs > self.env {
            self.env = abs + (self.env - abs) * self.env_attack;
        } else {
            self.env = abs + (self.env - abs) * self.env_release;
        }

        // Schmitt-trigger zero-crossing detector. The flip toggles each
        // time we cross +hysteresis going up after having dipped below
        // -hysteresis. This avoids double-triggering on a single edge.
        if x < -self.hysteresis {
            self.armed = true;
        }
        if self.armed && x > self.hysteresis && self.last_in <= self.hysteresis {
            self.flip = -self.flip;
            self.armed = false;
        }
        self.last_in = x;

        // Sub-octave = flip-flop * envelope.
        let raw_sub = self.flip * self.env;

        // Tone control: tone=0 → fully smoothed (round sine-ish), tone=1 → raw square.
        let smooth = self.out_lpf.process(raw_sub);
        let toned = raw_sub * tone + smooth * (1.0 - tone);

        // Mix sub with dry.
        let mixed = x * (1.0 - sub_mix) + toned * sub_mix;
        mixed * (level * 2.0)
    }
}

struct DualMonoProcessor { left: SubProcessor, right: SubProcessor }

impl StereoProcessor for DualMonoProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("sub_mix", "Sub", Some("Mix"), Some(70.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("tone", "Tone", Some("EQ"), Some(40.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("level", "Level", Some("Output"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn read_settings(p: &ParameterSet) -> Result<Settings> {
    Ok(Settings {
        sub_mix: required_f32(p, "sub_mix").map_err(anyhow::Error::msg)?,
        tone: required_f32(p, "tone").map_err(anyhow::Error::msg)?,
        level: required_f32(p, "level").map_err(anyhow::Error::msg)?,
    })
}

pub fn validate_params(p: &ParameterSet) -> Result<()> { let _ = read_settings(p)?; Ok(()) }
pub fn asset_summary(_: &ParameterSet) -> Result<String> {
    Ok("native='sub_octave' algorithm='Schmitt zero-cross flip-flop * envelope'".to_string())
}
fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

fn build(p: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let s = read_settings(p)?;
    Ok(match layout {
        AudioChannelLayout::Mono => BlockProcessor::Mono(Box::new(SubProcessor::new(s, sample_rate))),
        AudioChannelLayout::Stereo => BlockProcessor::Stereo(Box::new(DualMonoProcessor {
            left: SubProcessor::new(s, sample_rate),
            right: SubProcessor::new(s, sample_rate),
        })),
    })
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Native,
    schema, validate: validate_params, asset_summary, build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Settings { Settings { sub_mix: 70.0, tone: 40.0, level: 50.0 } }

    #[test]
    fn silence_input_produces_silence() {
        let mut p = SubProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_produces_finite_output() {
        let mut p = SubProcessor::new(defaults(), 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..2048 {
            let s = (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr).sin() * 0.5;
            let out = p.process_sample(s);
            assert!(out.is_finite());
        }
    }

    #[test]
    fn pure_sine_halves_the_dominant_frequency() {
        // Feed a pure sine at f and verify the sub-octave output crosses
        // zero at half the rate (i.e. one period per two input periods).
        // Use sub_mix = 100% (no dry leakage), tone = 100% (raw square),
        // level = 50% (unity-ish).
        let mut p = SubProcessor::new(
            Settings { sub_mix: 100.0, tone: 100.0, level: 50.0 },
            44_100.0,
        );
        let sr = 44_100.0_f32;
        let f_in = 220.0; // → expect 110 Hz output
        // Skip warm-up.
        for i in 0..2048 {
            let _ = p.process_sample((2.0 * std::f32::consts::PI * f_in * i as f32 / sr).sin() * 0.5);
        }
        // Count zero crossings over a measurement window.
        let mut prev = 0.0_f32;
        let mut crossings = 0;
        let window = (sr * 0.2) as usize; // 200 ms
        for i in 0..window {
            let s = (2.0 * std::f32::consts::PI * f_in * (i + 2048) as f32 / sr).sin() * 0.5;
            let out = p.process_sample(s);
            if (prev <= 0.0 && out > 0.0) || (prev >= 0.0 && out < 0.0) {
                crossings += 1;
            }
            prev = out;
        }
        let observed = crossings as f32 / 2.0 / 0.2;
        // Expect ~110 Hz; allow 30% tolerance for transient and edge cases.
        assert!(
            (observed - 110.0).abs() < 35.0,
            "expected ~110 Hz, observed {observed:.1} Hz",
        );
    }

    #[test]
    fn tone_zero_smooths_the_square() {
        // tone=0 → output goes through full LPF, peak should be lower
        // than tone=100% on the same input (since LPF kills harmonics).
        let sr = 44_100.0_f32;
        let f_in = 220.0;
        let make = |tone: f32| {
            let mut p = SubProcessor::new(
                Settings { sub_mix: 100.0, tone, level: 50.0 },
                sr,
            );
            for i in 0..2048 {
                let _ = p.process_sample((2.0 * std::f32::consts::PI * f_in * i as f32 / sr).sin() * 0.5);
            }
            let mut peak = 0.0_f32;
            for i in 0..2048 {
                let s = (2.0 * std::f32::consts::PI * f_in * (i + 2048) as f32 / sr).sin() * 0.5;
                peak = peak.max(p.process_sample(s).abs());
            }
            peak
        };
        let raw = make(100.0);
        let smooth = make(0.0);
        assert!(smooth < raw, "expected smooth ({smooth}) < raw ({raw})");
    }
}
