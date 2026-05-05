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
#[path = "native_wavefolder_tests.rs"]
mod tests;
