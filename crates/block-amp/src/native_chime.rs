use anyhow::Result;
use block_preamp::native_core::NativeAmpHeadProfile;
use block_cab::native_core::NativeCabProfile;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::native_core::{self, NativeAmpProfile, NativeAmpSchemaDefaults};
use crate::registry::{AmpBackendKind, AmpModelDefinition};

pub const MODEL_ID: &str = "chime";
pub const DISPLAY_NAME: &str = "Chime";

const HEAD_PROFILE: NativeAmpHeadProfile = NativeAmpHeadProfile {
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

const CAB_PROFILE: NativeCabProfile = NativeCabProfile {
    resonance_hz: 126.0,
    air_hz: 3_900.0,
    room_base_ms: 8.0,
    room_span_ms: 12.0,
    resonance_gain: 0.34,
    air_gain: 0.26,
    high_cut_scale: 0.88,
};

const PROFILE: NativeAmpProfile = NativeAmpProfile {
    head_profile: HEAD_PROFILE,
    cab_profile: CAB_PROFILE,
    fixed_presence: 64.0,
    fixed_depth: 28.0,
    cab_low_cut_hz: 78.0,
    cab_high_cut_hz: 8_800.0,
    cab_resonance: 44.0,
    cab_air: 36.0,
    cab_mic_position: 68.0,
    cab_mic_distance: 20.0,
    gain_bias: -10.0,
};

const DEFAULTS: NativeAmpSchemaDefaults = NativeAmpSchemaDefaults {
    gain: 38.0,
    treble: 58.0,
    bright: true,
    sag: 18.0,
    room_mix: 14.0,
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

pub const MODEL_DEFINITION: AmpModelDefinition = AmpModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "native",
    backend_kind: AmpBackendKind::Native,
    schema,
    validate: native_core::validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_ACOUSTIC_BASS,
};
