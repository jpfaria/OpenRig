//! Chain block model — data structs, validation/descriptor methods, and
//! per-effect-type dispatch.
//!
//! Phase 7 of issue #194 split the original 464-LOC `block.rs` by concern:
//! - `types.rs`     — pure data structs + serde
//! - `methods.rs`   — validation, descriptor materialisation, accessors
//! - `dispatch.rs`  — per-effect-type cross-crate dispatch (the three
//!   match-on-`effect_type` functions plus describe helpers)
//!
//! This module entry re-exports the public surface so `project::block::*`
//! callers (engine, infra-cpal, infra-yaml, adapter-gui, etc.) keep
//! working unchanged.

pub mod dispatch;
mod grid_schema;
mod ir_schema;
mod lv2_schema;
pub mod manifest_labels;
pub mod methods;
mod nam_schema;
pub mod param_writer;
pub mod types;
pub mod vst3_schema;

pub use dispatch::{build_audio_block_kind, normalize_block_params, schema_for_block_model};
pub use types::{
    AudioBlock, AudioBlockKind, BlockAudioDescriptor, BlockModelRef, CoreBlock, InputBlock,
    InsertBlock, NamBlock, OutputBlock, SelectBlock,
};

#[cfg(test)]
#[path = "../block_tests.rs"]
mod tests;
