use anyhow::Result;
use block_preamp::native_core::NativeAmpHeadProfile;
use block_cab::native_core::NativeCabProfile;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::native_core::{self, NativeAmpProfile, NativeAmpSchemaDefaults};
use crate::registry::{AmpBackendKind, AmpModelDefinition};

pub const MODEL_ID: &str = "tweed_breakup";
pub const DISPLAY_NAME: &str = "Tweed Breakup";

const HEAD_PROFILE: NativeAmpHeadProfile = NativeAmpHeadProfile {
    input_trim_db: 1.5,
    drive_scale: 2.8,
    asymmetry: 0.12,
    bright_mix: 0.12,
    low_voice: 0.92,
    mid_voice: 1.15,
    high_voice: 0.95,
    presence_voice: 0.55,
    depth_voice: 0.38,
    power_drive: 1.35,
    low_cut_hz: 48.0,
    top_end_hz: 8_400.0,
};

const CAB_PROFILE: NativeCabProfile = NativeCabProfile {
    resonance_hz: 92.0,
    air_hz: 3_200.0,
    room_base_ms: 12.0,
    room_span_ms: 16.0,
    resonance_gain: 0.30,
    air_gain: 0.22,
    high_cut_scale: 0.78,
};

const PROFILE: NativeAmpProfile = NativeAmpProfile {
    head_profile: HEAD_PROFILE,
    cab_profile: CAB_PROFILE,
    fixed_presence: 42.0,
    fixed_depth: 30.0,
    cab_low_cut_hz: 92.0,
    cab_high_cut_hz: 5_900.0,
    cab_resonance: 57.0,
    cab_air: 18.0,
    cab_mic_position: 42.0,
    cab_mic_distance: 18.0,
    gain_bias: -15.0,
};

const DEFAULTS: NativeAmpSchemaDefaults = NativeAmpSchemaDefaults {
    gain: 54.0,
    treble: 50.0,
    bright: false,
    sag: 34.0,
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
    panel_bg: [0x38, 0x30, 0x22],
    panel_text: [0x80, 0x90, 0xa0],
    brand_strip_bg: [0x1a, 0x1a, 0x1a],
    model_font: "Permanent Marker",
    schema,
    validate: native_core::validate_params,
    asset_summary,
    build,
};
