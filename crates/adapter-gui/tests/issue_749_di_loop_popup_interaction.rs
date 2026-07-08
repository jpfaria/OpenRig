//! #749 — HEADLESS proof of the DI loop panel flow (owner spec):
//! fone → a panel with a SELECT + play; click the select → options expand;
//! pick one → it selects AND the options collapse; play/stop.
//!
//! The panel is INLINE (not a PopupWindow) precisely so these clicks are
//! reachable by the slint testing backend (popup content is a separate surface
//! it cannot actuate). So this test dispatches REAL pointer events at the select
//! field, an option row, and the play button, and asserts each fires — no more
//! shipping the selection on a guess.

use adapter_gui::DiLoopHarness;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, SharedString, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn click_id(w: &impl ComponentHandle, id: &str, nth: usize) -> bool {
    let Some(el) = i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).nth(nth)
    else {
        return false;
    };
    let pos = el.absolute_position();
    let sz = el.size();
    let c = LogicalPosition::new(pos.x + sz.width / 2.0, pos.y + sz.height / 2.0);
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: c });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: c,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerReleased {
        position: c,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerExited);
    true
}

fn count_id(w: &impl ComponentHandle, id: &str) -> usize {
    i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).count()
}

#[test]
fn di_panel_full_flow_select_expands_pick_collapses_play_fires() {
    i_slint_backend_testing::init_no_event_loop();

    let w = DiLoopHarness::new().unwrap();
    w.set_sources(ModelRc::new(VecModel::from(vec![
        SharedString::from("clean-electric-guitar-loop"),
        SharedString::from("fabiano-antunes-strato-clean"),
        SharedString::from("Choose file…"),
    ])));
    w.set_selected_index(-1);
    w.set_playing(false);

    let picked: Rc<RefCell<Option<(i32, String)>>> = Rc::new(RefCell::new(None));
    let p = picked.clone();
    w.on_source_picked(move |i, s| *p.borrow_mut() = Some((i, s.to_string())));
    let played = Rc::new(Cell::new(false));
    let pl = played.clone();
    w.on_play(move || pl.set(true));

    w.show().unwrap();

    // 1. The select field is present; the options are NOT shown yet.
    assert!(
        count_id(&w, "DiLoopPanel::sel-ta") >= 1,
        "the select field must be present in the panel"
    );
    assert_eq!(
        count_id(&w, "DiLoopPanel::row-ta"),
        0,
        "the options must be collapsed until the select is clicked"
    );

    // 2. Click the select → options expand.
    assert!(
        click_id(&w, "DiLoopPanel::sel-ta", 0),
        "select field must be hittable"
    );
    assert!(
        count_id(&w, "DiLoopPanel::row-ta") >= 3,
        "clicking the select must expand the options (got {})",
        count_id(&w, "DiLoopPanel::row-ta")
    );

    // 3. Pick the 2nd option → it fires AND the options collapse.
    assert!(
        click_id(&w, "DiLoopPanel::row-ta", 1),
        "an option row must be hittable"
    );
    assert_eq!(
        picked.borrow().clone(),
        Some((1, "fabiano-antunes-strato-clean".to_string())),
        "picking an option must fire source-picked with its index + source"
    );
    assert_eq!(
        count_id(&w, "DiLoopPanel::row-ta"),
        0,
        "picking an option must collapse the options"
    );

    // 4. With a source selected, the play button shows and fires.
    w.set_selected_index(1);
    assert!(
        click_id(&w, "DiLoopPanel::play-ta", 0),
        "play must be hittable once selected"
    );
    assert!(played.get(), "clicking play must fire the play callback");
}
