use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_fender_showman";
pub const DISPLAY_NAME: &str = "Showman";
const BRAND: &str = "fender";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("fuller_tone_comp_clean_std", "Fuller Tone Comp Clean std", "amps/fender_showman/fuller_tone_comp_clean_std_2.nam"),
    ("fuller_tone_metal_std", "Fuller Tone Metal std", "amps/fender_showman/fuller_tone_metal_std_2.nam"),
    ("dweezil_s_bassguy_ola_cab_custom", "Dweezil's Bassguy OLA cab", "amps/fender_showman/dweezil_s_bassguy_ola_cab_custom_2.nam"),
    ("dweezil_s_bassguy_fuzz_1_custom", "Dweezil's Bassguy Fuzz 1", "amps/fender_showman/dweezil_s_bassguy_fuzz_1_custom_2.nam"),
    ("jonesy_s_pretty_dark_custom", "Jonesy's Pretty Dark", "amps/fender_showman/jonesy_s_pretty_dark_custom_2.nam"),
    ("super_6g4_comp_clean_custom", "Super 6G4 COMP clean", "amps/fender_showman/super_6g4_comp_clean_custom_2.nam"),
    ("super_brownie_country_ab_custom", "Super Brownie Country AB", "amps/fender_showman/super_brownie_country_ab_custom_2.nam"),
    ("super_verb_custom", "Super Verb", "amps/fender_showman/super_verb_custom_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("fuller_tone_comp_clean_std"),
        &[
            ("fuller_tone_comp_clean_std", "Fuller Tone Comp Clean std"),
            ("fuller_tone_metal_std", "Fuller Tone Metal std"),
            ("dweezil_s_bassguy_ola_cab_custom", "Dweezil's Bassguy OLA cab"),
            ("dweezil_s_bassguy_fuzz_1_custom", "Dweezil's Bassguy Fuzz 1"),
            ("jonesy_s_pretty_dark_custom", "Jonesy's Pretty Dark"),
            ("super_6g4_comp_clean_custom", "Super 6G4 COMP clean"),
            ("super_brownie_country_ab_custom", "Super Brownie Country AB"),
            ("super_verb_custom", "Super Verb"),
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
