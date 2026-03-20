use anyhow::{anyhow, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::{marshall_jcm_800, native, AmpHeadBackendKind};

pub struct AmpHeadModelDefinition {
    pub id: &'static str,
    pub backend_kind: AmpHeadBackendKind,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub validate: fn(&ParameterSet) -> Result<()>,
    pub asset_summary: fn(&ParameterSet) -> Result<String>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
}

fn build_marshall(params: &ParameterSet, _sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    marshall_jcm_800::build_processor_for_model(params, layout)
}

fn brit_schema() -> Result<ModelParameterSchema> {
    native::model_schema(native::BRIT_CRUNCH_HEAD_ID)
}

fn brit_validate(params: &ParameterSet) -> Result<()> {
    native::validate_params(native::BRIT_CRUNCH_HEAD_ID, params)
}

fn brit_asset_summary(params: &ParameterSet) -> Result<String> {
    native::asset_summary(native::BRIT_CRUNCH_HEAD_ID, params)
}

fn brit_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    native::build_processor_for_model(native::BRIT_CRUNCH_HEAD_ID, params, sample_rate, layout)
}

fn american_schema() -> Result<ModelParameterSchema> {
    native::model_schema(native::AMERICAN_CLEAN_HEAD_ID)
}

fn american_validate(params: &ParameterSet) -> Result<()> {
    native::validate_params(native::AMERICAN_CLEAN_HEAD_ID, params)
}

fn american_asset_summary(params: &ParameterSet) -> Result<String> {
    native::asset_summary(native::AMERICAN_CLEAN_HEAD_ID, params)
}

fn american_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    native::build_processor_for_model(native::AMERICAN_CLEAN_HEAD_ID, params, sample_rate, layout)
}

fn modern_schema() -> Result<ModelParameterSchema> {
    native::model_schema(native::MODERN_HIGH_GAIN_HEAD_ID)
}

fn modern_validate(params: &ParameterSet) -> Result<()> {
    native::validate_params(native::MODERN_HIGH_GAIN_HEAD_ID, params)
}

fn modern_asset_summary(params: &ParameterSet) -> Result<String> {
    native::asset_summary(native::MODERN_HIGH_GAIN_HEAD_ID, params)
}

fn modern_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    native::build_processor_for_model(
        native::MODERN_HIGH_GAIN_HEAD_ID,
        params,
        sample_rate,
        layout,
    )
}

const MARSHALL_JCM_800: AmpHeadModelDefinition = AmpHeadModelDefinition {
    id: marshall_jcm_800::MODEL_ID,
    backend_kind: AmpHeadBackendKind::Nam,
    schema: || Ok(marshall_jcm_800::model_schema()),
    validate: marshall_jcm_800::validate_params,
    asset_summary: marshall_jcm_800::asset_summary,
    build: build_marshall,
};

const BRIT_CRUNCH: AmpHeadModelDefinition = AmpHeadModelDefinition {
    id: native::BRIT_CRUNCH_HEAD_ID,
    backend_kind: AmpHeadBackendKind::Native,
    schema: brit_schema,
    validate: brit_validate,
    asset_summary: brit_asset_summary,
    build: brit_build,
};

const AMERICAN_CLEAN: AmpHeadModelDefinition = AmpHeadModelDefinition {
    id: native::AMERICAN_CLEAN_HEAD_ID,
    backend_kind: AmpHeadBackendKind::Native,
    schema: american_schema,
    validate: american_validate,
    asset_summary: american_asset_summary,
    build: american_build,
};

const MODERN_HIGH_GAIN: AmpHeadModelDefinition = AmpHeadModelDefinition {
    id: native::MODERN_HIGH_GAIN_HEAD_ID,
    backend_kind: AmpHeadBackendKind::Native,
    schema: modern_schema,
    validate: modern_validate,
    asset_summary: modern_asset_summary,
    build: modern_build,
};

pub const SUPPORTED_MODELS: &[&str] = &[
    MARSHALL_JCM_800.id,
    BRIT_CRUNCH.id,
    AMERICAN_CLEAN.id,
    MODERN_HIGH_GAIN.id,
];

const MODEL_DEFINITIONS: &[AmpHeadModelDefinition] = &[
    MARSHALL_JCM_800,
    BRIT_CRUNCH,
    AMERICAN_CLEAN,
    MODERN_HIGH_GAIN,
];

pub fn find_model_definition(model: &str) -> Result<&'static AmpHeadModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported amp-head model '{}'", model))
}
