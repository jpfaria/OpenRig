use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mesa_triple_rectifier";
pub const DISPLAY_NAME: &str = "Triple Rectifier";
const BRAND: &str = "mesa";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("pow_trec_150bld_dio_mdn_6l6_juice_01_sm5", "[POW] TREC-150BLD-DIO-MDN 6L6 Juice #01 - SM57", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_mdn_6l6_juice_01_sm57_2.nam"),
    ("pow_trec_150bld_dio_mdn_6l6_juice_01_di", "[POW] TREC-150BLD-DIO-MDN 6L6 Juice #01 - DI", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_mdn_6l6_juice_01_di_2.nam"),
    ("pow_trec_150bld_dio_mdn_6l6_juice_01_ble", "[POW] TREC-150BLD-DIO-MDN 6L6 Juice #01 - BLEND #2", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_mdn_6l6_juice_01_blend_2_2.nam"),
    ("pow_trec_150bld_dio_vnt_6l6_juice_36_di", "[POW] TREC-150BLD-DIO-VNT 6L6 Juice #36 - DI", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_vnt_6l6_juice_36_di_2.nam"),
    ("amp_trec_150bld_dio_vnt_chaosball_di", "[AMP] TREC-150BLD-DIO-VNT Chaosball - DI", "amps/mesa_triple_rectifier/amp_trec_150bld_dio_vnt_chaosball_di_2.nam"),
    ("pow_trec_150bld_dio_mdn_6l6_juice_01_ble_292599", "[POW] TREC-150BLD-DIO-MDN 6L6 Juice #01 - BLEND #3", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_mdn_6l6_juice_01_blend_3_2.nam"),
    ("pow_trec_150bld_dio_mdn_6l6_juice_01_sm5_292616", "[POW] TREC-150BLD-DIO-MDN 6L6 Juice #01 - SM58", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_mdn_6l6_juice_01_sm58_2.nam"),
    ("pow_trec_150bld_dio_vnt_6l6_juice_36_ble", "[POW] TREC-150BLD-DIO-VNT 6L6 Juice #36 - BLEND #1", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_vnt_6l6_juice_36_blend_1_2.nam"),
    ("pow_trec_150bld_dio_vnt_6l6_juice_36_ble_292761", "[POW] TREC-150BLD-DIO-VNT 6L6 Juice #36 - BLEND #2", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_vnt_6l6_juice_36_blend_2_2.nam"),
    ("pow_trec_150bld_dio_vnt_6l6_juice_36_ble_292779", "[POW] TREC-150BLD-DIO-VNT 6L6 Juice #36 - BLEND #3", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_vnt_6l6_juice_36_blend_3_2.nam"),
    ("pow_trec_150bld_dio_vnt_6l6_juice_36_sm5", "[POW] TREC-150BLD-DIO-VNT 6L6 Juice #36 - SM57", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_vnt_6l6_juice_36_sm57_2.nam"),
    ("pow_trec_150bld_dio_vnt_6l6_juice_36_sm5_292775", "[POW] TREC-150BLD-DIO-VNT 6L6 Juice #36 - SM58", "amps/mesa_triple_rectifier/pow_trec_150bld_dio_vnt_6l6_juice_36_sm58_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("pow_trec_150bld_dio_mdn_6l6_juice_01_sm5"),
        &[
            ("pow_trec_150bld_dio_mdn_6l6_juice_01_sm5", "[POW] TREC-150BLD-DIO-MDN 6L6 Juice #01 - SM57"),
            ("pow_trec_150bld_dio_mdn_6l6_juice_01_di", "[POW] TREC-150BLD-DIO-MDN 6L6 Juice #01 - DI"),
            ("pow_trec_150bld_dio_mdn_6l6_juice_01_ble", "[POW] TREC-150BLD-DIO-MDN 6L6 Juice #01 - BLEND #2"),
            ("pow_trec_150bld_dio_vnt_6l6_juice_36_di", "[POW] TREC-150BLD-DIO-VNT 6L6 Juice #36 - DI"),
            ("amp_trec_150bld_dio_vnt_chaosball_di", "[AMP] TREC-150BLD-DIO-VNT Chaosball - DI"),
            ("pow_trec_150bld_dio_mdn_6l6_juice_01_ble_292599", "[POW] TREC-150BLD-DIO-MDN 6L6 Juice #01 - BLEND #3"),
            ("pow_trec_150bld_dio_mdn_6l6_juice_01_sm5_292616", "[POW] TREC-150BLD-DIO-MDN 6L6 Juice #01 - SM58"),
            ("pow_trec_150bld_dio_vnt_6l6_juice_36_ble", "[POW] TREC-150BLD-DIO-VNT 6L6 Juice #36 - BLEND #1"),
            ("pow_trec_150bld_dio_vnt_6l6_juice_36_ble_292761", "[POW] TREC-150BLD-DIO-VNT 6L6 Juice #36 - BLEND #2"),
            ("pow_trec_150bld_dio_vnt_6l6_juice_36_ble_292779", "[POW] TREC-150BLD-DIO-VNT 6L6 Juice #36 - BLEND #3"),
            ("pow_trec_150bld_dio_vnt_6l6_juice_36_sm5", "[POW] TREC-150BLD-DIO-VNT 6L6 Juice #36 - SM57"),
            ("pow_trec_150bld_dio_vnt_6l6_juice_36_sm5_292775", "[POW] TREC-150BLD-DIO-VNT 6L6 Juice #36 - SM58"),
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
