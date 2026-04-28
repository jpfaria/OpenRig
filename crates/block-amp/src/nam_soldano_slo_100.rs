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

const CAPTURES: &[(&str, &str, &str)] = &[
    ("pre_slo100_ovd_lucky_7", "[PRE] SLO100-OVD Lucky 7", "amps/soldano_slo_100/pre_slo100_ovd_lucky_7.nam"),
    ("pow_slo100_6l6_juice_13_sm58", "[POW] SLO100 6L6 Juice #13 - SM58", "amps/soldano_slo_100/pow_slo100_6l6_juice_13_sm58.nam"),
    ("amp_slo100_nrm_crn_thing_of_joy_01_sm57", "[AMP] SLO100-NRM-CRN Thing of Joy #01 - SM57", "amps/soldano_slo_100/amp_slo100_nrm_crn_thing_of_joy_01_sm57.nam"),
    ("amp_slo100_nrm_crn_thing_of_joy_01_sm58", "[AMP] SLO100-NRM-CRN Thing of Joy #01 - SM58", "amps/soldano_slo_100/amp_slo100_nrm_crn_thing_of_joy_01_sm58.nam"),
    ("amp_slo100_ovd_the_king_sm58", "[AMP] SLO100-OVD The King - SM58", "amps/soldano_slo_100/amp_slo100_ovd_the_king_sm58.nam"),
    ("pow_slo100_6l6_juice_13_blend_1", "[POW] SLO100 6L6 Juice #13 - BLEND #1", "amps/soldano_slo_100/pow_slo100_6l6_juice_13_blend_1.nam"),
    ("amp_slo100_nrm_cln_journeyvan_di", "[AMP] SLO100-NRM-CLN Journeyvan - DI", "amps/soldano_slo_100/amp_slo100_nrm_cln_journeyvan_di.nam"),
    ("pow_slo100_6l6_juice_13_blend_3", "[POW] SLO100 6L6 Juice #13 - BLEND #3", "amps/soldano_slo_100/pow_slo100_6l6_juice_13_blend_3.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("pre_slo100_ovd_lucky_7"),
        &[
            ("pre_slo100_ovd_lucky_7", "[PRE] SLO100-OVD Lucky 7"),
            ("pow_slo100_6l6_juice_13_sm58", "[POW] SLO100 6L6 Juice #13 - SM58"),
            ("amp_slo100_nrm_crn_thing_of_joy_01_sm57", "[AMP] SLO100-NRM-CRN Thing of Joy #01 - SM57"),
            ("amp_slo100_nrm_crn_thing_of_joy_01_sm58", "[AMP] SLO100-NRM-CRN Thing of Joy #01 - SM58"),
            ("amp_slo100_ovd_the_king_sm58", "[AMP] SLO100-OVD The King - SM58"),
            ("pow_slo100_6l6_juice_13_blend_1", "[POW] SLO100 6L6 Juice #13 - BLEND #1"),
            ("amp_slo100_nrm_cln_journeyvan_di", "[AMP] SLO100-NRM-CLN Journeyvan - DI"),
            ("pow_slo100_6l6_juice_13_blend_3", "[POW] SLO100 6L6 Juice #13 - BLEND #3"),
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
