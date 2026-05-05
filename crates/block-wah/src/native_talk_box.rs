//! Talk-box / formant filter — 3-band parallel band-pass cascade
//! sweeping the vowel formants F1/F2/F3.
//!
//! Reference: Fant, G. (1960). "Acoustic Theory of Speech Production."
//! Standard adult-male formant centres for the cardinal vowels:
//!
//!         F1     F2     F3
//!   A    730   1090   2440
//!   E    530   1840   2480
//!   I    270   2290   3010
//!   O    570    840   2410
//!   U    300    870   2240
//!
//! The vowel knob slides continuously between A→E→I→O→U with linear
//! interpolation of all three formants — gives the classic
//! "Peter-Frampton-talk-box-without-the-tube" envelope-vowel sound
//! (and works as a stand-alone formant filter for vox-style guitars).
//!
//! Pro-tier topology:
//!   * 3 ZDF SVFs (block_core::dsp::Svf) — sweep-friendly resonance.
//!   * Q is shared across the three bands and follows the Intensity
//!     knob (higher = peakier, more "vowelly").
//!   * DcBlocker on the wet sum.
//!
//! RT-safe: 3 SVFs of state (8 floats), 1 DcBlocker. Zero alloc on
//! hot path.

use anyhow::{Error, Result};
use block_core::dsp::{flush_denormal, DcBlocker, Svf};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor,
};

use crate::registry::WahModelDefinition;
use crate::WahBackendKind;

pub const MODEL_ID: &str = "talk_box";
pub const DISPLAY_NAME: &str = "Talk Box";

/// Vowel formants table (F1, F2, F3) for cardinal vowels A/E/I/O/U.
const FORMANTS: [[f32; 3]; 5] = [
    [730.0, 1090.0, 2440.0], // A
    [530.0, 1840.0, 2480.0], // E
    [270.0, 2290.0, 3010.0], // I
    [570.0, 840.0, 2410.0],  // O
    [300.0, 870.0, 2240.0],  // U
];

/// Linearly interpolate F1/F2/F3 for a vowel position in [0, 4].
fn interpolate_vowel(t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 4.0);
    let i = t.floor() as usize;
    let frac = t - i as f32;
    if i >= 4 {
        return FORMANTS[4];
    }
    let a = FORMANTS[i];
    let b = FORMANTS[i + 1];
    [
        a[0] + (b[0] - a[0]) * frac,
        a[1] + (b[1] - a[1]) * frac,
        a[2] + (b[2] - a[2]) * frac,
    ]
}

#[derive(Clone, Copy)]
pub struct TalkBoxParams {
    pub vowel: f32,     // 0..=4 (A → U)
    pub intensity: f32, // Q (1.5..=20)
    pub mix: f32,
}

pub struct TalkBox {
    f1: Svf,
    f2: Svf,
    f3: Svf,
    dc_blocker: DcBlocker,
    mix: f32,
}

impl TalkBox {
    pub fn new(p: TalkBoxParams, sample_rate: f32) -> Self {
        let [f1, f2, f3] = interpolate_vowel(p.vowel);
        let q = p.intensity;
        Self {
            f1: Svf::new(f1, q, sample_rate),
            f2: Svf::new(f2, q, sample_rate),
            f3: Svf::new(f3, q, sample_rate),
            dc_blocker: DcBlocker::new(5.0, sample_rate),
            mix: p.mix.clamp(0.0, 1.0),
        }
    }
}

impl MonoProcessor for TalkBox {
    fn process_sample(&mut self, input: f32) -> f32 {
        let b1 = self.f1.process_band(input);
        let b2 = self.f2.process_band(input);
        let b3 = self.f3.process_band(input);
        // Equal contribution; F2 and F3 typically read quieter so add
        // a small boost to the upper formants for "vowel" colour.
        let wet_raw = b1 + 0.85 * b2 + 0.7 * b3;
        let wet = self.dc_blocker.process(flush_denormal(wet_raw));
        (1.0 - self.mix) * input + self.mix * wet
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: "wah".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "vowel",
                "Vowel",
                Some("Wah"),
                Some(0.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "intensity",
                "Intensity",
                Some("Wah"),
                Some(60.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                Some("Output"),
                Some(100.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    })
}

fn parse(params: &ParameterSet) -> Result<TalkBoxParams> {
    let vowel_pct = required_f32(params, "vowel").map_err(Error::msg)?;
    let intensity_pct = required_f32(params, "intensity").map_err(Error::msg)?;
    let mix_pct = required_f32(params, "mix").map_err(Error::msg)?;
    Ok(TalkBoxParams {
        // 0% → 0 (A), 100% → 4 (U). Continuous morph across vowels.
        vowel: (vowel_pct / 100.0) * 4.0,
        intensity: 1.5 + (intensity_pct / 100.0) * 18.5,
        mix: mix_pct / 100.0,
    })
}

fn validate(params: &ParameterSet) -> Result<()> {
    let _ = parse(params)?;
    Ok(())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let p = parse(params)?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(TalkBox::new(p, sample_rate)))),
        AudioChannelLayout::Stereo => {
            struct Dual { l: Box<dyn MonoProcessor>, r: Box<dyn MonoProcessor> }
            impl StereoProcessor for Dual {
                fn process_frame(&mut self, i: [f32; 2]) -> [f32; 2] {
                    [self.l.process_sample(i[0]), self.r.process_sample(i[1])]
                }
            }
            Ok(BlockProcessor::Stereo(Box::new(Dual {
                l: Box::new(TalkBox::new(p, sample_rate)),
                r: Box::new(TalkBox::new(p, sample_rate)),
            })))
        }
    }
}

pub const MODEL_DEFINITION: WahModelDefinition = WahModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: WahBackendKind::Native,
    schema,
    validate,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn p() -> TalkBoxParams {
        TalkBoxParams { vowel: 0.0, intensity: 8.0, mix: 1.0 }
    }

    #[test]
    fn silence_in_silence_out() {
        let mut t = TalkBox::new(p(), 44_100.0);
        for _ in 0..2048 {
            let out = t.process_sample(0.0);
            assert!(out.abs() < 1e-20, "talk-box silence: {out}");
        }
    }

    #[test]
    fn sine_input_finite() {
        let mut t = TalkBox::new(p(), 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..4096 {
            let x = (TAU * 440.0 * i as f32 / sr).sin();
            let y = t.process_sample(x);
            assert!(y.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn vowel_interpolation_endpoints_match_table() {
        assert_eq!(interpolate_vowel(0.0), FORMANTS[0]);
        assert_eq!(interpolate_vowel(1.0), FORMANTS[1]);
        assert_eq!(interpolate_vowel(4.0), FORMANTS[4]);
    }

    #[test]
    fn dry_mix_passes_input_through() {
        let mut t = TalkBox::new(
            TalkBoxParams { vowel: 0.0, intensity: 8.0, mix: 0.0 },
            44_100.0,
        );
        let sr = 44_100.0_f32;
        for i in 0..1024 {
            let x = (TAU * 440.0 * i as f32 / sr).sin();
            let y = t.process_sample(x);
            assert!((y - x).abs() < 1e-6, "mix=0 should be dry");
        }
    }
}
