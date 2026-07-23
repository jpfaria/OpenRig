use domain::ids::ChainId;
use serde::{Deserialize, Serialize};

use crate::block::{AudioBlock, AudioBlockKind, InputBlock, InsertBlock, OutputBlock};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ChainOutputMixdown {
    Sum,
    #[default]
    Average,
    Left,
    Right,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ChainInputMode {
    /// Single-channel input; upmixed to stereo for stereo outputs.
    #[default]
    #[serde(alias = "auto")]
    Mono,
    /// Two-channel input treated as a true stereo L/R pair.
    Stereo,
    /// Two independent mono pipelines (e.g. two guitars on separate inputs).
    DualMono,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ChainOutputMode {
    Mono,
    #[default]
    Stereo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingLayout {
    Mono,
    Stereo,
    DualMono,
}

/// Determines the audio processing layout from the combination
/// of input channels, output channels, and input mode.
pub fn processing_layout(
    input_channels: &[usize],
    output_channels: &[usize],
    input_mode: ChainInputMode,
) -> ProcessingLayout {
    let in_count = input_channels.len();
    let out_count = output_channels.len();

    // Dual mono: 2 independent mono pipelines
    if in_count >= 2 && matches!(input_mode, ChainInputMode::DualMono) {
        return ProcessingLayout::DualMono;
    }

    // Stereo input: always process as stereo
    if matches!(input_mode, ChainInputMode::Stereo) {
        return ProcessingLayout::Stereo;
    }

    // Mono input: output channel count determines final layout (upmix if needed)
    match out_count {
        0 | 1 => ProcessingLayout::Mono,
        _ => ProcessingLayout::Stereo,
    }
}

fn default_chain_volume() -> f32 {
    100.0
}

/// #717: a reference to one of a chain's already-bound output endpoints,
/// identifying the endpoint by its binding id + endpoint name (a name alone is
/// not unique across the chain's bindings). Used to route the dedicated DI
/// stream to a chosen output. Travels with the chain in `project.openrig`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DiOutputRef {
    pub binding_id: String,
    pub endpoint: String,
}

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
