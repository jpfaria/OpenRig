//! Parameter system — domain `ParameterSet`, schema definitions
//! (`ParameterSpec` / `ModelParameterSchema` / `ParameterDomain` / etc.),
//! the GUI-bound `BlockParameterDescriptor`, and the per-widget
//! constructor builders.
//!
//! Phase 6 of issue #194 split the original 541-LOC `param.rs` into
//! topical sub-modules; this module entry re-exports their surface so
//! `block_core::param::*` keeps working unchanged.

pub mod builders;
pub mod descriptor;
pub mod schema;
pub mod set;

pub use builders::{
    bool_parameter, curve_editor_parameter, enum_parameter, file_path_parameter, float_parameter,
    multi_slider_parameter, text_parameter,
};
pub use descriptor::BlockParameterDescriptor;
pub use schema::{
    CurveEditorRole, ModelParameterSchema, ParameterDomain, ParameterOption, ParameterSpec,
    ParameterUnit, ParameterWidget,
};
pub use set::{optional_string, required_bool, required_f32, required_string, ParameterSet};

// Bring crate-external types used in `param_tests.rs` (mounted via
// `#[path]` below) into this module's scope so the test file's
// `use super::*;` keeps resolving them unchanged.
#[cfg(test)]
pub(crate) use crate::audio_types::ModelAudioMode;
#[cfg(test)]
pub(crate) use domain::ids::{BlockId, ParameterId};
#[cfg(test)]
pub(crate) use domain::value_objects::ParameterValue;

#[cfg(test)]
#[path = "../param_tests.rs"]
mod tests;
