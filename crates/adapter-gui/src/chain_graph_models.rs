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
        edges: ModelRc::from(Rc::new(VecModel::from(edge_geometry(graph)))),
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

fn edge_geometry(graph: &ChainGraph) -> Vec<GraphEdgeGeometry> {
    let centre = |id: &str| graph.nodes.iter().find(|n| n.id == id).map(|n| (n.x, n.y));
    graph
        .edges
        .iter()
        .filter_map(|edge| {
            let (from_x, from_y) = centre(&edge.from_id)?;
            let (to_x, to_y) = centre(&edge.to_id)?;
            Some(GraphEdgeGeometry {
                from_id: edge.from_id.as_str().into(),
                to_id: edge.to_id.as_str().into(),
                from_x,
                from_y,
                to_x,
                to_y,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "chain_graph_models_tests.rs"]
mod tests;
