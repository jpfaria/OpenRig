use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_engl_ironball";
pub const DISPLAY_NAME: &str = "Ironball";
const BRAND: &str = "engl";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: voicing × usage.
// All captures are from the SLAMMIN ENGL Iron Lead pack. Holes return Err.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (voicing, usage, file)
    ("scoop",       "solo",   "amps/engl_ironball/slammin_engl_iron_lead_scoop_solo_s.nam"),
    ("balanced",    "rhythm", "amps/engl_ironball/slammin_engl_iron_lead_balanced_rhythm_s.nam"),
    ("bright",      "rhythm", "amps/engl_ironball/slammin_engl_iron_lead_bright_rhythm_s.nam"),
    ("bright",      "solo",   "amps/engl_ironball/slammin_engl_iron_lead_bright_solo_s.nam"),
    ("chunky",      "rhythm", "amps/engl_ironball/slammin_engl_iron_lead_chunky_rhythm_s.nam"),
    ("slight_scoop","rhythm", "amps/engl_ironball/slammin_engl_iron_lead_slightscoop_rhythm_s.nam"),
    ("mids",        "solo",   "amps/engl_ironball/slammin_engl_iron_lead_mids_solo_s.nam"),
    ("chunky",      "solo",   "amps/engl_ironball/slammin_engl_iron_lead_chunky_solo_s.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Amp"),
            Some("balanced"),
            &[
                ("balanced",     "Balanced"),
                ("bright",       "Bright"),
                ("chunky",       "Chunky"),
                ("mids",         "Mids"),
                ("scoop",        "Scoop"),
                ("slight_scoop", "Slight Scoop"),
            ],
        ),
        enum_parameter(
            "usage",
            "Usage",
            Some("Amp"),
            Some("rhythm"),
            &[
                ("rhythm", "Rhythm"),
                ("solo",   "Solo"),
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
    let usage = required_string(params, "usage").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, u, _)| *v == voicing && *u == usage)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for voicing={} usage={}",
                MODEL_ID, voicing, usage
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
