//! #328 — headless proof of the GraphView pointer contract, the canvas the
//! chain editor is built on. REAL pointer / wheel / key events dispatched at
//! real geometry through i-slint-backend-testing: a render PNG proves layout
//! only, this proves the gesture lands (#749/#761).
//!
//! The harness canvas sits at the window origin with zoom 1 and no pan, so a
//! node's layout coordinates ARE its window coordinates.

use adapter_gui::graph_view_model as model;
use adapter_gui::{
    ChainBlockItem, GraphAnchor, GraphEdgeGeometry, GraphNode, GraphViewHarness,
    GraphViewScrollHarness,
};
use i_slint_backend_testing::ElementHandle;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

fn node(id: &str, label: &str, x: f32, y: f32) -> GraphNode {
    GraphNode {
        id: id.into(),
        label: label.into(),
        category: "drive".into(),
        layout_x: x,
        layout_y: y,
        ..Default::default()
    }
}

fn harness(nodes: Vec<GraphNode>) -> GraphViewHarness {
    i_slint_backend_testing::init_no_event_loop();
    let w = GraphViewHarness::new().unwrap();
    w.set_nodes(ModelRc::new(VecModel::from(nodes)));
    w.show().unwrap();
    w
}

fn at(x: f32, y: f32) -> LogicalPosition {
    LogicalPosition::new(x, y)
}

/// Collects what a callback fired with.
fn recorder<T: 'static>() -> Rc<RefCell<Vec<T>>> {
    Rc::default()
}

fn click_at(w: &impl ComponentHandle, p: LogicalPosition) {
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: p });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: p,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerReleased {
        position: p,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerExited);
}

/// Press at `from`, move in ten steps to `to`, release there.
fn drag(w: &impl ComponentHandle, from: LogicalPosition, to: LogicalPosition) {
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: from });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: from,
        button: PointerEventButton::Left,
    });
    for step in 1..=10 {
        let t = step as f32 / 10.0;
        let p = at(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        win.dispatch_event(WindowEvent::PointerMoved { position: p });
    }
    win.dispatch_event(WindowEvent::PointerReleased {
        position: to,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerExited);
}

#[test]
fn clicking_a_node_card_fires_node_clicked_with_its_id() {
    let w = harness(vec![
        node("od", "Drive", 240.0, 200.0),
        node("amp", "Amp", 400.0, 200.0),
    ]);
    let clicked = recorder::<String>();
    let c = clicked.clone();
    w.on_node_clicked(move |id| c.borrow_mut().push(id.to_string()));

    click_at(&w, at(400.0, 200.0));

    assert_eq!(
        *clicked.borrow(),
        ["amp"],
        "clicking the Amp card must fire node-clicked(\"amp\") once"
    );
}

#[test]
fn dragging_a_node_past_the_click_threshold_ends_a_drag_not_a_click() {
    let w = harness(vec![node("od", "Drive", 240.0, 200.0)]);
    let clicked = recorder::<String>();
    let ended = recorder::<(String, f32, f32)>();
    let c = clicked.clone();
    w.on_node_clicked(move |id| c.borrow_mut().push(id.to_string()));
    let e = ended.clone();
    w.on_node_drag_ended(move |id, x, y| e.borrow_mut().push((id.to_string(), x, y)));

    drag(&w, at(240.0, 200.0), at(300.0, 200.0));

    assert!(
        clicked.borrow().is_empty(),
        "a 60px drag is not a click, got {:?}",
        clicked.borrow()
    );
    assert_eq!(
        *ended.borrow(),
        [("od".to_string(), 240.0, 200.0)],
        "drag-ended fires once with the node's layout position — the host moves nodes, \
         the canvas does not"
    );
}

/// The graph inside a 1200px-tall scroll area; the canvas spans window
/// y 100..400, so (600, 250) is empty canvas.
fn scroll_harness() -> GraphViewScrollHarness {
    i_slint_backend_testing::init_no_event_loop();
    let w = GraphViewScrollHarness::new().unwrap();
    w.set_nodes(ModelRc::new(VecModel::from(vec![node(
        "od", "Drive", 240.0, 150.0,
    )])));
    w.show().unwrap();
    w
}

fn wheel(w: &impl ComponentHandle, p: LogicalPosition, delta_y: f32) {
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: p });
    win.dispatch_event(WindowEvent::PointerScrolled {
        position: p,
        delta_x: 0.0,
        delta_y,
    });
}

