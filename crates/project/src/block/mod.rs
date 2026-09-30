//! Responsibility: routes the block model's public surface.
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

pub mod audio_block_methods;
pub mod block_params;
pub mod block_walk;
pub mod core_block_methods;
mod disk_audio_mode;
pub mod dispatch;
mod grid_schema;
mod ir_schema;
mod lv2_bundle_ports;
mod lv2_schema;
pub mod manifest_labels;
pub mod methods;
mod nam_schema;
pub mod param_writer;
pub mod path_ref;
pub mod port_duplication;
pub mod select_block_methods;
pub mod split_block;
pub mod split_block_methods;
pub mod split_lookup;
pub mod split_params;
pub mod types;
pub mod vst3_model_id;
pub mod vst3_schema;

pub use block_params::{block_params, block_params_mut};
pub use block_walk::{find_block_mut, for_each_block_mut, walk_blocks};
pub use dispatch::{build_audio_block_kind, normalize_block_params, schema_for_block_model};
pub use path_ref::{PathRef, PathSide};
pub use port_duplication::duplicates_chain_binding;
pub use split_block::{SplitBlock, SplitEnd};
pub use split_block_methods::validate_split_layout;
pub use split_lookup::{find_split, find_split_with_end, has_y_split, splits};
pub use types::{
    AudioBlock, AudioBlockKind, BlockAudioDescriptor, BlockModelRef, CoreBlock, InputBlock,
    InsertBlock, NamBlock, OutputBlock, SelectBlock,
};

#[cfg(test)]
#[path = "../block_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../block_tests_more.rs"]
mod tests_more;
