use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;

pub const MODEL_ID: &str = "room";
pub const DISPLAY_NAME: &str = "Room Reverb";

// Comb sizes for a small room (~0.5x of Freeverb hall sizes). R channel offset = +11.
const COMB_SIZES: [usize; 6] = [558, 594, 638, 678, 711, 745];
const ALLPASS_SIZES: [usize; 4] = [278, 220, 170, 112];
const STEREO_SPREAD: usize = 11;

struct Params {
    room_size: f32,
    damping: f32,
    mix: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            room_size: 40.0,
            damping: 50.0,
            mix: 25.0,
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
            float_parameter("mix", "Mix", None, Some(d.mix), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn params_from_set(params: &ParameterSet) -> Result<Params> {
    Ok(Params {
        room_size: required_f32(params, "room_size").map_err(Error::msg)? / 100.0,
        damping: required_f32(params, "damping").map_err(Error::msg)? / 100.0,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

pub struct RoomReverb {
    params: Params,
    combs_l: Vec<CombFilter>,
    combs_r: Vec<CombFilter>,
    allpasses_l: Vec<AllpassFilter>,
    allpasses_r: Vec<AllpassFilter>,
}

impl RoomReverb {
    fn new(params: Params, sample_rate: f32) -> Self {
        let scale = sample_rate / 44_100.0;
        let fb = room_feedback(params.room_size);
        let damp = params.damping;

        let mut combs_l: Vec<CombFilter> = COMB_SIZES
            .iter()
            .map(|&s| CombFilter::new((s as f32 * scale) as usize))
            .collect();
        for c in &mut combs_l {
            c.set_feedback(fb);
            c.set_damping(damp);
        }

        let mut combs_r: Vec<CombFilter> = COMB_SIZES
            .iter()
            .map(|&s| CombFilter::new(((s + STEREO_SPREAD) as f32 * scale) as usize))
            .collect();
        for c in &mut combs_r {
            c.set_feedback(fb);
            c.set_damping(damp);
        }

        let allpasses_l: Vec<AllpassFilter> = ALLPASS_SIZES
            .iter()
            .map(|&s| AllpassFilter::new((s as f32 * scale) as usize))
            .collect();

        let allpasses_r: Vec<AllpassFilter> = ALLPASS_SIZES
            .iter()
            .map(|&s| AllpassFilter::new(((s + STEREO_SPREAD) as f32 * scale) as usize))
            .collect();

        Self { params, combs_l, combs_r, allpasses_l, allpasses_r }
    }
}

impl StereoProcessor for RoomReverb {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mono = input[0];

        let mut wet_l: f32 = self.combs_l.iter_mut().map(|c| c.process(mono)).sum();
        wet_l /= self.combs_l.len() as f32;
        for ap in &mut self.allpasses_l {
            wet_l = ap.process(wet_l);
        }

        let mut wet_r: f32 = self.combs_r.iter_mut().map(|c| c.process(mono)).sum();
        wet_r /= self.combs_r.len() as f32;
        for ap in &mut self.allpasses_r {
            wet_r = ap.process(wet_r);
        }

        let dry = 1.0 - self.params.mix;
        [
            dry.mul_add(mono, self.params.mix * wet_l),
            dry.mul_add(mono, self.params.mix * wet_r),
        ]
    }
}

struct RoomAsMono(RoomReverb);

impl MonoProcessor for RoomAsMono {
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
            Ok(BlockProcessor::Stereo(Box::new(RoomReverb::new(p, sample_rate))))
        }
        AudioChannelLayout::Mono => {
            Ok(BlockProcessor::Mono(Box::new(RoomAsMono(RoomReverb::new(p, sample_rate)))))
        }
    }
}

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "",
    backend_kind: ReverbBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

fn room_feedback(room_size: f32) -> f32 {
    (0.2 + room_size.clamp(0.0, 1.0) * 0.77).clamp(0.0, 0.97)
}

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
            feedback: 0.7,
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
        self.buffer[self.index] = input + buffered * 0.5;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_output_is_finite() {
        let mut reverb = RoomReverb::new(Params::default(), 44_100.0);
        for i in 0..10_000 {
            let input = if i % 100 == 0 { 0.5 } else { 0.0 };
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [input, input]);
            assert!(l.is_finite(), "left output not finite at sample {i}");
            assert!(r.is_finite(), "right output not finite at sample {i}");
        }
    }
}
