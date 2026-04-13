use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_gx_ultracab";
pub const DISPLAY_NAME: &str = "GxUltraCab";
const BRAND: &str = "guitarix";

const PLUGIN_URI: &str = "http://guitarix.sourceforge.net/plugins/gx_ultracab_#_ultracab_";
const PLUGIN_DIR: &str = "gx_ultracab";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "gx_ultracab.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "gx_ultracab.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "gx_ultracab.dll";

// LV2 port indices — note stereo: out_l=0, in_l=1, out_r=2, in_r=3
const PORT_AUDIO_OUT_L: usize = 0;
const PORT_AUDIO_IN_L: usize = 1;
const PORT_AUDIO_OUT_R: usize = 2;
const PORT_AUDIO_IN_R: usize = 3;
const PORT_BYPASS: usize = 4;
const PORT_GAIN: usize = 5;
const PORT_MIDS: usize = 6;
const PORT_PUNCH: usize = 7;
const PORT_RESONANCE: usize = 8;
const PORT_SIZE: usize = 9;
const PORT_TOP: usize = 10;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_CAB.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("gain", "Level", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mids", "Mids", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("punch", "Punch", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("resonance", "Resonance", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("size", "Size", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("top", "Top", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn validate(params: &ParameterSet) -> Result<()> { let _ = params; Ok(()) }
fn asset_summary(params: &ParameterSet) -> Result<String> { let _ = params; Ok(String::new()) }

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let gain = required_f32(params, "gain").map_err(anyhow::Error::msg)? / 100.0;
    let mids = required_f32(params, "mids").map_err(anyhow::Error::msg)? / 100.0;
    let punch = required_f32(params, "punch").map_err(anyhow::Error::msg)? / 100.0;
    let resonance = required_f32(params, "resonance").map_err(anyhow::Error::msg)? / 100.0;
    let size = required_f32(params, "size").map_err(anyhow::Error::msg)? / 100.0;
    let top = required_f32(params, "top").map_err(anyhow::Error::msg)? / 100.0;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let control_ports = &[
        (PORT_BYPASS, 1.0), // lv2:enabled — 1.0 = active
        (PORT_GAIN, gain), (PORT_MIDS, mids), (PORT_PUNCH, punch),
        (PORT_RESONANCE, resonance), (PORT_SIZE, size), (PORT_TOP, top),
    ];

    match layout {
        AudioChannelLayout::Mono => {
            let processor = lv2::build_lv2_processor_with_extras(
                &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
                &[PORT_AUDIO_IN_L], &[PORT_AUDIO_OUT_L], control_ports,
                &[PORT_AUDIO_IN_R, PORT_AUDIO_OUT_R],
            )?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let processor = lv2::build_stereo_lv2_processor(
                &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
                &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R], &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R],
                control_ports,
            )?;
            Ok(BlockProcessor::Stereo(Box::new(processor)))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: CabModelDefinition = CabModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: CabBackendKind::Lv2, schema, validate, asset_summary, build,
    supported_instruments: block_core::GUITAR_ACOUSTIC_BASS, knob_layout: &[],
};
