use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "lv2_dragonfly_hall";
pub const DISPLAY_NAME: &str = "Dragonfly Hall Reverb";
const BRAND: &str = "dragonfly";

const PLUGIN_URI: &str = "https://github.com/michaelwillis/dragonfly-reverb";
const PLUGIN_DIR: &str = "DragonflyHallReverb";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "DragonflyHallReverb_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "DragonflyHallReverb_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "DragonflyHallReverb_dsp.dll";

// LV2 port indices (from TTL)
const PORT_AUDIO_IN_L: usize = 0;
const PORT_AUDIO_IN_R: usize = 1;
const PORT_AUDIO_OUT_L: usize = 2;
const PORT_AUDIO_OUT_R: usize = 3;
const PORT_ATOM_IN: usize = 4;
const PORT_ATOM_OUT: usize = 5;
const PORT_DRY_LEVEL: usize = 6;
const PORT_EARLY_LEVEL: usize = 7;
const PORT_LATE_LEVEL: usize = 8;
const PORT_SIZE: usize = 9;
const PORT_WIDTH: usize = 10;
const PORT_PREDELAY: usize = 11;
const PORT_DIFFUSE: usize = 12;
const PORT_LOW_CUT: usize = 13;
const PORT_LOW_XO: usize = 14;
const PORT_LOW_MULT: usize = 15;
const PORT_HIGH_CUT: usize = 16;
const PORT_HIGH_XO: usize = 17;
const PORT_HIGH_MULT: usize = 18;
const PORT_SPIN: usize = 19;
const PORT_WANDER: usize = 20;
const PORT_DECAY: usize = 21;
const PORT_EARLY_SEND: usize = 22;
const PORT_MODULATION: usize = 23;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("dry_level", "Dry Level", None, Some(80.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("early_level", "Early Level", None, Some(10.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("late_level", "Late Level", None, Some(20.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("size", "Size", None, Some(24.0), 10.0, 60.0, 1.0, ParameterUnit::None),
            float_parameter("width", "Width", None, Some(100.0), 50.0, 150.0, 1.0, ParameterUnit::Percent),
            float_parameter("predelay", "Predelay", None, Some(4.0), 0.0, 100.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("diffuse", "Diffuse", None, Some(90.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("low_cut", "Low Cut", None, Some(4.0), 0.0, 200.0, 1.0, ParameterUnit::Hertz),
            float_parameter("high_cut", "High Cut", None, Some(7600.0), 1000.0, 16000.0, 100.0, ParameterUnit::Hertz),
            float_parameter("decay", "Decay", None, Some(1.3), 0.1, 10.0, 0.1, ParameterUnit::None),
            float_parameter("modulation", "Modulation", None, Some(15.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let dry_level = required_f32(params, "dry_level").map_err(anyhow::Error::msg)?;
    let early_level = required_f32(params, "early_level").map_err(anyhow::Error::msg)?;
    let late_level = required_f32(params, "late_level").map_err(anyhow::Error::msg)?;
    let size = required_f32(params, "size").map_err(anyhow::Error::msg)?;
    let width = required_f32(params, "width").map_err(anyhow::Error::msg)?;
    let predelay = required_f32(params, "predelay").map_err(anyhow::Error::msg)?;
    let diffuse = required_f32(params, "diffuse").map_err(anyhow::Error::msg)?;
    let low_cut = required_f32(params, "low_cut").map_err(anyhow::Error::msg)?;
    let high_cut = required_f32(params, "high_cut").map_err(anyhow::Error::msg)?;
    let decay = required_f32(params, "decay").map_err(anyhow::Error::msg)?;
    let modulation = required_f32(params, "modulation").map_err(anyhow::Error::msg)?;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let control_ports = &[
        (PORT_DRY_LEVEL, dry_level),
        (PORT_EARLY_LEVEL, early_level),
        (PORT_LATE_LEVEL, late_level),
        (PORT_SIZE, size),
        (PORT_WIDTH, width),
        (PORT_PREDELAY, predelay),
        (PORT_DIFFUSE, diffuse),
        (PORT_LOW_CUT, low_cut),
        (PORT_LOW_XO, 500.0),
        (PORT_LOW_MULT, 1.3),
        (PORT_HIGH_CUT, high_cut),
        (PORT_HIGH_XO, 5500.0),
        (PORT_HIGH_MULT, 0.5),
        (PORT_SPIN, 3.3),
        (PORT_WANDER, 15.0),
        (PORT_DECAY, decay),
        (PORT_EARLY_SEND, 20.0),
        (PORT_MODULATION, modulation),
    ];

    // Always create a single stereo instance — DPF worker crashes with dual instances.
    let processor = lv2::build_stereo_lv2_processor_with_atoms(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R], &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R],
        control_ports, &[PORT_ATOM_IN, PORT_ATOM_OUT],
    )?;
    match layout {
        AudioChannelLayout::Mono => {
            Ok(BlockProcessor::Mono(Box::new(StereoAsMono(processor))))
        }
        AudioChannelLayout::Stereo => {
            Ok(BlockProcessor::Stereo(Box::new(processor)))
        }
    }
}

/// Wraps a stereo LV2 processor as mono: feeds mono to both inputs, takes left output.
struct StereoAsMono(lv2::StereoLv2Processor);

impl MonoProcessor for StereoAsMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        let [left, _] = block_core::StereoProcessor::process_frame(&mut self.0, [input, input]);
        left
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
