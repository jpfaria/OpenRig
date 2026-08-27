//! Responsibility: pushes audio through a model offline so tooling can measure it.

use crate::ffi::{nam_create, nam_destroy, nam_process_ffi};
use crate::plugin_config::NamPluginConfig;
use crate::plugin_params::DEFAULT_PLUGIN_PARAMS;

use anyhow::{bail, Result};
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

/// Open a NAM model file for offline diagnostics. The returned handle
/// must be released with [`close_model_diag`]. Uses the model's own
/// baked calibration (same as the runtime defaults), gate/EQ/IR off so
/// the raw model response is measured.
///
/// Returns an opaque wrapper handle (`*mut c_void`), the same type the
/// runtime FFI uses.
pub fn open_model_diag(model_path: &str) -> Result<*mut c_void> {
    let model_path_c = CString::new(model_path)?;
    let config = NamPluginConfig {
        model_path_utf8: model_path_c.as_ptr(),
        ir_path_utf8: std::ptr::null(),
        input_db: 0.0,
        output_db: 0.0,
        noise_gate_threshold_db: DEFAULT_PLUGIN_PARAMS.noise_gate_threshold_db,
        bass: DEFAULT_PLUGIN_PARAMS.bass,
        middle: DEFAULT_PLUGIN_PARAMS.middle,
        treble: DEFAULT_PLUGIN_PARAMS.treble,
        // Diagnostics measure the raw model at full size (issue #657).
        slim_size: 1.0,
        noise_gate_enabled: 0,
        eq_enabled: 0,
        ir_enabled: 0,
        audit_overrides_baked_output: 0,
    };
    let handle = unsafe { nam_create(&config) };
    drop(model_path_c);
    if handle.is_null() {
        bail!("failed to load NAM model '{}'", model_path);
    }
    Ok(handle)
}

/// Push a buffer through a model opened with [`open_model_diag`]. Offline
/// only. `input` and `output` must have the same length.
///
/// # Safety
///
/// `handle` must be a live pointer returned by [`open_model_diag`] and
/// not yet freed.
pub unsafe fn nam_process(handle: *mut c_void, input: &[f32], output: &mut [f32]) {
    debug_assert_eq!(input.len(), output.len());
    if handle.is_null() || input.is_empty() {
        return;
    }
    nam_process_ffi(
        handle,
        input.as_ptr(),
        output.as_mut_ptr(),
        input.len() as c_int,
    );
}

/// Release a handle returned by [`open_model_diag`].
///
/// # Safety
///
/// `handle` must be a valid pointer returned by [`open_model_diag`] and
/// not yet freed; the caller must not use it after this call returns.
pub unsafe fn close_model_diag(handle: *mut c_void) {
    if !handle.is_null() {
        nam_destroy(handle);
    }
}
