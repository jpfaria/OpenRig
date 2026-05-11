use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;

pub const MODEL_ID: &str = "hall";
pub const DISPLAY_NAME: &str = "Hall Reverb";

// Freeverb standard comb sizes (samples at 44100 Hz). R channel offset = +23.
const COMB_SIZES: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_SIZES: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;

struct Params {
    room_size: f32,
    pre_delay_ms: f32,
    damping: f32,
    mix: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            room_size: 75.0,
            pre_delay_ms: 20.0,
            damping: 40.0,
            mix: 30.0,
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
            float_parameter("pre_delay_ms", "Pre-Delay", None, Some(d.pre_delay_ms), 0.0, 100.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("damping", "Damping", None, Some(d.damping), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mix", "Mix", None, Some(d.mix), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn params_from_set(params: &ParameterSet) -> Result<Params> {
    Ok(Params {
        room_size: required_f32(params, "room_size").map_err(Error::msg)? / 100.0,
        pre_delay_ms: required_f32(params, "pre_delay_ms").map_err(Error::msg)?,
        damping: required_f32(params, "damping").map_err(Error::msg)? / 100.0,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

struct HallReverb {
    params: Params,
    sample_rate: f32,
    pre_delay: DelayLine,
    combs_l: Vec<CombFilter>,
    combs_r: Vec<CombFilter>,
    allpasses_l: Vec<AllpassFilter>,
    allpasses_r: Vec<AllpassFilter>,
}

impl HallReverb {
    fn new(params: Params, sample_rate: f32) -> Self {
        let scale = sample_rate / 44_100.0;
        let fb = room_feedback(params.room_size);
        let damp = params.damping;
        let max_pre_delay = (100.0 * sample_rate / 1000.0) as usize + 1;

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

        Self {
            pre_delay: DelayLine::new(max_pre_delay),
            combs_l,
            combs_r,
            allpasses_l,
            allpasses_r,
            params,
            sample_rate,
        }
    }
}

impl StereoProcessor for HallReverb {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mono = input[0];
        let delay_samples = (self.params.pre_delay_ms * self.sample_rate / 1000.0) as usize;
        let delayed = self.pre_delay.process(mono, delay_samples);

        let mut wet_l: f32 = self.combs_l.iter_mut().map(|c| c.process(delayed)).sum();
        wet_l /= self.combs_l.len() as f32;
        for ap in &mut self.allpasses_l {
            wet_l = ap.process(wet_l);
        }

        let mut wet_r: f32 = self.combs_r.iter_mut().map(|c| c.process(delayed)).sum();
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

struct HallAsMono(HallReverb);

impl MonoProcessor for HallAsMono {
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
            Ok(BlockProcessor::Stereo(Box::new(HallReverb::new(p, sample_rate))))
        }
        AudioChannelLayout::Mono => {
            Ok(BlockProcessor::Mono(Box::new(HallAsMono(HallReverb::new(p, sample_rate)))))
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

struct DelayLine {
    buffer: Vec<f32>,
    write_index: usize,
}

impl DelayLine {
    fn new(max_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; max_samples.max(1)],
            write_index: 0,
        }
    }

    fn process(&mut self, input: f32, delay_samples: usize) -> f32 {
        let len = self.buffer.len();
        let delay_samples = delay_samples.min(len - 1);
        self.buffer[self.write_index] = input;
        let read_index = (self.write_index + len - delay_samples) % len;
        self.write_index = (self.write_index + 1) % len;
        self.buffer[read_index]
    }
}

#[cfg(test)]
#[path = "native_hall_tests.rs"]
mod tests;
