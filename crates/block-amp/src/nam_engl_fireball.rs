use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_engl_fireball";
pub const DISPLAY_NAME: &str = "Fireball";
const BRAND: &str = "engl";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("fireball_presence_0", "ENGL Fireball presence 0", "amps/engl_fireball/engl_fireball_presence_0_2.nam"),
    ("fireball_ts9", "ENGL Fireball+Ts9", "amps/engl_fireball/engl_fireball_ts9_2.nam"),
    ("fireball_line_driver", "ENGL Fireball +line driver", "amps/engl_fireball/engl_fireball_line_driver_2.nam"),
    ("fireball", "ENGL Fireball", "amps/engl_fireball/engl_fireball_2.nam"),
    ("fireball_mid", "ENGL Fireball mid", "amps/engl_fireball/engl_fireball_mid_2.nam"),
    ("fireball_ts808", "ENGL Fireball + Ts808", "amps/engl_fireball/engl_fireball_ts808_2.nam"),
    ("fireball_precision_drive_3", "ENGL Fireball+precision drive 3", "amps/engl_fireball/engl_fireball_precision_drive_3_2.nam"),
    ("fireball_presence_9h", "ENGL Fireball presence 9h", "amps/engl_fireball/engl_fireball_presence_9h_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("fireball_presence_0"),
        &[
            ("fireball_presence_0", "ENGL Fireball presence 0"),
            ("fireball_ts9", "ENGL Fireball+Ts9"),
            ("fireball_line_driver", "ENGL Fireball +line driver"),
            ("fireball", "ENGL Fireball"),
            ("fireball_mid", "ENGL Fireball mid"),
            ("fireball_ts808", "ENGL Fireball + Ts808"),
            ("fireball_precision_drive_3", "ENGL Fireball+precision drive 3"),
            ("fireball_presence_9h", "ENGL Fireball presence 9h"),
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
