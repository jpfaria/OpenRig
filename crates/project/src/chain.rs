//! Responsibility: describes one chain of blocks.

use domain::ids::ChainId;
use serde::{Deserialize, Serialize};

use crate::block::{AudioBlock, AudioBlockKind, InputBlock, InsertBlock, OutputBlock};
pub use crate::chain_modes::{
    processing_layout, ChainInputMode, ChainOutputMixdown, ChainOutputMode, ProcessingLayout,
};
pub use crate::endpoint_ref::{DiOutputRef, EndpointRef};
pub use crate::looper::{LooperConfig, LooperSpeed, LOOPER_MAX_PER_CHAIN};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Chain {
    #[serde(skip, default = "ChainId::generate")]
    pub id: ChainId,
    #[serde(default)]
    pub description: Option<String>,
    pub instrument: String,
    pub enabled: bool,
    /// Output volume da chain em percentual. 100 = unity (sem mudança).
    /// 200 = 2× (+6 dB). 50 = metade (-6 dB). Aplicado no master output
    /// do `process_output_f32`. Controlado via slider na chain row UI.
    /// Persistido no YAML do projeto. Default 100.0 para projetos legados
    /// que não têm o campo.
    #[serde(default = "default_chain_volume")]
    pub volume: f32,
    /// #716: ids of the per-machine I/O bindings this chain uses. The chain's
    /// input/output endpoints are discovered from these bindings (the engine
    /// itself is unchanged — only where the I/O comes from). Empty for legacy
    /// projects that predate the binding registry.
    #[serde(default)]
    pub io_binding_ids: Vec<String>,
    #[serde(default)]
    pub blocks: Vec<AudioBlock>,
    /// #717: the chain's chosen DI-loop output endpoint (one of its
    /// already-bound outputs). The armed DI stream routes here instead of the
    /// chain's main output. `None` ⇒ the chain's main output (the default;
    /// legacy projects have no field and deserialize to `None`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub di_output: Option<DiOutputRef>,
    /// #323: the chain's loopers, in panel order. Empty for projects written
    /// before the looper existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loopers: Vec<LooperConfig>,
}

impl Chain {
    /// Returns all Input blocks with their indices in the blocks vec.
    pub fn input_blocks(&self) -> Vec<(usize, &InputBlock)> {
        self.blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| match &b.kind {
                AudioBlockKind::Input(input) => Some((i, input)),
                _ => None,
            })
            .collect()
    }

    /// Returns all Insert blocks with their indices in the blocks vec.
    pub fn insert_blocks(&self) -> Vec<(usize, &InsertBlock)> {
        self.blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| match &b.kind {
                AudioBlockKind::Insert(insert) => Some((i, insert)),
                _ => None,
            })
            .collect()
    }

    /// Returns all Output blocks with their indices in the blocks vec.
    pub fn output_blocks(&self) -> Vec<(usize, &OutputBlock)> {
        self.blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| match &b.kind {
                AudioBlockKind::Output(output) => Some((i, output)),
                _ => None,
            })
            .collect()
    }

    /// Returns the first Input block, if any.
    pub fn first_input(&self) -> Option<&InputBlock> {
        self.blocks.iter().find_map(|b| match &b.kind {
            AudioBlockKind::Input(input) => Some(input),
            _ => None,
        })
    }

    /// Returns the last Output block, if any.
    pub fn last_output(&self) -> Option<&OutputBlock> {
        self.blocks.iter().rev().find_map(|b| match &b.kind {
            AudioBlockKind::Output(output) => Some(output),
            _ => None,
        })
    }

    /// #716 domain rule: whether the chain has any audio I/O. True when it
    /// references at least one I/O binding (`io_binding_ids`), or carries an
    /// I/O block bound to a binding (`io` set). A chain with no I/O routes
    /// nothing — the dispatcher refuses to enable it.
    pub fn has_io(&self) -> bool {
        !self.io_binding_ids.is_empty()
            || self.blocks.iter().any(|b| match &b.kind {
                AudioBlockKind::Input(ib) => !ib.io.is_empty(),
                AudioBlockKind::Output(ob) => !ob.io.is_empty(),
                _ => false,
            })
    }
}

#[cfg(test)]
#[path = "chain_tests.rs"]
mod tests;

fn default_chain_volume() -> f32 {
    100.0
}
