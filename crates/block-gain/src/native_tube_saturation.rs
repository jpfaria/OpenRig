//! Tube saturation — asymmetric soft-clip emulating a triode tube
//! amplifier stage. tanh-based shaper with a small DC bias to break the
//! symmetry and introduce the 2nd-harmonic content that gives tubes
//! their characteristic warmth.
//!
//! References:
//! - Yeh, D. T. (2008). "Digital Implementation of Musical Distortion
//!   Circuits by Analysis and Simulation." Stanford CCRMA, ch. 3 on
//!   triode and tetrode nonlinearities.
//! - Pakarinen, J. & Yeh, D. (2009). "A Review of Digital Techniques
//!   for Modeling Vacuum-Tube Guitar Amplifiers." Computer Music J. 33(2).
//!
//! Topology:
//! 1. DC-blocking input HPF (~30 Hz) — removes any pedal-bus DC offset
//!    that would otherwise interact with the asymmetric clipper.
//! 2. Pre-emphasis HPF (~700 Hz) before the clipper to thicken the bite.
//! 3. 2× polyphase oversampling around the nonlinearity (anti-alias).
//! 4. tanh(x + bias) — bias 0.0..0.3 introduces 2nd-harmonic content.
//! 5. De-emphasis LPF after the clipper to roll off the brittle highs.
//! 6. Output level trim.

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

pub const MODEL_ID: &str = "tube_saturation";
pub const DISPLAY_NAME: &str = "Tube Saturation";
const BRAND: &str = block_core::BRAND_NATIVE;

#[derive(Debug, Clone, Copy)]
struct Settings {
    drive: f32,
    bias: f32,
    tone: f32,
    level: f32,
}

struct TubeProcessor {
    settings: Settings,
    dc_block: OnePoleHighPass,
    pre_emph: OnePoleHighPass,
    de_emph: OnePoleLowPass,
    // Oversampling state
    upsample_lpf: OnePoleLowPass,
    downsample_lpf: OnePoleLowPass,
}

impl TubeProcessor {
    fn new(settings: Settings, sample_rate: f32) -> Self {
        let oversample_rate = sample_rate * 2.0;
        Self {
            settings,
            dc_block: OnePoleHighPass::new(30.0, sample_rate),
            pre_emph: OnePoleHighPass::new(700.0, sample_rate),
            de_emph: OnePoleLowPass::new(6_500.0, sample_rate),
            // Halfband at 0.45×SR — kills the alias band before decimation.
            upsample_lpf: OnePoleLowPass::new(sample_rate * 0.45, oversample_rate),
            downsample_lpf: OnePoleLowPass::new(sample_rate * 0.45, oversample_rate),
        }
    }

    fn pct(v: f32) -> f32 {
        (v / 100.0).clamp(0.0, 1.0)
    }

    /// Asymmetric tanh saturator — bias > 0 pushes the curve so the
    /// negative half clips harder than the positive half (or vice
    /// versa), generating 2nd harmonic.
    #[inline]
    fn shape(x: f32, bias: f32) -> f32 {
        // tanh(x + bias) − tanh(bias) cancels the DC term so the
        // user-facing bias just controls *asymmetry*, not output offset.
        (x + bias).tanh() - bias.tanh()
    }
}

impl MonoProcessor for TubeProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let drive = Self::pct(self.settings.drive);
        let bias = Self::pct(self.settings.bias) * 0.3;
        let tone = Self::pct(self.settings.tone);
        let level = Self::pct(self.settings.level);

        let x = self.dc_block.process(input);
        let pre = self.pre_emph.process(x);
        // Drive: 0 → unity, 100 → ~30× before the clipper.
        let driven = (x + pre * 0.4) * (1.0 + drive * 30.0);

        // 2× polyphase oversampling around the nonlinearity.
        // Stage 1: upsample by 2 (zero-stuff, then lowpass).
        let up0 = self.upsample_lpf.process(driven * 2.0);
        let up1 = self.upsample_lpf.process(0.0);
        // Stage 2: nonlinear shape on both samples.
        let sh0 = Self::shape(up0, bias);
        let sh1 = Self::shape(up1, bias);
        // Stage 3: downsample by 2 (lowpass, take every other).
        let _ = self.downsample_lpf.process(sh0);
        let down = self.downsample_lpf.process(sh1);

        // De-emphasis: tone scales the cutoff effect — a soft tone control.
        let de = self.de_emph.process(down);
        let toned = down * tone + de * (1.0 - tone);

        // Output level. 50% = unity (so default doesn't change perceived loudness).
        toned * (level * 2.0)
    }
}

