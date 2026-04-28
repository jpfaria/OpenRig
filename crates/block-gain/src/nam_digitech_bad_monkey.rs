use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_digitech_bad_monkey";
pub const DISPLAY_NAME: &str = "DigiTech Bad Monkey";
const BRAND: &str = "digitech";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: voicing × size.
// 10 voicings (3 internal boost variants + 7 named overdrive emulations) ×
// 3 model sizes (feather / lite / standard) = 30 captures, full grid.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (voicing, size, file)
    ("boost1",      "feather",  "pedals/digitech_bad_monkey/badmonkey_boost1_v10_b4_t6_g0_feather.nam"),
    ("boost1",      "lite",     "pedals/digitech_bad_monkey/badmonkey_boost1_v10_b4_t6_g0_lite.nam"),
    ("boost1",      "standard", "pedals/digitech_bad_monkey/badmonkey_boost1_v10_b4_t6_g0_standard.nam"),
    ("boost2",      "feather",  "pedals/digitech_bad_monkey/badmonkey_boost2_v10_b4_t6_g4_feather.nam"),
    ("boost2",      "lite",     "pedals/digitech_bad_monkey/badmonkey_boost2_v10_b4_t6_g4_lite.nam"),
    ("boost2",      "standard", "pedals/digitech_bad_monkey/badmonkey_boost2_v10_b4_t6_g4_standard.nam"),
    ("boost3_g0",   "feather",  "pedals/digitech_bad_monkey/badmonkey_boost3_v10_b7_t6_g0_feather.nam"),
    ("boost3_g0",   "lite",     "pedals/digitech_bad_monkey/badmonkey_boost3_v10_b7_t6_g0_lite.nam"),
    ("boost3_g0",   "standard", "pedals/digitech_bad_monkey/badmonkey_boost3_v10_b7_t6_g0_standard.nam"),
    ("boost3_g4",   "feather",  "pedals/digitech_bad_monkey/badmonkey_boost3_v10_b7_t6_g4_feather.nam"),
    ("boost3_g4",   "lite",     "pedals/digitech_bad_monkey/badmonkey_boost3_v10_b7_t6_g4_lite.nam"),
    ("boost3_g4",   "standard", "pedals/digitech_bad_monkey/badmonkey_boost3_v10_b7_t6_g4_standard.nam"),
    ("fulltone",    "feather",  "pedals/digitech_bad_monkey/badmonkey_fulltone_feather.nam"),
    ("fulltone",    "lite",     "pedals/digitech_bad_monkey/badmonkey_fulltone_lite.nam"),
    ("fulltone",    "standard", "pedals/digitech_bad_monkey/badmonkey_fulltone_standard.nam"),
    ("glory",       "feather",  "pedals/digitech_bad_monkey/badmonkey_glory_feather.nam"),
    ("glory",       "lite",     "pedals/digitech_bad_monkey/badmonkey_glory_lite.nam"),
    ("glory",       "standard", "pedals/digitech_bad_monkey/badmonkey_glory_standard.nam"),
    ("klon",        "feather",  "pedals/digitech_bad_monkey/badmonkey_klon_feather.nam"),
    ("klon",        "lite",     "pedals/digitech_bad_monkey/badmonkey_klon_lite.nam"),
    ("klon",        "standard", "pedals/digitech_bad_monkey/badmonkey_klon_standard.nam"),
    ("noble_odr_1", "feather",  "pedals/digitech_bad_monkey/badmonkey_noble_odr_1_feather.nam"),
    ("noble_odr_1", "lite",     "pedals/digitech_bad_monkey/badmonkey_noble_odr_1_lite.nam"),
    ("noble_odr_1", "standard", "pedals/digitech_bad_monkey/badmonkey_noble_odr_1_standard.nam"),
    ("ts10",        "feather",  "pedals/digitech_bad_monkey/badmonkey_ts10_feather.nam"),
    ("ts10",        "lite",     "pedals/digitech_bad_monkey/badmonkey_ts10_lite.nam"),
    ("ts10",        "standard", "pedals/digitech_bad_monkey/badmonkey_ts10_standard.nam"),
    ("zen",         "feather",  "pedals/digitech_bad_monkey/badmonkey_zen_feather.nam"),
    ("zen",         "lite",     "pedals/digitech_bad_monkey/badmonkey_zen_lite.nam"),
    ("zen",         "standard", "pedals/digitech_bad_monkey/badmonkey_zen_standard.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Pedal"),
            Some("klon"),
            &[
                ("boost1",      "Boost (low gain)"),
                ("boost2",      "Boost (mid gain)"),
                ("boost3_g0",   "Boost Bright"),
                ("boost3_g4",   "Boost Bright + Drive"),
                ("fulltone",    "Fulltone OCD"),
                ("glory",       "Morning Glory"),
                ("klon",        "Klon Centaur"),
                ("noble_odr_1", "Nobels ODR-1"),
                ("ts10",        "Ibanez TS10"),
                ("zen",         "Hermida Zendrive"),
            ],
        ),
        enum_parameter(
            "size",
            "Model Size",
            Some("Asset"),
            Some("standard"),
            &[
                ("feather",  "Feather"),
                ("lite",     "Lite"),
                ("standard", "Standard"),
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
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let path = resolve_capture(params)?;
    Ok(format!("model='{}'", path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let voicing = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    let size = required_string(params, "size").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, s, _)| *v == voicing && *s == size)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for voicing={} size={}",
                MODEL_ID, voicing, size
            )
        })
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
