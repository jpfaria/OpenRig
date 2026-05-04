use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_laney_ironheart_irt60h";
pub const DISPLAY_NAME: &str = "Ironheart IRT60H";
const BRAND: &str = "laney";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: stage × boost. All Lead channel.
// "custom_eq" is a single curated EQ (B1.8/M-1/T2 G10 D60 TN1).
const CAPTURES: &[(&str, &str, &str)] = &[
    // (stage, boost, file)
    ("preamp",    "none",    "amps/laney_ironheart_irt60h/lead_preamp.nam"),
    ("preamp",    "boosted", "amps/laney_ironheart_irt60h/lead_boosted_preamp.nam"),
    ("preamp",    "custom",  "amps/laney_ironheart_irt60h/laney_irt60_lead_pre_b1_8_m_1_t2_g10_d60_tn1.nam"),
    ("power_amp", "none",    "amps/laney_ironheart_irt60h/lead_poweramp.nam"),
    ("power_amp", "boosted", "amps/laney_ironheart_irt60h/lead_boosted_poweramp.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "stage",
            "Stage",
            Some("Amp"),
            Some("preamp"),
            &[
                ("preamp",    "Preamp"),
                ("power_amp", "Power Amp"),
            ],
        ),
        enum_parameter(
            "boost",
            "Boost",
            Some("Amp"),
            Some("none"),
            &[
                ("none",    "None"),
                ("boosted", "Boosted"),
                ("custom",  "Custom EQ"),
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
    let stage = required_string(params, "stage").map_err(anyhow::Error::msg)?;
    let boost = required_string(params, "boost").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(s, b, _)| *s == stage && *b == boost)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for stage={} boost={}",
                MODEL_ID, stage, boost
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
