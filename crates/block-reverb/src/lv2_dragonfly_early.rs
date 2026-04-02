use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;
use anyhow::Result;
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string,
    ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_dragonfly_early";
pub const DISPLAY_NAME: &str = "Dragonfly Early Reflections";
const BRAND: &str = "dragonfly";

const PLUGIN_URI: &str = "urn:dragonfly:early";
const PLUGIN_DIR: &str = "DragonflyEarlyReflections.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "DragonflyEarlyReflections_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "DragonflyEarlyReflections_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "DragonflyEarlyReflections_dsp.dll";

// LV2 port indices (from TTL)
const PORT_AUDIO_IN_L: usize = 0;
const PORT_AUDIO_IN_R: usize = 1;
const PORT_AUDIO_OUT_L: usize = 2;
const PORT_AUDIO_OUT_R: usize = 3;
const PORT_DRY_LEVEL: usize = 4;
const PORT_WET_LEVEL: usize = 5;
const PORT_PROGRAM: usize = 6;
const PORT_SIZE: usize = 7;
const PORT_WIDTH: usize = 8;
const PORT_LOW_CUT: usize = 9;
const PORT_HIGH_CUT: usize = 10;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("dry_level", "Dry Level", None, Some(80.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("wet_level", "Wet Level", None, Some(20.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            enum_parameter("program", "Program", None, Some("concert_hall_b"), &[
                ("concert_hall_a", "Concert Hall A"),
                ("concert_hall_b", "Concert Hall B"),
                ("concert_hall_c", "Concert Hall C"),
                ("small_room", "Small Room"),
                ("medium_room", "Medium Room"),
                ("large_room", "Large Room"),
                ("church", "Church"),
                ("cathedral", "Cathedral"),
            ]),
            float_parameter("size", "Size", None, Some(20.0), 10.0, 60.0, 1.0, ParameterUnit::None),
            float_parameter("width", "Width", None, Some(100.0), 50.0, 150.0, 1.0, ParameterUnit::Percent),
            float_parameter("low_cut", "Low Cut", None, Some(50.0), 0.0, 200.0, 1.0, ParameterUnit::Hertz),
            float_parameter("high_cut", "High Cut", None, Some(10000.0), 1000.0, 16000.0, 100.0, ParameterUnit::Hertz),
        ],
    }
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let dry_level = required_f32(params, "dry_level").map_err(anyhow::Error::msg)?;
    let wet_level = required_f32(params, "wet_level").map_err(anyhow::Error::msg)?;
    let program_str = required_string(params, "program").map_err(anyhow::Error::msg)?;
    let program: f32 = match program_str.as_str() {
        "concert_hall_a" => 0.0,
        "concert_hall_b" => 1.0,
        "concert_hall_c" => 2.0,
        "small_room" => 3.0,
        "medium_room" => 4.0,
        "large_room" => 5.0,
        "church" => 6.0,
        "cathedral" => 7.0,
        _ => 2.0,
    };
    let size = required_f32(params, "size").map_err(anyhow::Error::msg)?;
    let width = required_f32(params, "width").map_err(anyhow::Error::msg)?;
    let low_cut = required_f32(params, "low_cut").map_err(anyhow::Error::msg)?;
    let high_cut = required_f32(params, "high_cut").map_err(anyhow::Error::msg)?;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let control_ports = &[
        (PORT_DRY_LEVEL, dry_level),
        (PORT_WET_LEVEL, wet_level),
        (PORT_PROGRAM, program),
        (PORT_SIZE, size),
        (PORT_WIDTH, width),
        (PORT_LOW_CUT, low_cut),
        (PORT_HIGH_CUT, high_cut),
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
