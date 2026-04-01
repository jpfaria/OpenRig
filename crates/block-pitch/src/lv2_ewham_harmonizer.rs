use crate::registry::PitchModelDefinition;
use crate::PitchBackendKind;
use anyhow::Result;
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string, ModelParameterSchema,
    ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_ewham_harmonizer";
pub const DISPLAY_NAME: &str = "Harmonizer";
const BRAND: &str = "infamous";

const PLUGIN_URI: &str = "http://ssj71.github.io/infamousPlugins/plugs.html#ewham";
const PLUGIN_DIR: &str = "ewham.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "ewham.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "ewham.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "ewham.dll";

// LV2 port indices (from ewham.ttl)
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_EXPRESSION: usize = 2;
const PORT_START: usize = 3;
const PORT_FINISH: usize = 4;
const PORT_MODE: usize = 5;
const PORT_LOCK: usize = 6;
// Port 7: Latency (output) — must be connected
const PORT_LATENCY: usize = 7;

fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "pitch".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            enum_parameter(
                "interval",
                "Interval",
                Some("Harmony"),
                Some("7"),
                &[
                    ("-12", "-1 Octave"),
                    ("-7", "-5th"),
                    ("-5", "-4th"),
                    ("-4", "-Major 3rd"),
                    ("-3", "-Minor 3rd"),
                    ("3", "+Minor 3rd"),
                    ("4", "+Major 3rd"),
                    ("5", "+4th"),
                    ("7", "+5th"),
                    ("12", "+1 Octave"),
                    ("24", "+2 Octaves"),
                ],
            ),
            enum_parameter(
                "mode",
                "Mode",
                Some("Harmony"),
                Some("harmonizer"),
                &[
                    ("shift", "Pitch Shift"),
                    ("harmonizer", "Harmonizer"),
                    ("chorus", "Chorus"),
                ],
            ),
            float_parameter(
                "expression",
                "Expression",
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

fn interval_str_to_semitones(s: &str) -> f32 {
    s.parse::<f32>().unwrap_or(7.0)
}

fn mode_str_to_float(s: &str) -> f32 {
    match s {
        "shift" => 0.0,
        "harmonizer" => 1.0,
        "chorus" => 2.0,
        _ => 1.0,
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
    interval: f32,
    mode: f32,
    expression: f32,
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
            (PORT_EXPRESSION, expression),
            (PORT_START, interval),     // Start = same as Finish for fixed interval
            (PORT_FINISH, interval),    // Finish = desired interval
            (PORT_MODE, mode),
            (PORT_LOCK, 2.0),           // Lock to semitone
            (PORT_LATENCY, 0.0),        // Output port — must be connected
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
    let interval_str = required_string(params, "interval").map_err(anyhow::Error::msg)?;
    let interval = interval_str_to_semitones(&interval_str);
    let mode_str = required_string(params, "mode").map_err(anyhow::Error::msg)?;
    let mode = mode_str_to_float(&mode_str);
    let expression = required_f32(params, "expression").map_err(anyhow::Error::msg)? / 100.0;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, interval, mode, expression)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, interval, mode, expression)?;
            let right = build_mono_processor(sample_rate, interval, mode, expression)?;
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
