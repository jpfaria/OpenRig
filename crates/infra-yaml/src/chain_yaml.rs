//! YAML <-> Chain conversion. Mirrors project::chain::Chain to/from the
//! YAML schema. Lifted out of `lib.rs` so the production file stays under
//! the size cap.

use anyhow::Result;
use project::block::AudioBlock;
use project::chain::{Chain, ChainInputMode, ChainOutputMixdown};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::block_yaml::AudioBlockYaml;
use crate::block_yaml_load::load_audio_block_value;
use crate::{default_instrument, generated_chain_id};

pub(crate) fn default_io_yaml_model() -> String {
    "standard".to_string()
}

fn default_chain_volume() -> f32 {
    100.0
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
pub(crate) struct ChainYaml {
    #[serde(default)]
    description: Option<String>,
    #[serde(default = "default_instrument")]
    instrument: String,
    #[serde(default, skip_serializing)]
    enabled: bool,
    /// Output volume da chain em percentual (issue #440). 100 = unity.
    /// Legados sem campo deserializam como 100.0.
    #[serde(default = "default_chain_volume")]
    volume: f32,
    // #716: ids of the I/O bindings this chain uses (input/output discovered
    // from the binding registry). Empty for legacy projects.
    #[serde(default)]
    io_binding_ids: Vec<String>,
    blocks: Vec<Value>,
    #[serde(default, skip_serializing)]
    output_mixdown: ChainOutputMixdown,
    #[serde(default, skip_serializing)]
    input_mode: ChainInputMode,
}

impl ChainYaml {
    pub(crate) fn into_chain(self, index: usize) -> Result<Chain> {
        let chain_id = generated_chain_id(index);
        log::debug!(
            "deserializing chain index={}, description={:?}, instrument='{}', enabled={}",
            index,
            self.description,
            self.instrument,
            self.enabled
        );

        // Parse all blocks from the blocks array (new format may include input/output blocks inline)
        let parsed_blocks: Vec<AudioBlock> = self
            .blocks
            .into_iter()
            .enumerate()
            .filter_map(|(block_index, block)| {
                load_audio_block_value(block, &chain_id, block_index)
            })
            .collect();

        // Model A (#716): I/O lives in the blocks array as pure
        // `{ model, io, endpoint }` references. Device-level legacy sections
        // no longer deserialize here — that migration is handled upstream.
        Ok(Chain {
            id: chain_id.clone(),
            description: self.description,
            instrument: self.instrument,
            enabled: self.enabled,
            volume: self.volume,
            io_binding_ids: self.io_binding_ids.clone(),
            blocks: parsed_blocks,
            di_output: None,
            loopers: vec![],
        })
    }

    pub(crate) fn from_chain(chain: &Chain) -> Result<Self> {
        // All blocks (including I/O) go into the blocks array
        let audio_blocks: Vec<Value> = chain
            .blocks
            .iter()
            .map(|block| {
                Ok(serde_yaml::to_value(AudioBlockYaml::from_audio_block(
                    block,
                )?)?)
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            description: chain.description.clone(),
            instrument: chain.instrument.clone(),
            enabled: false, // chains always start disabled on project load, regardless of saved state
            volume: chain.volume,
            io_binding_ids: chain.io_binding_ids.clone(),
            blocks: audio_blocks,
            output_mixdown: ChainOutputMixdown::Average,
            input_mode: ChainInputMode::default(),
        })
    }
}
