use anyhow::{bail, Result};
use domain::value_objects::ParameterValue;
use stage_core::param::{
    bool_parameter, file_path_parameter, float_parameter, optional_string, required_string,
    ModelParameterSchema, ParameterSet, ParameterSpec, ParameterUnit,
};
use stage_core::{ModelAudioMode, MonoProcessor};
use std::ffi::CString;
use std::os::raw::{c_char, c_void};
use crate::GENERIC_NAM_MODEL_ID;

pub fn supports_model(model: &str) -> bool {
    model == GENERIC_NAM_MODEL_ID
}

pub fn model_schema(include_file_params: bool) -> ModelParameterSchema {
    let mut parameters = Vec::new();

    if include_file_params {
        parameters.push(file_path_parameter(
            "model_path",
            "Model",
            None,
            None,
            &["nam"],
            false,
        ));
        parameters.push(file_path_parameter(
            "ir_path",
            "Impulse Response",
            None,
            Some(ParameterValue::Null),
            &["wav"],
            true,
        ));
    }

    parameters.extend(plugin_parameter_specs());

    ModelParameterSchema {
        effect_type: "nam".to_string(),
        model: GENERIC_NAM_MODEL_ID.to_string(),
        display_name: "Neural Amp Modeler".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters,
    }
}

pub fn plugin_parameter_specs() -> Vec<ParameterSpec> {
    plugin_parameter_specs_with_defaults(DEFAULT_PLUGIN_PARAMS)
}

pub fn plugin_parameter_specs_with_defaults(defaults: NamPluginParams) -> Vec<ParameterSpec> {
    vec![
        float_parameter(
            "input_db",
            "Input",
            None,
            Some(defaults.input_level_db),
            -24.0,
            24.0,
            0.1,
            ParameterUnit::Decibels,
        ),
        float_parameter(
            "output_db",
            "Output",
            None,
            Some(defaults.output_level_db),
            -24.0,
            24.0,
            0.1,
            ParameterUnit::Decibels,
        ),
        bool_parameter(
            "noise_gate.enabled",
            "Noise Gate",
            Some("Noise Gate"),
            Some(defaults.noise_gate_enabled),
        ),
        float_parameter(
            "noise_gate.threshold_db",
            "Threshold",
            Some("Noise Gate"),
            Some(defaults.noise_gate_threshold_db),
            -96.0,
            0.0,
            0.1,
            ParameterUnit::Decibels,
        ),
        bool_parameter(
            "eq.enabled",
            "EQ",
            Some("EQ"),
            Some(defaults.eq_enabled),
        ),
        float_parameter(
            "eq.bass",
            "Bass",
            Some("EQ"),
            Some(defaults.bass),
            0.0,
            10.0,
            0.1,
            ParameterUnit::None,
        ),
        float_parameter(
            "eq.middle",
            "Middle",
            Some("EQ"),
            Some(defaults.middle),
            0.0,
            10.0,
            0.1,
            ParameterUnit::None,
        ),
        float_parameter(
            "eq.treble",
            "Treble",
            Some("EQ"),
            Some(defaults.treble),
            0.0,
            10.0,
            0.1,
            ParameterUnit::None,
        ),
    ]
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
    pub bass: f32,
    pub middle: f32,
    pub treble: f32,
}

pub const DEFAULT_PLUGIN_PARAMS: NamPluginParams = NamPluginParams {
    input_level_db: 0.0,
    output_level_db: 0.0,
    noise_gate_threshold_db: -80.0,
    noise_gate_enabled: true,
    eq_enabled: true,
    bass: 5.0,
    middle: 5.0,
    treble: 5.0,
};

pub fn params_from_set(params: &ParameterSet) -> Result<(String, Option<String>, NamPluginParams)> {
    Ok((
        required_string(params, "model_path").map_err(anyhow::Error::msg)?,
        optional_string(params, "ir_path"),
        plugin_params_from_set_with_defaults(params, DEFAULT_PLUGIN_PARAMS)?,
    ))
}

pub fn plugin_params_from_set(params: &ParameterSet) -> Result<NamPluginParams> {
    plugin_params_from_set_with_defaults(params, DEFAULT_PLUGIN_PARAMS)
}

pub fn plugin_params_from_set_with_defaults(
    params: &ParameterSet,
    defaults: NamPluginParams,
) -> Result<NamPluginParams> {
    Ok(NamPluginParams {
        input_level_db: float_or_default(params, "input_db", defaults.input_level_db)?,
        output_level_db: float_or_default(params, "output_db", defaults.output_level_db)?,
        noise_gate_threshold_db: float_or_default(
            params,
            "noise_gate.threshold_db",
            defaults.noise_gate_threshold_db,
        )?,
        noise_gate_enabled: bool_or_default(
            params,
            "noise_gate.enabled",
            defaults.noise_gate_enabled,
        )?,
        eq_enabled: bool_or_default(params, "eq.enabled", defaults.eq_enabled)?,
        bass: float_or_default(params, "eq.bass", defaults.bass)?,
        middle: float_or_default(params, "eq.middle", defaults.middle)?,
        treble: float_or_default(params, "eq.treble", defaults.treble)?,
    })
}

unsafe extern "C" {
    fn nam_create(config: *const NamPluginConfig) -> *mut c_void;
    fn nam_destroy(handle: *mut c_void);
    fn nam_process(handle: *mut c_void, input: *const f32, output: *mut f32, nframes: i32);
}
pub struct NamProcessor {
    handle: *mut c_void,
    scratch_output: Vec<f32>,
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
            ir_enabled: ir_path.is_some() as u8,
        };
        let handle = unsafe { nam_create(&config) };
        if handle.is_null() {
            bail!(
                "failed to load NAM model '{}'",
                model_path.to_string_lossy()
            );
        }
        Ok(Self {
            handle,
            scratch_output: Vec::new(),
        })
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

    fn process_block(&mut self, buffer: &mut [f32]) {
        if buffer.is_empty() {
            return;
        }
        self.scratch_output.resize(buffer.len(), 0.0);
        unsafe {
            nam_process(
                self.handle,
                buffer.as_ptr(),
                self.scratch_output.as_mut_ptr(),
                buffer.len() as i32,
            );
        }
        buffer.copy_from_slice(&self.scratch_output);
    }
}

fn float_or_default(params: &ParameterSet, path: &str, default: f32) -> Result<f32> {
    match params.get(path) {
        Some(value) => value
            .as_f32()
            .ok_or_else(|| anyhow::anyhow!("invalid float parameter '{}'", path)),
        None => Ok(default),
    }
}

fn bool_or_default(params: &ParameterSet, path: &str, default: bool) -> Result<bool> {
    match params.get(path) {
        Some(value) => value
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("invalid bool parameter '{}'", path)),
        None => Ok(default),
    }
}
