use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_filta";
pub const DISPLAY_NAME: &str = "Filta";
const BRAND: &str = "artyfx";
const PLUGIN_URI: &str = "http://www.openavproductions.com/artyfx#filta";
const PLUGIN_DIR: &str = "artyfx-filta";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "artyfx.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "artyfx.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "artyfx.dll";

// Filta: stereo in/out
const PORT_AUDIO_IN_L: usize = 0;
const PORT_AUDIO_IN_R: usize = 1;
const PORT_AUDIO_OUT_L: usize = 2;
const PORT_AUDIO_OUT_R: usize = 3;
const PORT_FREQUENCY: usize = 4;
const PORT_ACTIVE: usize = 5;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_FILTER.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("frequency", "Frequency", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let frequency = required_f32(params, "frequency").map_err(anyhow::Error::msg)? / 100.0;
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    let control_ports = &[(PORT_FREQUENCY, frequency), (PORT_ACTIVE, 1.0)];

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

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: FilterModelDefinition = FilterModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: FilterBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
