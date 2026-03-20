use anyhow::{anyhow, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::{marshall_4x12_v30, native, CabBackendKind};

pub struct CabModelDefinition {
    pub id: &'static str,
    pub backend_kind: CabBackendKind,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub validate: fn(&ParameterSet) -> Result<()>,
    pub asset_summary: fn(&ParameterSet) -> Result<String>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
}

fn marshall_schema() -> Result<ModelParameterSchema> {
    Ok(marshall_4x12_v30::model_schema())
}

fn marshall_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    marshall_4x12_v30::build_processor_for_model(params, sample_rate, layout)
}

fn brit_schema() -> Result<ModelParameterSchema> {
    native::model_schema(native::BRIT_4X12_CAB_ID)
}

fn brit_validate(params: &ParameterSet) -> Result<()> {
    native::validate_params(native::BRIT_4X12_CAB_ID, params)
}

fn brit_asset_summary(params: &ParameterSet) -> Result<String> {
    native::asset_summary(native::BRIT_4X12_CAB_ID, params)
}

fn brit_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    native::build_processor_for_model(native::BRIT_4X12_CAB_ID, params, sample_rate, layout)
}

fn american_schema() -> Result<ModelParameterSchema> {
    native::model_schema(native::AMERICAN_2X12_CAB_ID)
}

fn american_validate(params: &ParameterSet) -> Result<()> {
    native::validate_params(native::AMERICAN_2X12_CAB_ID, params)
}

fn american_asset_summary(params: &ParameterSet) -> Result<String> {
    native::asset_summary(native::AMERICAN_2X12_CAB_ID, params)
}

fn american_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    native::build_processor_for_model(
        native::AMERICAN_2X12_CAB_ID,
        params,
        sample_rate,
        layout,
    )
}

fn vintage_schema() -> Result<ModelParameterSchema> {
    native::model_schema(native::VINTAGE_1X12_CAB_ID)
}

fn vintage_validate(params: &ParameterSet) -> Result<()> {
    native::validate_params(native::VINTAGE_1X12_CAB_ID, params)
}

fn vintage_asset_summary(params: &ParameterSet) -> Result<String> {
    native::asset_summary(native::VINTAGE_1X12_CAB_ID, params)
}

fn vintage_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    native::build_processor_for_model(native::VINTAGE_1X12_CAB_ID, params, sample_rate, layout)
}

const MARSHALL_4X12_V30: CabModelDefinition = CabModelDefinition {
    id: marshall_4x12_v30::MODEL_ID,
    backend_kind: CabBackendKind::Ir,
    schema: marshall_schema,
    validate: marshall_4x12_v30::validate_params,
    asset_summary: marshall_4x12_v30::asset_summary,
    build: marshall_build,
};

const BRIT_4X12: CabModelDefinition = CabModelDefinition {
    id: native::BRIT_4X12_CAB_ID,
    backend_kind: CabBackendKind::Native,
    schema: brit_schema,
    validate: brit_validate,
    asset_summary: brit_asset_summary,
    build: brit_build,
};

const AMERICAN_2X12: CabModelDefinition = CabModelDefinition {
    id: native::AMERICAN_2X12_CAB_ID,
    backend_kind: CabBackendKind::Native,
    schema: american_schema,
    validate: american_validate,
    asset_summary: american_asset_summary,
    build: american_build,
};

const VINTAGE_1X12: CabModelDefinition = CabModelDefinition {
    id: native::VINTAGE_1X12_CAB_ID,
    backend_kind: CabBackendKind::Native,
    schema: vintage_schema,
    validate: vintage_validate,
    asset_summary: vintage_asset_summary,
    build: vintage_build,
};

pub const SUPPORTED_MODELS: &[&str] = &[
    MARSHALL_4X12_V30.id,
    BRIT_4X12.id,
    AMERICAN_2X12.id,
    VINTAGE_1X12.id,
];

const MODEL_DEFINITIONS: &[CabModelDefinition] = &[
    MARSHALL_4X12_V30,
    BRIT_4X12,
    AMERICAN_2X12,
    VINTAGE_1X12,
];

pub fn find_model_definition(model: &str) -> Result<&'static CabModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported cab model '{}'", model))
}
