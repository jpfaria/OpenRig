//! Responsibility: names each port block of a chain after its binding.
//!
//! #398. An insert or a mid-chain input/output card shows the binding it
//! plays through ("Scarlett"), never its kind a second time ("INSERT" over
//! "INSERT"). A binding gone from the registry keeps its id; a port with no
//! binding reads `none`.

use infra_filesystem::IoBinding;
use project::block::{walk_blocks, AudioBlockKind};
use project::chain::Chain;

/// `(block id, binding name)` for every port block, at any depth.
pub(crate) fn port_block_names(
    chain: &Chain,
    registry: &[IoBinding],
    none: &str,
) -> Vec<(String, String)> {
    walk_blocks(&chain.blocks)
        .into_iter()
        .filter_map(|block| {
            let io = match &block.kind {
                AudioBlockKind::Insert(insert) => &insert.io,
                AudioBlockKind::Input(input) => &input.io,
                AudioBlockKind::Output(output) => &output.io,
                _ => return None,
            };
            let name = if io.is_empty() {
                none.to_string()
            } else {
                registry
                    .iter()
                    .find(|binding| &binding.id == io)
                    .map_or_else(|| io.clone(), |binding| binding.name.clone())
            };
            Some((block.id.0.clone(), name))
        })
        .collect()
}
