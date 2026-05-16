//! Standalone visual demo for `GraphView`. Hardcodes a signal-chain
//! topology (compressor → drive → split → 2 amps → time fx → reverb)
//! so the component can be validated without the rest of OpenRig.
//!
//! Kept as its own crate (not an `adapter-gui` example) so the gui crate
//! carries no demo build weight while the demo stays preserved and
//! runnable.
//!
//! Run:
//!     cargo run -p graphview-demo
//!
//! The demo imports the component from
//! `crates/adapter-gui/ui/components/graph_view.slint` via the `slint!`
//! macro — same source the production UI consumes, no duplication.

use adapter_gui::graph_view_model::{
    self as model, default_palette, linear_chain_layout, reorder_for_drop, topological_layout,
    BlockBlueprint, ChainStage, GridMetrics, NodeCategory,
};
use slint::Model;
use std::collections::HashMap;

slint::slint! {
    import { CheckBox } from "std-widgets.slint";
    import { GraphView, GraphNode, GraphEdgeGeometry }
        from "../../adapter-gui/ui/components/graph_view.slint";

    export component DemoWindow inherits Window {
        in property <[GraphNode]> nodes;
        in property <[GraphEdgeGeometry]> edges;
        out property <bool> auto <=> auto_box.checked;

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
                HorizontalLayout {
                    padding-left: 16px;
                    spacing: 16px;
                    Text {
                        text: "GraphView demo — drag a node, scroll zoom, drag empty area pan, double-click editor stub";
                        color: #c9d3e2;
                        font-size: 11px;
                        vertical-alignment: center;
                    }
                    auto_box := CheckBox {
                        text: "auto-layout";
                        checked: true;
                    }
                }
            }

            graph := GraphView {
                nodes: root.nodes;
                edges: root.edges;
                node-width: 110px;
                node-height: 56px;
                auto-layout: root.auto;
                node-clicked(id) => { root.node-clicked(id); }
                node-double-clicked(id) => { root.node-double-clicked(id); }
                node-dragged(id, x, y) => { root.node-dragged(id, x, y); }
                node-drag-ended(id, x, y) => { root.node-drag-ended(id, x, y); }
            }
        }
    }
}

/// Resolve every category to a Slint colour once, from the single-source
/// `default_palette()`. Host owns colour; the component is colour-blind.
fn palette_lookup() -> HashMap<String, (slint::Color, slint::Color)> {
    default_palette()
        .into_iter()
        .map(|s| {
            let to_c = |rgb: u32| {
                slint::Color::from_rgb_u8(
                    ((rgb >> 16) & 0xff) as u8,
                    ((rgb >> 8) & 0xff) as u8,
                    (rgb & 0xff) as u8,
                )
            };
            (s.category.to_string(), (to_c(s.fill), to_c(s.border)))
        })
        .collect()
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
    palette: &HashMap<String, (slint::Color, slint::Color)>,
) -> (Vec<GraphNode>, Vec<GraphEdgeGeometry>) {
    let coords: HashMap<&str, (f32, f32)> =
        nodes.iter().map(|n| (n.id.as_str(), (n.x, n.y))).collect();

    let neutral = (
        slint::Color::from_rgb_u8(0x8a, 0x94, 0xa2),
        slint::Color::from_rgb_u8(0x5c, 0x63, 0x6e),
    );
    let slint_nodes = nodes
        .iter()
        .map(|n| {
            let (fill, border) = palette.get(n.category.as_str()).copied().unwrap_or(neutral);
            GraphNode {
                id: n.id.clone().into(),
                label: n.label.clone().into(),
                category: n.category.as_str().into(),
                fill,
                border,
                layout_x: n.x,
                layout_y: n.y,
                bypass: n.bypass,
                selected: false,
            }
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

type NodeModel = std::rc::Rc<slint::VecModel<GraphNode>>;
type EdgeModel = std::rc::Rc<slint::VecModel<GraphEdgeGeometry>>;

/// Apply resolved coords back to the shared models in place (no Vec
/// rebuild — issue #435 invariant: zero per-frame reallocation).
fn apply_coords(nm: &NodeModel, em: &EdgeModel, coords: &HashMap<String, (f32, f32)>) {
    for i in 0..nm.row_count() {
        let mut nd = nm.row_data(i).unwrap();
        if let Some((x, y)) = coords.get(nd.id.as_str()) {
            nd.layout_x = *x;
            nd.layout_y = *y;
            nm.set_row_data(i, nd);
        }
    }
    for i in 0..em.row_count() {
        let mut ed = em.row_data(i).unwrap();
        if let Some((x, y)) = coords.get(ed.from_id.as_str()) {
            ed.from_x = *x;
            ed.from_y = *y;
        }
        if let Some((x, y)) = coords.get(ed.to_id.as_str()) {
            ed.to_x = *x;
            ed.to_y = *y;
        }
        em.set_row_data(i, ed);
    }
}

fn wire_callbacks(
    window: &DemoWindow,
    node_model: &NodeModel,
    edge_model: &EdgeModel,
    nodes: Vec<model::GraphNode>,
    edges: Vec<model::GraphEdge>,
) {
    let nm = node_model.clone();
    window.on_node_clicked(move |id| {
        log::info!("clicked: {id}");
        for i in 0..nm.row_count() {
            let mut n = nm.row_data(i).unwrap();
            let want = n.id == id;
            if n.selected != want {
                n.selected = want;
                nm.set_row_data(i, n);
            }
        }
    });

    window.on_node_double_clicked(|id| {
        log::info!("double-clicked: {id} (would open block editor)");
    });

    let (nm, em) = (node_model.clone(), edge_model.clone());
    window.on_node_dragged(move |id, x, y| {
        let mut c = HashMap::new();
        c.insert(id.to_string(), (x, y));
        apply_coords(&nm, &em, &c);
    });

    let (nm, em, weak) = (node_model.clone(), edge_model.clone(), window.as_weak());
    window.on_node_drag_ended(move |id, x, y| {
        log::info!("drag end: {id} -> ({x}, {y})");
        let Some(w) = weak.upgrade() else { return };
        if !w.get_auto() {
            return;
        }
        let relaid = reorder_for_drop(&nodes, &edges, id.as_str(), y, GridMetrics::default());
        let coords: HashMap<String, (f32, f32)> =
            relaid.iter().map(|n| (n.id.clone(), (n.x, n.y))).collect();
        apply_coords(&nm, &em, &coords);
    });
}

fn main() -> Result<(), slint::PlatformError> {
    env_logger::init();

    let (nodes, edges) = demo_chain();
    let errors = adapter_gui::graph_view_model::validate_graph(&nodes, &edges);
    assert!(errors.is_empty(), "demo graph is invalid: {errors:?}");

    let palette = palette_lookup();
    // Start from the topological auto-layout (auto-layout defaults on).
    let laid = topological_layout(&nodes, &edges, GridMetrics::default());
    let (slint_nodes, slint_edges) = into_slint_models(laid, edges.clone(), &palette);

    // Shared models, mutated in place (see apply_coords).
    let node_model: NodeModel = std::rc::Rc::new(slint::VecModel::from(slint_nodes));
    let edge_model: EdgeModel = std::rc::Rc::new(slint::VecModel::from(slint_edges));

    let window = DemoWindow::new()?;
    window.set_nodes(node_model.clone().into());
    window.set_edges(edge_model.clone().into());
    wire_callbacks(&window, &node_model, &edge_model, nodes, edges);

    window.run()
}
