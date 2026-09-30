//! #328 — headless proof of the GraphView pointer contract, the canvas the
//! chain editor is built on. REAL pointer / wheel / key events dispatched at
//! real geometry through i-slint-backend-testing: a render PNG proves layout
//! only, this proves the gesture lands (#749/#761).
//!
//! The harness canvas sits at the window origin with zoom 1 and no pan, so a
//! node's layout coordinates ARE its window coordinates.

use adapter_gui::{GraphNode, GraphViewHarness, GraphViewScrollHarness};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, VecModel};
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
