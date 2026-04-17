use crate::registry::DynModelDefinition;
use crate::DynBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_zamcomp";
pub const DISPLAY_NAME: &str = "ZamComp";
const BRAND: &str = "zam";
const PLUGIN_URI: &str = "urn:zamaudio:ZamComp";
const PLUGIN_DIR: &str = "ZamComp";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "ZamComp_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "ZamComp_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "ZamComp_dsp.dll";

// Ports: 0=AudioIn, 1=SidechainIn, 2=AudioOut, 3-12=control
const PORT_AUDIO_IN: usize = 0;
const PORT_SIDECHAIN_IN: usize = 1;
const PORT_AUDIO_OUT: usize = 2;
const PORT_ATTACK: usize = 3;
const PORT_RELEASE: usize = 4;
const PORT_KNEE: usize = 5;
const PORT_RATIO: usize = 6;
const PORT_THRESHOLD: usize = 7;
const PORT_MAKEUP: usize = 8;
const PORT_SLEW: usize = 9;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_DYNAMICS.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("attack", "Attack", None, Some(10.0), 0.1, 100.0, 0.1, ParameterUnit::Milliseconds),
            float_parameter("release", "Release", None, Some(80.0), 1.0, 500.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("threshold", "Threshold", None, Some(0.0), -80.0, 0.0, 1.0, ParameterUnit::Decibels),
            float_parameter("ratio", "Ratio", None, Some(4.0), 1.0, 20.0, 1.0, ParameterUnit::Ratio),
            float_parameter("knee", "Knee", None, Some(0.0), 0.0, 8.0, 1.0, ParameterUnit::Decibels),
            float_parameter("makeup", "Makeup", None, Some(0.0), 0.0, 30.0, 1.0, ParameterUnit::Decibels),
        ],
    })
}

struct DualMonoLv2 { left: lv2::Lv2Processor, right: lv2::Lv2Processor }
impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono(sample_rate: f32, attack: f32, release: f32, threshold: f32, ratio: f32, knee: f32, makeup: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor_with_extras(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[(PORT_ATTACK, attack), (PORT_RELEASE, release), (PORT_KNEE, knee),
          (PORT_RATIO, ratio), (PORT_THRESHOLD, threshold), (PORT_MAKEUP, makeup),
          (PORT_SLEW, 1.0)],
        &[PORT_SIDECHAIN_IN, 10, 11], // sidechain + gain_reduction + output_level meters
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let attack = required_f32(params, "attack").map_err(anyhow::Error::msg)?;
    let release = required_f32(params, "release").map_err(anyhow::Error::msg)?;
    let threshold = required_f32(params, "threshold").map_err(anyhow::Error::msg)?;
    let ratio = required_f32(params, "ratio").map_err(anyhow::Error::msg)?;
    let knee = required_f32(params, "knee").map_err(anyhow::Error::msg)?;
    let makeup = required_f32(params, "makeup").map_err(anyhow::Error::msg)?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(build_mono(sample_rate, attack, release, threshold, ratio, knee, makeup)?))),
        AudioChannelLayout::Stereo => {
            let left = build_mono(sample_rate, attack, release, threshold, ratio, knee, makeup)?;
            let right = build_mono(sample_rate, attack, release, threshold, ratio, knee, makeup)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

pub const MODEL_DEFINITION: DynModelDefinition = DynModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: DynBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
