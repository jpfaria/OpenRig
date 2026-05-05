use crate::UtilBackendKind;
use anyhow::{anyhow, Result};
use block_core::param::ModelParameterSchema;
use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor, StreamHandle};

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct UtilModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub brand: &'static str,
    pub backend_kind: UtilBackendKind,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub build: fn(
        &ParameterSet,
        usize,
        AudioChannelLayout,
    ) -> Result<(BlockProcessor, Option<StreamHandle>)>,
    pub supported_instruments: &'static [&'static str],
    pub knob_layout: &'static [block_core::KnobLayoutEntry],
    /// Stream kind produced by this model's StreamHandle. Empty string if no stream.
    /// Values: "stream" (key/value entries, e.g. tuner), "spectrum" (frequency band levels).
    pub stream_kind: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/generated_registry.rs"));

pub fn find_model_definition(model: &str) -> Result<&'static UtilModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported utility model '{}'", model))
}

/// Returns the stream kind for a utility model, or empty string if none.
pub fn util_stream_kind(model_id: &str) -> &'static str {
    MODEL_DEFINITIONS
        .iter()
        .find(|d| d.id == model_id)
        .map(|d| d.stream_kind)
        .unwrap_or("")
}
