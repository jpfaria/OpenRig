use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_tap_tubewarmth";
pub const DISPLAY_NAME: &str = "TAP Tubewarmth";
const BRAND: &str = "tap";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/tubewarmth";
const PLUGIN_DIR: &str = "tap-tubewarmth";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_tubewarmth.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_tubewarmth.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_tubewarmth.dll";

// LV2 port indices (from TTL)
const PORT_DRIVE: usize = 0;
const PORT_BLEND: usize = 1;
const PORT_AUDIO_IN: usize = 2;
const PORT_AUDIO_OUT: usize = 3;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "drive",
                "Drive",
                None,
                Some(5.0),
                0.1,
                10.0,
                0.1,
                ParameterUnit::None,
            ),
            float_parameter(
                "blend",
                "Blend",
                None,
                Some(10.0),
                -10.0,
                10.0,
                0.1,
                ParameterUnit::None,
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

fn build_mono_processor(
    sample_rate: f32,
    drive: f32,
    blend: f32,
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
            (PORT_DRIVE, drive),
            (PORT_BLEND, blend),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let drive = required_f32(params, "drive").map_err(anyhow::Error::msg)?;
    let blend = required_f32(params, "blend").map_err(anyhow::Error::msg)?;

    let _ = layout;
    let processor = build_mono_processor(sample_rate, drive, blend)?;
    struct MonoAsStereo(lv2::Lv2Processor);
    impl StereoProcessor for MonoAsStereo {
        fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
            let out = self.0.process_sample(input[0]);
            [out, out]
        }
    }
    Ok(BlockProcessor::Stereo(Box::new(MonoAsStereo(processor))))
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
    validate,
    asset_summary,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
