use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_boss_od_3";
pub const DISPLAY_NAME: &str = "Boss OD-3";
const BRAND: &str = "boss";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Three-axis pack: drive × tone × size (model resolution).
// 4 × 3 × 2 = 24 captures, full grid.
const CAPTURES: &[(&str, &str, &str, &str)] = &[
    // (drive, tone, size, file)
    ("no",  "low",    "standard", "pedals/boss_od_3/bossod3_nodrive_lowtone.nam"),
    ("no",  "low",    "nano",     "pedals/boss_od_3/bossod3_nodrive_lowtone_nano.nam"),
    ("no",  "center", "standard", "pedals/boss_od_3/bossod3_nodrive_centertone.nam"),
    ("no",  "center", "nano",     "pedals/boss_od_3/bossod3_nodrive_centertone_nano.nam"),
    ("no",  "high",   "standard", "pedals/boss_od_3/bossod3_nodrive_hightone.nam"),
    ("no",  "high",   "nano",     "pedals/boss_od_3/bossod3_nodrive_hightone_nano.nam"),
    ("low", "low",    "standard", "pedals/boss_od_3/bossod3_lowdrive_lowtone.nam"),
    ("low", "low",    "nano",     "pedals/boss_od_3/bossod3_lowdrive_lowtone_nano.nam"),
    ("low", "center", "standard", "pedals/boss_od_3/bossod3_lowdrive_centertone.nam"),
    ("low", "center", "nano",     "pedals/boss_od_3/bossod3_lowdrive_centertone_nano.nam"),
    ("low", "high",   "standard", "pedals/boss_od_3/bossod3_lowdrive_hightone.nam"),
    ("low", "high",   "nano",     "pedals/boss_od_3/bossod3_lowdrive_hightone_nano.nam"),
    ("mid", "low",    "standard", "pedals/boss_od_3/bossod3_middrive_lowtone.nam"),
    ("mid", "low",    "nano",     "pedals/boss_od_3/bossod3_middrive_lowtone_nano.nam"),
    ("mid", "center", "standard", "pedals/boss_od_3/bossod3_middrive_centertone.nam"),
    ("mid", "center", "nano",     "pedals/boss_od_3/bossod3_middrive_centertone_nano.nam"),
    ("mid", "high",   "standard", "pedals/boss_od_3/bossod3_middrive_hightone.nam"),
    ("mid", "high",   "nano",     "pedals/boss_od_3/bossod3_middrive_hightone_nano.nam"),
    ("hi",  "low",    "standard", "pedals/boss_od_3/bossod3_hidrive_lowtone.nam"),
    ("hi",  "low",    "nano",     "pedals/boss_od_3/bossod3_hidrive_lowtone_nano.nam"),
    ("hi",  "center", "standard", "pedals/boss_od_3/bossod3_hidrive_centertone.nam"),
    ("hi",  "center", "nano",     "pedals/boss_od_3/bossod3_hidrive_centertone_nano.nam"),
    ("hi",  "high",   "standard", "pedals/boss_od_3/bossod3_hidrive_hightone.nam"),
    ("hi",  "high",   "nano",     "pedals/boss_od_3/bossod3_hidrive_hightone_nano.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "drive",
            "Drive",
            Some("Pedal"),
            Some("mid"),
            &[
                ("no",  "Off"),
                ("low", "Low"),
                ("mid", "Mid"),
                ("hi",  "High"),
            ],
        ),
        enum_parameter(
            "tone",
            "Tone",
            Some("Pedal"),
            Some("center"),
            &[
                ("low",    "Low"),
                ("center", "Center"),
                ("high",   "High"),
            ],
        ),
        enum_parameter(
            "size",
            "Model Size",
            Some("Asset"),
            Some("standard"),
            &[
                ("standard", "Standard"),
                ("nano",     "Nano"),
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

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let path = resolve_capture(params)?;
    Ok(format!("model='{}'", path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let drive = required_string(params, "drive").map_err(anyhow::Error::msg)?;
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    let size = required_string(params, "size").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(d, t, s, _)| *d == drive && *t == tone && *s == size)
        .map(|(_, _, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for drive={} tone={} size={}",
                MODEL_ID, drive, tone, size
            )
        })
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
