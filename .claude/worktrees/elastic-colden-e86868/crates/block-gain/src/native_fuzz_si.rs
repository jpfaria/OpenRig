//! Silicon Fuzz Face — Dallas Arbiter Fuzz Face with Si BC108-style NPN
//! transistors. Si fuzz has a sharper, more aggressive clip with bright
//! upper harmonics — distinct from the smoother Ge variant.
//!
//! References:
//! - electrosmash.com — "Fuzz Face Analysis" (Si BC108 transistor pair
//!   in a positive-feedback configuration; ~70 dB total gain).
//! - Yeh, D. T. (2008). 'Digital Implementation of Musical Distortion
//!   Circuits', Stanford CCRMA — chapter on Fuzz Face.
//!
//! Topology:
//! - Input HPF (DC block + treble preservation)
//! - Massive pre-gain (Si Fuzz Face: ~70 dB)
//! - Two-stage soft clip (mimics the cascaded transistor saturation)
//! - Asymmetric clip thresholds for the Si bite
//! - 2× oversampling around the nonlinearity
//! - Output HPF + level
//!
//! Compared to native_fuzz_ge: harder clip, brighter, more aggressive.

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

pub const MODEL_ID: &str = "fuzz_si";
pub const DISPLAY_NAME: &str = "Fuzz Face (Si)";
const BRAND: &str = block_core::BRAND_NATIVE;

#[derive(Debug, Clone, Copy)]
struct Settings {
    fuzz: f32,
    tone: f32,
    level: f32,
}

struct FuzzProcessor {
    settings: Settings,
    in_hpf: OnePoleHighPass,
    out_hpf: OnePoleHighPass,
    tone_lpf: OnePoleLowPass,
    upsample_lpf: OnePoleLowPass,
    downsample_lpf: OnePoleLowPass,
}

impl FuzzProcessor {
    fn new(settings: Settings, sample_rate: f32) -> Self {
        let oversample_rate = sample_rate * 2.0;
        Self {
            settings,
            in_hpf: OnePoleHighPass::new(60.0, sample_rate),
            out_hpf: OnePoleHighPass::new(40.0, sample_rate),
            tone_lpf: OnePoleLowPass::new(3_500.0, sample_rate),
            upsample_lpf: OnePoleLowPass::new(sample_rate * 0.45, oversample_rate),
            downsample_lpf: OnePoleLowPass::new(sample_rate * 0.45, oversample_rate),
        }
    }

    fn pct(v: f32) -> f32 { (v / 100.0).clamp(0.0, 1.0) }

    /// Si transistor saturation — harder clip than Ge. We use a tanh
    /// shaper but with very high pre-gain and asymmetric clip thresholds:
    /// positive half clips at ~0.85, negative half at ~−1.0 (Si junction
    /// asymmetry).
    #[inline]
    fn si_shape(x: f32) -> f32 {
        let pos_clip = 0.85;
        let neg_clip = 1.0;
        let limit = if x > 0.0 { pos_clip } else { neg_clip };
        // Smoothed clip: x * limit / sqrt(1 + (x/limit)^2) is a soft
        // hyperbola that approaches ±limit asymptotically.
        let xn = x / limit;
        (xn / (1.0 + xn * xn).sqrt()) * limit
    }
}

impl MonoProcessor for FuzzProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let fuzz = Self::pct(self.settings.fuzz);
        let tone = Self::pct(self.settings.tone);
        let level = Self::pct(self.settings.level);

        let x = self.in_hpf.process(input);
        // Fuzz Face has fixed massive gain — knob effectively varies
        // the bias and feedback, producing a usable range. Map fuzz
        // 0..1 to gain 5..150.
        let pre = x * (5.0 + fuzz * 145.0);

        // Two-stage clip with 2× oversampling.
        let up0 = self.upsample_lpf.process(pre * 2.0);
        let up1 = self.upsample_lpf.process(0.0);
        let s0 = Self::si_shape(Self::si_shape(up0));
        let s1 = Self::si_shape(Self::si_shape(up1));
        let _ = self.downsample_lpf.process(s0);
        let down = self.downsample_lpf.process(s1);

        // Tone control.
        let warm = self.tone_lpf.process(down);
        let toned = down * tone + warm * (1.0 - tone);

        let out = self.out_hpf.process(toned);
        out * (level * 1.5)  // 50% = ~0.75x (Fuzz Face is loud at unity)
    }
}

struct DualMonoProcessor { left: FuzzProcessor, right: FuzzProcessor }

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
            float_parameter("fuzz", "Fuzz", Some("Gain"), Some(60.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("tone", "Tone", Some("EQ"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("level", "Level", Some("Output"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn read_settings(p: &ParameterSet) -> Result<Settings> {
    Ok(Settings {
        fuzz: required_f32(p, "fuzz").map_err(anyhow::Error::msg)?,
        tone: required_f32(p, "tone").map_err(anyhow::Error::msg)?,
        level: required_f32(p, "level").map_err(anyhow::Error::msg)?,
    })
}

pub fn validate_params(p: &ParameterSet) -> Result<()> { let _ = read_settings(p)?; Ok(()) }
pub fn asset_summary(_: &ParameterSet) -> Result<String> {
    Ok("native='fuzz_si' algorithm='Si Fuzz Face — 2-stage soft clip 2x oversampled'".to_string())
}
fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

fn build(p: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let s = read_settings(p)?;
    Ok(match layout {
        AudioChannelLayout::Mono => BlockProcessor::Mono(Box::new(FuzzProcessor::new(s, sample_rate))),
        AudioChannelLayout::Stereo => BlockProcessor::Stereo(Box::new(DualMonoProcessor {
            left: FuzzProcessor::new(s, sample_rate),
            right: FuzzProcessor::new(s, sample_rate),
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
#[path = "native_fuzz_si_tests.rs"]
mod tests;
