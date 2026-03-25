use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    bool_parameter, float_parameter, required_bool, required_f32, ModelParameterSchema,
    ParameterSet, ParameterUnit,
};
use block_core::{db_to_lin, AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "volume";
pub const DISPLAY_NAME: &str = "Volume";

struct VolumeProcessor {
    gain: f32,
    mute: bool,
}

impl MonoProcessor for VolumeProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        if self.mute { 0.0 } else { input * self.gain }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "volume",
                "Volume",
                None,
                Some(80.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            bool_parameter("mute", "Mute", None, Some(false)),
        ],
    }
}

fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = required_f32(params, "volume").map_err(anyhow::Error::msg)?;
    Ok(())
}

fn asset_summary(_params: &ParameterSet) -> Result<String> {
    Ok("native='volume'".to_string())
}

fn percent_to_db(percent: f32) -> f32 {
    if percent <= 0.0 {
        -60.0
    } else {
        // 0% = -60dB, 80% = 0dB (unity), 100% = +12dB
        let normalized = percent / 100.0;
        -60.0 + normalized * 72.0 // linear map: 0→-60dB, 100→+12dB
    }
}

fn build(
    params: &ParameterSet,
    _sample_rate: f32,
    _layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let volume_pct = required_f32(params, "volume").map_err(anyhow::Error::msg)?;
    let mute = required_bool(params, "mute").unwrap_or(false);
    let gain = db_to_lin(percent_to_db(volume_pct));

    Ok(BlockProcessor::Mono(Box::new(VolumeProcessor {
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
            param_key: "volume",
            svg_cx: 0.0,
            svg_cy: 0.0,
            svg_r: 0.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
        },
    ],
};
