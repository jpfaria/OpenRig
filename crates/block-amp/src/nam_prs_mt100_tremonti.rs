use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_prs_mt100_tremonti";
pub const DISPLAY_NAME: &str = "MT-100 Tremonti";
const BRAND: &str = "prs";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("overdrive_dime_it_sm57", "[AMP] PRS-MT100 OVERDRIVE Dime it SM57", "amps/prs_mt100_tremonti/amp_prs_mt100_overdrive_dime_it_sm57_2.nam"),
    ("clean_live_2023_di_std", "[AMP] PRS-MT100 CLEAN Live-2023 DI - STD", "amps/prs_mt100_tremonti/amp_prs_mt100_clean_live_2023_di_std_2.nam"),
    ("overdrive_bitey_di_std", "[AMP] PRS-MT100 OVERDRIVE Bitey DI - STD", "amps/prs_mt100_tremonti/amp_prs_mt100_overdrive_bitey_di_std_2.nam"),
    ("overdrive_sin_after_sin_sm57_std", "[AMP] PRS-MT100 OVERDRIVE Sin after Sin SM57 - STD", "amps/prs_mt100_tremonti/amp_prs_mt100_overdrive_sin_after_sin_sm57_std_2.nam"),
    ("overdrive_sin_after_sin_di_std", "[AMP] PRS-MT100 OVERDRIVE Sin after Sin DI - STD", "amps/prs_mt100_tremonti/amp_prs_mt100_overdrive_sin_after_sin_di_std_2.nam"),
    ("clean_cleanly_di_std", "[AMP] PRS-MT100 CLEAN Cleanly DI - STD", "amps/prs_mt100_tremonti/amp_prs_mt100_clean_cleanly_di_std_2.nam"),
    ("overdrive_single_coil_leads_sm57_std", "[AMP] PRS-MT100 OVERDRIVE Single Coil Leads SM57 - STD", "amps/prs_mt100_tremonti/amp_prs_mt100_overdrive_single_coil_leads_sm57_std_2.nam"),
    ("clean_live_2023_sm57_std", "[AMP] PRS-MT100 CLEAN Live-2023 SM57 - STD", "amps/prs_mt100_tremonti/amp_prs_mt100_clean_live_2023_sm57_std_2.nam"),
    ("clean_noon_sm57_std", "[AMP] PRS-MT100 CLEAN Noon SM57 - STD", "amps/prs_mt100_tremonti/amp_prs_mt100_clean_noon_sm57_std_2.nam"),
    ("clean_noon_di_std", "[AMP] PRS-MT100 CLEAN Noon DI - STD", "amps/prs_mt100_tremonti/amp_prs_mt100_clean_noon_di_std_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("overdrive_dime_it_sm57"),
        &[
            ("overdrive_dime_it_sm57", "[AMP] PRS-MT100 OVERDRIVE Dime it SM57"),
            ("clean_live_2023_di_std", "[AMP] PRS-MT100 CLEAN Live-2023 DI - STD"),
            ("overdrive_bitey_di_std", "[AMP] PRS-MT100 OVERDRIVE Bitey DI - STD"),
            ("overdrive_sin_after_sin_sm57_std", "[AMP] PRS-MT100 OVERDRIVE Sin after Sin SM57 - STD"),
            ("overdrive_sin_after_sin_di_std", "[AMP] PRS-MT100 OVERDRIVE Sin after Sin DI - STD"),
            ("clean_cleanly_di_std", "[AMP] PRS-MT100 CLEAN Cleanly DI - STD"),
            ("overdrive_single_coil_leads_sm57_std", "[AMP] PRS-MT100 OVERDRIVE Single Coil Leads SM57 - STD"),
            ("clean_live_2023_sm57_std", "[AMP] PRS-MT100 CLEAN Live-2023 SM57 - STD"),
            ("clean_noon_sm57_std", "[AMP] PRS-MT100 CLEAN Noon SM57 - STD"),
            ("clean_noon_di_std", "[AMP] PRS-MT100 CLEAN Noon DI - STD"),
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
