//! Responsibility: reads the NAM plugin parameters out of a parameter set.

use anyhow::Result;
use block_core::param::{optional_string, required_string, ParameterSet};

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
    /// A2 SlimmableContainer size, 0.0 (smallest submodel) .. 1.0 (full),
    /// forwarded to `SetSlimmableSize` through the FFI (issue #657). The
    /// user-facing `slim` knob is a 0..100 % percentage; this is its 0..1
    /// ratio. Inert for A1 models (not slimmable). 1.0 = historical
    /// full-size behavior.
    pub slim_size: f32,
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
    // Issue #657: full size by default — A2 models keep their historical
    // full-fidelity behavior and A1 models ignore it. `SLIM_PERCENT_FULL`
    // / 100.
    slim_size: 1.0,
};

/// Full-size value of the user-facing `slim` knob, as a percentage. The
/// knob is 0..100 % (0 = smallest submodel, 100 = full); the FFI /
/// `SetSlimmableSize` want a 0.0..1.0 ratio, so the param value is divided
/// by this. Single source of truth for the percent ⇄ ratio mapping
/// (issue #657).
pub const SLIM_PERCENT_FULL: f32 = 100.0;

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
        // Issue #657: the `slim` knob is a 0..100 % percentage; the FFI
        // wants a 0..1 ratio. Read it as percent and convert, clamping to
        // the valid range. Absent → the caller's ratio default (already
        // 0..1), so this never double-divides.
        slim_size: match params.get("slim") {
            Some(value) => {
                let percent = value
                    .as_f32()
                    .ok_or_else(|| anyhow::anyhow!("invalid float parameter 'slim'"))?;
                (percent / SLIM_PERCENT_FULL).clamp(0.0, 1.0)
            }
            None => defaults.slim_size,
        },
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

pub fn float_or_default(params: &ParameterSet, path: &str, default: f32) -> Result<f32> {
    match params.get(path) {
        Some(value) => value
            .as_f32()
            .ok_or_else(|| anyhow::anyhow!("invalid float parameter '{}'", path)),
        None => Ok(default),
    }
}

pub fn bool_or_default(params: &ParameterSet, path: &str, default: bool) -> Result<bool> {
    match params.get(path) {
        Some(value) => value
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("invalid bool parameter '{}'", path)),
        None => Ok(default),
    }
}
