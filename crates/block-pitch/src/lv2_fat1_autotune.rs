use crate::registry::PitchModelDefinition;
use crate::PitchBackendKind;
use anyhow::Result;
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string, ModelParameterSchema,
    ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_fat1_autotune";
pub const DISPLAY_NAME: &str = "x42 Autotune";
const BRAND: &str = "x42";

const PLUGIN_URI: &str = "http://gareus.org/oss/lv2/fat1#scales";
const PLUGIN_DIR: &str = "fat1.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "fat1.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "fat1.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "fat1.dll";

// LV2 port indices (from fat1.scales.ttl — variant #scales)
const PORT_MIDI_IN: usize = 0; // Atom port — must be connected to empty sequence
const PORT_AUDIO_IN: usize = 1;
const PORT_AUDIO_OUT: usize = 2;
const PORT_MODE: usize = 3;
const PORT_CHANNEL_FILTER: usize = 4;
const PORT_TUNING: usize = 5;
const PORT_BIAS: usize = 6;
const PORT_FILTER: usize = 7;
const PORT_CORRECTION: usize = 8;
const PORT_OFFSET: usize = 9;
const PORT_BEND_RANGE: usize = 10;
const PORT_FAST_MODE: usize = 11;
const PORT_SCALE: usize = 12;
// Output control ports — MUST be connected to valid buffers to avoid SIGSEGV
const PORT_OUT_NMASK: usize = 13;
const PORT_OUT_NSET: usize = 14;
const PORT_OUT_BEND: usize = 15;
const PORT_OUT_ERROR: usize = 16;
const PORT_OUT_LATENCY: usize = 17;

fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "pitch".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            enum_parameter(
                "scale",
                "Scale",
                Some("Pitch"),
                Some("chromatic"),
                &[
                    ("chromatic", "Chromatic"),
                    ("c_major", "C Major"),
                    ("db_major", "Db Major"),
                    ("d_major", "D Major"),
                    ("eb_major", "Eb Major"),
                    ("e_major", "E Major"),
                    ("f_major", "F Major"),
                    ("fs_major", "F# Major"),
                    ("g_major", "G Major"),
                    ("ab_major", "Ab Major"),
                    ("a_major", "A Major"),
                    ("bb_major", "Bb Major"),
                    ("b_major", "B Major"),
                    ("c_minor", "C Minor"),
                    ("db_minor", "Db Minor"),
                    ("d_minor", "D Minor"),
                    ("eb_minor", "Eb Minor"),
                    ("e_minor", "E Minor"),
                    ("f_minor", "F Minor"),
                    ("fs_minor", "F# Minor"),
                    ("g_minor", "G Minor"),
                    ("ab_minor", "Ab Minor"),
                    ("a_minor", "A Minor"),
                    ("bb_minor", "Bb Minor"),
                    ("b_minor", "B Minor"),
                ],
            ),
            enum_parameter(
                "speed",
                "Speed",
                Some("Pitch"),
                Some("med"),
                &[
                    ("fast", "Fast"),
                    ("med", "Medium"),
                    ("slow", "Slow"),
                ],
            ),
            float_parameter(
                "correction",
                "Correction",
                Some("Control"),
                Some(100.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "offset",
                "Offset",
                Some("Control"),
                Some(0.0),
                -200.0,
                200.0,
                1.0,
                ParameterUnit::None,
            ),
            float_parameter(
                "bias",
                "Bias",
                Some("Control"),
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

fn scale_str_to_float(s: &str) -> f32 {
    match s {
        "chromatic" => 0.0,
        "c_major" => 1.0, "db_major" => 2.0, "d_major" => 3.0, "eb_major" => 4.0,
        "e_major" => 5.0, "f_major" => 6.0, "fs_major" => 7.0, "g_major" => 8.0,
        "ab_major" => 9.0, "a_major" => 10.0, "bb_major" => 11.0, "b_major" => 12.0,
        "c_minor" => 13.0, "db_minor" => 14.0, "d_minor" => 15.0, "eb_minor" => 16.0,
        "e_minor" => 17.0, "f_minor" => 18.0, "fs_minor" => 19.0, "g_minor" => 20.0,
        "ab_minor" => 21.0, "a_minor" => 22.0, "bb_minor" => 23.0, "b_minor" => 24.0,
        _ => 0.0,
    }
}

fn speed_str_to_float(s: &str) -> f32 {
    match s {
        "fast" => 0.02,
        "med" => 0.1,
        "slow" => 0.5,
        _ => 0.1,
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
        // Split stereo into pre-allocated mono buffers (no heap alloc)
        for (i, frame) in buffer[..len].iter().enumerate() {
            self.left_buf[i] = frame[0];
            self.right_buf[i] = frame[1];
        }
        // Process each channel as a full block (calls run(N))
        self.left.process_block(&mut self.left_buf[..len]);
        self.right.process_block(&mut self.right_buf[..len]);
        // Merge back to stereo
        for (i, frame) in buffer[..len].iter_mut().enumerate() {
            frame[0] = self.left_buf[i];
            frame[1] = self.right_buf[i];
        }
    }
}

fn build_mono_processor(
    sample_rate: f32,
    scale: f32,
    correction: f32,
    offset: f32,
    filter: f32,
    bias: f32,
) -> Result<lv2::Lv2Processor> {
    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;

    lv2::build_lv2_processor_with_atoms(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN],
        &[PORT_AUDIO_OUT],
        &[
            (PORT_MODE, 0.0),            // Auto mode
            (PORT_CHANNEL_FILTER, 0.0),   // Any MIDI channel
            (PORT_TUNING, 440.0),         // Standard tuning
            (PORT_BIAS, bias),
            (PORT_FILTER, filter),
            (PORT_CORRECTION, correction),
            (PORT_OFFSET, offset),
            (PORT_BEND_RANGE, 2.0),
            (PORT_FAST_MODE, 1.0),        // Fast mode for live use
            (PORT_SCALE, scale),
            // Output control ports — must be connected to avoid SIGSEGV
            (PORT_OUT_NMASK, 0.0),
            (PORT_OUT_NSET, 0.0),
            (PORT_OUT_BEND, 0.0),
            (PORT_OUT_ERROR, 0.0),
            (PORT_OUT_LATENCY, 0.0),
        ],
        &[PORT_MIDI_IN],  // Atom sidechain port — empty sequence
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
    let scale_str = required_string(params, "scale").map_err(anyhow::Error::msg)?;
    let scale = scale_str_to_float(&scale_str);
    let correction = required_f32(params, "correction").map_err(anyhow::Error::msg)? / 100.0;
    let offset_cents = required_f32(params, "offset").map_err(anyhow::Error::msg)?;
    let offset = offset_cents / 100.0; // Convert cents to semitones
    let speed_str = required_string(params, "speed").map_err(anyhow::Error::msg)?;
    let filter = speed_str_to_float(&speed_str);
    let bias = required_f32(params, "bias").map_err(anyhow::Error::msg)? / 100.0;

    match layout {
        AudioChannelLayout::Mono => {
            let processor =
                build_mono_processor(sample_rate, scale, correction, offset, filter, bias)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left =
                build_mono_processor(sample_rate, scale, correction, offset, filter, bias)?;
            let right =
                build_mono_processor(sample_rate, scale, correction, offset, filter, bias)?;
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