#[test]
fn a_plain_wheel_over_the_graph_scrolls_the_list_instead_of_zooming() {
    let w = scroll_harness();

    wheel(&w, at(600.0, 250.0), -120.0);

    assert_eq!(
        w.get_zoom(),
        1.0,
        "a plain wheel must not zoom the graph — it belongs to the chains list \
         around it (#328 owner decision 6)"
    );
    assert!(
        w.get_list_scroll_y() < 0.0,
        "the wheel over the graph must scroll the list around it; viewport-y stayed {}",
        w.get_list_scroll_y()
    );
}

#[test]
fn cmd_or_ctrl_wheel_zooms_the_graph_and_leaves_the_list_still() {
    let w = scroll_harness();

    // Slint reports ⌘ on macOS and Ctrl on Windows/Linux as `control`.
    w.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    wheel(&w, at(600.0, 250.0), 120.0);
    w.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });

    assert!(
        (w.get_zoom() - 1.1).abs() < 1e-4,
        "Cmd/Ctrl + wheel must zoom one step, zoom is {}",
        w.get_zoom()
    );
    assert_eq!(
        w.get_list_scroll_y(),
        0.0,
        "a zoom gesture must not also scroll the list"
    );
}

fn typed(id: &str, kind: &str, label: &str, x: f32, y: f32) -> GraphNode {
    let routing = kind == "split" || kind == "mixer";
    GraphNode {
        kind: kind.into(),
        category: if routing { "util" } else { "drive" }.into(),
        ..node(id, label, x, y)
    }
}

/// in → split → amp → mixer → out: one card per kind, the way
/// graph_view_model emits them (routing nodes: empty label, "util").
fn one_of_each_kind() -> Vec<GraphNode> {
    vec![
        typed("in", "io_input", "In 1", 80.0, 200.0),
        typed("__split_1", "split", "", 240.0, 200.0),
        typed("amp", "block", "Amp", 400.0, 200.0),
        typed("__merge_1", "mixer", "", 560.0, 200.0),
        typed("out", "io_output", "Out 1", 720.0, 200.0),
    ]
}

#[test]
fn every_node_kind_is_a_clickable_card() {
    let nodes = one_of_each_kind();
    let w = harness(nodes.clone());
    let clicked = recorder::<String>();
    let c = clicked.clone();
    w.on_node_clicked(move |id| c.borrow_mut().push(id.to_string()));

    for n in &nodes {
        click_at(&w, at(n.layout_x, n.layout_y));
    }

    assert_eq!(
        *clicked.borrow(),
        ["in", "__split_1", "amp", "__merge_1", "out"],
        "the split and the mixer must be clickable cards, not 8px routing dots (#328 §5.2)"
    );
}

/// A block card with a model: type label, icon and a tooltip name.
fn block_node(id: &str, label: &str, x: f32, y: f32) -> GraphNode {
    GraphNode {
        block: ChainBlockItem {
            type_label: "AMP".into(),
            icon_kind: "amp".into(),
            display_name: "Clean Amp".into(),
            ..Default::default()
        },
        ..typed(id, "block", label, x, y)
    }
}

fn center(el: &ElementHandle) -> LogicalPosition {
    let p = el.absolute_position();
    let s = el.size();
    at(p.x + s.width / 2.0, p.y + s.height / 2.0)
}

/// Every element with `id`, left to right on screen.
fn handles(w: &GraphViewHarness, id: &str) -> Vec<ElementHandle> {
    let mut found: Vec<ElementHandle> = ElementHandle::find_by_element_id(w, id).collect();
    found.sort_by(|a, b| a.absolute_position().x.total_cmp(&b.absolute_position().x));
    found
}

fn hover(w: &GraphViewHarness, p: LogicalPosition) {
    w.window()
        .dispatch_event(WindowEvent::PointerMoved { position: p });
}

#[test]
fn a_block_cards_led_shows_bypass_and_toggles_it_without_a_click() {
    let mut rev = block_node("rev", "Rev", 400.0, 200.0);
    rev.bypass = true;
    let w = harness(vec![block_node("amp", "Amp", 240.0, 200.0), rev]);
    let toggled = recorder::<String>();
    let clicked = recorder::<String>();
    let t = toggled.clone();
    w.on_bypass_toggled(move |id| t.borrow_mut().push(id.to_string()));
    let c = clicked.clone();
    w.on_node_clicked(move |id| c.borrow_mut().push(id.to_string()));

    let leds = handles(&w, "GraphNodeCard::bypass-ta");
    assert_eq!(
        leds.iter()
            .map(|l| l.accessible_checked())
            .collect::<Vec<_>>(),
        [Some(true), Some(false)],
        "the LED reads on for a live block and off for a bypassed one"
    );

    click_at(&w, center(&leds[0]));
    assert_eq!(*toggled.borrow(), ["amp"]);
    assert!(
        clicked.borrow().is_empty(),
        "the LED toggles bypass; it must not also click the node"
    );

    click_at(&w, at(240.0, 200.0));
    assert_eq!(
        *clicked.borrow(),
        ["amp"],
        "the card body still clicks through to the node"
    );
}

