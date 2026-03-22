use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::native_core::{
    self, NativeAmpHeadProfile, NativeAmpHeadSchemaDefaults,
};
use crate::registry::AmpHeadModelDefinition;
use crate::AmpHeadBackendKind;

pub const MODEL_ID: &str = "american_clean";
pub const DISPLAY_NAME: &str = "American Clean";

const PROFILE: NativeAmpHeadProfile = NativeAmpHeadProfile {
    input_trim_db: 3.0,
    drive_scale: 1.75,
    asymmetry: 0.04,
    bright_mix: 0.22,
    low_voice: 1.05,
    mid_voice: 0.88,
    high_voice: 1.12,
    presence_voice: 0.44,
    depth_voice: 0.33,
    power_drive: 0.95,
    low_cut_hz: 36.0,
    top_end_hz: 10_500.0,
};

const DEFAULTS: NativeAmpHeadSchemaDefaults = NativeAmpHeadSchemaDefaults {
    gain: 34.0,
    presence: 54.0,
    depth: 42.0,
    bright: true,
    sag: 16.0,
};

fn schema() -> Result<ModelParameterSchema> {
    Ok(native_core::model_schema(MODEL_ID, DISPLAY_NAME, DEFAULTS))
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    native_core::build_processor_for_profile(PROFILE, params, sample_rate, layout)
}

fn asset_summary(params: &ParameterSet) -> Result<String> {
    native_core::asset_summary(MODEL_ID, params)
}

pub const MODEL_DEFINITION: AmpHeadModelDefinition = AmpHeadModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "native",
    backend_kind: AmpHeadBackendKind::Native,
    schema,
    validate: native_core::validate_params,
    asset_summary,
    build,
};
