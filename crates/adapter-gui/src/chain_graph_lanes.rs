//! Responsibility: letters the paths of every split on the graph.
//!
//! #398. A split or mixer hub carries one letter per path, so its glyph draws
//! one port per path however many there are. The first block of the outer
//! paths of a top-level split carries that path's tag (PATH A over the top
//! lane, the last path's tag under the bottom lane); middle and nested paths
//! go untagged, their tags would land on the lanes around them.

use domain::ids::BlockId;
use project::block::{path_letter, AudioBlockKind};
use project::chain::Chain;

use crate::chain_block_lists::split_by_id;
use crate::chain_graph_ports::hub_split;

/// One letter per path of the split `node_id` is a hub of; empty otherwise.
pub(crate) fn hub_lanes(chain: &Chain, node_id: &str) -> Vec<String> {
    hub_split(node_id)
        .and_then(|id| split_by_id(chain, &BlockId(id.to_string())))
        .map(|(_, split)| (0..split.paths.len()).map(path_letter).collect())
        .unwrap_or_default()
}

/// `(block id, path letter, path index)` for the first block of the first
/// and the last path of every top-level split.
pub(crate) fn path_tags(chain: &Chain) -> Vec<(String, String, usize)> {
    chain
        .blocks
        .iter()
        .filter_map(|block| match &block.kind {
            AudioBlockKind::Split(split) => Some(split),
            _ => None,
        })
        .flat_map(|split| {
            let last = split.paths.len().saturating_sub(1);
            split
                .paths
                .iter()
                .enumerate()
                .filter(move |(at, _)| *at == 0 || *at == last)
        })
        .filter_map(|(at, path)| {
            let first = path.first()?;
            (!matches!(first.kind, AudioBlockKind::Split(_)))
                .then(|| (first.id.0.clone(), path_letter(at), at))
        })
        .collect()
}
