use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::native_core::{
    self, NativeAmpHeadProfile, NativeAmpHeadSchemaDefaults,
};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;

pub const MODEL_ID: &str = "brit_crunch";
pub const DISPLAY_NAME: &str = "Brit Crunch";

const PROFILE: NativeAmpHeadProfile = NativeAmpHeadProfile {
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

const DEFAULTS: NativeAmpHeadSchemaDefaults = NativeAmpHeadSchemaDefaults {
    gain: 56.0,
    presence: 58.0,
    depth: 48.0,
    bright: false,
    sag: 24.0,
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

pub const MODEL_DEFINITION: PreampModelDefinition = PreampModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "native",
    backend_kind: PreampBackendKind::Native,
    panel_bg: [0x34, 0x2e, 0x28],
    panel_text: [0x80, 0x90, 0xa0],
    brand_strip_bg: [0x1a, 0x1a, 0x1a],
    model_font: "Permanent Marker",
    schema,
    validate: native_core::validate_params,
    asset_summary,
    build,
};
