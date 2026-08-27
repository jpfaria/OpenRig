//! Responsibility: answers metadata questions about a package without loading it.
//! Pure-metadata helpers used by `block-*` crates when instantiating
//! disk-backed plugins.
//!
//! These functions take only data shapes (`GridParameter`, `GridCapture`,
//! `ParameterValue`) plus the user's `ParameterSet`. They do no audio
//! work and pull in no nam/ir/lv2/vst3 dependency — that lives in each
//! `block-*` crate, which already has the right backend deps.
//!
//! Issue: #287

// Issue #792 split: the TTL port/scale-point parsing lives in
// dispatch_lv2_parse.rs. Re-exported here so `plugin_loader::dispatch::*`
// (lv2 crate, project schema) and the `super::dispatch::*` uses in
// dispatch_infer / dispatch_tests keep resolving unchanged.
pub use crate::capture_resolve::{first_capture_axis_values, resolve_capture};
pub use crate::dispatch_lv2_parse::lv2_control_value;
pub(crate) use crate::dispatch_lv2_parse::parse_ports;
pub(crate) use crate::lv2_ports::{find_plugin_blocks_in_text, parse_turtle_prefixes};
pub use crate::lv2_ports::{scan_lv2_ports, Lv2Port, Lv2PortRole, Lv2ScalePoint};

// `dispatch_tests.rs` hangs off this module and builds grids through
// `super::`, as it did before the split (#873).
#[cfg(test)]
pub(crate) use crate::manifest::{GridCapture, GridParameter};
#[cfg(test)]
pub(crate) use block_core::param::ParameterSet;
#[cfg(test)]
pub(crate) use domain::value_objects::ParameterValue;

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
