use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_peavey_xxx";
pub const DISPLAY_NAME: &str = "XXX";
const BRAND: &str = "peavey";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("12_peavey_xxx_ch2_kt_77_unbooste", "12. Peavey XXX CH2 KT-77 UnBoosted B-HiGain", "amps/peavey_xxx/12_peavey_xxx_ch2_kt_77_unboosted_b_higain.nam"),
    ("28_peavey_xxx_ch2_kt_77_weeping_", "28. Peavey XXX CH2 KT-77 Weeping Chaos-Fuzz", "amps/peavey_xxx/28_peavey_xxx_ch2_kt_77_weeping_chaos_fuzz.nam"),
    ("20_peavey_xxx_ch2_kt_77_tall_fon", "20. Peavey XXX CH2 KT-77 Tall Font D-Fuzz", "amps/peavey_xxx/20_peavey_xxx_ch2_kt_77_tall_font_d_fuzz.nam"),
    ("31_peavey_xxx_ch2_kt_77_os_2_dis", "31. Peavey XXX CH2 KT-77 OS-2-Distortion", "amps/peavey_xxx/31_peavey_xxx_ch2_kt_77_os_2_distortion.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("12_peavey_xxx_ch2_kt_77_unbooste"),
        &[
            ("12_peavey_xxx_ch2_kt_77_unbooste", "12. Peavey XXX CH2 KT-77 UnBoosted B-HiGain"),
            ("28_peavey_xxx_ch2_kt_77_weeping_", "28. Peavey XXX CH2 KT-77 Weeping Chaos-Fuzz"),
            ("20_peavey_xxx_ch2_kt_77_tall_fon", "20. Peavey XXX CH2 KT-77 Tall Font D-Fuzz"),
            ("31_peavey_xxx_ch2_kt_77_os_2_dis", "31. Peavey XXX CH2 KT-77 OS-2-Distortion"),
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
