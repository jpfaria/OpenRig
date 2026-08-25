//! Responsibility: keeps the historical `traits` path pointing at the processor contract.

pub use crate::dispatch::BlockProcessor;
pub use crate::output_gain::wrap_with_output_gain_db;
pub use crate::plugin_editor::{NamedModel, PluginEditorHandle};
pub use crate::processor::{MonoProcessor, StereoProcessor};

#[cfg(test)]
#[path = "traits_tests.rs"]
mod tests;
