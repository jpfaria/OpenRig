use crate::registry::WahModelDefinition;
use crate::WahBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_gx_quack";
pub const DISPLAY_NAME: &str = "GxQuack";
const BRAND: &str = "guitarix";

const PLUGIN_URI: &str = "http://guitarix.sourceforge.net/plugins/gx_quack_#_quack_";
const PLUGIN_DIR: &str = "gx_quack";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "gx_quack.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "gx_quack.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "gx_quack.dll";

// LV2 port indices (from TTL) — note: output is 0, input is 1
const PORT_AUDIO_OUT: usize = 0;
const PORT_AUDIO_IN: usize = 1;
const PORT_BYPASS: usize = 2;
const PORT_DEPTH: usize = 3;
const PORT_DRIVE: usize = 4;
const PORT_GAIN: usize = 5;
const PORT_MODE: usize = 6;
const PORT_PEAK: usize = 7;
const PORT_RANGE: usize = 8;
const PORT_TONE: usize = 9;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_WAH.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("depth", "Depth", None, Some(1.0), 0.0, 1.0, 0.01, ParameterUnit::None),
            float_parameter("drive", "Drive", None, Some(0.0), 0.0, 1.0, 0.01, ParameterUnit::None),
            float_parameter("gain", "Gain", None, Some(0.0), -12.0, 6.0, 0.1, ParameterUnit::Decibels),
            float_parameter("peak", "Peak", None, Some(5.0), -6.0, 20.0, 0.1, ParameterUnit::Decibels),
            float_parameter("range", "Range", None, Some(1.0), 0.0, 1.0, 0.01, ParameterUnit::None),
            float_parameter("tone", "Tone", None, Some(0.0), 0.0, 2.0, 0.01, ParameterUnit::None),
        ],
    }
}

fn validate(params: &ParameterSet) -> Result<()> {
    let _ = params;
    Ok(())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let depth = required_f32(params, "depth").map_err(anyhow::Error::msg)?;
    let drive = required_f32(params, "drive").map_err(anyhow::Error::msg)?;
    let gain = required_f32(params, "gain").map_err(anyhow::Error::msg)?;
    let peak = required_f32(params, "peak").map_err(anyhow::Error::msg)?;
    let range = required_f32(params, "range").map_err(anyhow::Error::msg)?;
    let tone = required_f32(params, "tone").map_err(anyhow::Error::msg)?;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let control_ports = &[
        (PORT_BYPASS, 0.0), // not bypassed
        (PORT_DEPTH, depth),
        (PORT_DRIVE, drive),
        (PORT_GAIN, gain),
        (PORT_MODE, 2.0), // fixed: auto wah mode
        (PORT_PEAK, peak),
        (PORT_RANGE, range),
        (PORT_TONE, tone),
    ];

    let _ = layout;
    let processor = lv2::build_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        control_ports,
    )?;

    struct MonoAsStereo(lv2::Lv2Processor);
    impl StereoProcessor for MonoAsStereo {
        fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
            let out = self.0.process_sample(input[0]);
            [out, out]
        }
    }
    Ok(BlockProcessor::Stereo(Box::new(MonoAsStereo(processor))))
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: WahModelDefinition = WahModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: WahBackendKind::Lv2,
    schema,
    validate,
    build,
    supported_instruments: block_core::GUITAR_ACOUSTIC_BASS,
    knob_layout: &[],
};