#[test]
fn hovering_a_block_card_reveals_a_remove_button_that_fires_remove_requested() {
    let w = harness(vec![block_node("amp", "Amp", 240.0, 200.0)]);
    let removed = recorder::<String>();
    let clicked = recorder::<String>();
    let r = removed.clone();
    w.on_remove_requested(move |id| r.borrow_mut().push(id.to_string()));
    let c = clicked.clone();
    w.on_node_clicked(move |id| c.borrow_mut().push(id.to_string()));

    hover(&w, at(240.0, 200.0));
    let remove = handles(&w, "GraphNodeCard::remove-ta");
    assert_eq!(remove.len(), 1, "a block card carries one × button");
    let p = center(&remove[0]);
    hover(&w, p);
    click_at(&w, p);

    assert_eq!(*removed.borrow(), ["amp"]);
    assert!(
        clicked.borrow().is_empty(),
        "the × removes; it must not click the node"
    );
}

#[test]
fn only_block_cards_carry_a_led_and_a_remove_button() {
    let mut nodes = one_of_each_kind();
    nodes[2] = block_node("amp", "Amp", 400.0, 200.0);
    let w = harness(nodes);
    assert_eq!(handles(&w, "GraphNodeCard::bypass-ta").len(), 1);
    assert_eq!(handles(&w, "GraphNodeCard::remove-ta").len(), 1);
}

#[test]
fn hovering_a_block_with_a_model_shows_its_tooltip() {
    let mut nodes = one_of_each_kind();
    nodes[2] = block_node("amp", "Amp", 400.0, 200.0);
    let w = harness(nodes);
    let tooltips = |w: &GraphViewHarness| {
        ElementHandle::find_by_element_type_name(w, "BlockHoverTooltip").count()
    };

    hover(&w, at(400.0, 200.0));
    assert_eq!(
        tooltips(&w),
        1,
        "hovering a block with a model shows the BlockChip tooltip"
    );

    hover(&w, at(80.0, 200.0));
    assert_eq!(tooltips(&w), 0, "an I/O node has no block tooltip");
}

#[test]
fn an_unavailable_block_card_reads_as_disabled() {
    let mut amp = block_node("amp", "Amp", 240.0, 200.0);
    amp.block.unavailable = true;
    let w = harness(vec![amp, block_node("rev", "Rev", 400.0, 200.0)]);
    let enabled = |label: &str| {
        ElementHandle::find_by_accessible_label(&w, label)
            .next()
            .and_then(|e| e.accessible_enabled())
    };
    assert_eq!(
        enabled("Amp"),
        Some(false),
        "an uninstalled model reads as disabled"
    );
    assert_eq!(enabled("Rev"), Some(true));
}

#[test]
fn a_block_cards_led_and_remove_hit_zones_scale_with_the_zoom() {
    let w = harness(vec![block_node("amp", "Amp", 240.0, 200.0)]);
    w.set_zoom(0.5);
    let size = |id: &str| {
        handles(&w, id)
            .first()
            .map(|h| (h.size().width, h.size().height))
    };
    assert_eq!(
        size("GraphNodeCard::remove-ta"),
        Some((12.0, 12.0)),
        "at half zoom the × is half size; a fixed 24px one covers most of a small card"
    );
    assert_eq!(
        size("GraphNodeCard::bypass-ta"),
        Some((14.0, 10.0)),
        "at half zoom the LED switch is half size"
    );
}

/// in → split → [a1] ∥ [b1] → mixer → out, laid out by the real model on
/// the default grid: in (80,200) · split (240,200) · a1 (400,140) ·
/// b1 (400,260) · mixer (560,200) · out (720,200). Anchors sit on the wire
/// midpoints: split→b1 at (320,230), a1→mixer at (480,170).
fn split_mix_chain() -> (
    Vec<model::GraphNode>,
    Vec<model::GraphEdge>,
    Vec<model::GraphAnchor>,
) {
    let amp =
        |id: &str| model::BlockBlueprint::new(id, id.to_uppercase(), model::NodeCategory::Amp);
    let stages = [
        model::ChainStage::Single(
            model::BlockBlueprint::new("in", "In 1", model::NodeCategory::Input)
                .with_kind(model::NodeKind::IoInput),
        ),
        model::ChainStage::Parallel {
            lanes: vec![vec![amp("a1")], vec![amp("b1")]],
            end: model::ParallelEnd::Merge,
        },
        model::ChainStage::Single(
            model::BlockBlueprint::new("out", "Out 1", model::NodeCategory::Output)
                .with_kind(model::NodeKind::IoOutput),
        ),
    ];
    let (nodes, edges) = model::linear_chain_layout(&stages, model::GridMetrics::default());
    let anchors = model::insert_anchors(&stages, &nodes);
    (nodes, edges, anchors)
}

