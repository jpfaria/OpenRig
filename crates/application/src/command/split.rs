//! Responsibility: names the split lifecycle commands.
//!
//! #328: a chain may hold one split (Split → Mix or Y → A/B, spec §1.1). These
//! commands create it, switch its end and remove it. Blocks go into its paths
//! through `AddBlock` / `InsertPrebuiltBlock` / `MoveBlock` with a `path`, and
//! its knobs are ordinary `SetBlockParameter*` writes on the split's block id.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use domain::ids::{BlockId, ChainId};
use project::block::SplitEnd;

/// Every state change that creates, reshapes or removes a chain's split.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum SplitCommand {
    /// Insert an empty split (no block in either path, the default knobs of
    /// spec §1.2) at `position` of the chain's top level. Refused when the
    /// chain already has a split, or when `end` is `y` and a processing block
    /// would follow it. Answers `BlockAdded` with the new split's id.
    AddSplit {
        chain: ChainId,
        position: usize,
        end: SplitEnd,
    },
    /// Switch the split between Split → Mix and Y → A/B. Switching to `y` is
    /// refused while a processing block follows the split.
    SetSplitEnd {
        chain: ChainId,
        split_id: BlockId,
        end: SplitEnd,
    },
    /// Remove the split: path A's blocks take its place and path B's blocks
    /// are dropped (the GUI asks first when path B is not empty).
    RemoveSplit { chain: ChainId, split_id: BlockId },
}
