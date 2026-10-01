//! Responsibility: maps a split block document onto the split block it describes.
//!
//! #328 (spec §2, §11.3): a chain preset or legacy project writes a split as
//! `type: split` with `end`, `params` and `paths`. Path blocks carry no id on
//! disk; block `i` of path `p` loads as `<split>::p<p>:<i>`. Files saved before
//! §11 hold `a` and `b` instead of `paths`; they load as paths 0 and 1. A path
//! block this machine cannot load is dropped with a warning, exactly like a
//! top-level block, and the rest of the split is kept.

use anyhow::{Context, Result};
use domain::ids::BlockId;
use project::block::split_params::normalize_split_params;
use project::block::{AudioBlock, AudioBlockKind, SplitBlock, SplitEnd};
use serde_yaml::Value;

use crate::block_yaml::AudioBlockYaml;
use crate::{flatten_parameter_set, parameter_set_to_yaml_value};

/// The paths a document holds: `paths`, or the pre-§11 `a`/`b` pair.
pub(crate) fn paths_or_legacy(
    paths: Vec<Vec<Value>>,
    a: Option<Vec<Value>>,
    b: Option<Vec<Value>>,
) -> Vec<Vec<Value>> {
    if paths.is_empty() && (a.is_some() || b.is_some()) {
        vec![a.unwrap_or_default(), b.unwrap_or_default()]
    } else {
        paths
    }
}

pub(crate) fn split_from_yaml(
    id: BlockId,
    enabled: bool,
    end: SplitEnd,
    params: Value,
    paths: Vec<Vec<Value>>,
) -> Result<AudioBlock> {
    let params = normalize_split_params(flatten_parameter_set(params)?, paths.len())
        .map_err(anyhow::Error::msg)?;
    let paths = paths
        .into_iter()
        .enumerate()
        .map(|(path, values)| load_path(&id, path, values))
        .collect();
    Ok(AudioBlock {
        id,
        enabled,
        kind: AudioBlockKind::Split(SplitBlock { end, params, paths }),
    })
}

pub(crate) fn split_to_yaml(block: &AudioBlock, split: &SplitBlock) -> Result<AudioBlockYaml> {
    Ok(AudioBlockYaml::Split {
        enabled: block.enabled,
        end: split.end,
        params: parameter_set_to_yaml_value(&split.params),
        paths: split
            .paths
            .iter()
            .enumerate()
            .map(|(path, blocks)| path_to_yaml(block, path, blocks))
            .collect::<Result<_>>()?,
        a: None,
        b: None,
    })
}

/// The id a path block loads with: `<split>::p<path>:<index>`.
fn path_block_id(split: &BlockId, path: usize, index: usize) -> BlockId {
    BlockId(format!("{}::p{}:{}", split.0, path, index))
}

fn load_path(split: &BlockId, path: usize, values: Vec<Value>) -> Vec<AudioBlock> {
    values
        .into_iter()
        .enumerate()
        .filter_map(|(index, value)| {
            let id = path_block_id(split, path, index);
            let loaded = serde_yaml::from_value::<AudioBlockYaml>(value)
                .map_err(anyhow::Error::from)
                .and_then(|yaml| yaml.into_audio_block_with_id(id.clone()));
            match loaded {
                Ok(block) => Some(block),
                Err(error) => {
                    log::warn!(
                        "ignoring unsupported or invalid block at {}: {}",
                        id.0,
                        error
                    );
                    None
                }
            }
        })
        .collect()
}

fn path_to_yaml(block: &AudioBlock, path: usize, blocks: &[AudioBlock]) -> Result<Vec<Value>> {
    blocks
        .iter()
        .enumerate()
        .map(|(index, nested)| {
            let yaml = AudioBlockYaml::from_audio_block(nested).with_context(|| {
                format!(
                    "failed to serialize path {} block {} of split '{}'",
                    path, index, block.id.0
                )
            })?;
            Ok(serde_yaml::to_value(yaml)?)
        })
        .collect()
}
