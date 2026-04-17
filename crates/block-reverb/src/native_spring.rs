use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, OnePoleHighPass,
    OnePoleLowPass, StereoProcessor,
};

use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;

pub const MODEL_ID: &str = "spring";
pub const DISPLAY_NAME: &str = "Spring Reverb";

// Allpass sizes (samples at 44100 Hz) for the spring coil diffusion chain.
// Two slightly different sets for L/R give a subtle stereo width.
const ALLPASS_SIZES_L: [usize; 6] = [601, 803, 1009, 1201, 1499, 1801];
const ALLPASS_SIZES_R: [usize; 6] = [619, 829, 1031, 1237, 1543, 1867];

// Feedback comb for the "boing" sustain characteristic of spring reverb.
const COMB_SIZE_L: usize = 2203;
const COMB_SIZE_R: usize = 2269;

struct Params {
    tension: f32,
    damping: f32,
    mix: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            tension: 50.0,
            damping: 30.0,
            mix: 35.0,
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
            float_parameter(
                "tension",
                "Tension",
                None,
                Some(d.tension),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "damping",
                "Damping",
                None,
                Some(d.damping),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(d.mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

fn params_from_set(params: &ParameterSet) -> Result<Params> {
    Ok(Params {
        tension: required_f32(params, "tension").map_err(Error::msg)? / 100.0,
        damping: required_f32(params, "damping").map_err(Error::msg)? / 100.0,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

/// Single-channel spring reverb path.
///
/// Simulates a spring tank: input diffuses through a chain of short allpass
/// filters (the coil), then feeds into a resonant comb with a highpass in the
/// feedback path to recreate the characteristic bright, metallic sustain.
struct SpringChannel {
    allpasses: Vec<AllpassFilter>,
    comb_buffer: Vec<f32>,
    comb_index: usize,
    hp: OnePoleHighPass,
    lp: OnePoleLowPass,
}

impl SpringChannel {
    fn new(allpass_sizes: &[usize], comb_size: usize, sample_rate: f32) -> Self {
        let scale = sample_rate / 44_100.0;
        let allpasses = allpass_sizes
            .iter()
            .map(|&s| AllpassFilter::new((s as f32 * scale) as usize))
            .collect();
        let comb_len = ((comb_size as f32 * scale) as usize).max(1);
        // Highpass at ~200 Hz removes low-end mud from the spring feedback,
        // giving the characteristic bright "boing" of a real spring reverb.
        let hp = OnePoleHighPass::new(200.0, sample_rate);
        let lp = OnePoleLowPass::new(8_000.0, sample_rate);
        Self {
            allpasses,
            comb_buffer: vec![0.0; comb_len],
            comb_index: 0,
            hp,
            lp,
        }
    }

    // `feedback` = tension-derived coefficient (0.70–0.95).
    fn process(&mut self, input: f32, feedback: f32) -> f32 {
        // Diffuse input through the allpass chain (spring coil).
        let mut diffused = input;
        for ap in &mut self.allpasses {
            diffused = ap.process(diffused);
        }

        let comb_out = self.comb_buffer[self.comb_index];

        // Apply highpass + lowpass to the feedback signal to shape spring tone.
        let feedback_sig = self.lp.process(self.hp.process(comb_out));

        self.comb_buffer[self.comb_index] = diffused + feedback_sig * feedback;
        self.comb_index = (self.comb_index + 1) % self.comb_buffer.len();

        comb_out
    }
}

struct SpringReverb {
    params: Params,
    channel_l: SpringChannel,
    channel_r: SpringChannel,
}

impl SpringReverb {
    fn new(params: Params, sample_rate: f32) -> Self {
        Self {
            channel_l: SpringChannel::new(&ALLPASS_SIZES_L, COMB_SIZE_L, sample_rate),
            channel_r: SpringChannel::new(&ALLPASS_SIZES_R, COMB_SIZE_R, sample_rate),
            params,
        }
    }

    // Map tension (0.0–1.0) to feedback coefficient (0.70–0.95).
    fn feedback(&self) -> f32 {
        0.70 + self.params.tension * 0.25
    }
}

impl StereoProcessor for SpringReverb {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mono = input[0];
        let fb = self.feedback();

        let wet_l = self.channel_l.process(mono, fb);
        let wet_r = self.channel_r.process(mono, fb);

        let dry = 1.0 - self.params.mix;
        [
            dry.mul_add(mono, self.params.mix * wet_l),
            dry.mul_add(mono, self.params.mix * wet_r),
        ]
    }
}

struct SpringAsMono(SpringReverb);

impl MonoProcessor for SpringAsMono {
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
            Ok(BlockProcessor::Stereo(Box::new(SpringReverb::new(p, sample_rate))))
        }
        AudioChannelLayout::Mono => {
            Ok(BlockProcessor::Mono(Box::new(SpringAsMono(SpringReverb::new(p, sample_rate)))))
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
    fn spring_output_is_finite() {
        let mut reverb = SpringReverb::new(Params::default(), 44_100.0);
        for i in 0..10_000 {
            let input = if i % 100 == 0 { 0.5 } else { 0.0 };
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [input, input]);
            assert!(l.is_finite(), "left output not finite at sample {i}");
            assert!(r.is_finite(), "right output not finite at sample {i}");
        }
    }

    #[test]
    fn process_frame_silence_output_finite() {
        let mut reverb = SpringReverb::new(Params::default(), 44_100.0);
        for i in 0..1024 {
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [0.0, 0.0]);
            assert!(l.is_finite(), "left NaN/Inf at sample {i}");
            assert!(r.is_finite(), "right NaN/Inf at sample {i}");
        }
    }

    #[test]
    fn process_frame_sine_output_finite_and_nonzero() {
        let mut reverb = SpringReverb::new(Params::default(), 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..1024 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [input, input]);
            assert!(l.is_finite(), "left not finite at sample {i}");
            assert!(r.is_finite(), "right not finite at sample {i}");
            if l.abs() > 1e-10 || r.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output for sine input");
    }

    #[test]
    fn process_block_stereo_all_finite() {
        let mut reverb = SpringReverb::new(Params::default(), 44_100.0);
        let sr = 44_100.0_f32;
        let mut buffer: Vec<[f32; 2]> = (0..1024)
            .map(|i| {
                let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
                [s, s]
            })
            .collect();
        StereoProcessor::process_block(&mut reverb, &mut buffer);
        for (i, [l, r]) in buffer.iter().enumerate() {
            assert!(l.is_finite(), "left not finite at frame {i}");
            assert!(r.is_finite(), "right not finite at frame {i}");
        }
    }

    #[test]
    fn process_frame_mono_adapter_silence_finite() {
        let mut mono = SpringAsMono(SpringReverb::new(Params::default(), 44_100.0));
        for i in 0..1024 {
            let out = MonoProcessor::process_sample(&mut mono, 0.0);
            assert!(out.is_finite(), "mono output not finite at sample {i}");
        }
    }

    #[test]
    fn process_frame_mono_adapter_sine_finite_and_nonzero() {
        let mut mono = SpringAsMono(SpringReverb::new(Params::default(), 44_100.0));
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..1024 {
            let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let out = MonoProcessor::process_sample(&mut mono, input);
            assert!(out.is_finite(), "mono not finite at sample {i}");
            if out.abs() > 1e-10 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output for sine input (mono)");
    }
}
