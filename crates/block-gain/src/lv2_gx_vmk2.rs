use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema,
    ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_gx_vmk2";
pub const DISPLAY_NAME: &str = "Vmk2";
const BRAND: &str = "guitarix";

const PLUGIN_URI: &str =
    "http://guitarix.sourceforge.net/plugins/gx_vmk2d_#_vmk2d_";
const PLUGIN_DIR: &str = "GxVmk2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "GxVmk2.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "GxVmk2.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "GxVmk2.dll";

// LV2 port indices (from TTL) — GxPlugins: AUDIO_OUT=0, AUDIO_IN=1
const PORT_AUDIO_OUT: usize = 0;
const PORT_AUDIO_IN: usize = 1;
const PORT_BASS: usize = 2;
const PORT_DEPTH: usize = 3;
const PORT_MRBSELECT: usize = 4;
const PORT_MRB: usize = 5;
const PORT_REVERBLEVEL: usize = 6;
const PORT_REVERB: usize = 7;
const PORT_SPEED: usize = 8;
const PORT_TREBLE: usize = 9;
const PORT_VIBE: usize = 10;
const PORT_VOLUME: usize = 11;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "bass",
                "Bass",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mrbselect",
                "MRB Select",
                None,
                Some(0.0),
                0.0,
                2.0,
                1.0,
                ParameterUnit::None,
            ),
            float_parameter(
                "mrb",
                "MRB",
                None,
                Some(0.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "reverblevel",
                "Reverb Level",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "reverb",
                "Reverb",
                None,
                Some(0.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "speed",
                "Speed",
                None,
                Some(0.5),
                0.01,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "treble",
                "Treble",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "vibe",
                "Vibe",
                None,
                Some(0.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "volume",
                "Volume",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = required_f32(params, "bass").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "depth").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "mrbselect").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "mrb").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "reverblevel").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "reverb").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "speed").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "treble").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "vibe").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "volume").map_err(anyhow::Error::msg)?;
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
    bass: f32,
    depth: f32,
    mrbselect: f32,
    mrb: f32,
    reverblevel: f32,
    reverb: f32,
    speed: f32,
    treble: f32,
    vibe: f32,
    volume: f32,
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
            (PORT_BASS, bass),
            (PORT_DEPTH, depth),
            (PORT_MRBSELECT, mrbselect),
            (PORT_MRB, mrb),
            (PORT_REVERBLEVEL, reverblevel),
            (PORT_REVERB, reverb),
            (PORT_SPEED, speed),
            (PORT_TREBLE, treble),
            (PORT_VIBE, vibe),
            (PORT_VOLUME, volume),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let bass = required_f32(params, "bass").map_err(anyhow::Error::msg)? / 100.0;
    let depth = required_f32(params, "depth").map_err(anyhow::Error::msg)? / 100.0;
    let mrbselect = required_f32(params, "mrbselect").map_err(anyhow::Error::msg)?;
    let mrb = required_f32(params, "mrb").map_err(anyhow::Error::msg)? / 100.0;
    let reverblevel = required_f32(params, "reverblevel").map_err(anyhow::Error::msg)? / 100.0;
    let reverb = required_f32(params, "reverb").map_err(anyhow::Error::msg)? / 100.0;
    let speed = required_f32(params, "speed").map_err(anyhow::Error::msg)?;
    let treble = required_f32(params, "treble").map_err(anyhow::Error::msg)? / 100.0;
    let vibe = required_f32(params, "vibe").map_err(anyhow::Error::msg)? / 100.0;
    let volume = required_f32(params, "volume").map_err(anyhow::Error::msg)? / 100.0;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(
                sample_rate, bass, depth, mrbselect, mrb, reverblevel, reverb,
                speed, treble, vibe, volume,
            )?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(
                sample_rate, bass, depth, mrbselect, mrb, reverblevel, reverb,
                speed, treble, vibe, volume,
            )?;
            let right = build_mono_processor(
                sample_rate, bass, depth, mrbselect, mrb, reverblevel, reverb,
                speed, treble, vibe, volume,
            )?;
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
