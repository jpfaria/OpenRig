//! #328 — real pointer events on a chain graph row (i-slint-backend-testing).
//! A render proves layout only; these prove the clicks land (#749/#761).
//! Node centres: the harness puts ChainRowGraph at (18, 20) at zoom 1, so a
//! node's window position is (18 + layout_x, 20 + layout_y).

use std::cell::RefCell;
use std::rc::Rc;

use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition};

use crate::{ChainGraphBridge, ChainRowGraphSplitMixHarness};

pub(crate) fn at(layout_x: f32, layout_y: f32) -> LogicalPosition {
    LogicalPosition::new(18.0 + layout_x, 20.0 + layout_y)
}

pub(crate) fn click_at(w: &impl ComponentHandle, p: LogicalPosition) {
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

#[test]
fn clicking_a_lane_card_reports_its_row_and_node() {
    i_slint_backend_testing::init_no_event_loop();
    let h = ChainRowGraphSplitMixHarness::new().unwrap();
    let clicked: Rc<RefCell<Option<(i32, String)>>> = Rc::new(RefCell::new(None));
    let seen = clicked.clone();
    ChainGraphBridge::get(&h)
        .on_node_clicked(move |ci, id| *seen.borrow_mut() = Some((ci, id.to_string())));
    h.show().unwrap();

    click_at(&h, at(446.0, 50.0)); // a1, lane A

    assert_eq!(*clicked.borrow(), Some((0, "a1".to_string())));
}

/// The row hands each node's strip tile to Part 5's canvas, which draws the
/// #333 tooltip for a block with a model and never for a routing node.
#[test]
fn hovering_a_block_card_shows_its_tooltip_and_a_routing_node_does_not() {
    i_slint_backend_testing::init_no_event_loop();
    let h = ChainRowGraphSplitMixHarness::new().unwrap();
    h.show().unwrap();
    let tooltips = || {
        i_slint_backend_testing::ElementHandle::find_by_element_type_name(&h, "BlockHoverTooltip")
            .count()
    };

    h.window().dispatch_event(WindowEvent::PointerMoved {
        position: at(446.0, 50.0),
    }); // a1
    assert_eq!(tooltips(), 1, "a block card shows the #333 tooltip");

    h.window().dispatch_event(WindowEvent::PointerMoved {
        position: at(314.0, 104.0),
    }); // split node
    assert_eq!(tooltips(), 0, "the split node has no model to describe");
}

use std::cell::Cell;

use crate::chain_graph_fixtures_tests::{chain, core, split};
use crate::graph_anchor::{move_target, parse_anchor};
use project::block::SplitEnd;

#[test]
fn dragging_a_lane_a_card_onto_lane_b_drops_it_into_path_b() {
    i_slint_backend_testing::init_no_event_loop();
    let h = ChainRowGraphSplitMixHarness::new().unwrap();
    // The harness row is `od → split(A: a1 | B: b1) → rev`, laid out exactly
    // as `chain_graph_adapter` lays this chain out.
    let row = chain(vec![
        core("od"),
        split("sp", SplitEnd::Mix, vec![core("a1")], vec![core("b1")]),
        core("rev"),
    ]);
    let bridge = ChainGraphBridge::get(&h);
    {
        let row = row.clone();
        bridge.on_resolve_drop_anchor(move |_, id, x, y| {
            crate::chain_graph_drop::drop_anchor_id(&row, &id, x, y).into()
        });
    }
    let dropped: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
    let seen = dropped.clone();
    bridge.on_node_dropped(move |_, id, anchor| {
        *seen.borrow_mut() = Some((id.to_string(), anchor.to_string()));
    });
    h.show().unwrap();
    let win = h.window();
    // a1 → next to the b1 → mixer wire (midpoint 512, 131), lane B.
    let (from, to) = (at(446.0, 50.0), at(512.0, 150.0));
    win.dispatch_event(WindowEvent::PointerMoved { position: from });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: from,
        button: PointerEventButton::Left,
    });
    for step in 1..=10 {
        let t = step as f32 / 10.0;
        let p = LogicalPosition::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        win.dispatch_event(WindowEvent::PointerMoved { position: p });
    }
    win.dispatch_event(WindowEvent::PointerReleased {
        position: to,
        button: PointerEventButton::Left,
    });

    let (id, anchor) = dropped
        .borrow()
        .clone()
        .expect("the drop reached the bridge");
    assert_eq!((id.as_str(), anchor.as_str()), ("a1", "path:sp:1:1"));
    let target = parse_anchor(&anchor)
        .and_then(|slot| move_target(&row, "a1", &slot))
        .unwrap_or_else(|| panic!("anchor {anchor:?} names no place"));
    assert_eq!(target.path.map(|p| p.path), Some(1));
}

