use crate::registry::{AmpBackendKind, AmpModelDefinition};
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "lv2_mda_combo";
pub const DISPLAY_NAME: &str = "MDA Combo";
const BRAND: &str = "mda";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/mda/Combo";
const PLUGIN_DIR: &str = "mod-mda-Combo";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "Combo.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "Combo.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "Combo.dll";

// LV2 port indices (from TTL)
const PORT_MODEL: usize = 0;
const PORT_DRIVE: usize = 1;
const PORT_BIAS: usize = 2;
const PORT_OUTPUT: usize = 3;
const PORT_STEREO: usize = 4;
const PORT_HPF_FREQ: usize = 5;
const PORT_HPF_RESO: usize = 6;
const PORT_AUDIO_IN_L: usize = 7;
const PORT_AUDIO_IN_R: usize = 8;
const PORT_AUDIO_OUT_L: usize = 9;
const PORT_AUDIO_OUT_R: usize = 10;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_AMP.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "drive", "Drive", None, Some(50.0),
                0.0, 100.0, 1.0, ParameterUnit::Percent,
            ),
            float_parameter(
                "bias", "Bias", None, Some(50.0),
                0.0, 100.0, 1.0, ParameterUnit::Percent,
            ),
            float_parameter(
                "output", "Output", None, Some(50.0),
                0.0, 100.0, 1.0, ParameterUnit::Percent,
            ),
        ],
    }
}

fn validate(params: &ParameterSet) -> Result<()> {
    let _ = params;
    Ok(())
}

fn asset_summary(params: &ParameterSet) -> Result<String> {
    let _ = params;
    Ok(String::new())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    // MDA lvz wrapper expects all params normalized 0-1
    let drive = required_f32(params, "drive").map_err(anyhow::Error::msg)? / 100.0;
    let bias = required_f32(params, "bias").map_err(anyhow::Error::msg)? / 100.0;
    let output = required_f32(params, "output").map_err(anyhow::Error::msg)? / 100.0;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let control_ports = &[
        (PORT_MODEL, 0.5_f32),
        (PORT_DRIVE, drive),
        (PORT_BIAS, bias),
        (PORT_OUTPUT, output),
        (PORT_STEREO, 0.0_f32),
        (PORT_HPF_FREQ, 0.0_f32),
        (PORT_HPF_RESO, 0.5_f32),
    ];

    let processor = lv2::build_stereo_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R],
        &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R],
        control_ports,
    )?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(StereoAsMono(processor)))),
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(processor))),
    }
}

struct StereoAsMono(lv2::StereoLv2Processor);
impl MonoProcessor for StereoAsMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        let [l, r] = block_core::StereoProcessor::process_frame(&mut self.0, [input, input]);
        (l + r) * 0.5
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: AmpModelDefinition = AmpModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: AmpBackendKind::Lv2,
    schema,
    validate,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_ACOUSTIC_BASS,
    knob_layout: &[],
};
