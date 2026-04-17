use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_marshall_jvm";
pub const DISPLAY_NAME: &str = "JVM";
const BRAND: &str = "marshall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarshallJvmParams {
    pub gain: &'static str,
    pub cabinet: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarshallJvmCapture {
    pub params: MarshallJvmParams,
    pub model_path: &'static str,
}

pub const CAPTURES: &[MarshallJvmCapture] = &[
    capture(
        "clean",
        "4x12_v30",
        "amps/marshall_jvm/jvm_clean_4x12_v30.nam",
    ),
    capture(
        "clean",
        "4x12_greenback",
        "amps/marshall_jvm/jvm_clean_4x12_greenback.nam",
    ),
    capture(
        "clean",
        "4x12_g12t",
        "amps/marshall_jvm/jvm_clean_4x12_g12t.nam",
    ),
    capture(
        "crunch",
        "4x12_v30",
        "amps/marshall_jvm/jvm_crunch_4x12_v30.nam",
    ),
    capture(
        "crunch",
        "4x12_greenback",
        "amps/marshall_jvm/jvm_crunch_4x12_greenback.nam",
    ),
    capture(
        "crunch",
        "4x12_g12t",
        "amps/marshall_jvm/jvm_crunch_4x12_g12t.nam",
    ),
    capture(
        "drive",
        "4x12_v30",
        "amps/marshall_jvm/jvm_drive_4x12_v30.nam",
    ),
    capture(
        "drive",
        "4x12_greenback",
        "amps/marshall_jvm/jvm_drive_4x12_greenback.nam",
    ),
    capture(
        "drive",
        "4x12_g12t",
        "amps/marshall_jvm/jvm_drive_4x12_g12t.nam",
    ),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "gain",
            "Gain",
            Some("Amp"),
            Some("clean"),
            &[("clean", "Clean"), ("crunch", "Crunch"), ("drive", "Drive")],
        ),
        enum_parameter(
            "cabinet",
            "Cabinet",
            Some("Amp"),
            Some("4x12_v30"),
            &[
                ("4x12_v30", "4x12 V30"),
                ("4x12_greenback", "4x12 Greenback"),
                ("4x12_g12t", "4x12 G12T"),
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
    let capture = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(capture.model_path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
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
    let capture = resolve_capture(params)?;
    Ok(format!("model='{}'", capture.model_path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static MarshallJvmCapture> {
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    let cabinet = required_string(params, "cabinet").map_err(anyhow::Error::msg)?;

    CAPTURES
        .iter()
        .find(|capture| capture.params.gain == gain && capture.params.cabinet == cabinet)
        .ok_or_else(|| {
            anyhow!(
                "amp-combo model '{}' does not support gain='{}' cabinet='{}'",
                MODEL_ID,
                gain,
                cabinet
            )
        })
}

const fn capture(
    gain: &'static str,
    cabinet: &'static str,
    model_path: &'static str,
) -> MarshallJvmCapture {
    MarshallJvmCapture {
        params: MarshallJvmParams { gain, cabinet },
        model_path,
    }
}
