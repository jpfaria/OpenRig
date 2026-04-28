use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_splawn_quickrod";
pub const DISPLAY_NAME: &str = "Quickrod";
const BRAND: &str = "splawn";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Three-axis pack: voicing × gain × mids. Sparse — only 7 of the 3×2×2 = 12
// combinations were captured. resolve_capture rejects holes so the UI keeps
// the three knobs independent.
const CAPTURES: &[(&str, &str, &str, &str)] = &[
    // (voicing, gain, mids, file)
    ("normal", "3", "3", "amps/splawn_quickrod/splawn3g7m_mids_3.nam"),
    ("normal", "3", "5", "amps/splawn_quickrod/splawn3g7m_mids_5.nam"),
    ("normal", "5", "5", "amps/splawn_quickrod/splawn5g7m_mids_5.nam"),
    ("hg",     "3", "5", "amps/splawn_quickrod/splawn_hg3g7m_mids5.nam"),
    ("hg",     "5", "5", "amps/splawn_quickrod/splawn_hg5g7m_mids5.nam"),
    ("hgt",    "3", "5", "amps/splawn_quickrod/splawnhgt3g7m.nam"),
    ("hgt",    "5", "5", "amps/splawn_quickrod/splawnhgt5g7m.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Amp"),
            Some("normal"),
            &[
                ("normal", "Normal"),
                ("hg",     "Hot Gain"),
                ("hgt",    "Hot Gain Tight"),
            ],
        ),
        enum_parameter(
            "gain",
            "Gain",
            Some("Amp"),
            Some("3"),
            &[
                ("3", "3"),
                ("5", "5"),
            ],
        ),
        enum_parameter(
            "mids",
            "Mids",
            Some("Amp"),
            Some("3"),
            &[
                ("3", "3"),
                ("5", "5"),
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
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    let mids = required_string(params, "mids").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, g, m, _)| *v == voicing && *g == gain && *m == mids)
        .map(|(_, _, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for voicing={} gain={} mids={}",
                MODEL_ID, voicing, gain, mids
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
