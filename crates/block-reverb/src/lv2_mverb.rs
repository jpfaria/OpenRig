use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "lv2_mverb";
pub const DISPLAY_NAME: &str = "MVerb";
const BRAND: &str = "distrho";

const PLUGIN_URI: &str = "http://distrho.sf.net/plugins/MVerb";
const PLUGIN_DIR: &str = "MVerb";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "MVerb_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "MVerb_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "MVerb_dsp.dll";

// LV2 port indices (from TTL)
const PORT_AUDIO_IN_L: usize = 0;
const PORT_AUDIO_IN_R: usize = 1;
const PORT_AUDIO_OUT_L: usize = 2;
const PORT_AUDIO_OUT_R: usize = 3;
const PORT_DAMPING: usize = 4;
const PORT_DENSITY: usize = 5;
const PORT_BANDWIDTH: usize = 6;
const PORT_DECAY: usize = 7;
const PORT_PREDELAY: usize = 8;
const PORT_SIZE: usize = 9;
const PORT_GAIN: usize = 10;
const PORT_MIX: usize = 11;
const PORT_EARLY_MIX: usize = 12;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("damping", "Damping", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("density", "Density", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("bandwidth", "Bandwidth", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("decay", "Decay", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("predelay", "Predelay", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("size", "Size", None, Some(75.0), 5.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("gain", "Gain", None, Some(100.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mix", "Mix", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("early_mix", "Early/Late Mix", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let damping = required_f32(params, "damping").map_err(anyhow::Error::msg)? / 100.0;
    let density = required_f32(params, "density").map_err(anyhow::Error::msg)? / 100.0;
    let bandwidth = required_f32(params, "bandwidth").map_err(anyhow::Error::msg)? / 100.0;
    let decay = required_f32(params, "decay").map_err(anyhow::Error::msg)? / 100.0;
    let predelay = required_f32(params, "predelay").map_err(anyhow::Error::msg)? / 100.0;
    let size = required_f32(params, "size").map_err(anyhow::Error::msg)? / 100.0;
    let gain = required_f32(params, "gain").map_err(anyhow::Error::msg)? / 100.0;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)? / 100.0;
    let early_mix = required_f32(params, "early_mix").map_err(anyhow::Error::msg)? / 100.0;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let control_ports = &[
        (PORT_DAMPING, damping),
        (PORT_DENSITY, density),
        (PORT_BANDWIDTH, bandwidth),
        (PORT_DECAY, decay),
        (PORT_PREDELAY, predelay),
        (PORT_SIZE, size),
        (PORT_GAIN, gain),
        (PORT_MIX, mix),
        (PORT_EARLY_MIX, early_mix),
    ];

    // Always single stereo instance — DPF reverb needs stereo processing
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
        let [l, _] = block_core::StereoProcessor::process_frame(&mut self.0, [input, input]);
        l
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: ReverbBackendKind::Lv2,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
