//! Responsibility: addresses one path of a chain's split.
//!
//! #328 (spec §3): the commands that add, insert or move a block take an
//! optional `PathRef`; `None` means the chain's top-level block list.

use domain::ids::BlockId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PathSide {
    A,
    B,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PathRef {
    pub split: BlockId,
    pub side: PathSide,
}
