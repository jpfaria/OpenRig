use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "friedman_be100_deluxe";
pub const DISPLAY_NAME: &str = "BE100 Deluxe";
const BRAND: &str = "friedman";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FriedmanBe100Params {
    pub channel: &'static str,
    pub mic: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FriedmanBe100Capture {
    pub params: FriedmanBe100Params,
    pub model_path: &'static str,
}

pub const CAPTURES: &[FriedmanBe100Capture] = &[
    capture("cln_tender",   "sm57",  "amps/friedman_be100_deluxe/be100_cln_tender_sm57.nam"),
    capture("cln_tender",   "sm58",  "amps/friedman_be100_deluxe/be100_cln_tender_sm58.nam"),
    capture("cln_tender",   "blend", "amps/friedman_be100_deluxe/be100_cln_tender_blend.nam"),
    capture("cln_rock",     "sm57",  "amps/friedman_be100_deluxe/be100_cln_rock_sm57.nam"),
    capture("cln_rock",     "sm58",  "amps/friedman_be100_deluxe/be100_cln_rock_sm58.nam"),
    capture("cln_rock",     "blend", "amps/friedman_be100_deluxe/be100_cln_rock_blend.nam"),
    capture("be",           "sm57",  "amps/friedman_be100_deluxe/be100_be_eddie_sm57.nam"),
    capture("be",           "sm58",  "amps/friedman_be100_deluxe/be100_be_eddie_sm58.nam"),
    capture("be",           "blend", "amps/friedman_be100_deluxe/be100_be_eddie_blend.nam"),
    capture("hbe_tallica",  "sm57",  "amps/friedman_be100_deluxe/be100_hbe_tallica_sm57.nam"),
    capture("hbe_tallica",  "sm58",  "amps/friedman_be100_deluxe/be100_hbe_tallica_sm58.nam"),
    capture("hbe_tallica",  "blend", "amps/friedman_be100_deluxe/be100_hbe_tallica_blend.nam"),
    capture("hbe_mammoth",  "sm57",  "amps/friedman_be100_deluxe/be100_hbe_mammoth_sm57.nam"),
    capture("hbe_mammoth",  "sm58",  "amps/friedman_be100_deluxe/be100_hbe_mammoth_sm58.nam"),
    capture("hbe_mammoth",  "blend", "amps/friedman_be100_deluxe/be100_hbe_mammoth_blend.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Amp"),
            Some("cln_tender"),
            &[
                ("cln_tender",  "Clean Tender"),
                ("cln_rock",    "Clean Rock"),
                ("be",          "BE"),
                ("hbe_tallica", "HBE Tallica"),
                ("hbe_mammoth", "HBE Mammoth"),
            ],
        ),
        enum_parameter(
            "mic",
            "Mic",
            Some("Cab"),
            Some("sm57"),
            &[
                ("sm57",  "SM57"),
                ("sm58",  "SM58"),
                ("blend", "Blend"),
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

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("model='{}'", capture.model_path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static FriedmanBe100Capture> {
    let channel = required_string(params, "channel").map_err(anyhow::Error::msg)?;
    let mic = required_string(params, "mic").map_err(anyhow::Error::msg)?;

    CAPTURES
        .iter()
        .find(|c| c.params.channel == channel && c.params.mic == mic)
        .ok_or_else(|| {
            anyhow!(
                "amp model '{}' does not support channel='{}' mic='{}'",
                MODEL_ID,
                channel,
                mic
            )
        })
}

const fn capture(channel: &'static str, mic: &'static str, model_path: &'static str) -> FriedmanBe100Capture {
    FriedmanBe100Capture {
        params: FriedmanBe100Params { channel, mic },
        model_path,
    }
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
