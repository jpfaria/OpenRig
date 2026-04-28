use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_soldano_slo_100";
pub const DISPLAY_NAME: &str = "SLO 100";
const BRAND: &str = "soldano";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: voicing × mic.
// Voicings cover Normal Clean, Normal Crunch, Overdrive variants and
// the 6L6 power amp captures.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (voicing, mic, file)
    ("nrm_clean_journeyvan", "di",     "amps/soldano_slo_100/amp_slo100_nrm_cln_journeyvan_di.nam"),
    ("nrm_crunch_thingofjoy","sm57",   "amps/soldano_slo_100/amp_slo100_nrm_crn_thing_of_joy_01_sm57.nam"),
    ("nrm_crunch_thingofjoy","sm58",   "amps/soldano_slo_100/amp_slo100_nrm_crn_thing_of_joy_01_sm58.nam"),
    ("ovd_the_king",         "sm58",   "amps/soldano_slo_100/amp_slo100_ovd_the_king_sm58.nam"),
    ("ovd_lucky_7_pre",      "di",     "amps/soldano_slo_100/pre_slo100_ovd_lucky_7.nam"),
    ("pow_6l6_juice",        "sm58",   "amps/soldano_slo_100/pow_slo100_6l6_juice_13_sm58.nam"),
    ("pow_6l6_juice",        "blend1", "amps/soldano_slo_100/pow_slo100_6l6_juice_13_blend_1.nam"),
    ("pow_6l6_juice",        "blend3", "amps/soldano_slo_100/pow_slo100_6l6_juice_13_blend_3.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Amp"),
            Some("nrm_crunch_thingofjoy"),
            &[
                ("nrm_clean_journeyvan",  "Normal Clean"),
                ("nrm_crunch_thingofjoy", "Normal Crunch"),
                ("ovd_the_king",          "OD The King"),
                ("ovd_lucky_7_pre",       "OD Lucky 7 (Preamp)"),
                ("pow_6l6_juice",         "Power 6L6 Juice"),
            ],
        ),
        enum_parameter(
            "mic",
            "Mic",
            Some("Amp"),
            Some("sm57"),
            &[
                ("di",     "DI (No Cab)"),
                ("sm57",   "SM57"),
                ("sm58",   "SM58"),
                ("blend1", "Blend #1"),
                ("blend3", "Blend #3"),
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
    let mic = required_string(params, "mic").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, m, _)| *v == voicing && *m == mic)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for voicing={} mic={}",
                MODEL_ID, voicing, mic
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
