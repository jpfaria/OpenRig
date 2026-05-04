use anyhow::{anyhow, Result};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_diezel_zerrer";
pub const DISPLAY_NAME: &str = "Zerrer";
const BRAND: &str = "diezel";

pub const NAM_PLUGIN_DEFAULTS: NamPluginParams = NamPluginParams {
    input_level_db: 0.0,
    output_level_db: 0.0,
    noise_gate_threshold_db: -80.0,
    noise_gate_enabled: true,
    eq_enabled: true,
    bass: 5.0,
    middle: 5.0,
    treble: 5.0,
};

// Two-axis pack: channel × capture index.
// Channels: CH1 clean, OD overdrive. Capture index = original Zerrer
// preset number (#04..#28) — different EQ/gain settings per capture.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (channel, preset, file)
    ("ch1", "preset_12", "preamp/diezel_zerrer/12_zerrer_ch1_g10_t12_m2_b3_48hz_24bit.nam"),
    ("od",  "preset_04", "preamp/diezel_zerrer/04_zerrer_od_g12_t2_m10_b11_v12_p3_d11_48hz_24bit.nam"),
    ("od",  "preset_05", "preamp/diezel_zerrer/05_zerrer_od_g3_t2_m10_b11_v12_p3_d11_48hz_24bit.nam"),
    ("od",  "preset_06", "preamp/diezel_zerrer/06_zerrer_od_g9_t2_m10_b11_v12_p3_d11_48hz_24bit.nam"),
    ("od",  "preset_25", "preamp/diezel_zerrer/25_zerrer_od_g4_t3_m7_b3_v1_p3_d11_48hz_24bit.nam"),
    ("od",  "preset_26", "preamp/diezel_zerrer/26_zerrer_od_g3_t3_m12_b3_v1_p3_d11_48hz_24bit.nam"),
    ("od",  "preset_27", "preamp/diezel_zerrer/27_zerrer_od_g11_t4_m3_b1_v1_p3_d11_48hz_24bit.nam"),
    ("od",  "preset_28", "preamp/diezel_zerrer/28_zerrer_od_g10_t4_m12_b3_v2_p3_d11_48hz_24bit.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema =
        model_schema_for(block_core::EFFECT_TYPE_PREAMP, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Amp"),
            Some("od"),
            &[
                ("ch1", "CH1 Clean"),
                ("od",  "OD"),
            ],
        ),
        enum_parameter(
            "preset",
            "Preset",
            Some("Amp"),
            Some("preset_04"),
            &[
                ("preset_04", "Preset #04"),
                ("preset_05", "Preset #05"),
                ("preset_06", "Preset #06"),
                ("preset_12", "Preset #12"),
                ("preset_25", "Preset #25"),
                ("preset_26", "Preset #26"),
                ("preset_27", "Preset #27"),
                ("preset_28", "Preset #28"),
            ],
        ),
    ];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let path = resolve_capture(params)?;
    let plugin_params = plugin_params_from_set_with_defaults(params, NAM_PLUGIN_DEFAULTS)?;
    let model_path = nam::resolve_nam_capture(path)?;
    build_processor_with_assets_for_layout(&model_path, None, plugin_params, sample_rate, layout)
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let channel = required_string(params, "channel").map_err(anyhow::Error::msg)?;
    let preset = required_string(params, "preset").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(c, p, _)| *c == channel && *p == preset)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "preamp '{}' has no capture for channel={} preset={}",
                MODEL_ID, channel, preset
            )
        })
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: PreampModelDefinition = PreampModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: PreampBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let path = resolve_capture(params)?;
    Ok(format!("asset_id='{}'", path))
}
