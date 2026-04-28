use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_way_huge_pork_pickle";
pub const DISPLAY_NAME: &str = "Way Huge Pork Pickle";
const BRAND: &str = "way_huge";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Three-axis pack: voice × drive × blend.
// 2 voicings (Pork Loin / Swollen Brine) × 3 drive steps × 2 blend states = 12.
const CAPTURES: &[(&str, &str, &str, &str)] = &[
    // (voice, drive, blend, file)
    ("pork", "low",  "no",   "pedals/way_huge_pork_pickle/pork_low_drive_no_blend.nam"),
    ("pork", "low",  "with", "pedals/way_huge_pork_pickle/pork_low_drive_with_blend.nam"),
    ("pork", "mid",  "no",   "pedals/way_huge_pork_pickle/pork_mid_drive_no_blend.nam"),
    ("pork", "mid",  "with", "pedals/way_huge_pork_pickle/pork_mid_drive_with_blend.nam"),
    ("pork", "high", "no",   "pedals/way_huge_pork_pickle/pork_high_drive_no_blend.nam"),
    ("pork", "high", "with", "pedals/way_huge_pork_pickle/pork_high_drive_with_blend.nam"),
    ("brine","low",  "no",   "pedals/way_huge_pork_pickle/pickle_low_drive_no_blend.nam"),
    ("brine","low",  "with", "pedals/way_huge_pork_pickle/pickle_low_drive_with_blend.nam"),
    ("brine","mid",  "no",   "pedals/way_huge_pork_pickle/pickle_mid_drive_no_blend.nam"),
    ("brine","mid",  "with", "pedals/way_huge_pork_pickle/pickle_mid_drive_with_blend.nam"),
    ("brine","high", "no",   "pedals/way_huge_pork_pickle/pickle_high_drive_no_blend.nam"),
    ("brine","high", "with", "pedals/way_huge_pork_pickle/pickle_high_drive_with_blend.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voice",
            "Voice",
            Some("Pedal"),
            Some("pork"),
            &[
                ("pork",  "Pork Loin"),
                ("brine", "Swollen Brine"),
            ],
        ),
        enum_parameter(
            "drive",
            "Drive",
            Some("Pedal"),
            Some("mid"),
            &[
                ("low",  "Low"),
                ("mid",  "Mid"),
                ("high", "High"),
            ],
        ),
        enum_parameter(
            "blend",
            "Clean Blend",
            Some("Pedal"),
            Some("with"),
            &[
                ("no",   "Off"),
                ("with", "On"),
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
    let voice = required_string(params, "voice").map_err(anyhow::Error::msg)?;
    let drive = required_string(params, "drive").map_err(anyhow::Error::msg)?;
    let blend = required_string(params, "blend").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, d, b, _)| *v == voice && *d == drive && *b == blend)
        .map(|(_, _, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for voice={} drive={} blend={}",
                MODEL_ID, voice, drive, blend
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
