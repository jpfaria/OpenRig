//! Responsibility: describes the config struct the NAM library is handed.

use std::os::raw::c_char;

/// Mirror of `NamPluginConfig` in `cpp/nam_wrapper.h`. Field order and
/// types MUST match the C struct exactly.
#[repr(C)]
pub(crate) struct NamPluginConfig {
    pub(crate) model_path_utf8: *const c_char,
    pub(crate) ir_path_utf8: *const c_char,
    pub(crate) input_db: f32,
    pub(crate) output_db: f32,
    pub(crate) noise_gate_threshold_db: f32,
    pub(crate) bass: f32,
    pub(crate) middle: f32,
    pub(crate) treble: f32,
    pub(crate) slim_size: f32,
    pub(crate) noise_gate_enabled: u8,
    pub(crate) eq_enabled: u8,
    pub(crate) ir_enabled: u8,
    pub(crate) audit_overrides_baked_output: u8,
}
