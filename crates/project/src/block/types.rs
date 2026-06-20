//! Data struct definitions for chain blocks. Pure type defs + serde
//! plumbing; no business logic, no per-effect-type dispatch.
//!
//! Lifted out of `block.rs` (Phase 7 of issue #194). Single responsibility:
//! the on-disk / in-memory shape of an `AudioBlock` and its variants.

use domain::ids::{BlockId, DeviceId};
use serde::{Deserialize, Serialize};

use block_core::ModelAudioMode;

use crate::chain::{ChainInputMode, ChainOutputMode};
use crate::param::ParameterSet;

/// Maximum number of options a single `SelectBlock` may carry.
pub(crate) const MAX_SELECT_OPTIONS: usize = 8;

const fn default_enabled() -> bool {
    true
}

pub(crate) fn default_io_model() -> String {
    "standard".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InsertEndpoint {
    pub device_id: DeviceId,
    #[serde(default)]
    pub mode: ChainInputMode,
    pub channels: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InsertBlock {
    #[serde(default = "default_io_model")]
    pub model: String,
    pub send: InsertEndpoint,
    #[serde(rename = "return")]
    pub return_: InsertEndpoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AudioBlock {
    pub id: BlockId,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub kind: AudioBlockKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BlockAudioDescriptor {
    pub block_id: BlockId,
    pub effect_type: String,
    pub model: String,
    pub display_name: String,
    pub audio_mode: ModelAudioMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum AudioBlockKind {
    Nam(NamBlock),
    Core(CoreBlock),
    Select(SelectBlock),
    Input(InputBlock),
    Output(OutputBlock),
    Insert(InsertBlock),
}

impl AudioBlockKind {
    /// Stable lowercase label for the variant — for diagnostics, logs,
    /// and debug surfaces. Adding a new variant fails to compile here,
    /// not at every callsite.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Nam(_) => "nam",
            Self::Core(_) => "core",
            Self::Select(_) => "select",
            Self::Input(_) => "input",
            Self::Output(_) => "output",
            Self::Insert(_) => "insert",
        }
    }

    /// A params-free signature of the block's MODEL identity (variant + model
    /// name). Used to tell whether two same-id blocks still occupy the same
    /// slot or whether the model was swapped (`ReplaceBlockModel`). Excludes
    /// params, entries, and scene state — those are per-scene diffs, not
    /// structure (#627: a same-id model swap must count as structural so the
    /// preset base is rewritten instead of being treated as a param diff).
    pub fn model_identity(&self) -> String {
        match self {
            Self::Nam(b) => format!("nam:{}", b.model),
            Self::Core(b) => format!("core:{}/{}", b.effect_type, b.model),
            Self::Select(b) => format!("select:{}", b.selected_block_id.0),
            Self::Input(b) => format!("input:{}", b.model),
            Self::Output(b) => format!("output:{}", b.model),
            Self::Insert(b) => format!("insert:{}", b.model),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InputEntry {
    pub device_id: DeviceId,
    #[serde(default)]
    pub mode: ChainInputMode,
    pub channels: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OutputEntry {
    pub device_id: DeviceId,
    #[serde(default)]
    pub mode: ChainOutputMode,
    pub channels: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InputBlock {
    #[serde(default = "default_io_model")]
    pub model: String,
    /// Registry binding id this block reads from (new schema, Task 5).
    /// Empty string signals a legacy block that still uses `entries`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub io: String,
    /// Endpoint name within the referenced binding (new schema, Task 5).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub endpoint: String,
    /// Legacy device entries. Clean break (#716): NEVER serialized — new
    /// projects persist only `io`/`endpoint`. Still DESERIALIZES so an old
    /// YAML with `entries:` loads without error; the values are ignored for
    /// routing (the chain opens unbound until reconfigured via the registry).
    /// Kept as an internal/test-only field so the pinned invariant tests
    /// (volume_invariants, golden, stream isolation) build chains directly.
    #[serde(default, skip_serializing)]
    pub entries: Vec<InputEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OutputBlock {
    #[serde(default = "default_io_model")]
    pub model: String,
    /// Registry binding id this block writes to (new schema, Task 5).
    /// Empty string signals a legacy block that still uses `entries`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub io: String,
    /// Endpoint name within the referenced binding (new schema, Task 5).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub endpoint: String,
    /// Legacy device entries. Clean break (#716): NEVER serialized — new
    /// projects persist only `io`/`endpoint`. Still DESERIALIZES so an old
    /// YAML with `entries:` loads without error; the values are ignored for
    /// routing (the chain opens unbound until reconfigured via the registry).
    /// Kept as an internal/test-only field so the pinned invariant tests
    /// (volume_invariants, golden, stream isolation) build chains directly.
    #[serde(default, skip_serializing)]
    pub entries: Vec<OutputEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NamBlock {
    pub model: String,
    pub params: ParameterSet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CoreBlock {
    pub effect_type: String,
    pub model: String,
    pub params: ParameterSet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SelectBlock {
    pub selected_block_id: BlockId,
    pub options: Vec<AudioBlock>,
}

#[derive(Clone, Copy)]
pub struct BlockModelRef<'a> {
    pub effect_type: &'a str,
    pub model: &'a str,
    pub params: &'a ParameterSet,
}
