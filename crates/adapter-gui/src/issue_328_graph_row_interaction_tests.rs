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
