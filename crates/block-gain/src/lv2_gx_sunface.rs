use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_gx_sunface";
pub const DISPLAY_NAME: &str = "Sun Face";
const BRAND: &str = "guitarix";

const PLUGIN_URI: &str = "http://guitarix.sourceforge.net/plugins/gx_SunFace_#_SunFace_";
const PLUGIN_DIR: &str = "gx_SunFace";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "gx_SunFace.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "gx_SunFace.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "gx_SunFace.dll";

// LV2 port indices (from TTL)
const PORT_AUDIO_OUT: usize = 0;
const PORT_AUDIO_IN: usize = 1;
const PORT_BYPASS: usize = 2;
const PORT_DRIVE: usize = 3;
const PORT_INPUT: usize = 4;
const PORT_VOLUME: usize = 5;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "drive",
                "Drive",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "input",
                "Input",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "volume",
                "Volume",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = required_f32(params, "drive").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "input").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "volume").map_err(anyhow::Error::msg)?;
    Ok(())
}

fn asset_summary(_params: &ParameterSet) -> Result<String> {
    Ok(format!("lv2='{}'", MODEL_ID))
}

struct DualMonoLv2 {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [
            self.left.process_sample(input[0]),
            self.right.process_sample(input[1]),
        ]
    }
}

fn build_mono_processor(
    sample_rate: f32,
    drive: f32,
    input: f32,
    volume: f32,
) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    lv2::build_lv2_processor(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN],
        &[PORT_AUDIO_OUT],
        &[
            (PORT_BYPASS, 1.0),
            (PORT_DRIVE, drive),
            (PORT_INPUT, input),
            (PORT_VOLUME, volume),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let drive = required_f32(params, "drive").map_err(anyhow::Error::msg)? / 100.0;
    let input = required_f32(params, "input").map_err(anyhow::Error::msg)? / 100.0;
    let volume = required_f32(params, "volume").map_err(anyhow::Error::msg)? / 100.0;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, drive, input, volume)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, drive, input, volume)?;
            let right = build_mono_processor(sample_rate, drive, input, volume)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Lv2,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
