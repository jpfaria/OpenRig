use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_caps_platex2";
pub const DISPLAY_NAME: &str = "CAPS Plate X2";
const BRAND: &str = "caps";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/caps/PlateX2";
const PLUGIN_DIR: &str = "mod-caps-PlateX2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "PlateX2.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "PlateX2.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "PlateX2.dll";

// LV2 port indices (from TTL) — stereo in, stereo out
const PORT_BANDWIDTH: usize = 0;
const PORT_TAIL: usize = 1;
const PORT_DAMPING: usize = 2;
const PORT_BLEND: usize = 3;
const PORT_AUDIO_IN_L: usize = 4;
const PORT_AUDIO_IN_R: usize = 5;
const PORT_AUDIO_OUT_L: usize = 6;
const PORT_AUDIO_OUT_R: usize = 7;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("bandwidth", "Bandwidth", None, Some(100.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("tail", "Tail", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("damping", "Damping", None, Some(0.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("blend", "Blend", None, Some(25.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let bandwidth = required_f32(params, "bandwidth").map_err(anyhow::Error::msg)? / 100.0;
    let tail = required_f32(params, "tail").map_err(anyhow::Error::msg)? / 100.0;
    let damping = required_f32(params, "damping").map_err(anyhow::Error::msg)? / 100.0;
    let blend = required_f32(params, "blend").map_err(anyhow::Error::msg)? / 100.0;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    let control_ports = &[(PORT_BANDWIDTH, bandwidth), (PORT_TAIL, tail), (PORT_DAMPING, damping), (PORT_BLEND, blend)];

    match layout {
        AudioChannelLayout::Mono => {
            let processor = lv2::build_lv2_processor_with_extras(
                &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
                &[PORT_AUDIO_IN_L], &[PORT_AUDIO_OUT_L], control_ports,
                &[PORT_AUDIO_IN_R, PORT_AUDIO_OUT_R],
            )?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let processor = lv2::build_stereo_lv2_processor(
                &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
                &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R], &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R], control_ports,
            )?;
            Ok(BlockProcessor::Stereo(Box::new(processor)))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: ReverbBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
