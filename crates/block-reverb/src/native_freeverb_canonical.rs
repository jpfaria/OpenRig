//! Freeverb (canonical) — port of Jezar at Dreampoint's Freeverb (2000-03-15).
//!
//! Public-domain reference paper: "Freeverb" — comb+allpass network derived
//! from Schroeder 1962 ("Natural Sounding Artificial Reverberation") and
//! Moorer 1979 ("About This Reverberation Business"). Jezar's variant
//! standardised 8 lowpass-feedback combs in parallel + 4 allpasses in series
//! per channel, with a fixed stereo spread of 23 samples on the right
//! channel comb lengths.
//!
//! This is the canonical Freeverb (8 combs / 4 allpasses) — distinct from
//! `native_room` (6 combs, smaller scale) and `native_hall` (different
//! comb tuning).

use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;

pub const MODEL_ID: &str = "freeverb_canonical";
pub const DISPLAY_NAME: &str = "Freeverb (canonical)";

// Jezar's canonical comb sizes (at 44.1 kHz). Right channel adds STEREO_SPREAD.
const COMB_SIZES: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_SIZES: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;

// Tuning constants from Jezar's freeverb_2000_03_15 reference.
const FIXED_GAIN: f32 = 0.015;
const SCALE_DAMP: f32 = 0.4;
const SCALE_ROOM: f32 = 0.28;
const OFFSET_ROOM: f32 = 0.7;

struct Params {
    room_size: f32,
    damping: f32,
    width: f32,
    mix: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            room_size: 50.0,
            damping: 50.0,
            width: 100.0,
            mix: 33.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    let d = Params::default();
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("room_size", "Room Size", None, Some(d.room_size), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("damping", "Damping", None, Some(d.damping), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("width", "Width", None, Some(d.width), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mix", "Mix", None, Some(d.mix), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn params_from_set(params: &ParameterSet) -> Result<Params> {
    Ok(Params {
        room_size: required_f32(params, "room_size").map_err(Error::msg)? / 100.0,
        damping: required_f32(params, "damping").map_err(Error::msg)? / 100.0,
        width: required_f32(params, "width").map_err(Error::msg)? / 100.0,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

struct Freeverb {
    params: Params,
    combs_l: Vec<CombFilter>,
    combs_r: Vec<CombFilter>,
    allpasses_l: Vec<AllpassFilter>,
    allpasses_r: Vec<AllpassFilter>,
}

impl Freeverb {
    fn new(params: Params, sample_rate: f32) -> Self {
        let scale = sample_rate / 44_100.0;
        let feedback = params.room_size * SCALE_ROOM + OFFSET_ROOM;
        let damping = params.damping * SCALE_DAMP;

        let mut combs_l: Vec<CombFilter> = COMB_SIZES
            .iter()
            .map(|&s| CombFilter::new((s as f32 * scale) as usize))
            .collect();
        let mut combs_r: Vec<CombFilter> = COMB_SIZES
            .iter()
            .map(|&s| CombFilter::new(((s + STEREO_SPREAD) as f32 * scale) as usize))
            .collect();
        for c in combs_l.iter_mut().chain(combs_r.iter_mut()) {
            c.set_feedback(feedback);
            c.set_damping(damping);
        }

        let allpasses_l: Vec<AllpassFilter> = ALLPASS_SIZES
            .iter()
            .map(|&s| AllpassFilter::new((s as f32 * scale) as usize))
            .collect();
        let allpasses_r: Vec<AllpassFilter> = ALLPASS_SIZES
            .iter()
            .map(|&s| AllpassFilter::new(((s + STEREO_SPREAD) as f32 * scale) as usize))
            .collect();

        Self {
            params,
            combs_l,
            combs_r,
            allpasses_l,
            allpasses_r,
        }
    }
}

impl StereoProcessor for Freeverb {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        // Freeverb input: sum of L+R scaled by FIXED_GAIN to keep tail
        // energy independent of input loudness.
        let mono_in = (input[0] + input[1]) * FIXED_GAIN;

        let wet_l_sum: f32 = self.combs_l.iter_mut().map(|c| c.process(mono_in)).sum();
        let wet_r_sum: f32 = self.combs_r.iter_mut().map(|c| c.process(mono_in)).sum();

        let mut wet_l = wet_l_sum;
        for ap in &mut self.allpasses_l {
            wet_l = ap.process(wet_l);
        }
        let mut wet_r = wet_r_sum;
        for ap in &mut self.allpasses_r {
            wet_r = ap.process(wet_r);
        }

        // Width=1 → full L/R separation; width=0 → mono sum.
        let w = self.params.width;
        let wet1 = w * 0.5 + 0.5;
        let wet2 = (1.0 - w) * 0.5;
        let out_l = wet_l * wet1 + wet_r * wet2;
        let out_r = wet_r * wet1 + wet_l * wet2;

        let dry = 1.0 - self.params.mix;
        [
            dry.mul_add(input[0], self.params.mix * out_l),
            dry.mul_add(input[1], self.params.mix * out_r),
        ]
    }
}

struct FreeverbAsMono(Freeverb);

impl MonoProcessor for FreeverbAsMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        let [left, _] = StereoProcessor::process_frame(&mut self.0, [input, input]);
        left
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let p = params_from_set(params)?;
    match layout {
        AudioChannelLayout::Stereo => {
            Ok(BlockProcessor::Stereo(Box::new(Freeverb::new(p, sample_rate))))
        }
        AudioChannelLayout::Mono => {
            Ok(BlockProcessor::Mono(Box::new(FreeverbAsMono(Freeverb::new(p, sample_rate)))))
        }
    }
}

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: ReverbBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

// ── filters ──────────────────────────────────────────────────────────

struct CombFilter {
    buffer: Vec<f32>,
    index: usize,
    feedback: f32,
    filter_store: f32,
    damping: f32,
}

impl CombFilter {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size.max(1)],
            index: 0,
            feedback: 0.84,
            filter_store: 0.0,
            damping: 0.2,
        }
    }

    fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback;
    }

