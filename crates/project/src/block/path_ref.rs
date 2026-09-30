//! Responsibility: addresses one path of one of a chain's splits.
//!
//! #328 (spec §3): the commands that add, insert or move a block take an
//! optional `PathRef`; `None` means the chain's top-level block list. The
//! split is named by its block id, so the address stays unambiguous when a
//! chain holds a Mix split and a Y split.

use domain::ids::BlockId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PathSide {
    A,
    B,
}

impl PathSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A => "a",
            Self::B => "b",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PathRef {
    pub split: BlockId,
    pub side: PathSide,
}
