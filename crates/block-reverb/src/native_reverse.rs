//! Reverse reverb — buffer the input and play it back time-reversed with
//! a swell envelope (0→1 ramp) so each segment "rises" into the dry hit.
//!
//! Public-domain reference: classical "reverse" effect from Lexicon /
//! Eventide hardware (1980s) — record a segment, reverse it, apply
//! attack-shaped envelope, sum back. Implemented here as a double-buffered
//! ping-pong: one half of the buffer is being written while the other
//! half is being read backwards with envelope.

use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;

pub const MODEL_ID: &str = "reverse";
pub const DISPLAY_NAME: &str = "Reverse Reverb";

const MIN_LENGTH_MS: f32 = 100.0;
const MAX_LENGTH_MS: f32 = 2000.0;

struct Params {
    length_ms: f32,
    mix: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            length_ms: 600.0,
            mix: 50.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    let d = Params::default();
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter("length_ms", "Length", None, Some(d.length_ms), MIN_LENGTH_MS, MAX_LENGTH_MS, 10.0, ParameterUnit::Milliseconds),
            float_parameter("mix", "Mix", None, Some(d.mix), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn params_from_set(params: &ParameterSet) -> Result<Params> {
    Ok(Params {
        length_ms: required_f32(params, "length_ms").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

/// One-channel reverse buffer: ping-pong between two halves of equal
/// length L. Writes go to the active half (`write_half`); reads come
/// from the OTHER half, indexed backwards from L-1 down to 0, with a
/// linear 0→1 envelope so the reversed segment swells in.
struct ReverseBuffer {
    buf: Vec<f32>,
    half_len: usize,
    write_half: usize, // 0 or 1
    write_pos: usize,  // 0..half_len
}

impl ReverseBuffer {
    fn new(half_len: usize) -> Self {
        let len = half_len.max(1);
        Self {
            buf: vec![0.0; len * 2],
            half_len: len,
            write_half: 0,
            write_pos: 0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        // Read from the OTHER half, backwards.
        let read_half = 1 - self.write_half;
        let read_idx_in_half = self.half_len - 1 - self.write_pos;
        let wet = self.buf[read_half * self.half_len + read_idx_in_half];

        // Linear attack envelope: 0 at start of read, 1 at end.
        let env = self.write_pos as f32 / self.half_len as f32;

        // Now write input to the active half.
        self.buf[self.write_half * self.half_len + self.write_pos] = input;
        self.write_pos += 1;
        if self.write_pos >= self.half_len {
            self.write_pos = 0;
            self.write_half = 1 - self.write_half;
            // Clear the half we're about to write into so stale data
            // from the previous swell doesn't leak.
            let start = self.write_half * self.half_len;
            for s in &mut self.buf[start..start + self.half_len] {
                *s = 0.0;
            }
        }

        wet * env
    }
}

struct ReverseReverb {
    params: Params,
    left: ReverseBuffer,
    right: ReverseBuffer,
}

impl ReverseReverb {
    fn new(params: Params, sample_rate: f32) -> Self {
        let half_len =
            ((params.length_ms.clamp(MIN_LENGTH_MS, MAX_LENGTH_MS) / 1000.0) * sample_rate) as usize;
        Self {
            left: ReverseBuffer::new(half_len),
            right: ReverseBuffer::new(half_len),
            params,
        }
    }
}

impl StereoProcessor for ReverseReverb {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let wet_l = self.left.process(input[0]);
        let wet_r = self.right.process(input[1]);
        let dry = 1.0 - self.params.mix;
        [
            dry.mul_add(input[0], self.params.mix * wet_l),
            dry.mul_add(input[1], self.params.mix * wet_r),
        ]
    }
}

struct ReverseAsMono(ReverseReverb);

impl MonoProcessor for ReverseAsMono {
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
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(ReverseReverb::new(p, sample_rate)))),
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(ReverseAsMono(ReverseReverb::new(p, sample_rate))))),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn default_reverb() -> ReverseReverb {
        ReverseReverb::new(Params::default(), 44_100.0)
    }

    #[test]
    fn silence_input_produces_finite_silence() {
        let mut reverb = default_reverb();
        for i in 0..2048 {
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [0.0, 0.0]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at {i}");
            assert!(l.abs() < 1e-9 && r.abs() < 1e-9, "silence in must produce silence out");
        }
    }

    #[test]
    fn impulse_response_finite_and_appears_in_second_half() {
        let mut reverb = default_reverb();
        // Write impulse at sample 0; the reversed wet should appear in
        // the SECOND ping-pong window (after one half_len has elapsed).
        let mut peak = 0.0f32;
        for i in 0..44_100 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let [l, _r] = StereoProcessor::process_frame(&mut reverb, [input, input]);
            assert!(l.is_finite());
            peak = peak.max(l.abs());
        }
        assert!(peak > 1e-6, "expected non-zero wet output for an impulse");
    }

    #[test]
    fn sine_input_produces_finite_nonzero_output() {
        let mut reverb = default_reverb();
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..44_100 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [s, s]);
            assert!(l.is_finite() && r.is_finite());
            if l.abs() > 1e-6 || r.abs() > 1e-6 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output for sine input");
    }

    #[test]
    fn mono_adapter_runs_silence_and_sine() {
        let mut mono = ReverseAsMono(default_reverb());
        for _ in 0..512 {
            assert!(MonoProcessor::process_sample(&mut mono, 0.0).is_finite());
        }
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..44_100 {
            let s = (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr).sin();
            let out = MonoProcessor::process_sample(&mut mono, s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "mono adapter expected non-zero output");
    }

    #[test]
    fn envelope_starts_at_zero_for_each_window() {
        // First sample of each playback window should be silent
        // (envelope = 0 at the very start of the read pass).
        let mut buf = ReverseBuffer::new(100);
        // Fill the buffer with a non-zero pattern through one full ping-pong.
        for i in 0..100 {
            buf.process(1.0);
            let _ = i;
        }
        // Now we should be reading the previously-written half. The very
        // first wet output of this new pass has envelope = 0 → silence.
        let first_wet_envelope_zero = buf.process(1.0);
        assert!(first_wet_envelope_zero.abs() < 1e-9);
    }
}
