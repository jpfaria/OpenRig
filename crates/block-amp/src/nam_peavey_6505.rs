use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_peavey_6505";
pub const DISPLAY_NAME: &str = "6505";
const BRAND: &str = "peavey";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("app_6505plus_clean_gain_06", "APP-6505Plus-Clean-Gain-06", "amps/peavey_6505/app_6505plus_clean_gain_06.nam"),
    ("app_6505plus_clean_gain_07", "APP-6505Plus-Clean-Gain-07", "amps/peavey_6505/app_6505plus_clean_gain_07.nam"),
    ("app_6505plus_clean_gain_08", "APP-6505Plus-Clean-Gain-08", "amps/peavey_6505/app_6505plus_clean_gain_08.nam"),
    ("app_6505plus_clean_gain_02", "APP-6505Plus-Clean-Gain-02", "amps/peavey_6505/app_6505plus_clean_gain_02.nam"),
    ("app_6505plus_midforward_gain_07", "APP-6505Plus-MidForward-Gain-07", "amps/peavey_6505/app_6505plus_midforward_gain_07.nam"),
    ("app_6505plus_scooped_gain_06", "APP-6505Plus-Scooped-Gain-06", "amps/peavey_6505/app_6505plus_scooped_gain_06.nam"),
    ("app_6505plus_clean_gain_09", "APP-6505Plus-Clean-Gain-09", "amps/peavey_6505/app_6505plus_clean_gain_09.nam"),
    ("app_6505plus_clean_gain_04", "APP-6505Plus-Clean-Gain-04", "amps/peavey_6505/app_6505plus_clean_gain_04.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("app_6505plus_clean_gain_06"),
        &[
            ("app_6505plus_clean_gain_06", "APP-6505Plus-Clean-Gain-06"),
            ("app_6505plus_clean_gain_07", "APP-6505Plus-Clean-Gain-07"),
            ("app_6505plus_clean_gain_08", "APP-6505Plus-Clean-Gain-08"),
            ("app_6505plus_clean_gain_02", "APP-6505Plus-Clean-Gain-02"),
            ("app_6505plus_midforward_gain_07", "APP-6505Plus-MidForward-Gain-07"),
            ("app_6505plus_scooped_gain_06", "APP-6505Plus-Scooped-Gain-06"),
            ("app_6505plus_clean_gain_09", "APP-6505Plus-Clean-Gain-09"),
            ("app_6505plus_clean_gain_04", "APP-6505Plus-Clean-Gain-04"),
        ],
    )];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let path = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let key = required_string(params, "capture").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("amp '{}' has no capture '{}'", MODEL_ID, key))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: AmpModelDefinition = AmpModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: AmpBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let path = resolve_capture(params)?;
    Ok(format!("model='{}'", path))
}
