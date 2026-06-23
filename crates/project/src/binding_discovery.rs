//! #716 discovery: resolve a chain's input/output from the I/O bindings it
//! SELECTS (`chain.io_binding_ids`) instead of from per-block I/O the user
//! edited by hand.
//!
//! The engine is unchanged — it still consumes bound `Input`/`Output` blocks
//! (`io` = binding id, `endpoint` = endpoint name) via
//! `engine::resolve_chain_streams`. This is the bridge: for each selected
//! binding it synthesises one bound `Input` block per input endpoint (at the
//! chain head) and one bound `Output` block per output endpoint (at the tail),
//! around the chain's existing effect blocks. The chain's own I/O blocks (if
//! any) are dropped — selection is now the single source of truth.
//!
//! A chain with no `io_binding_ids` (legacy / unbound) is returned unchanged.

use domain::ids::BlockId;
use domain::io_binding::IoBinding;

use crate::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};
use crate::chain::Chain;

/// Default model for a synthesised bound I/O block (mirrors the YAML default).
const BOUND_IO_MODEL: &str = "standard";

/// Return a copy of `chain` whose head holds one `Input` block per input
/// endpoint and whose tail holds one `Output` block per output endpoint of
/// every binding the chain selects, around its existing effect blocks. When the
/// chain selects no bindings it is returned unchanged.
pub fn resolve_bound_io_blocks(chain: &Chain, registry: &[IoBinding]) -> Chain {
    if chain.io_binding_ids.is_empty() {
        return chain.clone();
    }

    let mut inputs: Vec<AudioBlock> = Vec::new();
    let mut outputs: Vec<AudioBlock> = Vec::new();

    for binding_id in &chain.io_binding_ids {
        let Some(binding) = registry.iter().find(|b| &b.id == binding_id) else {
            continue; // selection references a binding not in the registry → skip
        };
        for ep in &binding.inputs {
            inputs.push(AudioBlock {
                id: BlockId(format!("{}:in:{}:{}", chain.id.0, binding.id, ep.name)),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: BOUND_IO_MODEL.to_string(),
                    io: binding.id.clone(),
                    endpoint: ep.name.clone(),
                    entries: Vec::new(),
                }),
            });
        }
        for ep in &binding.outputs {
            outputs.push(AudioBlock {
                id: BlockId(format!("{}:out:{}:{}", chain.id.0, binding.id, ep.name)),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: BOUND_IO_MODEL.to_string(),
                    io: binding.id.clone(),
                    endpoint: ep.name.clone(),
                    entries: Vec::new(),
                }),
            });
        }
    }

    // Keep only the chain's effect blocks; selection replaces any I/O blocks.
    let effects = chain
        .blocks
        .iter()
        .filter(|b| !matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
        .cloned();

    let mut blocks = Vec::with_capacity(inputs.len() + chain.blocks.len() + outputs.len());
    blocks.extend(inputs);
    blocks.extend(effects);
    blocks.extend(outputs);

    Chain {
        blocks,
        ..chain.clone()
    }
}
