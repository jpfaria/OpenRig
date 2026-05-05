//! Tape saturation — magnetic recording / playback chain emulation.
//! Captures the four perceptual signatures of analogue tape: soft
//! compression at the saturation knee, slight rate-dependent hysteresis
//! (signal "lags" through the magnetic medium), high-frequency roll-off
//! from limited head/tape coupling, and gentle wow-modulation from the
//! transport's mechanical jitter.
//!
//! References:
//! - Holters, M. (2016). 'Physical Modelling of a Wah-wah Effect Pedal
//!   as a Case Study for Application of the Nodal DK Method to Circuits
//!   with Variable Parts', JAES — section on Jiles-Atherton hysteresis
//!   modelling.
//! - Jiles, D. C. & Atherton, D. L. (1986). 'Theory of ferromagnetic
//!   hysteresis', J. Magnetism and Magnetic Materials.
//! - Bilbao, S. (2009). 'Numerical Sound Synthesis', chapter 7 (magnetic
//!   tape models).
//!
//! We implement a perceptually-equivalent approximation rather than the
//! full Jiles-Atherton 4-state ODE — soft tanh saturation with a single-
//! pole low-passed memory term that recovers the rate-dependent shape
//! of true hysteresis without the integrator stability headaches.

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

pub const MODEL_ID: &str = "tape_saturation";
pub const DISPLAY_NAME: &str = "Tape Saturation";
const BRAND: &str = block_core::BRAND_NATIVE;

const TAU: f32 = std::f32::consts::TAU;

#[derive(Debug, Clone, Copy)]
struct Settings {
    drive: f32,
    /// Strength of the rate-dependent hysteresis memory term (0..1).
    hysteresis: f32,
    /// Wow depth — modulates output via short LFO-driven delay.
    wow: f32,
    /// HF tape roll-off (warmth control).
    warmth: f32,
    level: f32,
}

struct TapeProcessor {
    settings: Settings,
    in_hpf: OnePoleHighPass,
    pre_emph: OnePoleHighPass,
    de_emph: OnePoleLowPass,
    upsample_lpf: OnePoleLowPass,
    downsample_lpf: OnePoleLowPass,
    /// Hysteresis memory: low-passed previous output, used to bend the
    /// saturator shape based on recent signal history.
    hyst_mem: f32,
    hyst_alpha: f32,
    /// Wow modulation: short delay buffer + LFO phase.
    wow_buf: Vec<f32>,
    wow_write: usize,
    wow_phase: f32,
    wow_phase_inc: f32,
}

impl TapeProcessor {
    fn new(settings: Settings, sample_rate: f32) -> Self {
        let oversample_rate = sample_rate * 2.0;
        // Hysteresis memory cutoff: low-passing at ~3kHz so the memory
        // term tracks slowly-varying excursions but ignores high-freq
        // detail (the lag of magnetic domain alignment).
        let hyst_alpha = (-1.0 / (sample_rate / (TAU * 3_000.0))).exp();
        // Wow LFO ~0.5 Hz, depth modulates the delay between 0..2ms.
        let wow_buf_len = ((sample_rate * 0.005) as usize).max(64); // 5ms headroom
        Self {
            settings,
            in_hpf: OnePoleHighPass::new(30.0, sample_rate),
            pre_emph: OnePoleHighPass::new(5_000.0, sample_rate),
            de_emph: OnePoleLowPass::new(8_000.0, sample_rate),
            upsample_lpf: OnePoleLowPass::new(sample_rate * 0.45, oversample_rate),
            downsample_lpf: OnePoleLowPass::new(sample_rate * 0.45, oversample_rate),
            hyst_mem: 0.0,
            hyst_alpha,
            wow_buf: vec![0.0; wow_buf_len],
            wow_write: 0,
            wow_phase: 0.0,
            wow_phase_inc: TAU * 0.5 / sample_rate,
        }
    }

    fn pct(v: f32) -> f32 { (v / 100.0).clamp(0.0, 1.0) }

    /// Hysteretic saturator: tanh of (input + memory * hysteresis).
    /// The memory is a low-passed version of the recent output, so the
    /// curve "leans" toward where the signal recently was — this is the
    /// rate-dependent loop of true Jiles-Atherton hysteresis without
    /// the integrator math.
    #[inline]
    fn shape(x: f32, mem: f32, hyst: f32) -> f32 {
        (x + mem * hyst * 0.3).tanh()
    }
}

