use anyhow::{anyhow, Result};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{
        plugin_parameter_specs_with_defaults, plugin_params_from_set_with_defaults, NamPluginParams,
    },
};
use stage_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use stage_core::{AudioChannelLayout, StageProcessor};

pub const MODEL_ID: &str = "marshall_jcm_800";

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarshallJcm800Params {
    pub volume: i32,
    pub presence: i32,
    pub bass: i32,
    pub middle: i32,
    pub treble: i32,
    pub gain: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarshallJcm800Capture {
    pub params: MarshallJcm800Params,
    pub model_path: &'static str,
}

pub const CAPTURES: &[MarshallJcm800Capture] = &[
    capture(50, 50, 50, 50, 50, 10, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV5 G1 - AZG - 700.nam"),
    capture(50, 50, 50, 50, 50, 20, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV5 G2 - AZG - 700.nam"),
    capture(50, 50, 50, 50, 50, 30, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV5 G3 - AZG - 700.nam"),
    capture(50, 50, 50, 50, 50, 40, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV5 G4 - AZG - 700.nam"),
    capture(50, 50, 50, 50, 50, 50, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV5 G5 - AZG - 700.nam"),
    capture(50, 50, 50, 50, 50, 60, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV5 G6 - AZG - 700.nam"),
    capture(50, 50, 50, 50, 50, 70, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV5 G7 - AZG - 700.nam"),
    capture(50, 50, 50, 50, 50, 80, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV5 G8 - AZG - 700.nam"),
    capture(50, 50, 50, 50, 50, 90, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV5 G9 - AZG - 700.nam"),
    capture(50, 50, 50, 50, 50, 100, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV5 G10 - AZG - 700.nam"),
    capture(60, 50, 50, 50, 50, 10, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV6 G1 - AZG - 700.nam"),
    capture(60, 50, 50, 50, 50, 20, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV6 G2 - AZG - 700.nam"),
    capture(60, 50, 50, 50, 50, 30, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV6 G3 - AZG - 700.nam"),
    capture(60, 50, 50, 50, 50, 40, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV6 G4 - AZG - 700.nam"),
    capture(60, 50, 50, 50, 50, 50, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV6 G5 - AZG - 700.nam"),
    capture(60, 50, 50, 50, 50, 60, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV6 G6 - AZG - 700.nam"),
    capture(60, 50, 50, 50, 50, 70, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV6 G7 - AZG - 700.nam"),
    capture(60, 50, 50, 50, 50, 80, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV6 G8 - AZG - 700.nam"),
    capture(60, 50, 50, 50, 50, 90, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV6 G9 - AZG - 700.nam"),
    capture(60, 50, 50, 50, 50, 100, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV6 G10 - AZG - 700.nam"),
    capture(70, 50, 50, 50, 50, 10, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV7 G1 - AZG - 700.nam"),
    capture(70, 50, 50, 50, 50, 20, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV7 G2 - AZG - 700.nam"),
    capture(70, 50, 50, 50, 50, 30, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV7 G3 - AZG - 700.nam"),
    capture(70, 50, 50, 50, 50, 40, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV7 G4 - AZG - 700.nam"),
    capture(70, 50, 50, 50, 50, 50, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV7 G5 - AZG - 700.nam"),
    capture(70, 50, 50, 50, 50, 60, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV7 G6 - AZG - 700.nam"),
    capture(70, 50, 50, 50, 50, 70, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV7 G7 - AZG - 700.nam"),
    capture(70, 50, 50, 50, 50, 80, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV7 G8 - AZG - 700.nam"),
    capture(70, 50, 50, 50, 50, 90, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV7 G9 - AZG - 700.nam"),
    capture(70, 50, 50, 50, 50, 100, "captures/nam/amps/heads/Marshall JCM 800 2203/JCM800 2203 - P5 B5 M5 T5 MV7 G10 - AZG - 700.nam"),
];

pub fn supports_model(model: &str) -> bool {
    matches!(model, MODEL_ID | "jcm800" | "jcm_800")
}

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, "Marshall JCM 800", false);
    let mut parameters = vec![
        float_parameter(
            "volume",
            "Volume",
            Some("Amp"),
            Some(50.0),
            50.0,
            70.0,
            10.0,
            ParameterUnit::Percent,
        ),
        float_parameter(
            "bass",
            "Bass",
            Some("Amp"),
            Some(50.0),
            50.0,
            50.0,
            0.0,
            ParameterUnit::Percent,
        ),
        float_parameter(
            "presence",
            "Presence",
            Some("Amp"),
            Some(50.0),
            50.0,
            50.0,
            0.0,
            ParameterUnit::Percent,
        ),
        float_parameter(
            "middle",
            "Middle",
            Some("Amp"),
            Some(50.0),
            50.0,
            50.0,
            0.0,
            ParameterUnit::Percent,
        ),
        float_parameter(
            "treble",
            "Treble",
            Some("Amp"),
            Some(50.0),
            50.0,
            50.0,
            0.0,
            ParameterUnit::Percent,
        ),
        float_parameter(
            "gain",
            "Gain",
            Some("Amp"),
            Some(40.0),
            10.0,
            100.0,
            10.0,
            ParameterUnit::Percent,
        ),
    ];
    parameters.extend(plugin_parameter_specs_with_defaults(NAM_PLUGIN_DEFAULTS));
    schema.parameters = parameters;
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    let capture = resolve_capture(params)?;
    let plugin_params = plugin_params_from_set_with_defaults(params, NAM_PLUGIN_DEFAULTS)?;
    build_processor_with_assets_for_layout(capture.model_path, None, plugin_params, layout)
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("model='{}'", capture.model_path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static MarshallJcm800Capture> {
    let requested = MarshallJcm800Params {
        volume: read_percent(params, "volume")?,
        presence: read_percent(params, "presence")?,
        bass: read_percent(params, "bass")?,
        middle: read_percent(params, "middle")?,
        treble: read_percent(params, "treble")?,
        gain: read_percent(params, "gain")?,
    };

    CAPTURES
        .iter()
        .find(|capture| capture.params == requested)
        .ok_or_else(|| {
            anyhow!(
                "amp model '{}' does not support volume={} presence={} bass={} middle={} treble={} gain={}",
                MODEL_ID,
                requested.volume,
                requested.presence,
                requested.bass,
                requested.middle,
                requested.treble,
                requested.gain
            )
        })
}

fn read_percent(params: &ParameterSet, path: &str) -> Result<i32> {
    let value = required_f32(params, path).map_err(anyhow::Error::msg)?;
    let rounded = value.round();
    if (value - rounded).abs() > 1e-4 {
        return Err(anyhow!(
            "amp model '{}' requires '{}' to be a whole-number percentage, got {}",
            MODEL_ID,
            path,
            value
        ));
    }
    Ok(rounded as i32)
}

const fn capture(
    volume: i32,
    presence: i32,
    bass: i32,
    middle: i32,
    treble: i32,
    gain: i32,
    model_path: &'static str,
) -> MarshallJcm800Capture {
    MarshallJcm800Capture {
        params: MarshallJcm800Params {
            volume,
            presence,
            bass,
            middle,
            treble,
            gain,
        },
        model_path,
    }
}
