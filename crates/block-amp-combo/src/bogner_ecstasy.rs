use anyhow::{anyhow, Result};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "bogner_ecstasy";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BognerEcstasyParams {
    pub gain: &'static str,
    pub cabinet: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BognerEcstasyCapture {
    pub params: BognerEcstasyParams,
    pub model_path: &'static str,
}

pub const CAPTURES: &[BognerEcstasyCapture] = &[
    capture(
        "clean",
        "4x12_v30",
        "captures/nam/amps/combo/bogner_ecstasy/ecstacy_clean_4x12_v30.nam",
    ),
    capture(
        "clean",
        "4x12_greenback",
        "captures/nam/amps/combo/bogner_ecstasy/ecstacy_clean_4x12_greenback.nam",
    ),
    capture(
        "clean",
        "4x12_g12t",
        "captures/nam/amps/combo/bogner_ecstasy/ecstacy_clean_4x12_g12t.nam",
    ),
    capture(
        "crunch",
        "4x12_v30",
        "captures/nam/amps/combo/bogner_ecstasy/ecstacy_crunch_blue_4x12_v30.nam",
    ),
    capture(
        "crunch",
        "4x12_greenback",
        "captures/nam/amps/combo/bogner_ecstasy/ecstacy_crunch_blue_4x12_greenback.nam",
    ),
    capture(
        "crunch",
        "4x12_g12t",
        "captures/nam/amps/combo/bogner_ecstasy/ecstacy_crunch_blue_4x12_g12t.nam",
    ),
    capture(
        "drive",
        "4x12_v30",
        "captures/nam/amps/combo/bogner_ecstasy/ecstacy_drive_red_4x12_v30.nam",
    ),
    capture(
        "drive",
        "4x12_greenback",
        "captures/nam/amps/combo/bogner_ecstasy/ecstacy_drive_red_4x12_greenback.nam",
    ),
    capture(
        "drive",
        "4x12_g12t",
        "captures/nam/amps/combo/bogner_ecstasy/ecstacy_drive_red_4x12_g12t.nam",
    ),
];

pub fn supports_model(model: &str) -> bool {
    model == MODEL_ID
}

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp_combo", MODEL_ID, "Bogner Ecstasy", false);
    schema.parameters = vec![
        enum_parameter(
            "gain",
            "Gain",
            Some("Amp Combo"),
            Some("clean"),
            &[("clean", "Clean"), ("crunch", "Crunch"), ("drive", "Drive")],
        ),
        enum_parameter(
            "cabinet",
            "Cabinet",
            Some("Amp Combo"),
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
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let capture = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        capture.model_path,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        layout,
    )
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("model='{}'", capture.model_path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static BognerEcstasyCapture> {
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
) -> BognerEcstasyCapture {
    BognerEcstasyCapture {
        params: BognerEcstasyParams { gain, cabinet },
        model_path,
    }
}
