use crate::registry::DelayModelDefinition;
use crate::DelayBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "lv2_mda_dubdelay";
pub const DISPLAY_NAME: &str = "MDA DubDelay";
const BRAND: &str = "mda";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/mda/DubDelay";
const PLUGIN_DIR: &str = "mod-mda-DubDelay";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "DubDelay.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "DubDelay.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "DubDelay.dll";

// LV2 port indices (from TTL) — stereo in/out
const PORT_DELAY: usize = 0;
const PORT_FEEDBACK: usize = 1;
const PORT_FB_TONE: usize = 2;
const PORT_LFO_DEPTH: usize = 3;
const PORT_LFO_RATE: usize = 4;
const PORT_MIX: usize = 5;
const PORT_OUTPUT: usize = 6;
const PORT_AUDIO_IN_L: usize = 7;
const PORT_AUDIO_IN_R: usize = 8;
const PORT_AUDIO_OUT_L: usize = 9;
const PORT_AUDIO_OUT_R: usize = 10;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_DELAY.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("delay", "Delay", None, Some(460.0), 1.0, 7500.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("feedback", "Feedback", None, Some(55.0), -110.0, 110.0, 1.0, ParameterUnit::Percent),
            float_parameter("fb_tone", "FB Tone", None, Some(0.0), -100.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("lfo_depth", "LFO Depth", None, Some(30.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("lfo_rate", "LFO Rate", None, Some(0.05), 0.01, 10.0, 0.01, ParameterUnit::Hertz),
            float_parameter("mix", "Mix", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("output", "Output", None, Some(0.0), -12.0, 6.0, 0.5, ParameterUnit::Decibels),
        ],
    }
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let delay = required_f32(params, "delay").map_err(anyhow::Error::msg)?;
    let feedback = required_f32(params, "feedback").map_err(anyhow::Error::msg)?;
    let fb_tone = required_f32(params, "fb_tone").map_err(anyhow::Error::msg)?;
    let lfo_depth = required_f32(params, "lfo_depth").map_err(anyhow::Error::msg)?;
    let lfo_rate = required_f32(params, "lfo_rate").map_err(anyhow::Error::msg)?;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)?;
    let output = required_f32(params, "output").map_err(anyhow::Error::msg)?;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let control_ports = &[
        (PORT_DELAY, delay), (PORT_FEEDBACK, feedback), (PORT_FB_TONE, fb_tone),
        (PORT_LFO_DEPTH, lfo_depth), (PORT_LFO_RATE, lfo_rate), (PORT_MIX, mix),
        (PORT_OUTPUT, output),
    ];

    let processor = lv2::build_stereo_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R], &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R],
        control_ports,
    )?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(StereoAsMono(processor)))),
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(processor))),
    }
}

struct StereoAsMono(lv2::StereoLv2Processor);
impl MonoProcessor for StereoAsMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        let [l, r] = block_core::StereoProcessor::process_frame(&mut self.0, [input, input]);
        (l + r) * 0.5
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: DelayModelDefinition = DelayModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: DelayBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
