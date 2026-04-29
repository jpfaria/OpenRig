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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InsertEndpoint {
    pub device_id: DeviceId,
    #[serde(default)]
    pub mode: ChainInputMode,
    pub channels: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InsertBlock {
    #[serde(default = "default_io_model")]
    pub model: String,
    pub send: InsertEndpoint,
    #[serde(rename = "return")]
    pub return_: InsertEndpoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioBlock {
    pub id: BlockId,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub kind: AudioBlockKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockAudioDescriptor {
    pub block_id: BlockId,
    pub effect_type: String,
    pub model: String,
    pub display_name: String,
    pub audio_mode: ModelAudioMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputEntry {
    pub device_id: DeviceId,
    #[serde(default)]
    pub mode: ChainInputMode,
    pub channels: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputEntry {
    pub device_id: DeviceId,
    #[serde(default)]
    pub mode: ChainOutputMode,
    pub channels: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputBlock {
    #[serde(default = "default_io_model")]
    pub model: String,
    pub entries: Vec<InputEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputBlock {
    #[serde(default = "default_io_model")]
    pub model: String,
    pub entries: Vec<OutputEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamBlock {
    pub model: String,
    pub params: ParameterSet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreBlock {
    pub effect_type: String,
    pub model: String,
    pub params: ParameterSet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
