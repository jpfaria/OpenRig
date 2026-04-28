use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mesa_badlander";
pub const DISPLAY_NAME: &str = "Badlander";
const BRAND: &str = "mesa";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("s_pre_divine_sheep_07_artist", "S-[PRE] Divine Sheep #07 (Artist)", "amps/mesa_badlander/s_pre_divine_sheep_07_artist.nam"),
    ("s_amp_020w_bold_clean_clean_blue", "S-[AMP] 020W BOLD CLEAN Clean-Blues #02 (Factory)", "amps/mesa_badlander/s_amp_020w_bold_clean_clean_blues_02_factory.nam"),
    ("s_pre_divine_sheep_04_artist", "S-[PRE] Divine Sheep #04 (Artist)", "amps/mesa_badlander/s_pre_divine_sheep_04_artist.nam"),
    ("s_pre_noon_07_author", "S-[PRE] Noon #07 (Author)", "amps/mesa_badlander/s_pre_noon_07_author.nam"),
    ("s_pre_astro_horsey_02_ts_artist", "S-[PRE] Astro Horsey #02 TS (Artist)", "amps/mesa_badlander/s_pre_astro_horsey_02_ts_artist.nam"),
    ("s_amp_100w_bold_clean_scoopy_dew", "S-[AMP] 100W BOLD CLEAN Scoopy Dew #02 (Author)", "amps/mesa_badlander/s_amp_100w_bold_clean_scoopy_dew_02_author.nam"),
    ("s_amp_100w_bold_crush_murder_ton", "S-[AMP] 100W BOLD CRUSH Murder Tones #01 (Reviewer)", "amps/mesa_badlander/s_amp_100w_bold_crush_murder_tones_01_reviewer.nam"),
    ("s_amp_100w_bold_crush_mrscary_bu", "S-[AMP] 100W BOLD CRUSH MrScary Bull #04 (Reviewer)", "amps/mesa_badlander/s_amp_100w_bold_crush_mrscary_bull_04_reviewer.nam"),
    ("s_amp_100w_bold_crush_divine_she", "S-[AMP] 100W BOLD CRUSH Divine Sheep #01 (Artist)", "amps/mesa_badlander/s_amp_100w_bold_crush_divine_sheep_01_artist.nam"),
    ("s_pow_100w_bold_clean_pushed_02_", "S-[POW] 100W BOLD Clean-Pushed #02 (Factory)", "amps/mesa_badlander/s_pow_100w_bold_clean_pushed_02_factory.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("s_pre_divine_sheep_07_artist"),
        &[
            ("s_pre_divine_sheep_07_artist", "S-[PRE] Divine Sheep #07 (Artist)"),
            ("s_amp_020w_bold_clean_clean_blue", "S-[AMP] 020W BOLD CLEAN Clean-Blues #02 (Factory)"),
            ("s_pre_divine_sheep_04_artist", "S-[PRE] Divine Sheep #04 (Artist)"),
            ("s_pre_noon_07_author", "S-[PRE] Noon #07 (Author)"),
            ("s_pre_astro_horsey_02_ts_artist", "S-[PRE] Astro Horsey #02 TS (Artist)"),
            ("s_amp_100w_bold_clean_scoopy_dew", "S-[AMP] 100W BOLD CLEAN Scoopy Dew #02 (Author)"),
            ("s_amp_100w_bold_crush_murder_ton", "S-[AMP] 100W BOLD CRUSH Murder Tones #01 (Reviewer)"),
            ("s_amp_100w_bold_crush_mrscary_bu", "S-[AMP] 100W BOLD CRUSH MrScary Bull #04 (Reviewer)"),
            ("s_amp_100w_bold_crush_divine_she", "S-[AMP] 100W BOLD CRUSH Divine Sheep #01 (Artist)"),
            ("s_pow_100w_bold_clean_pushed_02_", "S-[POW] 100W BOLD Clean-Pushed #02 (Factory)"),
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
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let key = required_string(params, "capture").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("amp '{}' has no capture '{}'", MODEL_ID, key))
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
