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

const CAPTURES: &[(&str, &str, &str)] = &[
    ("bogner_goldfinger_clean", "Bogner Goldfinger - Clean", "amps/bogner_goldfinger/bogner_goldfinger_clean.nam"),
    ("bogner_goldfinger_discusting_mid", "Bogner Goldfinger - discusting midgain", "amps/bogner_goldfinger/bogner_goldfinger_discusting_midgain.nam"),
    ("bogner_goldfinger_crunch", "Bogner Goldfinger - crunch+", "amps/bogner_goldfinger/bogner_goldfinger_crunch.nam"),
    ("bogner_goldfinger_higain", "Bogner Goldfinger - higain", "amps/bogner_goldfinger/bogner_goldfinger_higain.nam"),
    ("bogner_goldfinger_higain_with_ir", "Bogner Goldfinger - higain+ (with ir, sorry, my falt)", "amps/bogner_goldfinger/bogner_goldfinger_higain_with_ir_sorry_my_falt.nam"),
    ("bogner_goldfinger_clean_4db_10k", "Bogner Goldfinger - clean (-4db 10k)", "amps/bogner_goldfinger/bogner_goldfinger_clean_4db_10k.nam"),
    ("bogner_goldfinger_crunch_old_ver", "Bogner Goldfinger - crunch+ (old version, but its ok)", "amps/bogner_goldfinger/bogner_goldfinger_crunch_old_version_but_its_ok.nam"),
    ("bogner_goldfinger_higain_4db_10k", "Bogner Goldfinger - higain (-4db 10k)", "amps/bogner_goldfinger/bogner_goldfinger_higain_4db_10k.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("bogner_goldfinger_clean"),
        &[
            ("bogner_goldfinger_clean", "Bogner Goldfinger - Clean"),
            ("bogner_goldfinger_discusting_mid", "Bogner Goldfinger - discusting midgain"),
            ("bogner_goldfinger_crunch", "Bogner Goldfinger - crunch+"),
            ("bogner_goldfinger_higain", "Bogner Goldfinger - higain"),
            ("bogner_goldfinger_higain_with_ir", "Bogner Goldfinger - higain+ (with ir, sorry, my falt)"),
            ("bogner_goldfinger_clean_4db_10k", "Bogner Goldfinger - clean (-4db 10k)"),
            ("bogner_goldfinger_crunch_old_ver", "Bogner Goldfinger - crunch+ (old version, but its ok)"),
            ("bogner_goldfinger_higain_4db_10k", "Bogner Goldfinger - higain (-4db 10k)"),
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
