//! Standalone visual demo for `GraphView`. Hardcodes a signal-chain
//! topology (compressor → drive → split → 2 amps → time fx → reverb)
//! so the component can be validated without the rest of OpenRig.
//!
//! Run:
//!     cargo run -p adapter-gui --example graph_view_demo
//!
//! The example imports the component from
//! `crates/adapter-gui/ui/components/graph_view.slint` via the `slint!`
//! macro — same source the production UI consumes, no duplication.

use adapter_gui::graph_view_model::{
    self as model, linear_chain_layout, BlockBlueprint, ChainStage, GridMetrics, NodeCategory,
};
use slint::Model;

slint::slint! {
    import { GraphView, GraphNode, GraphEdgeGeometry }
        from "ui/components/graph_view.slint";

    export component DemoWindow inherits Window {
        in property <[GraphNode]> nodes;
        in property <[GraphEdgeGeometry]> edges;

        callback node-clicked(string);
        callback node-double-clicked(string);
        callback node-dragged(string, length, length);
        callback node-drag-ended(string, length, length);

        title: "GraphView demo";
        preferred-width: 1500px;
        preferred-height: 520px;
        background: #0b0d12;

        VerticalLayout {
            padding: 0px;

            Rectangle {
                height: 36px;
                background: #161b25;
                Text {
                    x: 16px;
                    text: "GraphView demo — drag a node, scroll to zoom, drag empty area to pan, double-click for editor stub";
                    color: #c9d3e2;
                    font-size: 11px;
                    vertical-alignment: center;
                }
            }

            graph := GraphView {
                nodes: root.nodes;
                edges: root.edges;
                node-width: 110px;
                node-height: 56px;
                node-clicked(id) => { root.node-clicked(id); }
                node-double-clicked(id) => { root.node-double-clicked(id); }
                node-dragged(id, x, y) => { root.node-dragged(id, x, y); }
                node-drag-ended(id, x, y) => { root.node-drag-ended(id, x, y); }
            }
        }
    }
}

fn demo_chain() -> (Vec<model::GraphNode>, Vec<model::GraphEdge>) {
    let stages = vec![
        ChainStage::Single(BlockBlueprint::new(
            "noise",
            "Noise Gate",
            NodeCategory::Dynamics,
        )),
        ChainStage::Single(BlockBlueprint::new(
            "comp",
            "Compressor",
            NodeCategory::Dynamics,
        )),
        ChainStage::Single(BlockBlueprint::new(
            "drive",
            "Overdrive",
            NodeCategory::Drive,
        )),
        ChainStage::Parallel(vec![
            vec![
                BlockBlueprint::new("amp_l", "Amp L (Vox)", NodeCategory::Amp),
                BlockBlueprint::new("dly_l", "Delay L 1/8D", NodeCategory::Time),
            ],
            vec![
                BlockBlueprint::new("amp_r", "Amp R (Vox)", NodeCategory::Amp),
                BlockBlueprint::new("dly_r", "Delay R 1/8", NodeCategory::Time),
            ],
        ]),
        ChainStage::Single(BlockBlueprint::new(
            "reverb",
            "Shimmer Reverb",
            NodeCategory::Reverb,
        )),
        ChainStage::Single(BlockBlueprint::new("out", "Output", NodeCategory::Output)),
    ];
    linear_chain_layout(&stages, GridMetrics::default())
}

/// Convert pure-Rust `GraphNode` / `GraphEdge` into the Slint-generated
/// structs the component consumes. Edge geometry resolves each edge's
/// node ids to actual coordinates so the Slint side doesn't need to
/// do the lookup per frame.
fn into_slint_models(
    nodes: Vec<model::GraphNode>,
    edges: Vec<model::GraphEdge>,
) -> (Vec<GraphNode>, Vec<GraphEdgeGeometry>) {
    use std::collections::HashMap;

    let coords: HashMap<&str, (f32, f32)> =
        nodes.iter().map(|n| (n.id.as_str(), (n.x, n.y))).collect();

    let slint_nodes = nodes
        .iter()
        .map(|n| GraphNode {
            id: n.id.clone().into(),
            label: n.label.clone().into(),
            category: n.category.as_str().into(),
            layout_x: n.x,
            layout_y: n.y,
            bypass: n.bypass,
            selected: false,
        })
        .collect();

    let slint_edges = edges
        .iter()
        .filter_map(|e| {
            let from = coords.get(e.from_id.as_str())?;
            let to = coords.get(e.to_id.as_str())?;
            Some(GraphEdgeGeometry {
                from_id: e.from_id.clone().into(),
                to_id: e.to_id.clone().into(),
                from_x: from.0,
                from_y: from.1,
                to_x: to.0,
                to_y: to.1,
            })
        })
        .collect();

    (slint_nodes, slint_edges)
}

fn main() -> Result<(), slint::PlatformError> {
    env_logger::init();

    let (nodes, edges) = demo_chain();
    let errors = adapter_gui::graph_view_model::validate_graph(&nodes, &edges);
    assert!(errors.is_empty(), "demo graph is invalid: {errors:?}");

    let (slint_nodes, slint_edges) = into_slint_models(nodes, edges);

    let window = DemoWindow::new()?;
    window.set_nodes(slint::ModelRc::new(slint::VecModel::from(slint_nodes)));
    window.set_edges(slint::ModelRc::new(slint::VecModel::from(slint_edges)));

    let weak = window.as_weak();
    window.on_node_clicked(move |id| {
        log::info!("clicked: {id}");
        if let Some(w) = weak.upgrade() {
            let mut nodes: Vec<_> = w.get_nodes().iter().collect();
            for n in nodes.iter_mut() {
                n.selected = n.id == id;
            }
            w.set_nodes(slint::ModelRc::new(slint::VecModel::from(nodes)));
        }
    });

    window.on_node_double_clicked(|id| {
        log::info!("double-clicked: {id} (would open block editor)");
    });

    let weak_drag = window.as_weak();
    window.on_node_dragged(move |id, x, y| {
        let Some(w) = weak_drag.upgrade() else { return };
        let mut nodes: Vec<_> = w.get_nodes().iter().collect();
        let mut edges: Vec<_> = w.get_edges().iter().collect();
        for n in nodes.iter_mut() {
            if n.id == id {
                n.layout_x = x;
                n.layout_y = y;
            }
        }
        for e in edges.iter_mut() {
            if e.from_id == id {
                e.from_x = x;
                e.from_y = y;
            }
            if e.to_id == id {
                e.to_x = x;
                e.to_y = y;
            }
        }
        w.set_nodes(slint::ModelRc::new(slint::VecModel::from(nodes)));
        w.set_edges(slint::ModelRc::new(slint::VecModel::from(edges)));
    });

    window.on_node_drag_ended(|id, x, y| {
        log::info!("drag end: {id} -> ({x}, {y})");
    });

    window.run()
}
