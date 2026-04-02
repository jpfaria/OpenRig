use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use anyhow::Result;
use block_core::param::{float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_artyfx_filta";
pub const DISPLAY_NAME: &str = "Filta";
const BRAND: &str = "openav";

const PLUGIN_URI: &str = "http://www.openavproductions.com/artyfx#filta";
const PLUGIN_DIR: &str = "artyfx.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "artyfx.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "artyfx.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "artyfx.dll";

// LV2 port indices (from filta.ttl)
// 0: in_left, 1: in_right (audio in)
// 2: out_left, 3: out_right (audio out)
// 4: frequency_control (0-1, default 0.5)
// 5: active (0-1, default 1)
const PORT_AUDIO_IN_L: usize = 0;
const PORT_AUDIO_IN_R: usize = 1;
const PORT_AUDIO_OUT_L: usize = 2;
const PORT_AUDIO_OUT_R: usize = 3;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_FILTER.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter("frequency", "Frequency", None, Some(0.5), 0.0, 1.0, 0.01, ParameterUnit::None),
        ],
    })
}

fn resolve_lib_path() -> Result<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let candidates = [
        exe_dir.as_ref().map(|d| d.join("../../").join(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
        Some(std::path::PathBuf::from(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }

    anyhow::bail!("LV2 binary '{}' not found in '{}'", PLUGIN_BINARY, lv2::default_lv2_lib_dir())
}

fn resolve_bundle_path() -> Result<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let candidates = [
        exe_dir.as_ref().map(|d| d.join("../../plugins").join(PLUGIN_DIR)),
        Some(std::path::PathBuf::from("plugins").join(PLUGIN_DIR)),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }

    anyhow::bail!("LV2 bundle '{}' not found in plugins/", PLUGIN_DIR)
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let frequency = required_f32(params, "frequency").map_err(anyhow::Error::msg)?;

    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;

    match layout {
        AudioChannelLayout::Mono => {
            // Connect both L and R input ports to the same buffer; discard right output.
            let processor = lv2::build_lv2_processor_with_extras(
                &lib_path,
                PLUGIN_URI,
                sample_rate as f64,
                &bundle_path,
                &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R],
                &[PORT_AUDIO_OUT_L],
                &[
                    (4, frequency),
                    (5, 1.0), // active
                ],
                &[PORT_AUDIO_OUT_R],
            )?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let processor = lv2::build_stereo_lv2_processor(
                &lib_path,
                PLUGIN_URI,
                sample_rate as f64,
                &bundle_path,
                &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R],
                &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R],
                &[
                    (4, frequency),
                    (5, 1.0), // active
                ],
            )?;
            Ok(BlockProcessor::Stereo(Box::new(processor)))
        }
    }
}

pub const MODEL_DEFINITION: FilterModelDefinition = FilterModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: FilterBackendKind::Lv2,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
