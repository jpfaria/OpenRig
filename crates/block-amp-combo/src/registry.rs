use anyhow::{anyhow, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::{bogner_ecstasy, native};

pub struct AmpComboModelDefinition {
    pub id: &'static str,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub validate: fn(&ParameterSet) -> Result<()>,
    pub asset_summary: fn(&ParameterSet) -> Result<String>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
}

fn bogner_schema() -> Result<ModelParameterSchema> {
    Ok(bogner_ecstasy::model_schema())
}

fn bogner_build(
    params: &ParameterSet,
    _sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    bogner_ecstasy::build_processor_for_model(params, layout)
}

fn blackface_schema() -> Result<ModelParameterSchema> {
    native::model_schema(native::BLACKFACE_CLEAN_COMBO_ID)
}

fn blackface_validate(params: &ParameterSet) -> Result<()> {
    native::validate_params(native::BLACKFACE_CLEAN_COMBO_ID, params)
}

fn blackface_asset_summary(params: &ParameterSet) -> Result<String> {
    native::asset_summary(native::BLACKFACE_CLEAN_COMBO_ID, params)
}

fn blackface_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    native::build_processor_for_model(native::BLACKFACE_CLEAN_COMBO_ID, params, sample_rate, layout)
}

fn tweed_schema() -> Result<ModelParameterSchema> {
    native::model_schema(native::TWEED_BREAKUP_COMBO_ID)
}

fn tweed_validate(params: &ParameterSet) -> Result<()> {
    native::validate_params(native::TWEED_BREAKUP_COMBO_ID, params)
}

fn tweed_asset_summary(params: &ParameterSet) -> Result<String> {
    native::asset_summary(native::TWEED_BREAKUP_COMBO_ID, params)
}

fn tweed_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    native::build_processor_for_model(native::TWEED_BREAKUP_COMBO_ID, params, sample_rate, layout)
}

fn chime_schema() -> Result<ModelParameterSchema> {
    native::model_schema(native::CHIME_COMBO_ID)
}

fn chime_validate(params: &ParameterSet) -> Result<()> {
    native::validate_params(native::CHIME_COMBO_ID, params)
}

fn chime_asset_summary(params: &ParameterSet) -> Result<String> {
    native::asset_summary(native::CHIME_COMBO_ID, params)
}

fn chime_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    native::build_processor_for_model(native::CHIME_COMBO_ID, params, sample_rate, layout)
}

const BOGNER_ECSTASY: AmpComboModelDefinition = AmpComboModelDefinition {
    id: bogner_ecstasy::MODEL_ID,
    schema: bogner_schema,
    validate: bogner_ecstasy::validate_params,
    asset_summary: bogner_ecstasy::asset_summary,
    build: bogner_build,
};

const BLACKFACE_CLEAN: AmpComboModelDefinition = AmpComboModelDefinition {
    id: native::BLACKFACE_CLEAN_COMBO_ID,
    schema: blackface_schema,
    validate: blackface_validate,
    asset_summary: blackface_asset_summary,
    build: blackface_build,
};

const TWEED_BREAKUP: AmpComboModelDefinition = AmpComboModelDefinition {
    id: native::TWEED_BREAKUP_COMBO_ID,
    schema: tweed_schema,
    validate: tweed_validate,
    asset_summary: tweed_asset_summary,
    build: tweed_build,
};

const CHIME: AmpComboModelDefinition = AmpComboModelDefinition {
    id: native::CHIME_COMBO_ID,
    schema: chime_schema,
    validate: chime_validate,
    asset_summary: chime_asset_summary,
    build: chime_build,
};

pub const SUPPORTED_MODELS: &[&str] = &[
    BOGNER_ECSTASY.id,
    BLACKFACE_CLEAN.id,
    TWEED_BREAKUP.id,
    CHIME.id,
];

const MODEL_DEFINITIONS: &[AmpComboModelDefinition] = &[
    BOGNER_ECSTASY,
    BLACKFACE_CLEAN,
    TWEED_BREAKUP,
    CHIME,
];

pub fn find_model_definition(model: &str) -> Result<&'static AmpComboModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported amp-combo model '{}'", model))
}
