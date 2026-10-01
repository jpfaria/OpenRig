//! Responsibility: reads a split from a saved file in any layout it was saved in.
//!
//! #328 (spec §11): a split saved before §11 holds its two paths as `a:` and
//! `b:` lists with `_a`/`_b` knob keys. Such a file still opens: the lists
//! become paths 0 and 1 and the knobs move to their per-path keys.

use serde::Deserialize;

use super::split_block::{SplitBlock, SplitEnd};
use super::split_param_keys::migrate_legacy_split_keys;
use super::types::AudioBlock;
use crate::param::ParameterSet;

#[derive(Deserialize)]
pub struct SplitBlockRecord {
    end: SplitEnd,
    params: ParameterSet,
    #[serde(default)]
    paths: Option<Vec<Vec<AudioBlock>>>,
    #[serde(default)]
    a: Vec<AudioBlock>,
    #[serde(default)]
    b: Vec<AudioBlock>,
}

impl From<SplitBlockRecord> for SplitBlock {
    fn from(record: SplitBlockRecord) -> Self {
        Self {
            end: record.end,
            params: migrate_legacy_split_keys(record.params),
            paths: record.paths.unwrap_or_else(|| vec![record.a, record.b]),
        }
    }
}
