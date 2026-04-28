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

// Two-axis pack: voicing × boost pedal.
// Holes (e.g. mid voicing has no boost variants) return Err.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (voicing, boost, file)
    ("default",     "none",            "amps/engl_fireball/engl_fireball.nam"),
    ("default",     "ts9",             "amps/engl_fireball/engl_fireball_ts9.nam"),
    ("default",     "ts808",           "amps/engl_fireball/engl_fireball_ts808.nam"),
    ("default",     "line_driver",     "amps/engl_fireball/engl_fireball_line_driver.nam"),
    ("default",     "precision_drive", "amps/engl_fireball/engl_fireball_precision_drive_3.nam"),
    ("mid",         "none",            "amps/engl_fireball/engl_fireball_mid.nam"),
    ("presence_0",  "none",            "amps/engl_fireball/engl_fireball_presence_0.nam"),
    ("presence_9h", "none",            "amps/engl_fireball/engl_fireball_presence_9h.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Amp"),
            Some("default"),
            &[
                ("default",     "Default"),
                ("mid",         "Mid"),
                ("presence_0",  "Presence 0"),
                ("presence_9h", "Presence 9h"),
            ],
        ),
        enum_parameter(
            "boost",
            "Boost",
            Some("Amp"),
            Some("none"),
            &[
                ("none",            "None"),
                ("ts9",             "TS9"),
                ("ts808",           "TS808"),
                ("line_driver",     "Line Driver"),
                ("precision_drive", "Precision Drive"),
            ],
        ),
    ];
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
    let voicing = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    let boost = required_string(params, "boost").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, b, _)| *v == voicing && *b == boost)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for voicing={} boost={}",
                MODEL_ID, voicing, boost
            )
        })
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
