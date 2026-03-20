use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::native_core::{
    self, NativeAmpHeadProfile, NativeAmpHeadSchemaDefaults,
};
use crate::registry::AmpHeadModelDefinition;
use crate::AmpHeadBackendKind;

pub const MODEL_ID: &str = "modern_high_gain_head";
pub const DISPLAY_NAME: &str = "Modern High Gain Head";

const PROFILE: NativeAmpHeadProfile = NativeAmpHeadProfile {
    input_trim_db: -1.0,
    drive_scale: 4.1,
    asymmetry: 0.18,
    bright_mix: 0.08,
    low_voice: 0.82,
    mid_voice: 0.92,
    high_voice: 1.02,
    presence_voice: 0.62,
    depth_voice: 0.58,
    power_drive: 1.55,
    low_cut_hz: 72.0,
    top_end_hz: 7_600.0,
};

const DEFAULTS: NativeAmpHeadSchemaDefaults = NativeAmpHeadSchemaDefaults {
    gain: 72.0,
    presence: 62.0,
    depth: 60.0,
    bright: false,
    sag: 30.0,
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
    backend_kind: AmpHeadBackendKind::Native,
    schema,
    validate: native_core::validate_params,
    asset_summary,
    build,
};
