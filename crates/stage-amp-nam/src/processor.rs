use anyhow::{bail, Result};
use domain::value_objects::ParameterValue;
use stage_core::param::{
    bool_parameter, file_path_parameter, float_parameter, optional_string, required_bool,
    required_f32, required_string, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use stage_core::{ModelChannelSupport, MonoProcessor};
use std::ffi::CString;
use std::os::raw::{c_char, c_void};

pub const DEFAULT_NAM_MODEL: &str = "neural_amp_modeler";

pub fn supports_model(model: &str) -> bool {
    model == DEFAULT_NAM_MODEL
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "nam".to_string(),
        model: DEFAULT_NAM_MODEL.to_string(),
        display_name: "Neural Amp Modeler".to_string(),
        channel_support: ModelChannelSupport::Mono,
        stereo_processing: None,
        parameters: vec![
            file_path_parameter("model_path", "Model", None, None, &["nam"], false),
            file_path_parameter(
                "ir_path",
                "Impulse Response",
                None,
                Some(ParameterValue::Null),
                &["wav"],
                true,
            ),
            float_parameter(
                "input_db",
                "Input",
                None,
                Some(0.0),
                -24.0,
                24.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            float_parameter(
                "output_db",
                "Output",
                None,
                Some(0.0),
                -24.0,
                24.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            bool_parameter(
                "noise_gate.enabled",
                "Enabled",
                Some("Noise Gate"),
                Some(true),
            ),
            float_parameter(
                "noise_gate.threshold_db",
                "Threshold",
                Some("Noise Gate"),
                Some(-80.0),
                -96.0,
                0.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            bool_parameter("eq.enabled", "Enabled", Some("EQ"), Some(true)),
            float_parameter(
                "eq.bass",
                "Bass",
                Some("EQ"),
                Some(5.0),
                0.0,
                10.0,
                0.1,
                ParameterUnit::None,
            ),
            float_parameter(
                "eq.middle",
                "Middle",
                Some("EQ"),
                Some(5.0),
                0.0,
                10.0,
                0.1,
                ParameterUnit::None,
            ),
            float_parameter(
                "eq.treble",
                "Treble",
                Some("EQ"),
                Some(5.0),
                0.0,
                10.0,
                0.1,
                ParameterUnit::None,
            ),
            bool_parameter("ir_enabled", "IR Enabled", None, Some(true)),
        ],
    }
}

#[repr(C)]
struct NamPluginConfig {
    model_path_utf8: *const c_char,
    ir_path_utf8: *const c_char,
    input_db: f32,
    output_db: f32,
    noise_gate_threshold_db: f32,
    bass: f32,
    middle: f32,
    treble: f32,
    noise_gate_enabled: u8,
    eq_enabled: u8,
    ir_enabled: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct NamPluginParams {
    pub input_level_db: f32,
    pub output_level_db: f32,
    pub noise_gate_threshold_db: f32,
    pub noise_gate_enabled: bool,
    pub eq_enabled: bool,
    pub ir_enabled: bool,
    pub bass: f32,
    pub middle: f32,
    pub treble: f32,
}

pub fn params_from_set(params: &ParameterSet) -> Result<(String, Option<String>, NamPluginParams)> {
    Ok((
        required_string(params, "model_path").map_err(anyhow::Error::msg)?,
        optional_string(params, "ir_path"),
        NamPluginParams {
            input_level_db: required_f32(params, "input_db").map_err(anyhow::Error::msg)?,
            output_level_db: required_f32(params, "output_db").map_err(anyhow::Error::msg)?,
            noise_gate_threshold_db: required_f32(params, "noise_gate.threshold_db")
                .map_err(anyhow::Error::msg)?,
            noise_gate_enabled: required_bool(params, "noise_gate.enabled")
                .map_err(anyhow::Error::msg)?,
            eq_enabled: required_bool(params, "eq.enabled").map_err(anyhow::Error::msg)?,
            ir_enabled: required_bool(params, "ir_enabled").map_err(anyhow::Error::msg)?,
            bass: required_f32(params, "eq.bass").map_err(anyhow::Error::msg)?,
            middle: required_f32(params, "eq.middle").map_err(anyhow::Error::msg)?,
            treble: required_f32(params, "eq.treble").map_err(anyhow::Error::msg)?,
        },
    ))
}

unsafe extern "C" {
    fn nam_create(config: *const NamPluginConfig) -> *mut c_void;
    fn nam_destroy(handle: *mut c_void);
    fn nam_process(handle: *mut c_void, input: *const f32, output: *mut f32, nframes: i32);
}
pub struct NamProcessor {
    handle: *mut c_void,
}
unsafe impl Send for NamProcessor {}
unsafe impl Sync for NamProcessor {}
impl Drop for NamProcessor {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { nam_destroy(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}
impl NamProcessor {
    pub fn new(model_path: &str, ir_path: Option<&str>, params: NamPluginParams) -> Result<Self> {
        let model_path = CString::new(model_path)?;
        let ir_path = ir_path.map(CString::new).transpose()?;
        let config = NamPluginConfig {
            model_path_utf8: model_path.as_ptr(),
            ir_path_utf8: ir_path
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr()),
            input_db: params.input_level_db,
            output_db: params.output_level_db,
            noise_gate_threshold_db: params.noise_gate_threshold_db,
            bass: params.bass,
            middle: params.middle,
            treble: params.treble,
            noise_gate_enabled: params.noise_gate_enabled as u8,
            eq_enabled: params.eq_enabled as u8,
            ir_enabled: params.ir_enabled as u8,
        };
        let handle = unsafe { nam_create(&config) };
        if handle.is_null() {
            bail!(
                "failed to load NAM model '{}'",
                model_path.to_string_lossy()
            );
        }
        Ok(Self { handle })
    }
}

impl MonoProcessor for NamProcessor {
    fn process_sample(&mut self, sample: f32) -> f32 {
        let input = [sample];
        let mut output = [0.0f32];
        unsafe {
            nam_process(self.handle, input.as_ptr(), output.as_mut_ptr(), 1);
        }
        output[0]
    }
}
