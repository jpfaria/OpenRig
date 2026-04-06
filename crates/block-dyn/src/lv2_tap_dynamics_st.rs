use crate::registry::DynModelDefinition;
use crate::DynBackendKind;
use anyhow::Result;
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string,
    ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_tap_dynamics_st";
pub const DISPLAY_NAME: &str = "TAP Dynamics Stereo";
const BRAND: &str = "tap";
const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/dynamics-st";
const PLUGIN_DIR: &str = "tap-dynamics-st";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_dynamics-st.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_dynamics-st.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_dynamics-st.dll";

// Ports: 0=attack, 1=release, 2=offset, 3=makeup, 4=env, 5=adj, 6=function, 7=stereo_mode, 8=InL, 9=InR, 10=OutL, 11=OutR
const PORT_ATTACK: usize = 0;
const PORT_RELEASE: usize = 1;
const PORT_OFFSET_GAIN: usize = 2;
const PORT_MAKEUP_GAIN: usize = 3;
const PORT_FUNCTION: usize = 6;
const PORT_STEREO_MODE: usize = 7;
const PORT_AUDIO_IN_L: usize = 8;
const PORT_AUDIO_IN_R: usize = 9;
const PORT_AUDIO_OUT_L: usize = 10;
const PORT_AUDIO_OUT_R: usize = 11;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_DYNAMICS.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("attack", "Attack", None, Some(128.0), 4.0, 500.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("release", "Release", None, Some(502.0), 4.0, 1000.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("offset_gain", "Offset Gain", None, Some(0.0), -20.0, 20.0, 1.0, ParameterUnit::Decibels),
            float_parameter("makeup_gain", "Makeup Gain", None, Some(0.0), -20.0, 20.0, 1.0, ParameterUnit::Decibels),
            enum_parameter("function", "Function", None, Some("0"), &[
                ("0", "Compressor (2:1)"),
                ("1", "Compressor (3:1)"),
                ("2", "Compressor (5:1)"),
                ("3", "Limiter (10:1)"),
                ("4", "Gate (-30dB)"),
                ("5", "Gate (-60dB)"),
                ("6", "Gate (-inf)"),
            ]),
        ],
    })
}

struct DualMonoLv2 { left: lv2::Lv2Processor, right: lv2::Lv2Processor }
impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let attack = required_f32(params, "attack").map_err(anyhow::Error::msg)?;
    let release = required_f32(params, "release").map_err(anyhow::Error::msg)?;
    let offset = required_f32(params, "offset_gain").map_err(anyhow::Error::msg)?;
    let makeup = required_f32(params, "makeup_gain").map_err(anyhow::Error::msg)?;
    let func_str = required_string(params, "function").map_err(anyhow::Error::msg)?;
    let function: f32 = func_str.parse().unwrap_or(0.0);

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    let control_ports = &[
        (PORT_ATTACK, attack), (PORT_RELEASE, release), (PORT_OFFSET_GAIN, offset),
        (PORT_MAKEUP_GAIN, makeup), (4, 0.0), (5, 0.0), (PORT_FUNCTION, function),
        (PORT_STEREO_MODE, 0.0),
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

pub const MODEL_DEFINITION: DynModelDefinition = DynModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: DynBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