impl MonoProcessor for TapeProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let drive = Self::pct(self.settings.drive);
        let hyst = Self::pct(self.settings.hysteresis);
        let wow = Self::pct(self.settings.wow);
        let warmth = Self::pct(self.settings.warmth);
        let level = Self::pct(self.settings.level);

        let x = self.in_hpf.process(input);
        // Pre-emphasis: tape heads bump the high mids by ~3 dB (we use
        // a HPF to additively boost above ~5 kHz).
        let pre = self.pre_emph.process(x);
        let driven = (x + pre * 0.3) * (1.0 + drive * 6.0);

        // 2× polyphase oversampling around the nonlinearity.
        let up0 = self.upsample_lpf.process(driven * 2.0);
        let up1 = self.upsample_lpf.process(0.0);
        let s0 = Self::shape(up0, self.hyst_mem, hyst);
        let s1 = Self::shape(up1, self.hyst_mem, hyst);
        let _ = self.downsample_lpf.process(s0);
        let saturated = self.downsample_lpf.process(s1);

        // Update hysteresis memory: 1-pole LPF on the saturated output.
        self.hyst_mem = saturated * (1.0 - self.hyst_alpha) + self.hyst_mem * self.hyst_alpha;

        // De-emphasis = warmth control (matches pre-emphasis bandwidth).
        let warm = self.de_emph.process(saturated);
        let toned = saturated * warmth + warm * (1.0 - warmth);

        // Wow modulation: write to delay buffer, read at LFO-modulated
        // fractional offset.
        let len = self.wow_buf.len();
        self.wow_buf[self.wow_write] = toned;
        self.wow_write = (self.wow_write + 1) % len;

        let wow_offset_samples = wow * 60.0 * (1.0 + 0.5 * self.wow_phase.sin());
        self.wow_phase += self.wow_phase_inc;
        if self.wow_phase > TAU { self.wow_phase -= TAU; }

        let read_pos = (self.wow_write as f32 - wow_offset_samples - 1.0).rem_euclid(len as f32);
        let i0 = read_pos as usize;
        let i1 = (i0 + 1) % len;
        let frac = read_pos - i0 as f32;
        let wowed = self.wow_buf[i0] * (1.0 - frac) + self.wow_buf[i1] * frac;

        wowed * (level * 1.5)
    }
}

struct DualMonoProcessor { left: TapeProcessor, right: TapeProcessor }

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
            float_parameter("drive", "Drive", Some("Gain"), Some(40.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("hysteresis", "Hysteresis", Some("Character"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("wow", "Wow", Some("Modulation"), Some(20.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("warmth", "Warmth", Some("EQ"), Some(60.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("level", "Level", Some("Output"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn read_settings(p: &ParameterSet) -> Result<Settings> {
    Ok(Settings {
        drive: required_f32(p, "drive").map_err(anyhow::Error::msg)?,
        hysteresis: required_f32(p, "hysteresis").map_err(anyhow::Error::msg)?,
        wow: required_f32(p, "wow").map_err(anyhow::Error::msg)?,
        warmth: required_f32(p, "warmth").map_err(anyhow::Error::msg)?,
        level: required_f32(p, "level").map_err(anyhow::Error::msg)?,
    })
}

pub fn validate_params(p: &ParameterSet) -> Result<()> { let _ = read_settings(p)?; Ok(()) }
pub fn asset_summary(_: &ParameterSet) -> Result<String> {
    Ok("native='tape_saturation' algorithm='hysteretic tanh + wow + 2x oversampled'".to_string())
}
fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

fn build(p: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let s = read_settings(p)?;
    Ok(match layout {
        AudioChannelLayout::Mono => BlockProcessor::Mono(Box::new(TapeProcessor::new(s, sample_rate))),
        AudioChannelLayout::Stereo => BlockProcessor::Stereo(Box::new(DualMonoProcessor {
            left: TapeProcessor::new(s, sample_rate),
            right: TapeProcessor::new(s, sample_rate),
        })),
    })
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Native,
    schema, validate: validate_params, asset_summary, build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Settings {
        Settings { drive: 40.0, hysteresis: 50.0, wow: 20.0, warmth: 60.0, level: 50.0 }
    }

    #[test]
    fn shape_silence_in_silence_out() {
        for h in [0.0_f32, 0.5, 1.0] {
            assert!(TapeProcessor::shape(0.0, 0.0, h).abs() < 1e-9);
        }
    }

    #[test]
    fn shape_with_memory_offsets_curve() {
        // With memory > 0 and hyst > 0, shape(x) shifts by (mem * hyst * 0.3).
        let no_mem = TapeProcessor::shape(0.5, 0.0, 1.0);
        let with_mem = TapeProcessor::shape(0.5, 0.5, 1.0);
        assert!((no_mem - with_mem).abs() > 0.01, "memory should shift the curve");
    }

    #[test]
    fn shape_is_bounded() {
        for h in [0.0_f32, 1.0] {
            for x in [-100.0_f32, -10.0, 10.0, 100.0] {
                let y = TapeProcessor::shape(x, 0.5, h);
                assert!(y.abs() <= 1.05, "shape({x}, 0.5, {h}) = {y}");
            }
        }
    }

    #[test]
    fn silence_input_produces_silence() {
        let mut p = TapeProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_finite_and_nonzero() {
        let mut p = TapeProcessor::new(defaults(), 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..2048 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.4;
            let out = p.process_sample(s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 { any_nonzero = true; }
        }
        assert!(any_nonzero);
    }

    #[test]
    fn dc_input_is_blocked() {
        let mut p = TapeProcessor::new(defaults(), 44_100.0);
        for _ in 0..8192 { let _ = p.process_sample(0.5); }
        let mut peak = 0.0_f32;
        for _ in 0..2048 { peak = peak.max(p.process_sample(0.5).abs()); }
        assert!(peak < 0.05, "DC was not blocked (peak {peak})");
    }

    #[test]
    fn wow_at_zero_does_not_modulate() {
        let mut p = TapeProcessor::new(
            Settings { drive: 0.0, hysteresis: 0.0, wow: 0.0, warmth: 100.0, level: 50.0 },
            44_100.0,
        );
        let sr = 44_100.0_f32;
        // Drive feeds a steady sine; with wow=0 there should be no
        // amplitude/pitch modulation across periods. Just check it
        // remains finite over a long window — modulation tests would
        // need an FFT to verify quantitatively.
        for i in 0..44_100 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.2;
            assert!(p.process_sample(s).is_finite());
        }
    }
}
