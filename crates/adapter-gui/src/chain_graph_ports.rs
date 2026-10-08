//! Responsibility: picks the hub port each graph wire end plugs into.
//!
//! #398. A split hub has one output port per path and a mixer one input port
//! per path; a wire into or out of a hub ends on the port of the path it runs
//! in, so it leaves the hub where the glyph draws that path. Every other end
//! sits on its node's centre line (-1).

use project::block::PathRef;
use project::chain::Chain;
use project::endpoint_disables::EndpointNode;

use crate::chain_graph_ids::{resolve_node, NodeRef};
use crate::graph_view_model::{MERGE_NODE_PREFIX, SPLIT_NODE_PREFIX};

/// The split block id a split or mixer hub node stands for.
pub(crate) fn hub_split(node_id: &str) -> Option<&str> {
    node_id
        .strip_prefix(SPLIT_NODE_PREFIX)
        .or_else(|| node_id.strip_prefix(MERGE_NODE_PREFIX))
}

/// The port of `hub_id` the wire to or from `other_id` plugs into; -1 when
/// `hub_id` is no hub or `other_id` sits outside its paths.
pub(crate) fn hub_port(chain: &Chain, hub_id: &str, other_id: &str) -> i32 {
    let Some(split) = hub_split(hub_id) else {
        return -1;
    };
    match parent_path(chain, other_id) {
        Some(path) if path.split.0 == split => path.path as i32,
        _ => -1,
    }
}

/// The split path a node sits in directly; a nested split's hubs sit where
/// its block does.
fn parent_path(chain: &Chain, node_id: &str) -> Option<PathRef> {
    match resolve_node(chain, hub_split(node_id).unwrap_or(node_id))? {
        NodeRef::Block { path, .. } => path,
        NodeRef::Endpoints(EndpointNode::PathOutput(leaf)) => Some(leaf),
        _ => None,
    }
}
