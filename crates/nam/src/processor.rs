//! Responsibility: runs a NAM model over a signal.

use crate::ffi::{nam_create, nam_destroy, nam_process_ffi};
use crate::model_stats::{note_model_created, note_model_dropped};
use crate::peak_safety::PeakSafety;
use crate::plugin_config::NamPluginConfig;
use anyhow::{bail, Result};
use block_core::MonoProcessor;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

// The importers reach these through `processor::` — that is where they were
// defined before the split (#873).
pub use crate::model_stats::{live_models, models_created, supports_model};
// The test modules mounted on the crate root reach these through
// `processor::`, where they lived before the split (#873).
#[cfg(test)]
pub use crate::params::{
    model_schema, plugin_parameter_specs, plugin_parameter_specs_with_defaults,
};
#[cfg(test)]
pub(crate) use crate::peak_safety::soft_clip;
#[cfg(test)]
pub use crate::plugin_params::{bool_or_default, float_or_default};
pub use crate::plugin_params::{
    params_from_set, plugin_params_from_set, plugin_params_from_set_with_defaults, NamPluginParams,
    DEFAULT_PLUGIN_PARAMS, SLIM_PERCENT_FULL,
};
#[cfg(test)]
pub use block_core::param::ParameterSet;

// Loudness alignment lives in `manifest.output_gain_db`, populated
// offline by `tools/nam_loudness_audit` (issue #413). The per-NAM
// `loudness_probe` module is kept around as the measurement engine
// the tool uses; it does not drive gain at runtime.

pub struct NamProcessor {
    handle: *mut c_void,
    scratch_output: Vec<f32>,
    peak_safety: PeakSafety,
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
            note_model_dropped();
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
            slim_size: params.slim_size,
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
        note_model_created();

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
            peak_safety: PeakSafety::new(),
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
            nam_process_ffi(self.handle, input.as_ptr(), output.as_mut_ptr(), 1);
        }
        self.peak_safety.process_one(output[0])
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
            nam_process_ffi(
                self.handle,
                buffer.as_ptr(),
                self.scratch_output.as_mut_ptr(),
                buffer.len() as c_int,
            );
        }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        let elapsed = t0.elapsed();
        buffer.copy_from_slice(&self.scratch_output);
        self.peak_safety.process_block(buffer);

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

#[cfg(test)]
#[path = "processor_tests.rs"]
mod tests;