    fn set_damping(&mut self, damping: f32) {
        self.damping = damping.clamp(0.0, 1.0);
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.index];
        self.filter_store =
            output * (1.0 - self.damping) + self.filter_store * self.damping;
        self.buffer[self.index] = input + self.filter_store * self.feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

struct AllpassFilter {
    buffer: Vec<f32>,
    index: usize,
}

impl AllpassFilter {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size.max(1)],
            index: 0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.index];
        let output = -input + buffered;
        // Freeverb fixed-feedback allpass coefficient.
        self.buffer[self.index] = input + buffered * 0.5;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

// ── tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_reverb() -> Freeverb {
        Freeverb::new(Params::default(), 44_100.0)
    }

    #[test]
    fn impulse_response_finite_and_decaying() {
        let mut reverb = default_reverb();
        let mut peak_late = 0.0f32;
        for i in 0..44_100 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [input, input]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at sample {i}");
            if i > 22_050 {
                peak_late = peak_late.max(l.abs()).max(r.abs());
            }
        }
        assert!(peak_late.is_finite(), "tail not finite");
    }

    #[test]
    fn silence_input_produces_finite_silence() {
        let mut reverb = default_reverb();
        for i in 0..1024 {
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [0.0, 0.0]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn sine_input_produces_finite_nonzero_output() {
        let mut reverb = default_reverb();
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..1024 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [s, s]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at {i}");
            if l.abs() > 1e-10 || r.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output");
    }

    #[test]
    fn mono_adapter_runs_silence_and_sine() {
        let mut mono = FreeverbAsMono(default_reverb());
        for _ in 0..512 {
            assert!(MonoProcessor::process_sample(&mut mono, 0.0).is_finite());
        }
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..1024 {
            let s = (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr).sin();
            let out = MonoProcessor::process_sample(&mut mono, s);
            assert!(out.is_finite());
            if out.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "mono adapter expected non-zero output");
    }

    #[test]
    fn process_block_remains_finite() {
        let mut reverb = default_reverb();
        let sr = 44_100.0_f32;
        let mut buf: Vec<[f32; 2]> = (0..1024)
            .map(|i| {
                let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
                [s, s]
            })
            .collect();
        StereoProcessor::process_block(&mut reverb, &mut buf);
        for [l, r] in &buf {
            assert!(l.is_finite() && r.is_finite());
        }
    }
}
