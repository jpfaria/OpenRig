//! Data struct definitions for chain blocks. Pure type defs + serde
//! plumbing; no business logic, no per-effect-type dispatch.
//!
//! Lifted out of `block.rs` (Phase 7 of issue #194). Single responsibility:
//! the on-disk / in-memory shape of an `AudioBlock` and its variants.

use domain::ids::BlockId;
use serde::{Deserialize, Serialize};

use block_core::ModelAudioMode;

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
pub struct InsertBlock {
    #[serde(default = "default_io_model")]
    pub model: String,
    /// Registry binding id for the external send/return loop (#716, model A):
    /// the SEND goes to this binding's output, the RETURN comes from its input.
    /// One E/S per insert; device endpoints are resolved from the registry.
    pub io: String,
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
pub struct InputBlock {
    #[serde(default = "default_io_model")]
    pub model: String,
    /// Registry binding id this block reads from. The chain's input device(s)
    /// are resolved from this binding in the per-machine registry — the chain
    /// itself never embeds device endpoints.
    pub io: String,
    /// Endpoint name within the referenced binding.
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OutputBlock {
    #[serde(default = "default_io_model")]
    pub model: String,
    /// Registry binding id this block writes to. The chain's output device(s)
    /// are resolved from this binding in the per-machine registry — the chain
    /// itself never embeds device endpoints.
    pub io: String,
    /// Endpoint name within the referenced binding.
    pub endpoint: String,
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
