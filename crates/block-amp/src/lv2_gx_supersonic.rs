use crate::registry::{AmpBackendKind, AmpModelDefinition};
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_gx_supersonic";
pub const DISPLAY_NAME: &str = "GxSupersonic";
const BRAND: &str = "guitarix";

const PLUGIN_URI: &str = "http://guitarix.sourceforge.net/plugins/gx_supersonic_#_supersonic_";
const PLUGIN_DIR: &str = "gx_supersonic";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "gx_supersonic.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "gx_supersonic.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "gx_supersonic.dll";

const PORT_AUDIO_OUT: usize = 0;
const PORT_AUDIO_IN: usize = 1;
const PORT_BYPASS: usize = 2;
const PORT_GAIN: usize = 3;
const PORT_BASS: usize = 4;
const PORT_TREBLE: usize = 5;
const PORT_VOLUME: usize = 6;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_AMP.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("gain", "Gain", None, Some(15.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("bass", "Bass", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("treble", "Treble", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("volume", "Volume", None, Some(25.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn validate(params: &ParameterSet) -> Result<()> { let _ = params; Ok(()) }
fn asset_summary(params: &ParameterSet) -> Result<String> { let _ = params; Ok(String::new()) }

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let gain = required_f32(params, "gain").map_err(anyhow::Error::msg)? / 100.0;
    let bass = required_f32(params, "bass").map_err(anyhow::Error::msg)? / 100.0;
    let treble = required_f32(params, "treble").map_err(anyhow::Error::msg)? / 100.0;
    let volume = required_f32(params, "volume").map_err(anyhow::Error::msg)? / 100.0;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let _ = layout;
    let processor = lv2::build_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[(PORT_BYPASS, 0.0), (PORT_GAIN, gain), (PORT_BASS, bass), (PORT_TREBLE, treble), (PORT_VOLUME, volume)],
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

pub const MODEL_DEFINITION: AmpModelDefinition = AmpModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: AmpBackendKind::Lv2, schema, validate, asset_summary, build,
    supported_instruments: block_core::GUITAR_ACOUSTIC_BASS, knob_layout: &[],
};
