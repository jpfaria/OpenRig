use crate::GENERIC_NAM_MODEL_ID;
use anyhow::{bail, Result};
use block_core::param::{
    bool_parameter, file_path_parameter, float_parameter, optional_string, required_string,
    ModelParameterSchema, ParameterSet, ParameterSpec, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};
use domain::value_objects::ParameterValue;
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Cumulative count of NAM models loaded via `nam_create` over the
/// process lifetime (never decremented). Memory-observability counter
/// (issue #588): a chain edit that reuses a block must NOT grow this — a
/// reload of an unchanged model is wasted work and a transient 2× footprint.
static MODELS_CREATED: AtomicUsize = AtomicUsize::new(0);

/// Number of NAM models currently resident in memory (incremented on load,
/// decremented on `Drop`). Memory-observability counter (issue #588): after
/// any chain edit this must equal the number of NAM blocks actually in the
/// live chain — a higher value is an orphaned model that was not freed.
static MODELS_LIVE: AtomicUsize = AtomicUsize::new(0);

/// Total NAM models loaded since process start (monotonic). See
/// [`MODELS_CREATED`].
pub fn models_created() -> usize {
    MODELS_CREATED.load(Ordering::Relaxed)
}

/// NAM models currently held in memory. See [`MODELS_LIVE`].
pub fn live_models() -> usize {
    MODELS_LIVE.load(Ordering::Relaxed)
}

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
    /// True quando o `output_gain_db` do manifest (audit-populated)
    /// já está empilhado no `input_level_db`. Sinal pro NamProcessor
    /// SKIPPAR o `recommended_output_db` baked pelo trainer — senão
    /// a atenuação típica do trainer (-7 a -8 dB) come o boost do
    /// audit e o app sai muito quieto (issue #413: "tudo baixo").
    pub audit_overrides_baked_output: bool,
}

pub const DEFAULT_PLUGIN_PARAMS: NamPluginParams = NamPluginParams {
    input_level_db: 0.0,
    output_level_db: 0.0,
    // Issue #496: was -80 dB while the gate was unwired (a no-op). Now
    // that the expander is applied, -50 dBFS sits above the amplified
    // model noise floor (worst hot case ≈ -53 dBFS) yet ~45 dB below
    // normal playing — it collapses the decay hiss without touching
    // played notes. Overridable per-model via `noise_gate.threshold_db`.
    noise_gate_threshold_db: -50.0,
    // Issue #612: the gate defaults OFF. The old `neural-amp-modeler-lv2`
    // engine had NO gate; a default-on downward expander ate the
    // decay/sustain and made the tone "sem vida" (lifeless). The gate
    // still works when the user enables it via `noise_gate.enabled`.
    noise_gate_enabled: false,
    eq_enabled: true,
    audit_overrides_baked_output: false,
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
        // Não vem de `params` — é setado pelo `from_package` quando
        // o manifest tem `output_gain_db`. Defaults inherit do caller.
        audit_overrides_baked_output: defaults.audit_overrides_baked_output,
    })
}

// --- Official NeuralAmpModelerCore C wrapper FFI (cpp/nam_wrapper.h) ---
//
// The C++ wrapper owns the whole signal chain: input gain → noise gate
// → model → gate → tone stack (EQ) → IR → output gain. Issue #612: the
// EQ (`bass/middle/treble`) is now applied by the official tone stack
// inside the wrapper instead of being parsed and dropped on the Rust
// side. ALL params cross the FFI here; Rust no longer re-applies input
// or output gain (the wrapper does), and only adds the memoryless
// `soft_clip` peak safety (issue #496) on the wrapper output — the
// wrapper does NOT clip.

/// Mirror of `NamPluginConfig` in `cpp/nam_wrapper.h`. Field order and
/// types MUST match the C struct exactly.
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
    audit_overrides_baked_output: u8,
}

// Loudness alignment lives in `manifest.output_gain_db`, populated
// offline by `tools/nam_loudness_audit` (issue #413). The per-NAM
// `loudness_probe` module is kept around as the measurement engine
// the tool uses; it does not drive gain at runtime.

/// Memoryless output saturator (issue #496).
///
/// A loud loudness calibration must not be allowed to clip on the
/// converter (harsh digital distortion) or amplify the model noise
/// floor into a hard wall on the decay. This rounds only the peaks
/// that would exceed full-scale: transparent below `THRESHOLD` (a
/// normally-played, well-calibrated model never reaches it, so tone
/// and loudness are untouched), then smoothly asymptotic to ±1.0 —
/// musical saturation instead of a ±1.0 brickwall. Memoryless: zero
/// latency, zero state, deterministic, safe on the audio thread.
#[inline]
fn soft_clip(x: f32) -> f32 {
    const THRESHOLD: f32 = 0.8;
    let a = x.abs();
    if a <= THRESHOLD {
        x
    } else {
        let over = a - THRESHOLD;
        x.signum() * (THRESHOLD + (1.0 - THRESHOLD) * (over / ((1.0 - THRESHOLD) + over)))
    }
}

