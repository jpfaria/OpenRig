//! Responsibility: names the split lifecycle commands.
//!
//! #328: a chain holds any number of splits, nested to any depth (spec §10).
//! These commands create a split, switch its end, add or remove a path and
//! remove it. Blocks
//! go into a split's paths through `AddBlock` / `InsertPrebuiltBlock` /
//! `MoveBlock` with a `path`, and its knobs are ordinary `SetBlockParameter*`
//! writes on the split's block id.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use domain::ids::{BlockId, ChainId};
use project::block::{PathRef, SplitEnd};

/// Every state change that creates, reshapes or removes one of a chain's splits.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum SplitCommand {
    /// Insert an empty split (no block in either path, the default knobs of
    /// spec §1.2) at `position` of the chain's top level, or of the split path
    /// `path` names. Refused when a block would follow a Y in its list.
    /// Answers `BlockAdded` with the new split's id.
    AddSplit {
        chain: ChainId,
        position: usize,
        end: SplitEnd,
        #[serde(default)]
        path: Option<PathRef>,
    },
    /// Switch a split, at any depth, between Split → Mix and Y → A/B.
    /// Switching to `y` is refused while a block follows the split in its list.
    SetSplitEnd {
        chain: ChainId,
        split_id: BlockId,
        end: SplitEnd,
    },
    /// Append an empty path to the split, at any depth, with the default
    /// knobs for it (spec §11.1).
    AddSplitPath { chain: ChainId, split_id: BlockId },
    /// Remove path `path` of the split, at any depth, with its blocks. The
    /// knobs, MIDI maps, scene values and output picks of the paths above it
    /// shift down one. Refused when the split would keep fewer than two paths.
    RemoveSplitPath {
        chain: ChainId,
        split_id: BlockId,
        path: usize,
    },
    /// Remove the split, at any depth: path 0's blocks take its place in the
    /// list that held it and the other paths' blocks are dropped (the GUI asks
    /// first when one of them is not empty).
    RemoveSplit { chain: ChainId, split_id: BlockId },
}
