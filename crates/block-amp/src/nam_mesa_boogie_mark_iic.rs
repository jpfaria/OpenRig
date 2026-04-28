use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mesa_boogie_mark_iic";
pub const DISPLAY_NAME: &str = "Mark IIC";
const BRAND: &str = "mesa";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("mark_iic_hetfield_rhythm", "Mark IIC+ Hetfield Rhythm", "amps/mesa_boogie_mark_iic/mark_iic_hetfield_rhythm.nam"),
    ("mark_iic_fast_lead", "Mark IIC+ Fast Lead", "amps/mesa_boogie_mark_iic/mark_iic_fast_lead.nam"),
    ("mark_iic_creamy_lead", "Mark IIC+ Creamy Lead", "amps/mesa_boogie_mark_iic/mark_iic_creamy_lead.nam"),
    ("mark_iic_tight_rhythm", "Mark IIC+ Tight Rhythm", "amps/mesa_boogie_mark_iic/mark_iic_tight_rhythm.nam"),
    ("mark_iic_phat_rhythm", "Mark IIC+ Phat Rhythm", "amps/mesa_boogie_mark_iic/mark_iic_phat_rhythm.nam"),
    ("mark_iic_yummy_clean", "Mark IIC+ Yummy Clean", "amps/mesa_boogie_mark_iic/mark_iic_yummy_clean.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("mark_iic_hetfield_rhythm"),
        &[
            ("mark_iic_hetfield_rhythm", "Mark IIC+ Hetfield Rhythm"),
            ("mark_iic_fast_lead", "Mark IIC+ Fast Lead"),
            ("mark_iic_creamy_lead", "Mark IIC+ Creamy Lead"),
            ("mark_iic_tight_rhythm", "Mark IIC+ Tight Rhythm"),
            ("mark_iic_phat_rhythm", "Mark IIC+ Phat Rhythm"),
            ("mark_iic_yummy_clean", "Mark IIC+ Yummy Clean"),
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
