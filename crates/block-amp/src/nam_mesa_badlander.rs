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

// Two-axis pack: stage × tone preset.
// stage = PRE (preamp DI) / AMP (full amp+cab) / POW (power amp).
const CAPTURES: &[(&str, &str, &str)] = &[
    // (stage, tone, file)
    ("pre", "divine_sheep_04",   "amps/mesa_badlander/s_pre_divine_sheep_04_artist.nam"),
    ("pre", "divine_sheep_07",   "amps/mesa_badlander/s_pre_divine_sheep_07_artist.nam"),
    ("pre", "noon_07",           "amps/mesa_badlander/s_pre_noon_07_author.nam"),
    ("pre", "astro_horsey_02",   "amps/mesa_badlander/s_pre_astro_horsey_02_ts_artist.nam"),
    ("amp", "020w_clean_blues",  "amps/mesa_badlander/s_amp_020w_bold_clean_clean_blues_02_factory.nam"),
    ("amp", "100w_scoopy_dew",   "amps/mesa_badlander/s_amp_100w_bold_clean_scoopy_dew_02_author.nam"),
    ("amp", "100w_murder_tones", "amps/mesa_badlander/s_amp_100w_bold_crush_murder_tones_01_reviewer.nam"),
    ("amp", "100w_mrscary_bull", "amps/mesa_badlander/s_amp_100w_bold_crush_mrscary_bull_04_reviewer.nam"),
    ("amp", "100w_divine_sheep", "amps/mesa_badlander/s_amp_100w_bold_crush_divine_sheep_01_artist.nam"),
    ("pow", "100w_clean_pushed", "amps/mesa_badlander/s_pow_100w_bold_clean_pushed_02_factory.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "stage",
            "Stage",
            Some("Amp"),
            Some("amp"),
            &[
                ("pre", "Preamp"),
                ("amp", "Full Amp"),
                ("pow", "Power Amp"),
            ],
        ),
        enum_parameter(
            "tone",
            "Tone",
            Some("Amp"),
            Some("100w_murder_tones"),
            &[
                ("020w_clean_blues",  "020W Clean Blues"),
                ("100w_scoopy_dew",   "100W Scoopy Dew"),
                ("100w_clean_pushed", "100W Clean Pushed"),
                ("100w_murder_tones", "100W Murder Tones"),
                ("100w_mrscary_bull", "100W MrScary Bull"),
                ("100w_divine_sheep", "100W Divine Sheep"),
                ("divine_sheep_04",   "Divine Sheep #04"),
                ("divine_sheep_07",   "Divine Sheep #07"),
                ("noon_07",           "Noon #07"),
                ("astro_horsey_02",   "Astro Horsey #02"),
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
    let stage = required_string(params, "stage").map_err(anyhow::Error::msg)?;
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(s, t, _)| *s == stage && *t == tone)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for stage={} tone={}",
                MODEL_ID, stage, tone
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
