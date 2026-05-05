use anyhow::Result;
use block_preamp::native_core::NativeAmpHeadProfile;
use block_cab::native_core::NativeCabProfile;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::native_core::{self, NativeAmpProfile, NativeAmpSchemaDefaults};
use crate::registry::{AmpBackendKind, AmpModelDefinition};

pub const MODEL_ID: &str = "blackface_clean";
pub const DISPLAY_NAME: &str = "Blackface Clean";

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
    resonance_hz: 102.0,
    air_hz: 4_600.0,
    room_base_ms: 10.0,
    room_span_ms: 14.0,
    resonance_gain: 0.26,
    air_gain: 0.32,
    high_cut_scale: 1.0,
};

const PROFILE: NativeAmpProfile = NativeAmpProfile {
    head_profile: HEAD_PROFILE,
    cab_profile: CAB_PROFILE,
    fixed_presence: 58.0,
    fixed_depth: 34.0,
    cab_low_cut_hz: 66.0,
    cab_high_cut_hz: 8_200.0,
    cab_resonance: 48.0,
    cab_air: 30.0,
    cab_mic_position: 58.0,
    cab_mic_distance: 22.0,
    gain_bias: -8.0,
};

const DEFAULTS: NativeAmpSchemaDefaults = NativeAmpSchemaDefaults {
    gain: 32.0,
    treble: 50.0,
    bright: true,
    sag: 14.0,
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
    brand: block_core::BRAND_NATIVE,
    backend_kind: AmpBackendKind::Native,
    schema,
    validate: native_core::validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[
        block_core::KnobLayoutEntry { param_key: "gain",      svg_cx: 130.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0,   max: 100.0, step: 1.0 },
        block_core::KnobLayoutEntry { param_key: "bass",      svg_cx: 222.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0,   max: 100.0, step: 1.0 },
        block_core::KnobLayoutEntry { param_key: "middle",    svg_cx: 302.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0,   max: 100.0, step: 1.0 },
        block_core::KnobLayoutEntry { param_key: "treble",    svg_cx: 382.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0,   max: 100.0, step: 1.0 },
        block_core::KnobLayoutEntry { param_key: "master",    svg_cx: 470.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0,   max: 100.0, step: 1.0 },
        block_core::KnobLayoutEntry { param_key: "sag",       svg_cx: 550.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0,   max: 100.0, step: 1.0 },
        block_core::KnobLayoutEntry { param_key: "room_mix",  svg_cx: 630.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0,   max: 100.0, step: 1.0 },
    ],
};

#[cfg(test)]
#[path = "native_blackface_clean_tests.rs"]
mod tests;
