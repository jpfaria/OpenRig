use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_boss_mt_2_metal_zone";
pub const DISPLAY_NAME: &str = "Boss MT-2 Metal Zone";
const BRAND: &str = "boss";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "5_battery_powered", model_path: "pedals/boss_mt_2_metal_zone/boss_metalzone_clean_boost_5_battery_powered.nam" },
    NamCapture { tone: "6_battery_powered", model_path: "pedals/boss_mt_2_metal_zone/boss_metalzone_clean_boost_6_battery_powered.nam" },
    NamCapture { tone: "7_battery_powered", model_path: "pedals/boss_mt_2_metal_zone/boss_metalzone_clean_boost_7_battery_powered.nam" },
    NamCapture { tone: "8_battery_powered", model_path: "pedals/boss_mt_2_metal_zone/boss_metalzone_clean_boost_8_battery_powered.nam" },
    NamCapture { tone: "setting",           model_path: "pedals/boss_mt_2_metal_zone/boss_metalzone_clean_boost_setting.nam" },
    NamCapture { tone: "setting_2",         model_path: "pedals/boss_mt_2_metal_zone/boss_metalzone_clean_boost_setting_2.nam" },
    NamCapture { tone: "setting_3",         model_path: "pedals/boss_mt_2_metal_zone/boss_metalzone_clean_boost_setting_3.nam" },
    NamCapture { tone: "setting_4",         model_path: "pedals/boss_mt_2_metal_zone/boss_metalzone_clean_boost_setting_4.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("5_battery_powered"),
        &[
            ("5_battery_powered", "5 Battery Powered"),
            ("6_battery_powered", "6 Battery Powered"),
            ("7_battery_powered", "7 Battery Powered"),
            ("8_battery_powered", "8 Battery Powered"),
            ("setting",           "Setting"),
            ("setting_2",         "Setting 2"),
            ("setting_3",         "Setting 3"),
            ("setting_4",         "Setting 4"),
        ],
    )];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let capture = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(capture.model_path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("model='{}'", capture.model_path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static NamCapture> {
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.tone == tone)
        .ok_or_else(|| anyhow!("gain model '{}' does not support tone='{}'", MODEL_ID, tone))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
