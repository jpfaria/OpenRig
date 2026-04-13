use crate::registry::PitchModelDefinition;
use crate::PitchBackendKind;
use anyhow::Result;
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string, ModelParameterSchema,
    ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_mda_repsycho";
pub const DISPLAY_NAME: &str = "MDA RePsycho!";
const BRAND: &str = "mda";

const PLUGIN_URI: &str = "http://drobilla.net/plugins/mda/RePsycho";
const PLUGIN_DIR: &str = "mod-mda-RePsycho.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "RePsycho.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "RePsycho.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "RePsycho.dll";

// LV2 port indices (from RePsycho.ttl)
const PORT_TUNE: usize = 0;
const PORT_FINE: usize = 1;
const PORT_DECAY: usize = 2;
const PORT_THRESH: usize = 3;
const PORT_HOLD: usize = 4;
const PORT_MIX: usize = 5;
const PORT_QUALITY: usize = 6;
const PORT_LEFT_IN: usize = 7;
const PORT_RIGHT_IN: usize = 8;
const PORT_LEFT_OUT: usize = 9;
const PORT_RIGHT_OUT: usize = 10;

fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "pitch".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter(
                "tune",
                "Tune",
                Some("Pitch"),
                Some(0.0),
                -24.0,
                0.0,
                1.0,
                ParameterUnit::None,
            ),
            float_parameter(
                "fine",
                "Fine",
                Some("Pitch"),
                Some(0.0),
                -100.0,
                0.0,
                1.0,
                ParameterUnit::None,
            ),
            float_parameter(
                "thresh",
                "Threshold",
                Some("Detection"),
                Some(-12.0),
                -30.0,
                0.0,
                0.5,
                ParameterUnit::None,
            ),
            float_parameter(
                "hold",
                "Hold",
                Some("Detection"),
                Some(122.5),
                10.0,
                260.0,
                1.0,
                ParameterUnit::None,
            ),
            float_parameter(
                "mix",
                "Mix",
                Some("Control"),
                Some(100.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            enum_parameter(
                "quality",
                "Quality",
                Some("Control"),
                Some("high"),
                &[("low", "Low"), ("high", "High")],
            ),
        ],
    }
}

fn quality_to_float(s: &str) -> f32 {
    match s {
        "low" => 0.0,
        "high" => 1.0,
        _ => 1.0,
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
    let tune = required_f32(params, "tune").map_err(anyhow::Error::msg)?;
    let fine = required_f32(params, "fine").map_err(anyhow::Error::msg)?;
    let thresh = required_f32(params, "thresh").map_err(anyhow::Error::msg)?;
    let hold = required_f32(params, "hold").map_err(anyhow::Error::msg)?;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)?;
    let quality_str = required_string(params, "quality").map_err(anyhow::Error::msg)?;
    let quality = quality_to_float(&quality_str);

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    // MDA lvz wrapper expects all params normalized 0-1
    let control_ports = &[
        (PORT_TUNE, (tune + 24.0) / 24.0),
        (PORT_FINE, (fine + 100.0) / 100.0),
        (PORT_DECAY, 0.0),
        (PORT_THRESH, (thresh + 30.0) / 30.0),
        (PORT_HOLD, (hold - 10.0) / 250.0),
        (PORT_MIX, mix / 100.0),
        (PORT_QUALITY, quality),
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
