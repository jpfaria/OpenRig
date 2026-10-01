//! Responsibility: names the graph nodes that travel with a dragged split.
//!
//! #328 (spec §5.1). A split card — its split node or its mixer — is dragged
//! as one piece: the split, every block of its paths at any depth, and the
//! routing nodes of the splits nested there all follow the pointer, and none
//! of them can be its destination.

use domain::ids::BlockId;
use project::block::{walk_blocks, AudioBlock, AudioBlockKind, PathRef};
use project::chain::Chain;

use crate::chain_block_lists::split_by_id;
use crate::chain_graph_ids::{leaf_output_node_id, resolve_node, NodeRef};
use crate::graph_view_model::{merge_node_id, split_node_id};

/// The split a split or mixer node `node_id` belongs to.
pub(crate) fn dragged_split(chain: &Chain, node_id: &str) -> Option<BlockId> {
    match resolve_node(chain, node_id)? {
        NodeRef::Split { id } | NodeRef::Mixer { id } => Some(id),
        _ => None,
    }
}

/// Every block the split `id` holds in its paths, at any depth.
pub(crate) fn held_blocks<'a>(chain: &'a Chain, id: &BlockId) -> Vec<&'a AudioBlock> {
    split_by_id(chain, id)
        .map(|(_, split)| {
            split
                .paths
                .iter()
                .flat_map(|path| walk_blocks(path))
                .collect()
        })
        .unwrap_or_default()
}

/// The ids of every graph node the split `id` carries, its own included.
pub(crate) fn group_node_ids(chain: &Chain, id: &BlockId) -> Vec<String> {
    let mut ids = routing_ids(chain, id);
    for block in held_blocks(chain, id) {
        match &block.kind {
            AudioBlockKind::Split(_) => ids.extend(routing_ids(chain, &block.id)),
            _ => ids.push(block.id.0.clone()),
        }
    }
    ids
}

/// The split node, the mixer and the leaf outputs a split may draw.
fn routing_ids(chain: &Chain, id: &BlockId) -> Vec<String> {
    let paths = split_by_id(chain, id).map_or(0, |(_, split)| split.paths.len());
    let mut ids = vec![split_node_id(&id.0), merge_node_id(&id.0)];
    ids.extend((0..paths).map(|path| {
        leaf_output_node_id(&PathRef {
            split: id.clone(),
            path,
        })
    }));
    ids
}
