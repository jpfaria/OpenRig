// Snapshot of complexity debt that existed on develop before the
// #548 build break was fixed (issue #576). Refactor of long fns and
// complex types is tracked under god-file ticket #276 and follow-ups.
// Allowing crate-wide keeps the QG honest about NEW regressions
// instead of perpetually re-reporting the existing snapshot.
#![allow(clippy::too_many_lines)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

//! Utility implementations.
mod processor;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData, StreamHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum UtilBackendKind {
    Native,
    Nam,
    Ir,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn util_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            UtilBackendKind::Native => "NATIVE",
            UtilBackendKind::Nam => "NAM",
            UtilBackendKind::Ir => "IR",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
        thumbnail_path: None,
        available: true,
    })
}

pub fn util_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.display_name)
        .unwrap_or("")
}

pub fn util_brand(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.brand)
        .unwrap_or("")
}

pub fn util_type_label(model: &str) -> &'static str {
    util_model_visual(model).map(|v| v.type_label).unwrap_or("")
}

pub fn utility_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn util_stream_kind(model_id: &str) -> &'static str {
    registry::util_stream_kind(model_id)
}

pub fn build_utility_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: usize,
    layout: AudioChannelLayout,
) -> Result<(BlockProcessor, Option<StreamHandle>)> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
