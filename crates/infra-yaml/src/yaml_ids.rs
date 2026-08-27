//! Responsibility: mints the chain ids the YAML layer assigns when the document carries none.

use domain::ids::ChainId;

pub(crate) fn generated_chain_id(index: usize) -> ChainId {
    ChainId(format!("chain:{}", index))
}

pub(crate) fn generated_preset_chain_id(preset_id: &str) -> ChainId {
    ChainId(format!("preset:{}", preset_id))
}
