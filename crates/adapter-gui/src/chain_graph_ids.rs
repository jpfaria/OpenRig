//! Responsibility: names every node of a chain graph.
//!
//! #328 (spec §5.2, §11). A block node carries the block's own `BlockId`, so a
//! gesture on a card names the block wherever it sits. The input/output nodes
//! and the split/mixer routing nodes are not blocks. `linear_chain_layout`
//! names a split's nodes after the split's own id (`__split_<id>` and, when it
//! ends in a Mix, `__merge_<id>`), and every Y leaf ends in its own output
//! node `__out_<split>_<path>`, so every name stays stable at any depth.

use domain::ids::BlockId;
use project::block::{y_leaves, AudioBlock, AudioBlockKind, PathRef, SplitEnd};
use project::chain::Chain;
use project::endpoint_disables::EndpointNode;

use crate::chain_block_lists::split_by_id;
use crate::graph_view_model::{MERGE_NODE_PREFIX, SPLIT_NODE_PREFIX};

pub(crate) const INPUT_NODE_ID: &str = "__io_input";
pub(crate) const OUTPUT_NODE_ID: &str = "__io_output";

const LEAF_OUTPUT_PREFIX: &str = "__out_";

/// What a graph node stands for in its chain.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NodeRef {
    /// The block at `index` of the list `path` names (`None` = top level).
    Block {
        id: BlockId,
        path: Option<PathRef>,
        index: usize,
    },
    /// The split node of the split block `id`.
    Split { id: BlockId },
    /// The mixer node of the Mix split block `id`.
    Mixer { id: BlockId },
    /// An input or output node — which endpoint set it shows.
    Endpoints(EndpointNode),
}

/// Id of the output node Y leaf `leaf` ends in.
pub(crate) fn leaf_output_node_id(leaf: &PathRef) -> String {
    format!("{LEAF_OUTPUT_PREFIX}{}_{}", leaf.split.0, leaf.path)
}

pub(crate) fn resolve_node(chain: &Chain, node_id: &str) -> Option<NodeRef> {
    match node_id {
        INPUT_NODE_ID => return Some(NodeRef::Endpoints(EndpointNode::Input)),
        OUTPUT_NODE_ID => return Some(NodeRef::Endpoints(EndpointNode::Output)),
        _ => {}
    }
    if let Some(rest) = node_id.strip_prefix(LEAF_OUTPUT_PREFIX) {
        return leaf_of(chain, rest).map(|leaf| NodeRef::Endpoints(EndpointNode::PathOutput(leaf)));
    }
    if let Some(id) = node_id.strip_prefix(SPLIT_NODE_PREFIX) {
        let id = BlockId(id.to_string());
        return split_by_id(chain, &id).map(|_| NodeRef::Split { id });
    }
    if let Some(id) = node_id.strip_prefix(MERGE_NODE_PREFIX) {
        let id = BlockId(id.to_string());
        return split_by_id(chain, &id)
            .filter(|(_, split)| split.end == SplitEnd::Mix)
            .map(|_| NodeRef::Mixer { id });
    }
    block_in(&chain.blocks, None, node_id)
}

/// The Y leaf `<split>_<path>` names, when the chain has it.
fn leaf_of(chain: &Chain, rest: &str) -> Option<PathRef> {
    let (split, path) = rest.rsplit_once('_')?;
    let leaf = PathRef {
        split: BlockId(split.to_string()),
        path: path.parse().ok()?,
    };
    y_leaves(&chain.blocks).contains(&leaf).then_some(leaf)
}

fn block_in(blocks: &[AudioBlock], path: Option<PathRef>, node_id: &str) -> Option<NodeRef> {
    for (index, block) in blocks.iter().enumerate() {
        if block.id.0 == node_id {
            return Some(NodeRef::Block {
                id: block.id.clone(),
                path,
                index,
            });
        }
        if let AudioBlockKind::Split(split) = &block.kind {
            for (at, lane) in split.paths.iter().enumerate() {
                let lane_path = PathRef {
                    split: block.id.clone(),
                    path: at,
                };
                if let Some(found) = block_in(lane, Some(lane_path), node_id) {
                    return Some(found);
                }
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "chain_graph_ids_tests.rs"]
mod tests;
