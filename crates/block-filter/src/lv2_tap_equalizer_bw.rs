use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use anyhow::Result;
use block_core::param::{float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_tap_equalizer_bw";
pub const DISPLAY_NAME: &str = "TAP Equalizer/BW";
const BRAND: &str = "tap";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/eqbw";
const PLUGIN_DIR: &str = "tap-eqbw.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_eqbw.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_eqbw.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_eqbw.dll";

// LV2 port indices (from tap_eqbw.ttl)
// Ports 0-7:   Band1-8 Gain [dB]   (-50 to 20, default 0)
// Ports 8-15:  Band1-8 Freq [Hz]
// Ports 16-23: Band1-8 Bandwidth [octaves] (0.1 to 5, default 1)
// Port 24: Audio Input
// Port 25: Audio Output
const PORT_AUDIO_IN: usize = 24;
const PORT_AUDIO_OUT: usize = 25;

const BAND_FREQ_DEFAULTS: [f32; 8] = [100.0, 200.0, 400.0, 1000.0, 3000.0, 6000.0, 12000.0, 15000.0];
const BAND_FREQ_MINS: [f32; 8] = [40.0, 100.0, 200.0, 400.0, 1000.0, 3000.0, 6000.0, 10000.0];
const BAND_FREQ_MAXS: [f32; 8] = [280.0, 500.0, 1000.0, 2800.0, 5000.0, 9000.0, 18000.0, 20000.0];

fn schema() -> Result<ModelParameterSchema> {
    let mut parameters = Vec::new();

    for i in 1..=8usize {
        parameters.push(float_parameter(
            &format!("band{i}_gain"),
            &format!("Band {i} Gain"),
            None,
            Some(0.0),
            -50.0,
            20.0,
            0.1,
            ParameterUnit::Decibels,
        ));
    }
    for i in 1..=8usize {
        parameters.push(float_parameter(
            &format!("band{i}_freq"),
            &format!("Band {i} Freq"),
            None,
            Some(BAND_FREQ_DEFAULTS[i - 1]),
            BAND_FREQ_MINS[i - 1],
            BAND_FREQ_MAXS[i - 1],
            1.0,
            ParameterUnit::Hertz,
        ));
    }
    for i in 1..=8usize {
        parameters.push(float_parameter(
            &format!("band{i}_bw"),
            &format!("Band {i} BW"),
            None,
            Some(1.0),
            0.1,
            5.0,
            0.01,
            ParameterUnit::None,
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

fn build_mono_processor(
    sample_rate: f32,
    gains: &[f32; 8],
    freqs: &[f32; 8],
    bws: &[f32; 8],
) -> Result<lv2::Lv2Processor> {
    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;

    let control_ports: Vec<(usize, f32)> = (0..8)
        .map(|i| (i, gains[i]))
        .chain((8..16).map(|i| (i, freqs[i - 8])))
        .chain((16..24).map(|i| (i, bws[i - 16])))
        .collect();

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
    let mut gains = [0.0f32; 8];
    let mut freqs = [0.0f32; 8];
    let mut bws = [1.0f32; 8];
    for i in 1..=8usize {
        gains[i - 1] = required_f32(params, &format!("band{i}_gain")).map_err(anyhow::Error::msg)?;
        freqs[i - 1] = required_f32(params, &format!("band{i}_freq")).map_err(anyhow::Error::msg)?;
        bws[i - 1] = required_f32(params, &format!("band{i}_bw")).map_err(anyhow::Error::msg)?;
    }

    match layout {
        AudioChannelLayout::Mono => {
            Ok(BlockProcessor::Mono(Box::new(build_mono_processor(sample_rate, &gains, &freqs, &bws)?)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, &gains, &freqs, &bws)?;
            let right = build_mono_processor(sample_rate, &gains, &freqs, &bws)?;
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
