//! Responsibility: maps a split block document onto the split block it describes.
//!
//! #328 (spec §2): a chain preset or legacy project writes a split as
//! `type: split` with `end`, `params`, `a` and `b`. Path blocks carry no id on
//! disk; they load as `<split>::a:<i>` / `<split>::b:<i>`, the Select id
//! scheme. A path block this machine cannot load is dropped with a warning,
//! exactly like a top-level block, and the rest of the split is kept.

use anyhow::{Context, Result};
use domain::ids::BlockId;
use project::block::split_params::normalize_split_params;
use project::block::{AudioBlock, AudioBlockKind, PathSide, SplitBlock, SplitEnd};
use serde_yaml::Value;

use crate::block_yaml::AudioBlockYaml;
use crate::{flatten_parameter_set, parameter_set_to_yaml_value};

pub(crate) fn split_from_yaml(
    id: BlockId,
    enabled: bool,
    end: SplitEnd,
    params: Value,
    paths: [Vec<Value>; 2],
) -> Result<AudioBlock> {
    let params =
        normalize_split_params(flatten_parameter_set(params)?).map_err(anyhow::Error::msg)?;
    let [a, b] = paths;
    let a = load_path(&id, PathSide::A, a);
    let b = load_path(&id, PathSide::B, b);
    Ok(AudioBlock {
        id,
        enabled,
        kind: AudioBlockKind::Split(SplitBlock { end, params, a, b }),
    })
}

pub(crate) fn split_to_yaml(block: &AudioBlock, split: &SplitBlock) -> Result<AudioBlockYaml> {
    Ok(AudioBlockYaml::Split {
        enabled: block.enabled,
        end: split.end,
        params: parameter_set_to_yaml_value(&split.params),
        a: path_to_yaml(block, PathSide::A, &split.a)?,
        b: path_to_yaml(block, PathSide::B, &split.b)?,
    })
}

/// The id a path block loads with: `<split>::a:<index>`.
fn path_block_id(split: &BlockId, side: PathSide, index: usize) -> BlockId {
    BlockId(format!("{}::{}:{}", split.0, side.as_str(), index))
}

fn load_path(split: &BlockId, side: PathSide, values: Vec<Value>) -> Vec<AudioBlock> {
    values
        .into_iter()
        .enumerate()
        .filter_map(|(index, value)| {
            let id = path_block_id(split, side, index);
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

fn path_to_yaml(block: &AudioBlock, side: PathSide, blocks: &[AudioBlock]) -> Result<Vec<Value>> {
    blocks
        .iter()
        .enumerate()
        .map(|(index, nested)| {
            let yaml = AudioBlockYaml::from_audio_block(nested).with_context(|| {
                format!(
                    "failed to serialize path {} block {} of split '{}'",
                    side.as_str(),
                    index,
                    block.id.0
                )
            })?;
            Ok(serde_yaml::to_value(yaml)?)
        })
        .collect()
}
