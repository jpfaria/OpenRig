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

const CAPTURES: &[(&str, &str, &str)] = &[
    ("pre_unnamed_lo_down_02_std", "[PRE] UNNAMED Lo-Down #02 - STD", "preamp/fortin_meshuggah_preamp/pre_unnamed_lo_down_02_std.nam"),
    ("pre_unnamed_hi_up_04_std", "[PRE] UNNAMED Hi-Up #04 - STD", "preamp/fortin_meshuggah_preamp/pre_unnamed_hi_up_04_std.nam"),
    ("pre_unnamed_lo_down_03_std", "[PRE] UNNAMED Lo-Down #03 - STD", "preamp/fortin_meshuggah_preamp/pre_unnamed_lo_down_03_std.nam"),
    ("pre_unnamed_hi_down_02_std", "[PRE] UNNAMED Hi-Down #02 - STD", "preamp/fortin_meshuggah_preamp/pre_unnamed_hi_down_02_std.nam"),
    ("pre_unnamed_lo_down_04_std", "[PRE] UNNAMED Lo-Down #04 - STD", "preamp/fortin_meshuggah_preamp/pre_unnamed_lo_down_04_std.nam"),
    ("pre_unnamed_hi_up_01_std", "[PRE] UNNAMED Hi-Up #01 - STD", "preamp/fortin_meshuggah_preamp/pre_unnamed_hi_up_01_std.nam"),
    ("pre_unnamed_hi_down_04_std", "[PRE] UNNAMED Hi-Down #04 - STD", "preamp/fortin_meshuggah_preamp/pre_unnamed_hi_down_04_std.nam"),
    ("pre_unnamed_lo_up_01_std", "[PRE] UNNAMED Lo-Up #01 - STD", "preamp/fortin_meshuggah_preamp/pre_unnamed_lo_up_01_std.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema =
        model_schema_for(block_core::EFFECT_TYPE_PREAMP, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("pre_unnamed_lo_down_02_std"),
        &[
            ("pre_unnamed_lo_down_02_std", "[PRE] UNNAMED Lo-Down #02 - STD"),
            ("pre_unnamed_hi_up_04_std", "[PRE] UNNAMED Hi-Up #04 - STD"),
            ("pre_unnamed_lo_down_03_std", "[PRE] UNNAMED Lo-Down #03 - STD"),
            ("pre_unnamed_hi_down_02_std", "[PRE] UNNAMED Hi-Down #02 - STD"),
            ("pre_unnamed_lo_down_04_std", "[PRE] UNNAMED Lo-Down #04 - STD"),
            ("pre_unnamed_hi_up_01_std", "[PRE] UNNAMED Hi-Up #01 - STD"),
            ("pre_unnamed_hi_down_04_std", "[PRE] UNNAMED Hi-Down #04 - STD"),
            ("pre_unnamed_lo_up_01_std", "[PRE] UNNAMED Lo-Up #01 - STD"),
        ],
    )];
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
    let key = required_string(params, "capture").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("preamp '{}' has no capture '{}'", MODEL_ID, key))
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
