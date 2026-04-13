use crate::GENERIC_NAM_MODEL_ID;
use anyhow::{bail, Result};
use domain::value_objects::ParameterValue;
use block_core::param::{
    bool_parameter, file_path_parameter, float_parameter, optional_string, required_string,
    ModelParameterSchema, ParameterSet, ParameterSpec, ParameterUnit,
};
use block_core::{db_to_lin, ModelAudioMode, MonoProcessor};

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
        bool_parameter("eq.enabled", "EQ", Some("EQ"), Some(defaults.eq_enabled)),
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

// --- NeuralAudioCAPI FFI (from neural-amp-modeler-lv2) ---

/// Opaque model handle from NeuralAudioCAPI
#[repr(C)]
struct NeuralModel {
    _opaque: [u8; 0],
}

// On Windows use raw-dylib so no .lib import library is required — the DLL is
// found by name at runtime.  On other platforms the build script emits the
// standard dylib link directive.
#[cfg_attr(target_os = "windows", link(name = "libNeuralAudioCAPI", kind = "raw-dylib"))]
unsafe extern "C" {
    // wchar_t is u32 on macOS/Linux, u16 on Windows
    #[cfg(not(target_os = "windows"))]
    fn CreateModelFromFile(model_path: *const u32) -> *mut NeuralModel;
    #[cfg(target_os = "windows")]
    fn CreateModelFromFile(model_path: *const u16) -> *mut NeuralModel;
    fn DeleteModel(model: *mut NeuralModel);
    fn Process(model: *mut NeuralModel, input: *const f32, output: *mut f32, num_samples: usize);
    fn GetRecommendedInputDBAdjustment(model: *mut NeuralModel) -> f32;
    fn GetRecommendedOutputDBAdjustment(model: *mut NeuralModel) -> f32;
}

pub struct NamProcessor {
    model: *mut NeuralModel,
    input_gain: f32,
    output_gain: f32,
    scratch_input: Vec<f32>,
    scratch_output: Vec<f32>,
}

unsafe impl Send for NamProcessor {}
unsafe impl Sync for NamProcessor {}

impl Drop for NamProcessor {
    fn drop(&mut self) {
        if !self.model.is_null() {
            unsafe { DeleteModel(self.model) };
            self.model = std::ptr::null_mut();
        }
    }
}

impl NamProcessor {
    pub fn new(model_path: &str, _ir_path: Option<&str>, params: NamPluginParams) -> Result<Self> {
        // wchar_t is u32 on macOS/Linux (UTF-32), u16 on Windows (UTF-16)
        #[cfg(not(target_os = "windows"))]
        let model = {
            let wide_path: Vec<u32> = model_path.chars().map(|c| c as u32).chain(std::iter::once(0)).collect();
            unsafe { CreateModelFromFile(wide_path.as_ptr()) }
        };
        #[cfg(target_os = "windows")]
        let model = {
            let wide_path: Vec<u16> = model_path.encode_utf16().chain(std::iter::once(0)).collect();
            unsafe { CreateModelFromFile(wide_path.as_ptr()) }
        };
        if model.is_null() {
            bail!("failed to load NAM model '{}'", model_path);
        }

        let recommended_input_db = unsafe { GetRecommendedInputDBAdjustment(model) };
        let recommended_output_db = unsafe { GetRecommendedOutputDBAdjustment(model) };

        let input_gain = db_to_lin(params.input_level_db + recommended_input_db);
        let output_gain = db_to_lin(params.output_level_db + recommended_output_db);

        log::info!(
            "NAM model loaded: '{}', input_adj={:.1}dB, output_adj={:.1}dB",
            model_path, recommended_input_db, recommended_output_db
        );

        Ok(Self {
            model,
            input_gain,
            output_gain,
            scratch_input: Vec::new(),
            scratch_output: Vec::new(),
        })
    }
}

/// Diagnostic counter for periodic NAM audio health logging.
/// Only compiled on Linux/aarch64 where NAM audio issues have been observed.
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
static NAM_DIAG_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl MonoProcessor for NamProcessor {
    fn process_sample(&mut self, sample: f32) -> f32 {
        let input = [sample * self.input_gain];
        let mut output = [0.0f32];
        unsafe {
            Process(self.model, input.as_ptr(), output.as_mut_ptr(), 1);
        }
        output[0] * self.output_gain
    }

    fn process_block(&mut self, buffer: &mut [f32]) {
        if buffer.is_empty() {
            return;
        }
        // Apply input gain
        self.scratch_input.resize(buffer.len(), 0.0);
        for (dst, src) in self.scratch_input.iter_mut().zip(buffer.iter()) {
            *dst = *src * self.input_gain;
        }
        // Process through neural model
        self.scratch_output.resize(buffer.len(), 0.0);
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        let t0 = std::time::Instant::now();
        unsafe {
            Process(
                self.model,
                self.scratch_input.as_ptr(),
                self.scratch_output.as_mut_ptr(),
                buffer.len(),
            );
        }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        let elapsed = t0.elapsed();
        // Apply output gain
        for (dst, src) in buffer.iter_mut().zip(self.scratch_output.iter()) {
            *dst = *src * self.output_gain;
        }

        // Periodic diagnostic logging on aarch64 to investigate NAM audio quality
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        {
            let count = NAM_DIAG_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // Log every ~2 seconds at 48kHz/1024 ≈ 47 callbacks/sec → every 94 callbacks
            if count % 94 == 0 {
                let out_rms =
                    (buffer.iter().map(|s| s * s).sum::<f32>() / buffer.len() as f32).sqrt();
                let has_nan = buffer.iter().any(|s| s.is_nan());
                let peak_out = buffer.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                let elapsed_us = elapsed.as_micros();
                let budget_us = (buffer.len() as u64 * 1_000_000) / 48000;
                log::warn!(
                    "[NAM-DIAG] blk={} len={} process_us={} budget_us={} load={:.0}% out_rms={:.4} peak={:.4} nan={}",
                    count, buffer.len(), elapsed_us, budget_us,
                    elapsed_us as f64 / budget_us as f64 * 100.0,
                    out_rms, peak_out, has_nan,
                );
            }
        }
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