// The build script (`crates/nam/build.rs`) links the cmake-built
// `libnam_wrapper` on every platform, so a plain `extern "C"` is enough
// — no per-OS `raw-dylib`/import-library handling is required.
unsafe extern "C" {
    fn nam_create(config: *const NamPluginConfig) -> *mut c_void;
    fn nam_destroy(handle: *mut c_void);
    fn nam_process(handle: *mut c_void, input: *const f32, output: *mut f32, nframes: c_int);
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
            // Memory-observability (issue #588): mirror the increment in
            // `new`. Only decrement for a model that was actually loaded.
            MODELS_LIVE.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

impl NamProcessor {
    pub fn new(
        model_path: &str,
        ir_path: Option<&str>,
        params: NamPluginParams,
        sample_rate: f32,
    ) -> Result<Self> {
        // Single source of truth for stacking trainer recommendations on
        // top of user knobs lives in `gain_offsets`. The user knobs cross
        // the FFI as `input_db` / `output_db`; `recommended_*_db` are zero
        // here because the per-model calibration is now applied INSIDE the
        // wrapper from the official core's own `GetLoudness()` /
        // `GetInputLevel()` (issue #612), driving a nonlinear NAM at the
        // level it was trained at instead of raw unity (the "abafado"
        // fix). That wrapper-side calibration is gated by
        // `audit_overrides_baked_output`, which crosses the FFI below:
        // when the catalog audit already owns the output level (the
        // `from_package` runtime path) the model normalization is
        // suppressed so the two never double-count.
        let (resolved_input_db, resolved_output_db) =
            crate::gain_offsets::resolve_gain_offsets(crate::gain_offsets::GainOffsetInputs {
                input_level_db: params.input_level_db,
                output_level_db: params.output_level_db,
                recommended_input_db: 0.0,
                recommended_output_db: 0.0,
                audit_overrides_baked_output: params.audit_overrides_baked_output,
            });

        // CStrings must outlive `nam_create` — the wrapper copies the
        // path bytes during construction, but the pointers stored in the
        // config must be valid for the duration of that call.
        let model_path_c = CString::new(model_path)?;
        let ir_path_c = ir_path.map(CString::new).transpose()?;
        let config = NamPluginConfig {
            model_path_utf8: model_path_c.as_ptr(),
            ir_path_utf8: ir_path_c
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr()),
            input_db: resolved_input_db,
            output_db: resolved_output_db,
            noise_gate_threshold_db: params.noise_gate_threshold_db,
            bass: params.bass,
            middle: params.middle,
            treble: params.treble,
            noise_gate_enabled: params.noise_gate_enabled as u8,
            eq_enabled: params.eq_enabled as u8,
            ir_enabled: ir_path_c.is_some() as u8,
            audit_overrides_baked_output: params.audit_overrides_baked_output as u8,
        };
        let handle = unsafe { nam_create(&config) };
        if handle.is_null() {
            bail!("failed to load NAM model '{}'", model_path);
        }
        // Keep the CStrings alive until after the FFI call above.
        drop(model_path_c);
        drop(ir_path_c);

        // Memory-observability (issue #588): a model was just loaded into
        // memory. Mirror this decrement in `Drop`.
        MODELS_CREATED.fetch_add(1, Ordering::Relaxed);
        MODELS_LIVE.fetch_add(1, Ordering::Relaxed);

        log::info!(
            "NAM model loaded: '{}', input_adj={:+.2}dB, output_adj={:+.2}dB \
             (audit_override={}, eq={}, ir={})",
            model_path,
            resolved_input_db,
            resolved_output_db,
            params.audit_overrides_baked_output,
            params.eq_enabled,
            ir_path.is_some(),
        );

        let _ = sample_rate; // currently unused; staged for future per-SR DSP

        Ok(Self {
            handle,
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
        // The wrapper applies input gain → gate → model → gate → EQ →
        // IR → output gain. Rust only adds the memoryless peak safety
        // (issue #496), since the wrapper does not clip.
        let input = [sample];
        let mut output = [0.0f32];
        unsafe {
            nam_process(self.handle, input.as_ptr(), output.as_mut_ptr(), 1);
        }
        soft_clip(output[0])
    }

    fn process_block(&mut self, buffer: &mut [f32]) {
        if buffer.is_empty() {
            return;
        }
        // The wrapper owns the whole signal chain (input gain → gate →
        // model → gate → EQ → IR → output gain), reading from `buffer`
        // and writing into the scratch buffer. Rust then applies only
        // the memoryless `soft_clip` peak safety (issue #496) — the
        // wrapper does NOT clip. The noise gate / EQ / IR are all
        // handled inside the official core wrapper (issue #612).
        self.scratch_output.resize(buffer.len(), 0.0);
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        let t0 = std::time::Instant::now();
        unsafe {
            nam_process(
                self.handle,
                buffer.as_ptr(),
                self.scratch_output.as_mut_ptr(),
                buffer.len() as c_int,
            );
        }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        let elapsed = t0.elapsed();
        for (dst, src) in buffer.iter_mut().zip(self.scratch_output.iter()) {
            *dst = soft_clip(*src);
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

#[cfg(test)]
#[path = "processor_tests.rs"]
mod tests;
