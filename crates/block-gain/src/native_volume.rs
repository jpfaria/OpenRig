use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    bool_parameter, float_parameter, required_bool, required_f32, ModelParameterSchema,
    ParameterSet, ParameterUnit,
};
use block_core::{db_to_lin, AudioChannelLayout, BlockProcessor, ModelAudioMode, StereoProcessor};

pub const MODEL_ID: &str = "volume";
pub const DISPLAY_NAME: &str = "Volume";

struct VolumeProcessor {
    gain: f32,
    mute: bool,
}

impl StereoProcessor for VolumeProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        if self.mute {
            [0.0, 0.0]
        } else {
            [input[0] * self.gain, input[1] * self.gain]
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter(
                "volume_db",
                "Volume",
                None,
                Some(0.0),
                -60.0,
                12.0,
                0.5,
                ParameterUnit::Decibels,
            ),
            bool_parameter("mute", "Mute", None, Some(false)),
        ],
    }
}

fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = required_f32(params, "volume_db").map_err(anyhow::Error::msg)?;
    Ok(())
}

fn asset_summary(_params: &ParameterSet) -> Result<String> {
    Ok("native='volume'".to_string())
}

fn build(
    params: &ParameterSet,
    _sample_rate: f32,
    _layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let volume_db = required_f32(params, "volume_db").map_err(anyhow::Error::msg)?;
    let mute = required_bool(params, "mute").unwrap_or(false);
    let gain = db_to_lin(volume_db);

    Ok(BlockProcessor::Stereo(Box::new(VolumeProcessor {
        gain,
        mute,
    })))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: GainBackendKind::Native,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[
        block_core::KnobLayoutEntry {
            param_key: "volume_db",
            svg_cx: 0.0,
            svg_cy: 0.0,
            svg_r: 0.0,
            min: -60.0,
            max: 12.0,
            step: 0.5,
        },
    ],
};
