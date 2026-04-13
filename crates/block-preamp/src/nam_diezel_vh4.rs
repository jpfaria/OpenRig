use anyhow::{anyhow, Result};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{
    enum_parameter, required_string, ModelParameterSchema, ParameterSet,
};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_diezel_vh4";
pub const DISPLAY_NAME: &str = "VH4";
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

macro_rules! capture {
    ($channel:expr, $gain:expr, $boost:expr, $nam_file:literal) => {
        DiezelVh4Capture {
            channel: $channel,
            gain: $gain,
            boost: $boost,
            nam_file: $nam_file,
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Ch2,
    Ch3,
    Ch4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GainLevel {
    BrLg,
    BrMg,
    Lg01,
    Lg02,
    Lg03,
    Mg01,
    Mg02,
    Mg03,
    Hg01,
    Hg02,
    Hg03,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boost {
    None,
    Ts9,
}

#[derive(Debug, Clone, Copy)]
pub struct DiezelVh4Capture {
    pub channel: Channel,
    pub gain: GainLevel,
    pub boost: Boost,
    pub nam_file: &'static str,
}

pub const CAPTURES: &[DiezelVh4Capture] = &[
    // Channel 2 — no boost
    capture!(Channel::Ch2, GainLevel::BrLg, Boost::None, "preamp/diezel_vh4/diezel_vh4_ch_2_br_lg.nam"),
    capture!(Channel::Ch2, GainLevel::BrMg, Boost::None, "preamp/diezel_vh4/diezel_vh4_ch_2_br_mg.nam"),
    capture!(Channel::Ch2, GainLevel::Lg01, Boost::None, "preamp/diezel_vh4/diezel_vh4_ch_2_lg_01.nam"),
    capture!(Channel::Ch2, GainLevel::Lg02, Boost::None, "preamp/diezel_vh4/diezel_vh4_ch_2_lg_02.nam"),
    capture!(Channel::Ch2, GainLevel::Lg03, Boost::None, "preamp/diezel_vh4/diezel_vh4_ch_2_lg_03.nam"),
    capture!(Channel::Ch2, GainLevel::Mg01, Boost::None, "preamp/diezel_vh4/diezel_vh4_ch_2_mg_01.nam"),
    capture!(Channel::Ch2, GainLevel::Mg02, Boost::None, "preamp/diezel_vh4/diezel_vh4_ch_2_mg_02.nam"),
    capture!(Channel::Ch2, GainLevel::Mg03, Boost::None, "preamp/diezel_vh4/diezel_vh4_ch_2_mg_03.nam"),
    // Channel 3 — no boost
    capture!(Channel::Ch3, GainLevel::Hg01, Boost::None, "preamp/diezel_vh4/diezel_vh4_ch_3_hg_01.nam"),
    capture!(Channel::Ch3, GainLevel::Hg02, Boost::None, "preamp/diezel_vh4/diezel_vh4_ch_3_hg_02.nam"),
    capture!(Channel::Ch3, GainLevel::Hg03, Boost::None, "preamp/diezel_vh4/diezel_vh4_ch_3_hg_03.nam"),
    // Channel 3 — TS9 boost
    capture!(Channel::Ch3, GainLevel::Hg01, Boost::Ts9, "preamp/diezel_vh4/diezel_vh4_ch_3_ts9_hg_01.nam"),
    capture!(Channel::Ch3, GainLevel::Hg02, Boost::Ts9, "preamp/diezel_vh4/diezel_vh4_ch_3_ts9_hg_02.nam"),
    capture!(Channel::Ch3, GainLevel::Hg03, Boost::Ts9, "preamp/diezel_vh4/diezel_vh4_ch_3_ts9_hg_03.nam"),
    // Channel 4 — no boost
    capture!(Channel::Ch4, GainLevel::Hg01, Boost::None, "preamp/diezel_vh4/diezel_vh4_ch_4_hg_01.nam"),
    capture!(Channel::Ch4, GainLevel::Hg02, Boost::None, "preamp/diezel_vh4/diezel_vh4_ch_4_hg_02.nam"),
    capture!(Channel::Ch4, GainLevel::Hg03, Boost::None, "preamp/diezel_vh4/diezel_vh4_ch_4_hg_03.nam"),
    // Channel 4 — TS9 boost
    capture!(Channel::Ch4, GainLevel::Hg01, Boost::Ts9, "preamp/diezel_vh4/diezel_vh4_ch_4_ts9_hg_01.nam"),
    capture!(Channel::Ch4, GainLevel::Hg02, Boost::Ts9, "preamp/diezel_vh4/diezel_vh4_ch_4_ts9_hg_02.nam"),
    capture!(Channel::Ch4, GainLevel::Hg03, Boost::Ts9, "preamp/diezel_vh4/diezel_vh4_ch_4_ts9_hg_03.nam"),
];

fn parse_channel(value: &str) -> Result<Channel> {
    match value {
        "2" => Ok(Channel::Ch2),
        "3" => Ok(Channel::Ch3),
        "4" => Ok(Channel::Ch4),
        _ => Err(anyhow!("invalid channel '{}' for model '{}'", value, MODEL_ID)),
    }
}

fn parse_gain(value: &str) -> Result<GainLevel> {
    match value {
        "br_lg" => Ok(GainLevel::BrLg),
        "br_mg" => Ok(GainLevel::BrMg),
        "lg_01" => Ok(GainLevel::Lg01),
        "lg_02" => Ok(GainLevel::Lg02),
        "lg_03" => Ok(GainLevel::Lg03),
        "mg_01" => Ok(GainLevel::Mg01),
        "mg_02" => Ok(GainLevel::Mg02),
        "mg_03" => Ok(GainLevel::Mg03),
        "hg_01" => Ok(GainLevel::Hg01),
        "hg_02" => Ok(GainLevel::Hg02),
        "hg_03" => Ok(GainLevel::Hg03),
        _ => Err(anyhow!("invalid gain '{}' for model '{}'", value, MODEL_ID)),
    }
}

fn parse_boost(value: &str) -> Result<Boost> {
    match value {
        "none" => Ok(Boost::None),
        "ts9" => Ok(Boost::Ts9),
        _ => Err(anyhow!("invalid boost '{}' for model '{}'", value, MODEL_ID)),
    }
}

fn gain_priority(g: &GainLevel) -> i32 {
    match g {
        GainLevel::BrLg => 0,
        GainLevel::BrMg => 1,
        GainLevel::Lg01 => 2,
        GainLevel::Lg02 => 3,
        GainLevel::Lg03 => 4,
        GainLevel::Mg01 => 5,
        GainLevel::Mg02 => 6,
        GainLevel::Mg03 => 7,
        GainLevel::Hg01 => 8,
        GainLevel::Hg02 => 9,
        GainLevel::Hg03 => 10,
    }
}

fn channel_priority(c: &Channel) -> i32 {
    match c {
        Channel::Ch2 => 2,
        Channel::Ch3 => 3,
        Channel::Ch4 => 4,
    }
}

fn boost_priority(b: &Boost) -> i32 {
    match b {
        Boost::None => 0,
        Boost::Ts9 => 1,
    }
}

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_PREAMP, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "channel",
            "Channel",
            Some("Amp"),
            Some("3"),
            &[("2", "Ch 2"), ("3", "Ch 3"), ("4", "Ch 4")],
        ),
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Amp"),
            Some("high"),
            &[("bright", "Bright"), ("low", "Low"), ("mid", "Mid"), ("high", "High")],
        ),
        enum_parameter(
            "gain_level",
            "Gain",
            Some("Amp"),
            Some("1"),
            &[("1", "1"), ("2", "2"), ("3", "3")],
        ),
        enum_parameter(
            "boost",
            "Boost",
            Some("Amp"),
            Some("none"),
            &[("none", "None"), ("ts9", "TS9")],
        ),
    ];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let capture = resolve_capture(params)?;
    let plugin_params = plugin_params_from_set_with_defaults(params, NAM_PLUGIN_DEFAULTS)?;
    let model_path = nam::resolve_nam_capture(capture.nam_file)?;
    build_processor_with_assets_for_layout(
        &model_path,
        None,
        plugin_params,
        sample_rate,
        layout,
    )
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
    let capture = resolve_capture(params)?;
    Ok(format!("asset_id='{}'", capture.nam_file))
}

