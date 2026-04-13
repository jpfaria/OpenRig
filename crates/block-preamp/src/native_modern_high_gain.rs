use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::native_core::{
    self, NativeAmpHeadProfile, NativeAmpHeadSchemaDefaults,
};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;

pub const MODEL_ID: &str = "modern_high_gain";
pub const DISPLAY_NAME: &str = "Modern High Gain";

const PROFILE: NativeAmpHeadProfile = NativeAmpHeadProfile {
    input_trim_db: -1.0,
    drive_scale: 4.1,
    asymmetry: 0.18,
    bright_mix: 0.08,
    low_voice: 0.82,
    mid_voice: 0.92,
    high_voice: 1.02,
    presence_voice: 0.62,
    depth_voice: 0.58,
    power_drive: 1.55,
    low_cut_hz: 72.0,
    top_end_hz: 7_600.0,
};

const DEFAULTS: NativeAmpHeadSchemaDefaults = NativeAmpHeadSchemaDefaults {
    gain: 72.0,
    presence: 62.0,
    depth: 60.0,
    bright: false,
    sag: 30.0,
};

fn schema() -> Result<ModelParameterSchema> {
    Ok(native_core::model_schema(MODEL_ID, DISPLAY_NAME, DEFAULTS))
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    native_core::build_processor_for_profile(PROFILE, params, sample_rate, layout)
}

fn asset_summary(params: &ParameterSet) -> Result<String> {
    native_core::asset_summary(MODEL_ID, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn default_params() -> block_core::param::ParameterSet {
        let schema = native_core::model_schema(MODEL_ID, DISPLAY_NAME, DEFAULTS);
        block_core::param::ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize")
    }

    #[test]
    fn process_frame_silence_output_is_finite() {
        let params = default_params();
        let mut proc = match build(&params, 44100.0, AudioChannelLayout::Mono).unwrap() {
            BlockProcessor::Mono(p) => p,
            _ => panic!("expected Mono"),
        };
        for i in 0..1024 {
            let out = proc.process_sample(0.0);
            assert!(out.is_finite(), "non-finite at sample {i}: {out}");
        }
    }

    #[test]
    fn process_frame_sine_output_is_finite() {
        let params = default_params();
        let mut proc = match build(&params, 44100.0, AudioChannelLayout::Mono).unwrap() {
            BlockProcessor::Mono(p) => p,
            _ => panic!("expected Mono"),
        };
        for i in 0..1024 {
            let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            let out = proc.process_sample(input);
            assert!(out.is_finite(), "non-finite at sample {i}: {out}");
        }
    }

    #[test]
    fn process_block_1024_frames_all_finite() {
        let params = default_params();
        let mut proc = match build(&params, 44100.0, AudioChannelLayout::Mono).unwrap() {
            BlockProcessor::Mono(p) => p,
            _ => panic!("expected Mono"),
        };
        let mut buf = vec![0.0_f32; 1024];
        for (i, sample) in buf.iter_mut().enumerate() {
            *sample = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        }
        proc.process_block(&mut buf);
        for (i, &s) in buf.iter().enumerate() {
            assert!(s.is_finite(), "non-finite at index {i}: {s}");
        }
    }
}

pub const MODEL_DEFINITION: PreampModelDefinition = PreampModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: PreampBackendKind::Native,
    schema,
    validate: native_core::validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[
        block_core::KnobLayoutEntry { param_key: "input",     svg_cx: 44.0,  svg_cy: 90.0, svg_r: 16.0, min: 0.0,   max: 100.0, step: 1.0 },
        block_core::KnobLayoutEntry { param_key: "gain",      svg_cx: 130.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0,   max: 100.0, step: 1.0 },
        block_core::KnobLayoutEntry { param_key: "bass",      svg_cx: 222.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0,   max: 100.0, step: 1.0 },
        block_core::KnobLayoutEntry { param_key: "middle",    svg_cx: 302.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0,   max: 100.0, step: 1.0 },
        block_core::KnobLayoutEntry { param_key: "treble",    svg_cx: 382.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0,   max: 100.0, step: 1.0 },
        block_core::KnobLayoutEntry { param_key: "presence",  svg_cx: 470.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0,   max: 100.0, step: 1.0 },
        block_core::KnobLayoutEntry { param_key: "depth",     svg_cx: 550.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0,   max: 100.0, step: 1.0 },
        block_core::KnobLayoutEntry { param_key: "sag",       svg_cx: 630.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0,   max: 100.0, step: 1.0 },
        block_core::KnobLayoutEntry { param_key: "master",    svg_cx: 706.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0,   max: 100.0, step: 1.0 },
    ],
};
