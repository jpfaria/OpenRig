//! Bitcrusher — combined bit-depth quantisation and sample-rate reduction
//! (sample-and-hold). Aliasing IS the desired character so no anti-alias
//! filtering is applied around the SRR stage.
//!
//! References:
//! - Pirkle, W. C. (2014). "Designing Audio Effect Plugins in C++" — chapter
//!   on lo-fi effects, the canonical bitcrusher topology used here (bit
//!   reduction → sample-and-hold → optional dry/wet mix).
//! - Reiss, J. & McPherson, A. (2014). "Audio Effects: Theory,
//!   Implementation and Application", section 7.3 (downsampling effects).
//!
//! Consolidates issue #120.

use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor,
};

pub const MODEL_ID: &str = "bitcrusher";
pub const DISPLAY_NAME: &str = "Bitcrusher";
const BRAND: &str = block_core::BRAND_NATIVE;

#[derive(Debug, Clone, Copy)]
struct Settings {
    bits: f32,
    rate_pct: f32,
    mix: f32,
}

struct BitcrusherProcessor {
    settings: Settings,
    /// Phase accumulator for the sample-and-hold rate reducer.
    /// We tick by `rate_pct` per input sample; when it crosses 1.0 we
    /// take a new input sample, otherwise we hold the previous output.
    sh_phase: f32,
    sh_held: f32,
}

impl BitcrusherProcessor {
    fn new(settings: Settings, _sample_rate: f32) -> Self {
        Self {
            settings,
            sh_phase: 1.0, // start "ready to take a sample"
            sh_held: 0.0,
        }
    }

    fn pct(v: f32) -> f32 {
        (v / 100.0).clamp(0.0, 1.0)
    }

    /// Quantise to `bits` bits in the [-1, 1] range. bits in [1.0, 16.0].
    /// We allow fractional bits for smooth user control — the levels
    /// follow `2^bits` even when bits isn't integer.
    #[inline]
    fn quantise(x: f32, bits: f32) -> f32 {
        let levels = (2.0_f32).powf(bits.max(1.0));
        let step = 2.0 / levels;
        // Round to nearest level.
        (x / step).round() * step
    }
}

impl MonoProcessor for BitcrusherProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let bits = 1.0 + Self::pct(self.settings.bits) * 15.0; // 1..16 bits
        let rate_pct = Self::pct(self.settings.rate_pct).max(0.001); // avoid div-by-zero
        let mix = Self::pct(self.settings.mix);

        // Sample-and-hold rate reducer.
        self.sh_phase += rate_pct;
        if self.sh_phase >= 1.0 {
            self.sh_phase -= 1.0;
            self.sh_held = Self::quantise(input, bits);
        }

        let wet = self.sh_held;
        wet * mix + input * (1.0 - mix)
    }
}

struct DualMonoProcessor {
    left: BitcrusherProcessor,
    right: BitcrusherProcessor,
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
            float_parameter("bits", "Bits", Some("Resolution"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("rate_pct", "Rate", Some("SRR"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mix", "Mix", Some("Output"), Some(100.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn read_settings(p: &ParameterSet) -> Result<Settings> {
    Ok(Settings {
        bits: required_f32(p, "bits").map_err(anyhow::Error::msg)?,
        rate_pct: required_f32(p, "rate_pct").map_err(anyhow::Error::msg)?,
        mix: required_f32(p, "mix").map_err(anyhow::Error::msg)?,
    })
}

pub fn validate_params(p: &ParameterSet) -> Result<()> {
    let _ = read_settings(p)?;
    Ok(())
}

pub fn asset_summary(_: &ParameterSet) -> Result<String> {
    Ok("native='bitcrusher' algorithm='bit-quantise + sample-and-hold'".to_string())
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
            BlockProcessor::Mono(Box::new(BitcrusherProcessor::new(s, sample_rate)))
        }
        AudioChannelLayout::Stereo => BlockProcessor::Stereo(Box::new(DualMonoProcessor {
            left: BitcrusherProcessor::new(s, sample_rate),
            right: BitcrusherProcessor::new(s, sample_rate),
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
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Settings {
        Settings { bits: 50.0, rate_pct: 50.0, mix: 100.0 }
    }

    #[test]
    fn quantise_one_bit_collapses_to_two_levels() {
        // 1 bit → 2 levels → step = 1.0 → values round to ±1 or 0.
        let mut seen = std::collections::HashSet::new();
        for i in 0..1000 {
            let x = (i as f32 / 999.0) * 2.0 - 1.0; // -1..+1
            let q = BitcrusherProcessor::quantise(x, 1.0);
            // round-to-nearest with step 1.0 gives -1, 0, or 1.
            seen.insert((q * 1000.0) as i32);
        }
        assert!(seen.len() <= 5, "expected ~3 levels at 1 bit, got {seen:?}");
    }

    #[test]
    fn quantise_16_bit_matches_input_closely() {
        for x in [-0.7_f32, -0.1, 0.0, 0.05, 0.3, 0.99] {
            let q = BitcrusherProcessor::quantise(x, 16.0);
            assert!((q - x).abs() < 1.0 / 32_768.0, "16-bit quant: {x} → {q}");
        }
    }

    #[test]
    fn sample_and_hold_at_50_percent_repeats_every_other_sample() {
        // rate_pct = 50% with default 100% mix and 16 bits (low quant).
        let mut p = BitcrusherProcessor::new(
            Settings { bits: 100.0, rate_pct: 50.0, mix: 100.0 },
            44_100.0,
        );
        // Feed alternating values; SRR should hold each across two output samples.
        let mut last = None;
        let mut held_count = 0;
        for i in 0..40 {
            let v = if i % 2 == 0 { 0.5 } else { -0.5 };
            let out = p.process_sample(v);
            if let Some(prev) = last {
                if (out - prev as f32).abs() < 1e-3 {
                    held_count += 1;
                }
            }
            last = Some(out);
        }
        // We should see consecutive equal pairs for at least half the samples.
        assert!(held_count > 10, "expected sample-and-hold repeats, saw {held_count}");
    }

    #[test]
    fn silence_input_produces_silence() {
        let mut p = BitcrusherProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_finite_and_nonzero() {
        let mut p = BitcrusherProcessor::new(defaults(), 44_100.0);
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
    fn mix_zero_passes_dry_input_through() {
        let mut p = BitcrusherProcessor::new(
            Settings { bits: 50.0, rate_pct: 50.0, mix: 0.0 },
            44_100.0,
        );
        for i in 0..512 {
            let s = i as f32 * 0.001;
            let out = p.process_sample(s);
            // mix=0 → out should equal input (within float tolerance).
            assert!((out - s).abs() < 1e-6);
        }
    }
}
