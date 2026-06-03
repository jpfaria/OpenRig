//! Capability predicates keyed on a block's `effect_type`.
//!
//! Pure domain logic (no UI, no state) so the rules are testable in
//! isolation and consumed identically by every transport.

use crate::constants::{EFFECT_TYPE_IR, EFFECT_TYPE_NAM};

/// Whether a block of `effect_type` selects its model from the plugin
/// **catalog** (`true`) or loads a **file** the user picks (`false`).
///
/// The generic NAM (`nam`) and IR (`ir`) loader blocks load a `.nam` /
/// `.wav` file directly — they have no catalog model to choose, so the
/// Block Editor hides the model select/search picker for them (issue
/// #608). Every other effect_type — including NAM-backed gain/amp/preamp
/// pedals and cab IRs, which live under their natural effect_type and pick
/// a capture from the catalog — keeps the picker.
pub fn effect_type_uses_model_catalog(effect_type: &str) -> bool {
    !matches!(effect_type, EFFECT_TYPE_NAM | EFFECT_TYPE_IR)
}

#[cfg(test)]
#[path = "effect_type_caps_tests.rs"]
mod tests;
