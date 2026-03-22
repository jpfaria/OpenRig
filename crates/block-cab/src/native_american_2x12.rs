use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::native_core::{self, NativeCabProfile, NativeCabSchemaDefaults};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;

pub const MODEL_ID: &str = "american_2x12";
pub const DISPLAY_NAME: &str = "American 2x12";

const PROFILE: NativeCabProfile = NativeCabProfile {
    resonance_hz: 102.0,
    air_hz: 4_600.0,
    room_base_ms: 10.0,
    room_span_ms: 14.0,
    resonance_gain: 0.26,
    air_gain: 0.32,
    high_cut_scale: 1.0,
};

const DEFAULTS: NativeCabSchemaDefaults = NativeCabSchemaDefaults {
    low_cut_hz: 64.0,
    high_cut_hz: 8_400.0,
    resonance: 55.0,
    air: 26.0,
    mic_position: 50.0,
    mic_distance: 24.0,
    room_mix: 12.0,
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

pub const MODEL_DEFINITION: CabModelDefinition = CabModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "",
    backend_kind: CabBackendKind::Native,
    schema,
    validate: native_core::validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
};
