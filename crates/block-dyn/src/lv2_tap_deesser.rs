use crate::registry::DynModelDefinition;
use crate::DynBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_tap_deesser";
pub const DISPLAY_NAME: &str = "TAP DeEsser";
const BRAND: &str = "tap";
const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/deesser";
const PLUGIN_DIR: &str = "tap-deesser";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_deesser.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_deesser.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_deesser.dll";

// Ports: 0=threshold, 1=frequency, 2=sidechain, 3=monitor, 4=attenuation(out), 5=AudioIn, 6=AudioOut
const PORT_THRESHOLD: usize = 0;
const PORT_FREQUENCY: usize = 1;
const PORT_SIDECHAIN: usize = 2;
const PORT_MONITOR: usize = 3;
const PORT_AUDIO_IN: usize = 5;
const PORT_AUDIO_OUT: usize = 6;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_DYNAMICS.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("threshold", "Threshold", None, Some(-30.0), -50.0, 0.0, 1.0, ParameterUnit::Decibels),
            float_parameter("frequency", "Frequency", None, Some(5500.0), 2000.0, 16000.0, 100.0, ParameterUnit::Hertz),
        ],
    })
}

struct DualMonoLv2 { left: lv2::Lv2Processor, right: lv2::Lv2Processor }
impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono(sample_rate: f32, threshold: f32, frequency: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor_with_extras(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[(PORT_THRESHOLD, threshold), (PORT_FREQUENCY, frequency),
          (PORT_SIDECHAIN, 0.0), (PORT_MONITOR, 0.0)],
        &[4], // attenuation meter output
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let threshold = required_f32(params, "threshold").map_err(anyhow::Error::msg)?;
    let frequency = required_f32(params, "frequency").map_err(anyhow::Error::msg)?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(build_mono(sample_rate, threshold, frequency)?))),
        AudioChannelLayout::Stereo => {
            let left = build_mono(sample_rate, threshold, frequency)?;
            let right = build_mono(sample_rate, threshold, frequency)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

pub const MODEL_DEFINITION: DynModelDefinition = DynModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: DynBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
