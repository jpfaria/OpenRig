use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_invada_tube";
pub const DISPLAY_NAME: &str = "Invada Tube";
const BRAND: &str = "invada";

const PLUGIN_URI: &str = "http://invadarecords.com/plugins/lv2/tube/mono";
const PLUGIN_DIR: &str = "invada";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "inv_tube.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "inv_tube.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "inv_tube.dll";

// LV2 port indices (from TTL)
const PORT_BYPASS: usize = 0;
const PORT_DRIVE: usize = 1;
const PORT_DCOFFSET: usize = 2;
const PORT_PHASE: usize = 3;
const PORT_MIX: usize = 4;
const PORT_METER_DRIVE: usize = 5;
const PORT_METER_IN: usize = 6;
const PORT_METER_OUT: usize = 7;
const PORT_AUDIO_IN: usize = 8;
const PORT_AUDIO_OUT: usize = 9;

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
                Some(0.0),
                0.0,
                18.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(75.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
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
    mix: f32,
) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    // Output control ports (meters) need dummy buffer slots — include them in control_ports
    lv2::build_lv2_processor(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN],
        &[PORT_AUDIO_OUT],
        &[
            (PORT_BYPASS, 0.0),
            (PORT_DRIVE, drive),
            (PORT_DCOFFSET, 0.0),
            (PORT_PHASE, 0.0),
            (PORT_MIX, mix),
            (PORT_METER_DRIVE, 0.0),
            (PORT_METER_IN, -60.0),
            (PORT_METER_OUT, -60.0),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let drive = required_f32(params, "drive").map_err(anyhow::Error::msg)?;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)?;

    let _ = layout;
    let processor = build_mono_processor(sample_rate, drive, mix)?;
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
