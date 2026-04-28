use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_orange_or15";
pub const DISPLAY_NAME: &str = "OR15";
const BRAND: &str = "orange";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: channel × stacked pedal.
// All captures use Feather output. Holes return Err.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (channel, pedal, file)
    ("clean",  "fuzz_rasputin", "amps/orange_or15/orange_or15_clean_with_foxpedal_defector_fuzz_rasputin_mode_feather.nam"),
    ("clean",  "fuzz_defector", "amps/orange_or15/orange_or15_clean_with_foxpedal_defector_fuzz_feather.nam"),
    ("clean",  "boss_hm_2w",    "amps/orange_or15/orange_or15_clean_with_boss_hm_2w_heavy_metal_pedal_feather.nam"),
    ("clean",  "tc_eyemaster",  "amps/orange_or15/orange_or15_clean_with_tc_electronic_eyemaster_feather.nam"),
    ("clean",  "sf300_v1",      "amps/orange_or15/orange_or15_clean_with_behringer_sf300_super_fuzz_mode_1_5_1_feather.nam"),
    ("clean",  "sf300_v2",      "amps/orange_or15/orange_or15_clean_with_behringer_sf300_super_fuzz_mode_1_5_feather.nam"),
    ("crunch", "mud_killer",    "amps/orange_or15/orange_or15_crunch_with_eea_mud_killer_fat_boost_feather.nam"),
    ("crunch", "mk_into_grn",   "amps/orange_or15/orange_or15_crunch_with_eea_mudkiller_into_ehx_green_russian_feather.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Amp"),
            Some("clean"),
            &[
                ("clean",  "Clean"),
                ("crunch", "Crunch"),
            ],
        ),
        enum_parameter(
            "pedal",
            "Stacked Pedal",
            Some("Amp"),
            Some("boss_hm_2w"),
            &[
                ("fuzz_rasputin", "Foxpedal Rasputin"),
                ("fuzz_defector", "Foxpedal Defector"),
                ("boss_hm_2w",    "Boss HM-2w"),
                ("tc_eyemaster",  "TC Eyemaster"),
                ("sf300_v1",      "SF300 SuperFuzz (V1)"),
                ("sf300_v2",      "SF300 SuperFuzz (V2)"),
                ("mud_killer",    "EEA Mud Killer"),
                ("mk_into_grn",   "MudKiller > Russian"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let channel = required_string(params, "channel").map_err(anyhow::Error::msg)?;
    let pedal = required_string(params, "pedal").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(c, p, _)| *c == channel && *p == pedal)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for channel={} pedal={}",
                MODEL_ID, channel, pedal
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

pub const MODEL_DEFINITION: AmpModelDefinition = AmpModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: AmpBackendKind::Nam,
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
    Ok(format!("model='{}'", path))
}
