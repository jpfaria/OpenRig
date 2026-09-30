//! Responsibility: names the split lifecycle commands.
//!
//! #328: a chain may hold one Split → Mix and, after it, one Y → A/B (spec
//! §1.1). These commands create a split, switch its end and remove it. Blocks
//! go into a split's paths through `AddBlock` / `InsertPrebuiltBlock` /
//! `MoveBlock` with a `path`, and its knobs are ordinary `SetBlockParameter*`
//! writes on the split's block id.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use domain::ids::{BlockId, ChainId};
use project::block::SplitEnd;

/// Every state change that creates, reshapes or removes one of a chain's splits.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum SplitCommand {
    /// Insert an empty split (no block in either path, the default knobs of
    /// spec §1.2) at `position` of the chain's top level. Refused when the
    /// chain already has a split with the same `end`, when a Mix would follow
    /// the Y, or when `end` is `y` and a processing block would follow it.
    /// Answers `BlockAdded` with the new split's id.
    AddSplit {
        chain: ChainId,
        position: usize,
        end: SplitEnd,
    },
    /// Switch a split between Split → Mix and Y → A/B. Refused when the chain
    /// would hold two splits of one end, and switching to `y` is refused while
    /// a processing block follows the split.
    SetSplitEnd {
        chain: ChainId,
        split_id: BlockId,
        end: SplitEnd,
    },
    /// Remove the split: path A's blocks take its place and path B's blocks
    /// are dropped (the GUI asks first when path B is not empty).
    RemoveSplit { chain: ChainId, split_id: BlockId },
}
