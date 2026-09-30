//! Responsibility: names every node of a chain graph.
//!
//! #328 (spec §5.2). A block node carries the block's own `BlockId`, so a
//! gesture on a card names the block wherever it sits. The input/output nodes
//! and the split/mixer routing nodes are not blocks; their ids are the fixed
//! strings below. The split/mixer ids are the ones `linear_chain_layout` gives
//! the chain's single `Parallel` stage (one split per chain, spec §1.1).

use domain::ids::BlockId;
use project::block::{AudioBlockKind, PathRef, PathSide};
use project::chain::Chain;
use project::endpoint_disables::EndpointNode;

use crate::chain_block_lists::split_of;

pub(crate) const INPUT_NODE_ID: &str = "__io_input";
pub(crate) const OUTPUT_NODE_ID: &str = "__io_output";
pub(crate) const PATH_A_OUTPUT_NODE_ID: &str = "__io_output_a";
pub(crate) const PATH_B_OUTPUT_NODE_ID: &str = "__io_output_b";
pub(crate) const SPLIT_NODE_ID: &str = "__split_1";
pub(crate) const MIXER_NODE_ID: &str = "__merge_1";

/// What a graph node stands for in its chain.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NodeRef {
    /// The block at `index` of the list `path` names (`None` = top level).
    Block {
        id: BlockId,
        path: Option<PathRef>,
        index: usize,
    },
    /// The chain's split node (one split per chain: `split_of` finds it).
    Split,
    /// The chain's mixer node (Split → Mix).
    Mixer,
    /// An input or output node — which endpoint set it shows.
    Endpoints(EndpointNode),
}

pub(crate) fn resolve_node(chain: &Chain, node_id: &str) -> Option<NodeRef> {
    match node_id {
        INPUT_NODE_ID => return Some(NodeRef::Endpoints(EndpointNode::Input)),
        OUTPUT_NODE_ID => return Some(NodeRef::Endpoints(EndpointNode::Output)),
        PATH_A_OUTPUT_NODE_ID => return Some(NodeRef::Endpoints(EndpointNode::PathAOutput)),
        PATH_B_OUTPUT_NODE_ID => return Some(NodeRef::Endpoints(EndpointNode::PathBOutput)),
        SPLIT_NODE_ID => return split_of(chain).map(|_| NodeRef::Split),
        MIXER_NODE_ID => return split_of(chain).map(|_| NodeRef::Mixer),
        _ => {}
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

#[cfg(test)]
#[path = "chain_graph_ids_tests.rs"]
mod tests;
