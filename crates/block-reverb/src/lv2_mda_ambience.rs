use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_mda_ambience";
pub const DISPLAY_NAME: &str = "MDA Ambience";
const BRAND: &str = "mda";

const PLUGIN_URI: &str = "http://drobilla.net/plugins/mda/Ambience";
const PLUGIN_DIR: &str = "mod-mda-Ambience.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "Ambience.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "Ambience.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "Ambience.dll";

// LV2 port indices (from TTL)
const PORT_SIZE: usize = 0;
const PORT_HF_DAMP: usize = 1;
const PORT_MIX: usize = 2;
const PORT_OUTPUT: usize = 3;
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
            float_parameter("size", "Room Size", None, Some(7.0), 0.0, 10.0, 0.1, ParameterUnit::None),
            float_parameter("hf_damp", "Damping", None, Some(70.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mix", "Mix", None, Some(90.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("output", "Level", None, Some(0.0), -20.0, 20.0, 0.5, ParameterUnit::Decibels),
        ],
    }
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let size = required_f32(params, "size").map_err(anyhow::Error::msg)?;
    let hf_damp = required_f32(params, "hf_damp").map_err(anyhow::Error::msg)?;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)?;
    let output = required_f32(params, "output").map_err(anyhow::Error::msg)?;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let control_ports = &[
        (PORT_SIZE, size),
        (PORT_HF_DAMP, hf_damp),
        (PORT_MIX, mix),
        (PORT_OUTPUT, output),
    ];

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
                &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R], &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R],
                control_ports,
            )?;
            Ok(BlockProcessor::Stereo(Box::new(processor)))
        }
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
