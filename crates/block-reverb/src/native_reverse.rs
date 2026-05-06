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
#[path = "native_reverse_tests.rs"]
mod tests;
