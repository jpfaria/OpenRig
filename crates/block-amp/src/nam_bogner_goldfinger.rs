use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_bogner_goldfinger";
pub const DISPLAY_NAME: &str = "Goldfinger";
const BRAND: &str = "bogner";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: gain × variant.
// Holes (e.g. midgain has no -4dB/IR/old variants) return Err so the UI
// surfaces validation directly to the user.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (gain, variant, file)
    ("clean",   "default",   "amps/bogner_goldfinger/bogner_goldfinger_clean.nam"),
    ("clean",   "minus_4db", "amps/bogner_goldfinger/bogner_goldfinger_clean_4db_10k.nam"),
    ("midgain", "default",   "amps/bogner_goldfinger/bogner_goldfinger_discusting_midgain.nam"),
    ("crunch",  "default",   "amps/bogner_goldfinger/bogner_goldfinger_crunch.nam"),
    ("crunch",  "old",       "amps/bogner_goldfinger/bogner_goldfinger_crunch_old_version_but_its_ok.nam"),
    ("higain",  "default",   "amps/bogner_goldfinger/bogner_goldfinger_higain.nam"),
    ("higain",  "minus_4db", "amps/bogner_goldfinger/bogner_goldfinger_higain_4db_10k.nam"),
    ("higain",  "with_ir",   "amps/bogner_goldfinger/bogner_goldfinger_higain_with_ir_sorry_my_falt.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "gain",
            "Gain",
            Some("Amp"),
            Some("clean"),
            &[
                ("clean",   "Clean"),
                ("midgain", "Mid Gain"),
                ("crunch",  "Crunch"),
                ("higain",  "Hi Gain"),
            ],
        ),
        enum_parameter(
            "variant",
            "Variant",
            Some("Amp"),
            Some("default"),
            &[
                ("default",   "Default"),
                ("minus_4db", "-4 dB / 10k Cut"),
                ("old",       "Old Version"),
                ("with_ir",   "With IR"),
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
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    let variant = required_string(params, "variant").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(g, v, _)| *g == gain && *v == variant)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for gain={} variant={}",
                MODEL_ID, gain, variant
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
