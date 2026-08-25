//! Responsibility: serializes audio blocks into their YAML values.

use anyhow::Result;
use serde_yaml::Value;

use crate::block_yaml::AudioBlockYaml;

pub fn serialize_audio_blocks(blocks: &[project::block::AudioBlock]) -> Result<Vec<Value>> {
    blocks
        .iter()
        .map(|block| {
            Ok(serde_yaml::to_value(AudioBlockYaml::from_audio_block(
                block,
            )?)?)
        })
        .collect()
}
