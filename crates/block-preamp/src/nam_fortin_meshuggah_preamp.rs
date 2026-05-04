use anyhow::{anyhow, Result};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_fortin_meshuggah_preamp";
pub const DISPLAY_NAME: &str = "Meshuggah Preamp";
const BRAND: &str = "fortin";

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

// Three-axis pack: range × direction × take.
// range = lo / hi   (low or high gain footprint)
// direction = up / down  (sweep direction in original capture pack)
const CAPTURES: &[(&str, &str, &str, &str)] = &[
    // (range, direction, take, file)
    ("lo", "down", "02", "preamp/fortin_meshuggah_preamp/pre_unnamed_lo_down_02_std.nam"),
    ("lo", "down", "03", "preamp/fortin_meshuggah_preamp/pre_unnamed_lo_down_03_std.nam"),
    ("lo", "down", "04", "preamp/fortin_meshuggah_preamp/pre_unnamed_lo_down_04_std.nam"),
    ("lo", "up",   "01", "preamp/fortin_meshuggah_preamp/pre_unnamed_lo_up_01_std.nam"),
    ("hi", "down", "02", "preamp/fortin_meshuggah_preamp/pre_unnamed_hi_down_02_std.nam"),
    ("hi", "down", "04", "preamp/fortin_meshuggah_preamp/pre_unnamed_hi_down_04_std.nam"),
    ("hi", "up",   "01", "preamp/fortin_meshuggah_preamp/pre_unnamed_hi_up_01_std.nam"),
    ("hi", "up",   "04", "preamp/fortin_meshuggah_preamp/pre_unnamed_hi_up_04_std.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema =
        model_schema_for(block_core::EFFECT_TYPE_PREAMP, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "range",
            "Range",
            Some("Amp"),
            Some("lo"),
            &[
                ("lo", "Lo Range"),
                ("hi", "Hi Range"),
            ],
        ),
        enum_parameter(
            "direction",
            "Direction",
            Some("Amp"),
            Some("down"),
            &[
                ("down", "Down"),
                ("up",   "Up"),
            ],
        ),
        enum_parameter(
            "take",
            "Take",
            Some("Amp"),
            Some("02"),
            &[
                ("01", "Take #01"),
                ("02", "Take #02"),
                ("03", "Take #03"),
                ("04", "Take #04"),
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
    let range = required_string(params, "range").map_err(anyhow::Error::msg)?;
    let direction = required_string(params, "direction").map_err(anyhow::Error::msg)?;
    let take = required_string(params, "take").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(r, d, t, _)| *r == range && *d == direction && *t == take)
        .map(|(_, _, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "preamp '{}' has no capture for range={} direction={} take={}",
                MODEL_ID, range, direction, take
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
