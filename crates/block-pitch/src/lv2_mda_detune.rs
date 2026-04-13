use crate::registry::PitchModelDefinition;
use crate::PitchBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_mda_detune";
pub const DISPLAY_NAME: &str = "MDA Detune";
const BRAND: &str = "mda";

const PLUGIN_URI: &str = "http://drobilla.net/plugins/mda/Detune";
const PLUGIN_DIR: &str = "mod-mda-Detune.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "Detune.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "Detune.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "Detune.dll";

// LV2 port indices (from Detune.ttl)
const PORT_DETUNE: usize = 0;
const PORT_MIX: usize = 1;
const PORT_LEVEL: usize = 2;
const PORT_LATENCY: usize = 3;
const PORT_LEFT_IN: usize = 4;
const PORT_RIGHT_IN: usize = 5;
const PORT_LEFT_OUT: usize = 6;
const PORT_RIGHT_OUT: usize = 7;

fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "pitch".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter(
                "detune",
                "Detune",
                Some("Pitch"),
                Some(2.4),
                0.0,
                300.0,
                0.1,
                ParameterUnit::None,
            ),
            float_parameter(
                "mix",
                "Mix",
                Some("Control"),
                Some(50.0),
                0.0,
                99.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "level",
                "Level",
                Some("Control"),
                Some(0.0),
                -20.0,
                20.0,
                0.1,
                ParameterUnit::None,
            ),
        ],
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
    let detune = required_f32(params, "detune").map_err(anyhow::Error::msg)?;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)?;
    let level = required_f32(params, "level").map_err(anyhow::Error::msg)?;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    // MDA lvz wrapper expects all params normalized 0-1
    let control_ports = &[
        (PORT_DETUNE, detune / 300.0),
        (PORT_MIX, mix / 99.0),
        (PORT_LEVEL, (level + 20.0) / 40.0),
        (PORT_LATENCY, 0.0),
    ];

    match layout {
        AudioChannelLayout::Mono => {
            let processor = lv2::build_lv2_processor(
                &lib_path,
                PLUGIN_URI,
                sample_rate as f64,
                &bundle_path,
                &[PORT_LEFT_IN],
                &[PORT_LEFT_OUT],
                control_ports,
            )?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let processor = lv2::build_stereo_lv2_processor(
                &lib_path,
                PLUGIN_URI,
                sample_rate as f64,
                &bundle_path,
                &[PORT_LEFT_IN, PORT_RIGHT_IN],
                &[PORT_LEFT_OUT, PORT_RIGHT_OUT],
                control_ports,
            )?;
            Ok(BlockProcessor::Stereo(Box::new(processor)))
        }
    }
}

pub const MODEL_DEFINITION: PitchModelDefinition = PitchModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: PitchBackendKind::Lv2,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
