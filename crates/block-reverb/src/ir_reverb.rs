//! IR (convolution) reverb backend.
//!
//! Wraps a wet impulse-response convolution (from `crates/ir`) with a
//! dry/wet mixer, an optional pre-delay on the wet path, and a wet level
//! trim. Unlike a cab block (100% wet, spectral-peak normalised), a reverb
//! blends the dry signal with a long diffuse tail, so the wet path stays
//! gain-passive and the mix is the primary control.
//!
//! Issue: #733

use anyhow::Result;
use block_core::param::{float_parameter, ParameterSet, ParameterSpec, ParameterUnit};
use block_core::{AudioChannelLayout, BlockProcessor, MonoProcessor, StereoProcessor};
use plugin_loader::LoadedPackage;

/// Wet fraction (`%`) default — a tasteful blend, dry-dominant.
const DEFAULT_MIX_PCT: f32 = 30.0;
/// Pre-delay (ms) default — none.
const DEFAULT_PRE_DELAY_MS: f32 = 0.0;
/// Wet level trim (dB) default — unity.
const DEFAULT_LEVEL_DB: f32 = 0.0;

/// Parameter schema shared by every IR-backed reverb block. Single source for
/// the editor knobs (see `project::block::dispatch`) and the processor
/// defaults below — both read the same keys/defaults, so they never drift.
///
/// These are processing controls layered on top of any capture-grid axes the
/// manifest declares (the IR file selectors); they are not capture selectors.
pub fn ir_reverb_parameter_specs() -> Vec<ParameterSpec> {
    vec![
        float_parameter(
            "mix",
            "Mix",
            None,
            Some(DEFAULT_MIX_PCT),
            0.0,
            100.0,
            1.0,
            ParameterUnit::Percent,
        ),
        float_parameter(
            "pre_delay_ms",
            "Pre-Delay",
            None,
            Some(DEFAULT_PRE_DELAY_MS),
            0.0,
            200.0,
            1.0,
            ParameterUnit::Milliseconds,
        ),
        float_parameter(
            "level",
            "Wet Level",
            None,
            Some(DEFAULT_LEVEL_DB),
            -24.0,
            24.0,
            0.1,
            ParameterUnit::Decibels,
        ),
    ]
}

/// Build an IR-convolution reverb processor from a `type: reverb` +
/// `backend: ir` package: the gain-passive wet convolution (reused from
/// `crates/ir`) wrapped in a dry/wet mixer with a pre-delay on the wet path
/// and a wet level trim.
pub fn build_ir_reverb_from_package(
    package: &LoadedPackage,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let (wet, _audit_db) =
        ir::from_package::build_convolution_from_package(package, params, sample_rate, layout)?;

    let mix = params.get_f32("mix").unwrap_or(DEFAULT_MIX_PCT) / 100.0;
    let level_db = params.get_f32("level").unwrap_or(DEFAULT_LEVEL_DB);
    let wet_gain = 10.0_f32.powf(level_db / 20.0);
    let pre_delay_ms = params
        .get_f32("pre_delay_ms")
        .unwrap_or(DEFAULT_PRE_DELAY_MS)
        .max(0.0);
    let pre_delay_frames = (pre_delay_ms / 1000.0 * sample_rate).round() as usize;

    Ok(match wet {
        BlockProcessor::Stereo(wet) => BlockProcessor::Stereo(Box::new(DryWetStereo::new(
            wet,
            mix,
            wet_gain,
            PreDelayStereo::new(pre_delay_frames),
        ))),
        BlockProcessor::Mono(wet) => BlockProcessor::Mono(Box::new(DryWetMono::new(
            wet,
            mix,
            wet_gain,
            PreDelayMono::new(pre_delay_frames),
        ))),
    })
}

/// Fixed-length pre-delay ring buffer on the stereo wet path. A length of 0
/// is a pass-through (no allocation walk, returns the input frame).
pub(crate) struct PreDelayStereo {
    buffer: Vec<[f32; 2]>,
    index: usize,
}

impl PreDelayStereo {
    pub(crate) fn new(frames: usize) -> Self {
        Self {
            buffer: vec![[0.0; 2]; frames],
            index: 0,
        }
    }

    fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        if self.buffer.is_empty() {
            return input;
        }
        let out = self.buffer[self.index];
        self.buffer[self.index] = input;
        self.index = (self.index + 1) % self.buffer.len();
        out
    }
}

/// Blends a dry stereo signal with a wet convolution tail. `mix` is the wet
/// fraction in `0.0..=1.0`; `wet_gain` is a linear trim on the wet path; the
/// wet input is fed through `pre_delay` first.
pub(crate) struct DryWetStereo {
    wet: Box<dyn StereoProcessor>,
    mix: f32,
    wet_gain: f32,
    pre_delay: PreDelayStereo,
}

impl DryWetStereo {
    pub(crate) fn new(
        wet: Box<dyn StereoProcessor>,
        mix: f32,
        wet_gain: f32,
        pre_delay: PreDelayStereo,
    ) -> Self {
        Self {
            wet,
            mix: mix.clamp(0.0, 1.0),
            wet_gain,
            pre_delay,
        }
    }
}

impl StereoProcessor for DryWetStereo {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let delayed = self.pre_delay.process(input);
        let wet = self.wet.process_frame(delayed);
        let dry = 1.0 - self.mix;
        let wet_l = self.mix * self.wet_gain * wet[0];
        let wet_r = self.mix * self.wet_gain * wet[1];
        [dry.mul_add(input[0], wet_l), dry.mul_add(input[1], wet_r)]
    }
}

/// Fixed-length pre-delay ring buffer on the mono wet path. Length 0 is a
/// pass-through.
pub(crate) struct PreDelayMono {
    buffer: Vec<f32>,
    index: usize,
}

impl PreDelayMono {
    pub(crate) fn new(frames: usize) -> Self {
        Self {
            buffer: vec![0.0; frames],
            index: 0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        if self.buffer.is_empty() {
            return input;
        }
        let out = self.buffer[self.index];
        self.buffer[self.index] = input;
        self.index = (self.index + 1) % self.buffer.len();
        out
    }
}

/// Mono counterpart of [`DryWetStereo`].
pub(crate) struct DryWetMono {
    wet: Box<dyn MonoProcessor>,
    mix: f32,
    wet_gain: f32,
    pre_delay: PreDelayMono,
}

impl DryWetMono {
    pub(crate) fn new(
        wet: Box<dyn MonoProcessor>,
        mix: f32,
        wet_gain: f32,
        pre_delay: PreDelayMono,
    ) -> Self {
        Self {
            wet,
            mix: mix.clamp(0.0, 1.0),
            wet_gain,
            pre_delay,
        }
    }
}

impl MonoProcessor for DryWetMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        let delayed = self.pre_delay.process(input);
        let wet = self.wet.process_sample(delayed);
        let dry = 1.0 - self.mix;
        dry.mul_add(input, self.mix * self.wet_gain * wet)
    }
}

#[cfg(test)]
#[path = "ir_reverb_tests.rs"]
mod tests;
