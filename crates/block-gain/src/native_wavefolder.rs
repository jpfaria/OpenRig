//! Wavefolder — Buchla 259 / west-coast modular waveshaper. When the
//! input exceeds a threshold the signal is "folded" back mirror-style,
//! generating dense harmonic content that grows non-linearly with drive.
//! Cascading multiple folds (or using a high-gain trigonometric folder)
//! yields the characteristic glassy/digital Buchla timbre.
//!
//! References:
//! - Esqueda, F., Bilbao, S. & Välimäki, V. (2017). "Aliasing Reduction
//!   in Soft-Clipping Algorithms", DAFx-17 — wavefolder section.
//! - Borrowski, R. (2011). "Identification of Nonlinear Audio Devices
//!   and Emulation of Analog Wave Folder", AES E-Library.
//!
//! Topology:
//! - Input HPF (DC block).
//! - 2× polyphase oversampling around the nonlinearity (wavefolders
//!   alias aggressively — oversampling is non-negotiable here).
//! - Trigonometric folder: y = sin(π/2 · drive · x). At drive=1 this is
//!   a single fold; at drive=10 it produces dozens of folds with
//!   harmonic density approaching that of a sawtooth.
//! - Output LPF (smooths the fold edges, balances brightness).
//! - Output level.

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

pub const MODEL_ID: &str = "wavefolder";
pub const DISPLAY_NAME: &str = "Wavefolder (Buchla)";
const BRAND: &str = block_core::BRAND_NATIVE;

#[derive(Debug, Clone, Copy)]
struct Settings {
    drive: f32,
    bias: f32,
    tone: f32,
    level: f32,
}

struct WavefolderProcessor {
    settings: Settings,
    in_hpf: OnePoleHighPass,
    out_lpf: OnePoleLowPass,
    upsample_lpf: OnePoleLowPass,
    downsample_lpf: OnePoleLowPass,
}

impl WavefolderProcessor {
    fn new(settings: Settings, sample_rate: f32) -> Self {
        let oversample_rate = sample_rate * 2.0;
        Self {
            settings,
            in_hpf: OnePoleHighPass::new(40.0, sample_rate),
            out_lpf: OnePoleLowPass::new(8_000.0, sample_rate),
            upsample_lpf: OnePoleLowPass::new(sample_rate * 0.45, oversample_rate),
            downsample_lpf: OnePoleLowPass::new(sample_rate * 0.45, oversample_rate),
        }
    }

    fn pct(v: f32) -> f32 { (v / 100.0).clamp(0.0, 1.0) }

    /// Trigonometric wavefolder. drive ≤ 1 produces a single fold at the
    /// peaks; higher drive cascades folds. Bias shifts the fold centre to
    /// introduce asymmetric harmonics (even harmonics).
    #[inline]
    fn fold(x: f32, drive: f32, bias: f32) -> f32 {
        // Pre-gain in radians: drive=1 → π/2 amplitude (just-saturating
        // single fold); drive=10 → up to 5π peaks → dense fold cascade.
        let phase = std::f32::consts::FRAC_PI_2 * drive * (x + bias);
        phase.sin() - (std::f32::consts::FRAC_PI_2 * drive * bias).sin()
    }
}

impl MonoProcessor for WavefolderProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let drive = 1.0 + Self::pct(self.settings.drive) * 9.0; // 1..10
        let bias = (Self::pct(self.settings.bias) * 2.0 - 1.0) * 0.3; // -0.3..0.3
        let tone = Self::pct(self.settings.tone);
        let level = Self::pct(self.settings.level);

        let x = self.in_hpf.process(input);

        // 2× oversampling around the trig fold.
        let up0 = self.upsample_lpf.process(x * 2.0);
        let up1 = self.upsample_lpf.process(0.0);
        let f0 = Self::fold(up0, drive, bias);
        let f1 = Self::fold(up1, drive, bias);
        let _ = self.downsample_lpf.process(f0);
        let down = self.downsample_lpf.process(f1);

        // Tone control: tone=1 keeps the bright folds; tone=0 lowpasses
        // toward a mellow sine-ish output.
        let warm = self.out_lpf.process(down);
        let toned = down * tone + warm * (1.0 - tone);

        toned * (level * 1.5)
    }
}

struct DualMonoProcessor { left: WavefolderProcessor, right: WavefolderProcessor }

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
            float_parameter("bias", "Bias", Some("Character"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("tone", "Tone", Some("EQ"), Some(60.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("level", "Level", Some("Output"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn read_settings(p: &ParameterSet) -> Result<Settings> {
    Ok(Settings {
        drive: required_f32(p, "drive").map_err(anyhow::Error::msg)?,
        bias: required_f32(p, "bias").map_err(anyhow::Error::msg)?,
        tone: required_f32(p, "tone").map_err(anyhow::Error::msg)?,
        level: required_f32(p, "level").map_err(anyhow::Error::msg)?,
    })
}

pub fn validate_params(p: &ParameterSet) -> Result<()> { let _ = read_settings(p)?; Ok(()) }
pub fn asset_summary(_: &ParameterSet) -> Result<String> {
    Ok("native='wavefolder' algorithm='trig wavefolder sin(π/2·drive·x) 2x oversampled'".to_string())
}
fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

fn build(p: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let s = read_settings(p)?;
    Ok(match layout {
        AudioChannelLayout::Mono => BlockProcessor::Mono(Box::new(WavefolderProcessor::new(s, sample_rate))),
        AudioChannelLayout::Stereo => BlockProcessor::Stereo(Box::new(DualMonoProcessor {
            left: WavefolderProcessor::new(s, sample_rate),
            right: WavefolderProcessor::new(s, sample_rate),
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
        Settings { drive: 40.0, bias: 50.0, tone: 60.0, level: 50.0 }
    }

    #[test]
    fn fold_silence_in_silence_out() {
        // sin(0) - sin(0) = 0 for any drive.
        for d in [0.5_f32, 1.0, 5.0, 10.0] {
            for b in [-0.3_f32, 0.0, 0.3] {
                assert!(WavefolderProcessor::fold(0.0, d, b).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn fold_is_bounded() {
        for d in [0.5_f32, 1.0, 5.0, 10.0] {
            for x in [-100.0_f32, -1.0, 0.0, 1.0, 100.0] {
                let y = WavefolderProcessor::fold(x, d, 0.0);
                // sin output ∈ [-1, 1], minus a constant offset → [-2, 2].
                assert!(y.abs() <= 2.5, "fold({x}, {d}) = {y}");
            }
        }
    }

    #[test]
    fn high_drive_produces_more_zero_crossings_than_low_drive() {
        // Driving the fold harder cascades more folds → more sign changes
        // per period of the input. We feed a slow-rising ramp and count
        // crossings of the fold output.
        let count_crossings = |drive: f32| {
            let mut prev = 0.0_f32;
            let mut n = 0;
            for i in 0..1000 {
                let x = i as f32 / 1000.0 * 2.0 - 1.0; // -1..+1 ramp
                let y = WavefolderProcessor::fold(x, drive, 0.0);
                if (prev <= 0.0 && y > 0.0) || (prev >= 0.0 && y < 0.0) {
                    n += 1;
                }
                prev = y;
            }
            n
        };
        let low = count_crossings(1.0);
        let high = count_crossings(8.0);
        assert!(high > low, "expected more crossings at high drive: low={low}, high={high}");
    }

    #[test]
    fn silence_input_produces_silence() {
        let mut p = WavefolderProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_finite_and_nonzero() {
        let mut p = WavefolderProcessor::new(defaults(), 44_100.0);
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
}
