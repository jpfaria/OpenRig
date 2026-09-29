//! Responsibility: describes the data a split block holds.
//!
//! #328 (spec §1.1): a chain split. Blocks before it in the chain are shared
//! by both paths. With `Mix` the blocks after it are shared again after the
//! mixer; with `Y` nothing but the chain's own ports may follow it and each
//! path ends at its own outputs. The split and mixer knobs live in `params`
//! (see [`super::split_params`]).

use serde::{Deserialize, Serialize};

use super::split_params::default_split_params;
use super::types::AudioBlock;
use crate::param::ParameterSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SplitBlock {
    pub end: SplitEnd,
    pub params: ParameterSet,
    pub a: Vec<AudioBlock>,
    pub b: Vec<AudioBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SplitEnd {
    /// Split → Mix: both paths meet at the mixer and the chain continues.
    Mix,
    /// Y → A/B: each path ends at its own output node.
    Y,
}

impl SplitEnd {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mix => "mix",
            Self::Y => "y",
        }
    }
}

impl SplitBlock {
    /// An empty split with the Ampero default knobs.
    pub fn new(end: SplitEnd) -> Self {
        Self {
            end,
            params: default_split_params(),
            a: Vec::new(),
            b: Vec::new(),
        }
    }
}