#[test]
fn the_confirm_dialog_fires_the_split_removal() {
    i_slint_backend_testing::init_no_event_loop();
    let w = crate::AppWindow::new().unwrap();
    let fired = Rc::new(Cell::new(false));
    let f = fired.clone();
    let overlay = crate::ChainGraphOverlayState::get(&w);
    overlay.on_confirm_remove_split(move || f.set(true));
    overlay.set_confirm_remove_split_name("Split (+ 1 blocks in path B)".into());
    overlay.set_confirm_remove_split_open(true);
    w.show().unwrap();
    let button = i_slint_backend_testing::ElementHandle::find_by_element_id(
        &w,
        "ConfirmDeleteBlockDialog::confirm-area",
    )
    .next()
    .expect("the confirmation is up");
    let (pos, size) = (button.absolute_position(), button.size());
    click_at(
        &w,
        LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0),
    );
    assert!(fired.get(), "confirm reached Rust");
    assert!(
        !overlay.get_confirm_remove_split_open(),
        "the dialog closed"
    );
}

/// An app whose graph gestures and split editor are wired to a session on
/// the real dispatcher.
fn wired_graph_app(
    chain: project::chain::Chain,
) -> (
    crate::AppWindow,
    Rc<RefCell<Option<crate::state::ProjectSession>>>,
) {
    use crate::chain_graph_fixtures_tests::{rows, session_with};
    let app = crate::AppWindow::new().unwrap();
    let session = session_with(vec![chain]);
    let (project_chains, toast_timer) = (rows(), Rc::new(slint::Timer::default()));
    crate::chain_graph_wiring::wire(
        &app,
        crate::chain_graph_wiring::ChainGraphWiringCtx {
            project_session: session.clone(),
            project_chains: project_chains.clone(),
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(Vec::new())),
            toast_timer: toast_timer.clone(),
        },
    );
    crate::split_editor_wiring::wire(
        &app,
        crate::split_editor_wiring::SplitEditorWiringCtx {
            project_session: session.clone(),
            project_chains,
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(Vec::new())),
            toast_timer,
        },
    );
    (app, session)
}

fn top_ids(session: &Rc<RefCell<Option<crate::state::ProjectSession>>>) -> Vec<String> {
    crate::chain_graph_fixtures_tests::chain_in(session, 0)
        .blocks
        .iter()
        .map(|b| b.id.0.clone())
        .collect()
}

#[test]
fn mix_then_y_clicking_the_y_node_opens_the_editor_on_the_y() {
    i_slint_backend_testing::init_no_event_loop();
    let (app, _session) = wired_graph_app(crate::chain_graph_fixtures_tests::mix_then_y_chain());
    ChainGraphBridge::get(&app).invoke_node_clicked(0, "__split_y".into());
    let state = crate::ChainGraphOverlayState::get(&app);
    assert!(state.get_split_editor_open(), "the split editor opened");
    assert_eq!(state.get_split_editor_split_id().as_str(), "y");
    assert!(state.get_split_editor_end_y(), "the Y's end is lit");
}

#[test]
fn mix_then_y_confirming_the_y_removal_removes_the_y_and_keeps_the_mix() {
    i_slint_backend_testing::init_no_event_loop();
    let (app, session) = wired_graph_app(crate::chain_graph_fixtures_tests::mix_then_y_chain());
    ChainGraphBridge::get(&app).invoke_remove_requested(0, "__split_y".into());
    app.show().unwrap();
    let button = i_slint_backend_testing::ElementHandle::find_by_element_id(
        &app,
        "ConfirmDeleteBlockDialog::confirm-area",
    )
    .next()
    .expect("removing the Y asks first: its path B holds yb");
    let (pos, size) = (button.absolute_position(), button.size());
    click_at(
        &app,
        LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0),
    );
    assert_eq!(top_ids(&session), vec!["pre", "mx", "mid", "ya"]);
}
