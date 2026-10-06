//! Responsibility: publishes a chain graph as the row's Slint models.
//!
//! #328 (spec §5.2). Built once per row rebuild (`project_chains_refresh.rs`).
//! The meter tick clones the row and writes it back
//! (`meter_wiring_poll.rs`); these models ride along as the same `Rc`s, so a
//! tick never rebuilds a graph. A block node carries its strip tile in
//! `GraphNode.block`, which Part 5's card and tooltip draw.

use std::rc::Rc;

use slint::{Color, ModelRc, VecModel};

use project::chain::Chain;
use project::endpoint_disables::EndpointNode;

use crate::chain_block_item::chain_block_item_from_block;
use crate::chain_block_lists::block_at;
use crate::chain_graph_adapter::ChainGraph;
use crate::chain_graph_ids::{resolve_node, NodeRef};
use crate::graph_view_model::{default_palette, GraphNode as LaidOutNode, NodeCategory, NodeKind};
use crate::{ChainBlockItem, GraphAnchor, GraphEdgeGeometry, GraphNode};

pub(crate) struct RowGraphModels {
    pub(crate) nodes: ModelRc<GraphNode>,
    pub(crate) edges: ModelRc<GraphEdgeGeometry>,
    pub(crate) anchors: ModelRc<GraphAnchor>,
}

pub(crate) fn row_graph_models(chain: &Chain, graph: &ChainGraph) -> RowGraphModels {
    let nodes: Vec<GraphNode> = graph
        .nodes
        .iter()
        .map(|node| slint_node(chain, node))
        .collect();
    let anchors: Vec<GraphAnchor> = graph
        .anchors
        .iter()
        .map(|anchor| GraphAnchor {
            id: anchor.id.as_str().into(),
            layout_x: anchor.x,
            layout_y: anchor.y,
            always_visible: anchor.always_visible,
        })
        .collect();
    RowGraphModels {
        nodes: ModelRc::from(Rc::new(VecModel::from(nodes))),
        edges: ModelRc::from(Rc::new(VecModel::from(edge_geometry(chain, graph)))),
        anchors: ModelRc::from(Rc::new(VecModel::from(anchors))),
    }
}

/// The strip's tile for a block node.
fn block_tile(chain: &Chain, node_id: &str) -> ChainBlockItem {
    match resolve_node(chain, node_id) {
        Some(NodeRef::Block { path, index, .. }) => block_at(chain, index, path.as_ref())
            .map(chain_block_item_from_block)
            .unwrap_or_default(),
        _ => ChainBlockItem::default(),
    }
}

fn slint_node(chain: &Chain, node: &LaidOutNode) -> GraphNode {
    let (fill, border) = category_colours(node.category);
    GraphNode {
        id: node.id.as_str().into(),
        label: node.label.as_str().into(),
        category: node.category.as_str().into(),
        fill,
        border,
        layout_x: node.x,
        layout_y: node.y,
        bypass: node.bypass,
        kind: node.kind.as_str().into(),
        // An empty tile on I/O, split and mixer nodes: no tooltip there.
        block: if node.kind == NodeKind::Block {
            block_tile(chain, &node.id)
        } else {
            ChainBlockItem::default()
        },
        ..Default::default()
    }
}

/// Every node takes the GraphView palette — its single source of truth
/// (`graph_view_model/palette.rs`).
fn category_colours(category: NodeCategory) -> (Color, Color) {
    default_palette()
        .into_iter()
        .find(|style| style.category == category.as_str())
        .map(|style| (rgb(style.fill), rgb(style.border)))
        .unwrap_or_default()
}

fn rgb(value: u32) -> Color {
    Color::from_rgb_u8((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

/// One wire per edge; an edge that bends through a point (an empty lane)
/// draws as two wires meeting there, each still moving with its own node.
fn edge_geometry(chain: &Chain, graph: &ChainGraph) -> Vec<GraphEdgeGeometry> {
    let centre = |id: &str| graph.nodes.iter().find(|n| n.id == id).map(|n| (n.x, n.y));
    graph
        .edges
        .iter()
        .filter_map(|edge| {
            let from = centre(&edge.from_id)?;
            let to = centre(&edge.to_id)?;
            Some(match edge.via {
                None => {
                    let lane = wire_lane(chain, &edge.from_id, &edge.to_id);
                    vec![wire(&edge.from_id, from, &edge.to_id, to, lane)]
                }
                Some(via) => {
                    let lane = via.path as i32;
                    vec![
                        wire(&edge.from_id, from, "", (via.x, via.y), lane),
                        wire("", (via.x, via.y), &edge.to_id, to, lane),
                    ]
                }
            })
        })
        .flatten()
        .collect()
}

/// #398: the split path a wire runs in — the path of the block (or Y leaf
/// output) at either end; -1 when both ends sit on the trunk.
fn wire_lane(chain: &Chain, from_id: &str, to_id: &str) -> i32 {
    let lane = |id: &str| match resolve_node(chain, id) {
        Some(NodeRef::Block {
            path: Some(path), ..
        }) => Some(path.path as i32),
        Some(NodeRef::Endpoints(EndpointNode::PathOutput(leaf))) => Some(leaf.path as i32),
        _ => None,
    };
    lane(to_id).or_else(|| lane(from_id)).unwrap_or(-1)
}

fn wire(
    from_id: &str,
    from: (f32, f32),
    to_id: &str,
    to: (f32, f32),
    path: i32,
) -> GraphEdgeGeometry {
    GraphEdgeGeometry {
        from_id: from_id.into(),
        to_id: to_id.into(),
        from_x: from.0,
        from_y: from.1,
        to_x: to.0,
        to_y: to.1,
        path,
    }
}

#[cfg(test)]
#[path = "chain_graph_models_tests.rs"]
mod tests;
