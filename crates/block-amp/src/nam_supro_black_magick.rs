use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_supro_black_magick";
pub const DISPLAY_NAME: &str = "Supro Black Magick";
const BRAND: &str = "supro";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("t_3_srl", "DI Supro 1695TJ In1-2 V1-5 V2-0 T-3 SRL", "amps/supro_black_magick/di_supro_1695tj_in1_2_v1_5_v2_0_t_3_srl_2.nam"),
    ("t_6_srl", "DI Supro 1695TJ In1-2 V1-5 V2-0 T-6 SRL", "amps/supro_black_magick/di_supro_1695tj_in1_2_v1_5_v2_0_t_6_srl_2.nam"),
    ("t_5_p12q", "DI Supro 1695TJ In1-2 V1-5 V2-0 T-5 P12Q", "amps/supro_black_magick/di_supro_1695tj_in1_2_v1_5_v2_0_t_5_p12q_2.nam"),
    ("t_4_srl", "DI Supro 1695TJ In1-2 V1-5 V2-0 T-4 SRL", "amps/supro_black_magick/di_supro_1695tj_in1_2_v1_5_v2_0_t_4_srl_2.nam"),
    ("t_5_srl", "DI Supro 1695TJ In1-2 V1-5 V2-0 T-5 SRL", "amps/supro_black_magick/di_supro_1695tj_in1_2_v1_5_v2_0_t_5_srl_2.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("t_3_srl"),
        &[
            ("t_3_srl", "DI Supro 1695TJ In1-2 V1-5 V2-0 T-3 SRL"),
            ("t_6_srl", "DI Supro 1695TJ In1-2 V1-5 V2-0 T-6 SRL"),
            ("t_5_p12q", "DI Supro 1695TJ In1-2 V1-5 V2-0 T-5 P12Q"),
            ("t_4_srl", "DI Supro 1695TJ In1-2 V1-5 V2-0 T-4 SRL"),
            ("t_5_srl", "DI Supro 1695TJ In1-2 V1-5 V2-0 T-5 SRL"),
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
