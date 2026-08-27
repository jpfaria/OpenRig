//! Responsibility: describes the parameters a native cab takes.
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::ModelAudioMode;

use crate::native_cab_settings::{NativeCabSchemaDefaults, NativeCabSettings};

pub fn model_schema(
    model_id: &'static str,
    display_name: &'static str,
    defaults: NativeCabSchemaDefaults,
) -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "cab".into(),
        model: model_id.into(),
        display_name: display_name.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "low_cut_hz",
                "Low Cut",
                Some("Filtering"),
                Some(defaults.low_cut_hz),
                20.0,
                250.0,
                1.0,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "high_cut_hz",
                "High Cut",
                Some("Filtering"),
                Some(defaults.high_cut_hz),
                2_000.0,
                12_000.0,
                10.0,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "resonance",
                "Resonance",
                Some("Speaker"),
                Some(defaults.resonance),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "air",
                "Air",
                Some("Mic"),
                Some(defaults.air),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mic_position",
                "Mic Position",
                Some("Mic"),
                Some(defaults.mic_position),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mic_distance",
                "Mic Distance",
                Some("Mic"),
                Some(defaults.mic_distance),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "room_mix",
                "Room Mix",
                Some("Room"),
                Some(defaults.room_mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "output",
                "Output",
                Some("Output"),
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn settings_from_params(params: &ParameterSet) -> Result<NativeCabSettings> {
    Ok(NativeCabSettings {
        low_cut_hz: required_f32(params, "low_cut_hz").map_err(anyhow::Error::msg)?,
        high_cut_hz: required_f32(params, "high_cut_hz").map_err(anyhow::Error::msg)?,
        resonance: required_f32(params, "resonance").map_err(anyhow::Error::msg)?,
        air: required_f32(params, "air").map_err(anyhow::Error::msg)?,
        mic_position: required_f32(params, "mic_position").map_err(anyhow::Error::msg)?,
        mic_distance: required_f32(params, "mic_distance").map_err(anyhow::Error::msg)?,
        room_mix: required_f32(params, "room_mix").map_err(anyhow::Error::msg)?,
        output: required_f32(params, "output").map_err(anyhow::Error::msg)?,
    })
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = settings_from_params(params)?;
    Ok(())
}

pub fn asset_summary(model_id: &'static str, _params: &ParameterSet) -> Result<String> {
    Ok(format!("native voice='{model_id}'"))
}