fn voicing_gain_to_gain_level(voicing: &str, level: &str) -> GainLevel {
    match (voicing, level) {
        ("bright", "1") | ("bright", "2") | ("bright", "3") => {
            // Bright only has Low and Mid variants
            if level == "1" { GainLevel::BrLg } else { GainLevel::BrMg }
        }
        ("low", "1") => GainLevel::Lg01,
        ("low", "2") => GainLevel::Lg02,
        ("low", _)   => GainLevel::Lg03,
        ("mid", "1") => GainLevel::Mg01,
        ("mid", "2") => GainLevel::Mg02,
        ("mid", _)   => GainLevel::Mg03,
        ("high", "1") => GainLevel::Hg01,
        ("high", "2") => GainLevel::Hg02,
        ("high", _)   => GainLevel::Hg03,
        _ => GainLevel::Hg01,
    }
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static DiezelVh4Capture> {
    let channel = parse_channel(&required_string(params, "channel").map_err(anyhow::Error::msg)?)?;
    let boost = parse_boost(&required_string(params, "boost").map_err(anyhow::Error::msg)?)?;

    // Support both old "gain" param (combined) and new "voicing"+"gain_level" (separate)
    let gain = if let Ok(voicing) = required_string(params, "voicing").map_err(anyhow::Error::msg) {
        let level = required_string(params, "gain_level").map_err(anyhow::Error::msg).unwrap_or_else(|_| "1".to_string());
        voicing_gain_to_gain_level(&voicing, &level)
    } else {
        parse_gain(&required_string(params, "gain").map_err(anyhow::Error::msg)?)?
    };

    // Try exact match first
    if let Some(capture) = CAPTURES.iter().find(|c| c.channel == channel && c.gain == gain && c.boost == boost) {
        return Ok(capture);
    }

    // Find nearest match: channel first, then boost, then gain
    CAPTURES
        .iter()
        .min_by_key(|c| {
            let dc = (channel_priority(&c.channel) - channel_priority(&channel)).abs() * 1000;
            let db = (boost_priority(&c.boost) - boost_priority(&boost)).abs() * 100;
            let dg = (gain_priority(&c.gain) - gain_priority(&gain)).abs();
            dc + db + dg
        })
        .ok_or_else(|| anyhow!("no captures available for model '{}'", MODEL_ID))
}
