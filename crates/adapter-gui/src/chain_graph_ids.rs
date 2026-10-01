//! Responsibility: names every node of a chain graph.
//!
//! #328 (spec §5.2). A block node carries the block's own `BlockId`, so a
//! gesture on a card names the block wherever it sits. The input/output nodes
//! and the split/mixer routing nodes are not blocks. `linear_chain_layout`
//! gives the n-th split of the chain (1-based, top-level order) the nodes
//! `__split_n` and, when it ends in a Mix, `__merge_n`; a chain holds at most
//! a Mix, then a Y (spec §1.1).

use domain::ids::BlockId;
use project::block::{AudioBlockKind, PathRef, PathSide, SplitEnd};
use project::chain::Chain;
use project::endpoint_disables::EndpointNode;

pub(crate) const INPUT_NODE_ID: &str = "__io_input";
pub(crate) const OUTPUT_NODE_ID: &str = "__io_output";
pub(crate) const PATH_A_OUTPUT_NODE_ID: &str = "__io_output_a";
pub(crate) const PATH_B_OUTPUT_NODE_ID: &str = "__io_output_b";

const SPLIT_NODE_PREFIX: &str = "__split_";
const MIXER_NODE_PREFIX: &str = "__merge_";

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

pub(crate) fn resolve_node(chain: &Chain, node_id: &str) -> Option<NodeRef> {
    match node_id {
        INPUT_NODE_ID => return Some(NodeRef::Endpoints(EndpointNode::Input)),
        OUTPUT_NODE_ID => return Some(NodeRef::Endpoints(EndpointNode::Output)),
        PATH_A_OUTPUT_NODE_ID => return Some(NodeRef::Endpoints(EndpointNode::PathAOutput)),
        PATH_B_OUTPUT_NODE_ID => return Some(NodeRef::Endpoints(EndpointNode::PathBOutput)),
        _ => {}
    }
    if let Some(ordinal) = node_id.strip_prefix(SPLIT_NODE_PREFIX) {
        return nth_split(chain, ordinal).map(|(id, _)| NodeRef::Split { id });
    }
    if let Some(ordinal) = node_id.strip_prefix(MIXER_NODE_PREFIX) {
        return nth_split(chain, ordinal)
            .filter(|(_, end)| *end == SplitEnd::Mix)
            .map(|(id, _)| NodeRef::Mixer { id });
    }
    for (index, block) in chain.blocks.iter().enumerate() {
        if block.id.0 == node_id {
            return Some(NodeRef::Block {
                id: block.id.clone(),
                path: None,
                index,
            });
        }
        if let AudioBlockKind::Split(split) = &block.kind {
            for (side, lane) in [(PathSide::A, &split.a), (PathSide::B, &split.b)] {
                if let Some(index) = lane.iter().position(|b| b.id.0 == node_id) {
                    return Some(NodeRef::Block {
                        id: lane[index].id.clone(),
                        path: Some(PathRef {
                            split: block.id.clone(),
                            side,
                        }),
                        index,
                    });
                }
            }
        }
    }
    None
}

/// The id and end of the split a 1-based `ordinal` names.
fn nth_split(chain: &Chain, ordinal: &str) -> Option<(BlockId, SplitEnd)> {
    let n: usize = ordinal.parse().ok()?;
    let (position, split) = project::block::splits(&chain.blocks).nth(n.checked_sub(1)?)?;
    Some((chain.blocks[position].id.clone(), split.end))
}

#[cfg(test)]
#[path = "chain_graph_ids_tests.rs"]
mod tests;