/// The graph as a host hands it to the canvas: the Rust model mapped field
/// by field onto the Slint structs, with the drop resolver wired to
/// `graph_view_model::resolve_drop_anchor`.
fn canvas_for(
    nodes: &[model::GraphNode],
    edges: &[model::GraphEdge],
    anchors: &[model::GraphAnchor],
) -> GraphViewHarness {
    let w = harness(
        nodes
            .iter()
            .map(|n| GraphNode {
                id: n.id.as_str().into(),
                label: n.label.as_str().into(),
                category: n.category.as_str().into(),
                kind: n.kind.as_str().into(),
                layout_x: n.x,
                layout_y: n.y,
                bypass: n.bypass,
                ..Default::default()
            })
            .collect(),
    );
    let pos = |id: &str| {
        nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| (n.x, n.y))
            .unwrap_or_default()
    };
    let geometry: Vec<GraphEdgeGeometry> = edges
        .iter()
        .map(|e| {
            let (from_x, from_y) = pos(&e.from_id);
            let (to_x, to_y) = pos(&e.to_id);
            GraphEdgeGeometry {
                from_id: e.from_id.as_str().into(),
                to_id: e.to_id.as_str().into(),
                from_x,
                from_y,
                to_x,
                to_y,
            }
        })
        .collect();
    w.set_edges(ModelRc::new(VecModel::from(geometry)));
    let slint_anchors: Vec<GraphAnchor> = anchors
        .iter()
        .map(|a| GraphAnchor {
            id: a.id.as_str().into(),
            layout_x: a.x,
            layout_y: a.y,
            always_visible: a.always_visible,
        })
        .collect();
    w.set_anchors(ModelRc::new(VecModel::from(slint_anchors)));
    let (nodes, anchors) = (nodes.to_vec(), anchors.to_vec());
    w.on_resolve_drop_anchor(move |id, x, y| {
        model::resolve_drop_anchor(&nodes, &anchors, &id, x, y, model::GridMetrics::default())
            .map(|a| SharedString::from(a.id.as_str()))
            .unwrap_or_default()
    });
    w
}

#[test]
fn clicking_a_wire_anchor_fires_add_requested_with_its_slot_id() {
    let (nodes, edges, anchors) = split_mix_chain();
    let w = canvas_for(&nodes, &edges, &anchors);
    let added = recorder::<String>();
    let a = added.clone();
    w.on_add_requested(move |id| a.borrow_mut().push(id.to_string()));

    assert_eq!(
        handles(&w, "BlockInsertSlot::hover-area").len(),
        anchors.len(),
        "one + per wire"
    );
    click_at(&w, at(320.0, 230.0));

    assert_eq!(
        *added.borrow(),
        ["lane:1:1:0"],
        "the + on split → b1 adds first in path B"
    );
}

#[test]
fn dragging_a_block_onto_the_other_lanes_anchor_fires_node_dropped() {
    let (nodes, edges, anchors) = split_mix_chain();
    let w = canvas_for(&nodes, &edges, &anchors);
    let dropped = recorder::<(String, String)>();
    let d = dropped.clone();
    w.on_node_dropped(move |id, anchor| d.borrow_mut().push((id.to_string(), anchor.to_string())));

    drag(&w, at(400.0, 140.0), at(320.0, 230.0));

    assert_eq!(
        *dropped.borrow(),
        [("a1".to_string(), "lane:1:1:0".to_string())],
        "path A's block dropped on split → b1 moves first into path B"
    );
}

#[test]
fn dropping_a_block_on_its_own_wire_fires_no_node_dropped() {
    let (nodes, edges, anchors) = split_mix_chain();
    let w = canvas_for(&nodes, &edges, &anchors);
    let dropped = recorder::<(String, String)>();
    let ended = recorder::<String>();
    let d = dropped.clone();
    w.on_node_dropped(move |id, anchor| d.borrow_mut().push((id.to_string(), anchor.to_string())));
    let e = ended.clone();
    w.on_node_drag_ended(move |id, _, _| e.borrow_mut().push(id.to_string()));

    drag(&w, at(400.0, 140.0), at(480.0, 170.0));

    assert!(
        dropped.borrow().is_empty(),
        "a1 → mixer is a1's own wire: no move, got {:?}",
        dropped.borrow()
    );
    assert_eq!(*ended.borrow(), ["a1"], "the drag itself still ends");
}