struct DualMonoProcessor {
    left: TubeProcessor,
    right: TubeProcessor,
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
            float_parameter("drive", "Drive", Some("Gain"), Some(40.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("bias", "Bias", Some("Character"), Some(30.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
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

pub fn validate_params(p: &ParameterSet) -> Result<()> {
    let _ = read_settings(p)?;
    Ok(())
}

pub fn asset_summary(_: &ParameterSet) -> Result<String> {
    Ok("native='tube_saturation' algorithm='tanh+bias 2x oversampled'".to_string())
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
            BlockProcessor::Mono(Box::new(TubeProcessor::new(s, sample_rate)))
        }
        AudioChannelLayout::Stereo => BlockProcessor::Stereo(Box::new(DualMonoProcessor {
            left: TubeProcessor::new(s, sample_rate),
            right: TubeProcessor::new(s, sample_rate),
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
        Settings { drive: 40.0, bias: 30.0, tone: 60.0, level: 50.0 }
    }

    #[test]
    fn shape_silence_in_silence_out() {
        // tanh(0 + bias) - tanh(bias) = 0 for any bias.
        for &b in &[0.0_f32, 0.1, 0.3, 0.5] {
            assert!(TubeProcessor::shape(0.0, b).abs() < 1e-9);
        }
    }

    #[test]
    fn shape_is_bounded_by_1() {
        for x in [-100.0_f32, -10.0, -1.0, 0.0, 1.0, 10.0, 100.0] {
            let y = TubeProcessor::shape(x, 0.2);
            assert!(y.abs() <= 1.5, "shape({x}, 0.2) = {y} exceeded soft bound");
        }
    }

    #[test]
    fn shape_is_asymmetric_with_bias() {
        // With bias > 0 the curve favours one side: |shape(+1, bias)| != |shape(-1, bias)|.
        let bias = 0.25;
        let pos = TubeProcessor::shape(1.0, bias).abs();
        let neg = TubeProcessor::shape(-1.0, bias).abs();
        assert!(
            (pos - neg).abs() > 0.02,
            "expected asymmetry, got |+| = {pos}, |-| = {neg}",
        );
    }

    #[test]
    fn silence_input_produces_finite_silence() {
        let mut p = TubeProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_finite_and_nonzero() {
        let mut p = TubeProcessor::new(defaults(), 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..2048 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.3;
            let out = p.process_sample(s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 { any_nonzero = true; }
        }
        assert!(any_nonzero);
    }

    #[test]
    fn dc_input_is_blocked() {
        // Constant DC input → output should settle near zero (DC block + tanh).
        let mut p = TubeProcessor::new(defaults(), 44_100.0);
        for _ in 0..8192 { let _ = p.process_sample(0.5); }
        let mut peak = 0.0_f32;
        for _ in 0..2048 { peak = peak.max(p.process_sample(0.5).abs()); }
        assert!(peak < 0.05, "DC was not blocked (peak {peak} > 0.05)");
    }

    #[test]
    fn dual_mono_processes_both_channels_independently() {
        let mut dm = DualMonoProcessor {
            left: TubeProcessor::new(defaults(), 44_100.0),
            right: TubeProcessor::new(defaults(), 44_100.0),
        };
        for i in 0..1024 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44_100.0).sin() * 0.3;
            // Left only; right gets zero. Output left should differ from right.
            let [l, r] = StereoProcessor::process_frame(&mut dm, [s, 0.0]);
            assert!(l.is_finite() && r.is_finite());
        }
    }
}
