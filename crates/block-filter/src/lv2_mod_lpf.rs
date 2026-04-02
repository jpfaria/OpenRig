use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_mod_lpf";
pub const DISPLAY_NAME: &str = "MOD Low Pass";
const BRAND: &str = "mod";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/mod-devel/LowPassFilter";
const PLUGIN_DIR: &str = "mod-lpf";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "mod-lpf.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "mod-lpf.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "mod-lpf.dll";

// Port indices: 0=AudioIn, 1=AudioOut, 2=Freq (20-20000 Hz), 3=Order (0-3 enum)
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_FREQ: usize = 2;
const PORT_ORDER: usize = 3;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_FILTER.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("frequency", "Frequency", None, Some(600.0), 20.0, 20000.0, 10.0, ParameterUnit::Hertz),
        ],
    })
}



struct DualMonoLv2 { left: lv2::Lv2Processor, right: lv2::Lv2Processor }
impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono(sample_rate: f32, freq: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[(PORT_FREQ, freq), (PORT_ORDER, 2.0)],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let freq = required_f32(params, "frequency").map_err(anyhow::Error::msg)?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(build_mono(sample_rate, freq)?))),
        AudioChannelLayout::Stereo => {
            let left = build_mono(sample_rate, freq)?;
            let right = build_mono(sample_rate, freq)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

pub const MODEL_DEFINITION: FilterModelDefinition = FilterModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: FilterBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
