//! Responsibility: picks the on-disk format version a document needs.
//!
//! #328 (spec §2): a split is the first shape an older build cannot read. A
//! document that holds one is stamped `version: 2`, so an older build refuses
//! it with its "newer version" error instead of failing inside serde; a
//! split-free document stays at `version: 1`, so older builds keep opening it.

use crate::block::{AudioBlock, AudioBlockKind};
use crate::rig::{RigProject, PRESET_FORMAT_VERSION, PROJECT_FORMAT_VERSION};

/// The first format version that can hold a `Split` block.
pub const SPLIT_FORMAT_VERSION: u32 = 2;
/// The newest format version this build reads, for projects and presets.
pub const MAX_READABLE_FORMAT_VERSION: u32 = SPLIT_FORMAT_VERSION;

/// Whether a block list needs the split format. A split only ever sits at the
/// top of a list (a path never holds one), so the top level is enough.
pub fn blocks_need_split_format(blocks: &[AudioBlock]) -> bool {
    blocks
        .iter()
        .any(|block| matches!(block.kind, AudioBlockKind::Split(_)))
}

/// The version a `project.openrig` document is written with.
pub fn project_format_version(rig: &RigProject) -> u32 {
    if rig
        .presets
        .values()
        .any(|preset| blocks_need_split_format(&preset.blocks))
    {
        SPLIT_FORMAT_VERSION
    } else {
        PROJECT_FORMAT_VERSION
    }
}

/// The version a chain preset file with `blocks` is written with.
pub fn preset_format_version(blocks: &[AudioBlock]) -> u32 {
    if blocks_need_split_format(blocks) {
        SPLIT_FORMAT_VERSION
    } else {
        PRESET_FORMAT_VERSION
    }
}
