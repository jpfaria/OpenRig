//! Responsibility: routes the project crate's public surface.
// Snapshot of complexity debt that existed on develop before the
// #548 build break was fixed (issue #576). Refactor of long fns and
// complex types is tracked under god-file ticket #276 and follow-ups.
// Allowing crate-wide keeps the QG honest about NEW regressions
// instead of perpetually re-reporting the existing snapshot.
#![allow(clippy::too_many_lines)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

pub mod binding_discovery;
pub mod block;
pub mod catalog;
mod catalog_availability;
mod catalog_build;
mod catalog_colors;
mod catalog_label;
mod catalog_listing;
mod catalog_model_info;
mod catalog_registry;
mod catalog_types;
pub mod chain;
pub mod chain_modes;
pub mod channel_mode_conv;
pub mod device;
pub mod endpoint_ref;
pub mod io_binding;
pub mod looper;
pub mod midi;
pub mod migrate;
pub mod param;
pub mod project;
pub mod project_disable_unavailable;
pub mod rig;
pub mod rig_command;
pub mod rig_methods;
pub mod rig_nav;
pub mod rig_sync;
pub mod rig_validate;
pub mod rig_write_back;
pub mod vst3_editor;

#[cfg(test)]
#[path = "midi_tests.rs"]
mod midi_tests;
