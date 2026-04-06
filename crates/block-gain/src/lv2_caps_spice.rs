use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_caps_spice";
pub const DISPLAY_NAME: &str = "Spice";
const BRAND: &str = "caps";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/caps/Spice";
const PLUGIN_DIR: &str = "mod-caps-Spice";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "Spice.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "Spice.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "Spice.dll";

// LV2 port indices (from TTL)
const PORT_LOF: usize = 0;
const PORT_LOCOMP: usize = 1;
const PORT_LOGAIN: usize = 2;
const PORT_HIF: usize = 3;
const PORT_HIGAIN: usize = 4;
const PORT_AUDIO_IN: usize = 5;
const PORT_AUDIO_OUT: usize = 6;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "lof",
                "Low Frequency",
                None,
                Some(225.0),
                50.0,
                400.0,
                1.0,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "locomp",
                "Low Compression",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "logain",
                "Low Gain",
                None,
                Some(25.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "hif",
                "High Frequency",
                None,
                Some(1350.0),
                400.0,
                5000.0,
                1.0,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "higain",
                "High Gain",
                None,
                Some(25.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = required_f32(params, "lof").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "locomp").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "logain").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "hif").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "higain").map_err(anyhow::Error::msg)?;
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
    lof: f32,
    locomp: f32,
    logain: f32,
    hif: f32,
    higain: f32,
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
            (PORT_LOF, lof),
            (PORT_LOCOMP, locomp),
            (PORT_LOGAIN, logain),
            (PORT_HIF, hif),
            (PORT_HIGAIN, higain),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let lof = required_f32(params, "lof").map_err(anyhow::Error::msg)?;
    let locomp = required_f32(params, "locomp").map_err(anyhow::Error::msg)? / 100.0;
    let logain = required_f32(params, "logain").map_err(anyhow::Error::msg)? / 100.0;
    let hif = required_f32(params, "hif").map_err(anyhow::Error::msg)?;
    let higain = required_f32(params, "higain").map_err(anyhow::Error::msg)? / 100.0;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, lof, locomp, logain, hif, higain)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, lof, locomp, logain, hif, higain)?;
            let right = build_mono_processor(sample_rate, lof, locomp, logain, hif, higain)?;
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
