//! Pure resolution of NAM model gain offsets.
//!
//! Two sources of dB-domain gain layer onto the user's
//! `input_level_db` / `output_level_db` knobs:
//!
//! 1. **Trainer-baked** `recommended_input_db` / `recommended_output_db`
//!    that the NAM model file ships with (the `Get*DBAdjustment` C
//!    entrypoints). Historically they were always summed in, so a
//!    typical preamp with `recommended_output_db = -8 dB` quietly
//!    attenuated **every** signal even when the UI knobs read `0 dB`.
//!    The user could not see this offset — it was a hidden post-amp
//!    loss that put `-12 dB` on the output meter while the input fed
//!    full-scale.
//!
//! 2. **Audit-populated** `manifest.output_gain_db` (issue #491). When
//!    `from_package` finds it, the offset is summed into
//!    `params.output_level_db` **and** the
//!    `audit_overrides_baked_output` flag is set so the trainer's
//!    recommendation no longer stacks on top.
//!
//! The contract this file pins: `audit_overrides_baked_output` is the
//! single source of truth for whether the trainer recommendations
//! apply.
//!
//! - `audit_overrides_baked_output == true` ⇒ the trainer's
//!   recommendations are **ignored**, both on the input and on the
//!   output side. The audit (when present) has already injected the
//!   right values into the user-facing knobs (or left them at 0,
//!   meaning unity). UI knobs read what they actually do.
//! - `audit_overrides_baked_output == false` ⇒ the trainer's
//!   recommendations apply (legacy behaviour for models that haven't
//!   gone through the audit pipeline yet).
//!
//! This file holds the pure dB arithmetic; the processor just calls
//! [`resolve_gain_offsets`] and converts to linear gain.

/// Inputs to the gain-offset resolver. All fields are dB values
/// (signed). `audit_overrides_baked_output` is the manifest signal
/// described in the module doc.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GainOffsetInputs {
    pub input_level_db: f32,
    pub output_level_db: f32,
    pub recommended_input_db: f32,
    pub recommended_output_db: f32,
    pub audit_overrides_baked_output: bool,
}

/// Resolved dB offsets to feed `db_to_lin`. `(input_db, output_db)`.
pub fn resolve_gain_offsets(inputs: GainOffsetInputs) -> (f32, f32) {
    if inputs.audit_overrides_baked_output {
        (inputs.input_level_db, inputs.output_level_db)
    } else {
        (
            inputs.input_level_db + inputs.recommended_input_db,
            inputs.output_level_db + inputs.recommended_output_db,
        )
    }
}

#[cfg(test)]
#[path = "gain_offsets_tests.rs"]
mod tests;
