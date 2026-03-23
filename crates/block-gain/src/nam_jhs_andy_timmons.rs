use anyhow::{anyhow, Result};
use asset_runtime::{materialize, EmbeddedAsset};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{
    enum_parameter, required_string, ModelParameterSchema, ParameterSet,
};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_jhs_andy_timmons";
pub const DISPLAY_NAME: &str = "Andy Timmons";
const BRAND: &str = "jhs";

pub const NAM_PLUGIN_DEFAULTS: NamPluginParams = NamPluginParams {
    input_level_db: 0.0,
    output_level_db: 0.0,
    noise_gate_threshold_db: -80.0,
    noise_gate_enabled: true,
    eq_enabled: false,
    bass: 5.0,
    middle: 5.0,
    treble: 5.0,
};

macro_rules! capture {
    ($voicing:expr, $boost:expr, $quality:expr, $asset_id:literal, $relative_path:literal) => {
        JhsAndyTimmonsCapture {
            voicing: $voicing,
            boost: $boost,
            quality: $quality,
            asset: EmbeddedAsset::new(
                $asset_id,
                $relative_path,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../",
                    $relative_path
                )),
            ),
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voicing {
    Bright,
    Dark,
    Main,
    DeliverUs,
    MainBrighter,
    Scoopy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boost {
    Off,
    On,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    C,
    S,
    Xs,
}

#[derive(Debug, Clone, Copy)]
pub struct JhsAndyTimmonsCapture {
    pub voicing: Voicing,
    pub boost: Boost,
    pub quality: Quality,
    pub asset: EmbeddedAsset,
}

pub const CAPTURES: &[JhsAndyTimmonsCapture] = &[
    // BRIGHT
    capture!(Voicing::Bright, Boost::On,  Quality::C,  "gain.nam_jhs_andy_timmons.bright.boost.c",  "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_BRIGHT_BOOST_C.nam"),
    capture!(Voicing::Bright, Boost::On,  Quality::S,  "gain.nam_jhs_andy_timmons.bright.boost.s",  "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_BRIGHT_BOOST_S.nam"),
    capture!(Voicing::Bright, Boost::On,  Quality::Xs, "gain.nam_jhs_andy_timmons.bright.boost.xs", "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_BRIGHT_BOOST_XS.nam"),
    capture!(Voicing::Bright, Boost::Off, Quality::C,  "gain.nam_jhs_andy_timmons.bright.c",        "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_BRIGHT_C.nam"),
    capture!(Voicing::Bright, Boost::Off, Quality::S,  "gain.nam_jhs_andy_timmons.bright.s",        "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_BRIGHT_S.nam"),
    capture!(Voicing::Bright, Boost::Off, Quality::Xs, "gain.nam_jhs_andy_timmons.bright.xs",       "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_BRIGHT_XS.nam"),
    // DARK
    capture!(Voicing::Dark, Boost::On,  Quality::C,  "gain.nam_jhs_andy_timmons.dark.boost.c",  "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_DARK_BOOST_C.nam"),
    capture!(Voicing::Dark, Boost::On,  Quality::S,  "gain.nam_jhs_andy_timmons.dark.boost.s",  "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_DARK_BOOST_S.nam"),
    capture!(Voicing::Dark, Boost::On,  Quality::Xs, "gain.nam_jhs_andy_timmons.dark.boost.xs", "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_DARK_BOOST_XS.nam"),
    capture!(Voicing::Dark, Boost::Off, Quality::C,  "gain.nam_jhs_andy_timmons.dark.c",        "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_DARK_C.nam"),
    capture!(Voicing::Dark, Boost::Off, Quality::S,  "gain.nam_jhs_andy_timmons.dark.s",        "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_DARK_S.nam"),
    capture!(Voicing::Dark, Boost::Off, Quality::Xs, "gain.nam_jhs_andy_timmons.dark.xs",       "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_DARK_XS.nam"),
    // MAIN
    capture!(Voicing::Main, Boost::On,  Quality::C,  "gain.nam_jhs_andy_timmons.main.boost.c",  "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_MAIN_BOOST_C.nam"),
    capture!(Voicing::Main, Boost::On,  Quality::S,  "gain.nam_jhs_andy_timmons.main.boost.s",  "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_MAIN_BOOST_S.nam"),
    capture!(Voicing::Main, Boost::On,  Quality::Xs, "gain.nam_jhs_andy_timmons.main.boost.xs", "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_MAIN_BOOST_XS.nam"),
    capture!(Voicing::Main, Boost::Off, Quality::C,  "gain.nam_jhs_andy_timmons.main.c",        "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_MAIN_C.nam"),
    capture!(Voicing::Main, Boost::Off, Quality::S,  "gain.nam_jhs_andy_timmons.main.s",        "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_MAIN_S.nam"),
    capture!(Voicing::Main, Boost::Off, Quality::Xs, "gain.nam_jhs_andy_timmons.main.xs",       "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_JHS_TIMMONS_MAIN_XS.nam"),
    // DELIVER US (Rivera)
    capture!(Voicing::DeliverUs, Boost::Off, Quality::C,  "gain.nam_jhs_andy_timmons.deliver_us.c",  "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_TIMMONS_RIVERA_DELIVER_US_C.nam"),
    capture!(Voicing::DeliverUs, Boost::Off, Quality::S,  "gain.nam_jhs_andy_timmons.deliver_us.s",  "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_TIMMONS_RIVERA_DELIVER_US_S.nam"),
    capture!(Voicing::DeliverUs, Boost::Off, Quality::Xs, "gain.nam_jhs_andy_timmons.deliver_us.xs", "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_TIMMONS_RIVERA_DELIVER_US_XS.nam"),
    // MAIN BRIGHTER (Rivera)
    capture!(Voicing::MainBrighter, Boost::On,  Quality::C,  "gain.nam_jhs_andy_timmons.main_brighter.boost.c",  "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_TIMMONS_RIVERA_MAIN_BRIGHTER_BOOST_C.nam"),
    capture!(Voicing::MainBrighter, Boost::On,  Quality::S,  "gain.nam_jhs_andy_timmons.main_brighter.boost.s",  "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_TIMMONS_RIVERA_MAIN_BRIGHTER_BOOST_S.nam"),
    capture!(Voicing::MainBrighter, Boost::On,  Quality::Xs, "gain.nam_jhs_andy_timmons.main_brighter.boost.xs", "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_TIMMONS_RIVERA_MAIN_BRIGHTER_BOOST_XS.nam"),
    capture!(Voicing::MainBrighter, Boost::Off, Quality::C,  "gain.nam_jhs_andy_timmons.main_brighter.c",        "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_TIMMONS_RIVERA_MAIN_BRIGHTER_C.nam"),
    capture!(Voicing::MainBrighter, Boost::Off, Quality::S,  "gain.nam_jhs_andy_timmons.main_brighter.s",        "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_TIMMONS_RIVERA_MAIN_BRIGHTER_S.nam"),
    capture!(Voicing::MainBrighter, Boost::Off, Quality::Xs, "gain.nam_jhs_andy_timmons.main_brighter.xs",       "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_TIMMONS_RIVERA_MAIN_BRIGHTER_XS.nam"),
    // SCOOPY (Rivera)
    capture!(Voicing::Scoopy, Boost::On,  Quality::C,  "gain.nam_jhs_andy_timmons.scoopy.boost.c",  "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_TIMMONS_RIVERA_SCOOPY_BOOST_C.nam"),
    capture!(Voicing::Scoopy, Boost::On,  Quality::S,  "gain.nam_jhs_andy_timmons.scoopy.boost.s",  "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_TIMMONS_RIVERA_SCOOPY_BOOST_S.nam"),
    capture!(Voicing::Scoopy, Boost::On,  Quality::Xs, "gain.nam_jhs_andy_timmons.scoopy.boost.xs", "captures/nam/pedals/jhs_andy_timmons/SLAMMIN_TIMMONS_RIVERA_SCOOPY_BOOST_XS.nam"),
];

fn parse_voicing(value: &str) -> Result<Voicing> {
    match value {
        "bright" => Ok(Voicing::Bright),
        "dark" => Ok(Voicing::Dark),
        "main" => Ok(Voicing::Main),
        "deliver_us" => Ok(Voicing::DeliverUs),
        "main_brighter" => Ok(Voicing::MainBrighter),
        "scoopy" => Ok(Voicing::Scoopy),
        _ => Err(anyhow!("invalid voicing '{}' for model '{}'", value, MODEL_ID)),
    }
}

fn parse_boost(value: &str) -> Result<Boost> {
    match value {
        "off" => Ok(Boost::Off),
        "on" => Ok(Boost::On),
        _ => Err(anyhow!("invalid boost '{}' for model '{}'", value, MODEL_ID)),
    }
}

fn parse_quality(value: &str) -> Result<Quality> {
    match value {
        "c" => Ok(Quality::C),
        "s" => Ok(Quality::S),
        "xs" => Ok(Quality::Xs),
        _ => Err(anyhow!("invalid quality '{}' for model '{}'", value, MODEL_ID)),
    }
}

fn voicing_priority(v: &Voicing) -> i32 {
    match v {
        Voicing::Bright => 0,
        Voicing::Dark => 1,
        Voicing::Main => 2,
        Voicing::DeliverUs => 3,
        Voicing::MainBrighter => 4,
        Voicing::Scoopy => 5,
    }
}

fn boost_priority(b: &Boost) -> i32 {
    match b {
        Boost::Off => 0,
        Boost::On => 1,
    }
}

fn quality_priority(q: &Quality) -> i32 {
    match q {
        Quality::C => 0,
        Quality::S => 1,
        Quality::Xs => 2,
    }
}

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Tone"),
            Some("main"),
            &[
                ("bright", "Bright"),
                ("dark", "Dark"),
                ("main", "Main"),
                ("deliver_us", "Deliver Us"),
                ("main_brighter", "Main Brighter"),
                ("scoopy", "Scoopy"),
            ],
        ),
        enum_parameter(
            "boost",
            "Boost",
            Some("Gain"),
            Some("off"),
            &[("off", "Off"), ("on", "On")],
        ),
        enum_parameter(
            "quality",
            "Quality",
            Some("Model"),
            Some("s"),
            &[("c", "Compact"), ("s", "Standard"), ("xs", "Extra Standard")],
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
    let model_path = materialize(&capture.asset)?;
    build_processor_with_assets_for_layout(
        &model_path.to_string_lossy(),
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

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("asset_id='{}'", capture.asset.id))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static JhsAndyTimmonsCapture> {
    let voicing = parse_voicing(&required_string(params, "voicing").map_err(anyhow::Error::msg)?)?;
    let boost = parse_boost(&required_string(params, "boost").map_err(anyhow::Error::msg)?)?;
    let quality = parse_quality(&required_string(params, "quality").map_err(anyhow::Error::msg)?)?;

    // Try exact match first
    if let Some(capture) = CAPTURES.iter().find(|c| c.voicing == voicing && c.boost == boost && c.quality == quality) {
        return Ok(capture);
    }

    // Find nearest match: voicing first, then boost, then quality
    CAPTURES
        .iter()
        .min_by_key(|c| {
            let dv = (voicing_priority(&c.voicing) - voicing_priority(&voicing)).abs() * 1000;
            let db = (boost_priority(&c.boost) - boost_priority(&boost)).abs() * 100;
            let dq = (quality_priority(&c.quality) - quality_priority(&quality)).abs();
            dv + db + dq
        })
        .ok_or_else(|| anyhow!("no captures available for model '{}'", MODEL_ID))
}
