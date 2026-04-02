use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use anyhow::Result;
use block_core::param::{float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_zamgeq31";
pub const DISPLAY_NAME: &str = "ZamGEQ31";
const BRAND: &str = "zam";

const PLUGIN_URI: &str = "urn:zamaudio:ZamGEQ31";
const PLUGIN_DIR: &str = "ZamGEQ31.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "ZamGEQ31_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "ZamGEQ31_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "ZamGEQ31_dsp.dll";

// LV2 port indices (from ZamGEQ31_dsp.ttl)
// 0: Audio In, 1: Audio Out
// 2: Master Gain (-30 to 30, default 0)
// 3-33: Band 1-31 gains (-12 to 12, default 0)
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;

const BAND_NAMES: [&str; 31] = [
    "32Hz", "40Hz", "50Hz", "63Hz", "79Hz", "100Hz", "126Hz", "158Hz", "200Hz", "251Hz",
    "316Hz", "398Hz", "501Hz", "631Hz", "794Hz", "1kHz", "1.3kHz", "1.6kHz", "2kHz", "2.5kHz",
    "3.2kHz", "4kHz", "5kHz", "6.3kHz", "8kHz", "10kHz", "12.7kHz", "16.1kHz", "20.8kHz",
    "Band30", "Band31",
];

fn schema() -> Result<ModelParameterSchema> {
    let mut parameters = vec![
        float_parameter("master", "Master Gain", None, Some(0.0), -30.0, 30.0, 0.1, ParameterUnit::Decibels),
    ];

    for i in 1..=31usize {
        parameters.push(float_parameter(
            &format!("band{i}"),
            BAND_NAMES[i - 1],
            None,
            Some(0.0),
            -12.0,
            12.0,
            0.1,
            ParameterUnit::Decibels,
        ));
    }

    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_FILTER.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters,
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

struct DualMonoLv2 {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono_processor(sample_rate: f32, master: f32, bands: &[f32; 31]) -> Result<lv2::Lv2Processor> {
    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;

    let mut control_ports: Vec<(usize, f32)> = vec![(2, master)];
    for i in 0..31 {
        control_ports.push((3 + i, bands[i]));
    }

    lv2::build_lv2_processor(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN],
        &[PORT_AUDIO_OUT],
        &control_ports,
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let master = required_f32(params, "master").map_err(anyhow::Error::msg)?;
    let mut bands = [0.0f32; 31];
    for i in 1..=31usize {
        bands[i - 1] = required_f32(params, &format!("band{i}")).map_err(anyhow::Error::msg)?;
    }

    match layout {
        AudioChannelLayout::Mono => {
            Ok(BlockProcessor::Mono(Box::new(build_mono_processor(sample_rate, master, &bands)?)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, master, &bands)?;
            let right = build_mono_processor(sample_rate, master, &bands)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
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
