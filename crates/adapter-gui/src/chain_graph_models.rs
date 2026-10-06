//! Responsibility: publishes a chain graph as the row's Slint models.
//!
//! #328 (spec §5.2). Built once per row rebuild (`project_chains_refresh.rs`).
//! The meter tick clones the row and writes it back
//! (`meter_wiring_poll.rs`); these models ride along as the same `Rc`s, so a
//! tick never rebuilds a graph. A block node carries its strip tile in
//! `GraphNode.block`, which Part 5's card and tooltip draw.

use std::rc::Rc;

use slint::{Color, ModelRc, SharedString, VecModel};

use project::chain::Chain;
use project::endpoint_disables::EndpointNode;

use crate::chain_block_item::chain_block_item_from_block;
use crate::chain_block_lists::block_at;
use crate::chain_graph_adapter::ChainGraph;
use crate::chain_graph_ids::{resolve_node, NodeRef};
use crate::chain_graph_lanes::{hub_lanes, path_tags};
use crate::chain_graph_ports::{hub_port, hub_split};
use crate::graph_view_model::{default_palette, GraphNode as LaidOutNode, NodeCategory, NodeKind};
use crate::{ChainBlockItem, GraphAnchor, GraphEdgeGeometry, GraphNode};

pub(crate) struct RowGraphModels {
    pub(crate) nodes: ModelRc<GraphNode>,
    pub(crate) edges: ModelRc<GraphEdgeGeometry>,
    pub(crate) anchors: ModelRc<GraphAnchor>,
}

pub(crate) fn row_graph_models(chain: &Chain, graph: &ChainGraph) -> RowGraphModels {
    let tags = path_tags(chain);
    let nodes: Vec<GraphNode> = graph
        .nodes
        .iter()
        .map(|node| slint_node(chain, node, &tags))
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

fn slint_node(chain: &Chain, node: &LaidOutNode, tags: &[(String, String, usize)]) -> GraphNode {
    let (fill, border) = category_colours(node.category);
    let tag = tags.iter().find(|(id, _, _)| *id == node.id);
    let lanes: Vec<SharedString> = hub_lanes(chain, &node.id)
        .into_iter()
        .map(SharedString::from)
        .collect();
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
        lanes: ModelRc::from(Rc::new(VecModel::from(lanes))),
        path_tag: tag
            .map(|(_, letter, _)| letter.as_str())
            .unwrap_or_default()
            .into(),
        path_index: tag.map_or(-1, |(_, _, at)| *at as i32),
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
/// Each end names its node by index and the hub port it plugs into (#398).
fn edge_geometry(chain: &Chain, graph: &ChainGraph) -> Vec<GraphEdgeGeometry> {
    let end = |id: &str, port: i32| {
        let index = graph.nodes.iter().position(|n| n.id == id)?;
        let node = &graph.nodes[index];
        Some(WireEnd {
            id: node.id.as_str(),
            index: index as i32,
            at: (node.x, node.y),
            port,
        })
    };
    graph
        .edges
        .iter()
        .filter_map(|edge| {
            Some(match edge.via {
                None => {
                    let from = end(&edge.from_id, hub_port(chain, &edge.from_id, &edge.to_id))?;
                    let to = end(&edge.to_id, hub_port(chain, &edge.to_id, &edge.from_id))?;
                    vec![wire(from, to, wire_lane(chain, &edge.from_id, &edge.to_id))]
                }
                Some(via) => {
                    let lane = via.path as i32;
                    let port = |id: &str| if hub_split(id).is_some() { lane } else { -1 };
                    let from = end(&edge.from_id, port(&edge.from_id))?;
                    let to = end(&edge.to_id, port(&edge.to_id))?;
                    let bend = WireEnd {
                        id: "",
                        index: -1,
                        at: (via.x, via.y),
                        port: -1,
                    };
                    vec![wire(from, bend, lane), wire(bend, to, lane)]
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

/// One end of a wire: its node (none for a bend point), where it sits and
/// the hub port it plugs into.
#[derive(Clone, Copy)]
struct WireEnd<'a> {
    id: &'a str,
    index: i32,
    at: (f32, f32),
    port: i32,
}

fn wire(from: WireEnd, to: WireEnd, path: i32) -> GraphEdgeGeometry {
    GraphEdgeGeometry {
        from_id: from.id.into(),
        to_id: to.id.into(),
        from_x: from.at.0,
        from_y: from.at.1,
        to_x: to.at.0,
        to_y: to.at.1,
        path,
        from_index: from.index,
        to_index: to.index,
        from_port: from.port,
        to_port: to.port,
    }
}

#[cfg(test)]
#[path = "chain_graph_models_tests.rs"]
mod tests;
