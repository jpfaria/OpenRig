use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use anyhow::Result;
use block_core::param::{float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_zameq2";
pub const DISPLAY_NAME: &str = "ZamEQ2";
const BRAND: &str = "zam";

const PLUGIN_URI: &str = "urn:zamaudio:ZamEQ2";
const PLUGIN_DIR: &str = "ZamEQ2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "ZamEQ2_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "ZamEQ2_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "ZamEQ2_dsp.dll";

// LV2 port indices (from ZamEQ2_dsp.ttl)
// 0: Audio In, 1: Audio Out
// 2: Boost/Cut 1 (-50 to 20, default 0)
// 3: Bandwidth 1 (0.1-6, default 1)
// 4: Frequency 1 (20-14000, default 500)
// 5: Boost/Cut 2 (-50 to 20, default 0)
// 6: Bandwidth 2 (0.1-6, default 1)
// 7: Frequency 2 (20-14000, default 3000)
// 8: Boost/Cut L (-50 to 20, default 0)
// 9: Frequency L (20-14000, default 250)
// 10: Boost/Cut H (-50 to 20, default 0)
// 11: Frequency H (20-14000, default 8000)
// 12: Master Gain (-12 to 12, default 0)
// 13: Peaks ON (0-1, default 0)
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_FILTER.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("boost1", "Peak 1 Gain", None, Some(0.0), -50.0, 20.0, 0.1, ParameterUnit::Decibels),
            float_parameter("bw1", "Peak 1 BW", None, Some(1.0), 0.1, 6.0, 0.01, ParameterUnit::None),
            float_parameter("freq1", "Peak 1 Freq", None, Some(500.0), 20.0, 14000.0, 1.0, ParameterUnit::Hertz),
            float_parameter("boost2", "Peak 2 Gain", None, Some(0.0), -50.0, 20.0, 0.1, ParameterUnit::Decibels),
            float_parameter("bw2", "Peak 2 BW", None, Some(1.0), 0.1, 6.0, 0.01, ParameterUnit::None),
            float_parameter("freq2", "Peak 2 Freq", None, Some(3000.0), 20.0, 14000.0, 1.0, ParameterUnit::Hertz),
            float_parameter("boostl", "Low Shelf Gain", None, Some(0.0), -50.0, 20.0, 0.1, ParameterUnit::Decibels),
            float_parameter("freql", "Low Shelf Freq", None, Some(250.0), 20.0, 14000.0, 1.0, ParameterUnit::Hertz),
            float_parameter("boosth", "High Shelf Gain", None, Some(0.0), -50.0, 20.0, 0.1, ParameterUnit::Decibels),
            float_parameter("freqh", "High Shelf Freq", None, Some(8000.0), 20.0, 14000.0, 1.0, ParameterUnit::Hertz),
            float_parameter("master", "Master Gain", None, Some(0.0), -12.0, 12.0, 0.1, ParameterUnit::Decibels),
        ],
    })
}



struct DualMonoLv2 {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono_processor(
    sample_rate: f32,
    boost1: f32, bw1: f32, freq1: f32,
    boost2: f32, bw2: f32, freq2: f32,
    boostl: f32, freql: f32,
    boosth: f32, freqh: f32,
    master: f32,
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
            (2, boost1),
            (3, bw1),
            (4, freq1),
            (5, boost2),
            (6, bw2),
            (7, freq2),
            (8, boostl),
            (9, freql),
            (10, boosth),
            (11, freqh),
            (12, master),
            (13, 0.0), // Peaks ON = off
        ],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let boost1 = required_f32(params, "boost1").map_err(anyhow::Error::msg)?;
    let bw1    = required_f32(params, "bw1").map_err(anyhow::Error::msg)?;
    let freq1  = required_f32(params, "freq1").map_err(anyhow::Error::msg)?;
    let boost2 = required_f32(params, "boost2").map_err(anyhow::Error::msg)?;
    let bw2    = required_f32(params, "bw2").map_err(anyhow::Error::msg)?;
    let freq2  = required_f32(params, "freq2").map_err(anyhow::Error::msg)?;
    let boostl = required_f32(params, "boostl").map_err(anyhow::Error::msg)?;
    let freql  = required_f32(params, "freql").map_err(anyhow::Error::msg)?;
    let boosth = required_f32(params, "boosth").map_err(anyhow::Error::msg)?;
    let freqh  = required_f32(params, "freqh").map_err(anyhow::Error::msg)?;
    let master = required_f32(params, "master").map_err(anyhow::Error::msg)?;

    match layout {
        AudioChannelLayout::Mono => {
            Ok(BlockProcessor::Mono(Box::new(build_mono_processor(
                sample_rate,
                boost1, bw1, freq1, boost2, bw2, freq2,
                boostl, freql, boosth, freqh, master,
            )?)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, boost1, bw1, freq1, boost2, bw2, freq2, boostl, freql, boosth, freqh, master)?;
            let right = build_mono_processor(sample_rate, boost1, bw1, freq1, boost2, bw2, freq2, boostl, freql, boosth, freqh, master)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

pub const MODEL_DEFINITION: FilterModelDefinition = FilterModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: FilterBackendKind::Lv2,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
