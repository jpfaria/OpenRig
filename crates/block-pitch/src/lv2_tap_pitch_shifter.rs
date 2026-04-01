use crate::registry::PitchModelDefinition;
use crate::PitchBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_tap_pitch_shifter";
pub const DISPLAY_NAME: &str = "TAP Pitch Shifter";
const BRAND: &str = "tap";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/pitch";
const PLUGIN_DIR: &str = "tap-pitch.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_pitch.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_pitch.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_pitch.dll";

// LV2 port indices (from tap_pitch.ttl)
const PORT_SEMITONE: usize = 0;
const PORT_RATE: usize = 1;
const PORT_DRY_LEVEL: usize = 2;
const PORT_WET_LEVEL: usize = 3;
const PORT_LATENCY: usize = 4;
const PORT_AUDIO_IN: usize = 5;
const PORT_AUDIO_OUT: usize = 6;

fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "pitch".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "semitone",
                "Semitone",
                Some("Pitch"),
                Some(0.0),
                -12.0,
                12.0,
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
        ],
    }
}

fn resolve_lib_path() -> Result<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let candidates = [
        exe_dir
            .as_ref()
            .map(|d| d.join("../../").join(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
        Some(std::path::PathBuf::from(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }

    anyhow::bail!(
        "LV2 binary '{}' not found in '{}'",
        PLUGIN_BINARY,
        lv2::default_lv2_lib_dir()
    )
}

fn resolve_bundle_path() -> Result<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let candidates = [
        exe_dir
            .as_ref()
            .map(|d| d.join("../../plugins").join(PLUGIN_DIR)),
        Some(std::path::PathBuf::from("plugins").join(PLUGIN_DIR)),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }

    anyhow::bail!("LV2 bundle '{}' not found in plugins/", PLUGIN_DIR)
}

const MAX_BLOCK: usize = 4096;

struct DualMonoLv2 {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
    left_buf: Box<[f32; MAX_BLOCK]>,
    right_buf: Box<[f32; MAX_BLOCK]>,
}

impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [
            self.left.process_sample(input[0]),
            self.right.process_sample(input[1]),
        ]
    }

    fn process_block(&mut self, buffer: &mut [[f32; 2]]) {
        let len = buffer.len().min(MAX_BLOCK);
        for (i, frame) in buffer[..len].iter().enumerate() {
            self.left_buf[i] = frame[0];
            self.right_buf[i] = frame[1];
        }
        self.left.process_block(&mut self.left_buf[..len]);
        self.right.process_block(&mut self.right_buf[..len]);
        for (i, frame) in buffer[..len].iter_mut().enumerate() {
            frame[0] = self.left_buf[i];
            frame[1] = self.right_buf[i];
        }
    }
}

fn build_mono_processor(
    sample_rate: f32,
    semitone: f32,
    dry_level: f32,
    wet_level: f32,
) -> Result<lv2::Lv2Processor> {
    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;

    lv2::build_lv2_processor(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN],
        &[PORT_AUDIO_OUT],
        &[
            (PORT_SEMITONE, semitone),
            (PORT_RATE, 0.0),
            (PORT_DRY_LEVEL, dry_level),
            (PORT_WET_LEVEL, wet_level),
            (PORT_LATENCY, 0.0),
        ],
    )
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let semitone = required_f32(params, "semitone").map_err(anyhow::Error::msg)?;
    let mix_pct = required_f32(params, "mix").map_err(anyhow::Error::msg)?;
    // mix=0% → dry=0dB, wet=-90dB (all dry)
    // mix=100% → dry=-90dB, wet=0dB (all wet)
    let dry_level = -90.0 * (mix_pct / 100.0);
    let wet_level = -90.0 + 90.0 * (mix_pct / 100.0);

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, semitone, dry_level, wet_level)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, semitone, dry_level, wet_level)?;
            let right = build_mono_processor(sample_rate, semitone, dry_level, wet_level)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 {
                left,
                right,
                left_buf: Box::new([0.0; MAX_BLOCK]),
                right_buf: Box::new([0.0; MAX_BLOCK]),
            })))
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
