//! #771 — HEADLESS proof of the DI panel's OUTPUT select (owner spec): a
//! second select under the source select listing the chain's already-bound
//! output endpoints; clicking it expands the output options INLINE; picking
//! one fires `output-picked(index, label)` and collapses.
//!
//! Same testing approach as #749: the panel is INLINE (not a PopupWindow) so
//! these are REAL pointer events the headless backend can dispatch.

use adapter_gui::DiLoopHarness;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
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
fn output_select_expands_pick_fires_and_collapses() {
    i_slint_backend_testing::init_no_event_loop();

    let w = DiLoopHarness::new().unwrap();
    w.set_sources(ModelRc::new(VecModel::from(vec![
        SharedString::from("clean-electric-guitar-loop"),
        SharedString::from("Choose file…"),
    ])));
    w.set_selected_index(0);
    w.set_outputs(ModelRc::new(VecModel::from(vec![
        SharedString::from("Main Out"),
        SharedString::from("FX Out"),
    ])));
    w.set_output_selected_index(0);
    w.set_playing(false);

    let picked: Rc<RefCell<Option<(i32, String)>>> = Rc::new(RefCell::new(None));
    let p = picked.clone();
    w.on_output_picked(move |i, s| *p.borrow_mut() = Some((i, s.to_string())));

    w.show().unwrap();

    // 1. The output select field is present; its options are collapsed.
    assert!(
        count_id(&w, "DiLoopPanel::out-sel-ta") >= 1,
        "the output select field must be present in the panel"
    );
    assert_eq!(
        count_id(&w, "DiLoopPanel::out-row-ta"),
        0,
        "the output options must be collapsed until the select is clicked"
    );

    // 2. Click the output select → its options expand.
    assert!(
        click_id(&w, "DiLoopPanel::out-sel-ta", 0),
        "output select field must be hittable"
    );
    assert_eq!(
        count_id(&w, "DiLoopPanel::out-row-ta"),
        2,
        "clicking the output select must expand the chain's bound outputs"
    );

    // 3. Pick the 2nd output → fires with its index + label AND collapses.
    assert!(
        click_id(&w, "DiLoopPanel::out-row-ta", 1),
        "an output row must be hittable"
    );
    assert_eq!(
        picked.borrow().clone(),
        Some((1, "FX Out".to_string())),
        "picking an output must fire output-picked with its index + label"
    );
    assert_eq!(
        count_id(&w, "DiLoopPanel::out-row-ta"),
        0,
        "picking an output must collapse the options"
    );
}

#[test]
fn output_select_hidden_when_chain_has_no_bound_outputs() {
    i_slint_backend_testing::init_no_event_loop();

    let w = DiLoopHarness::new().unwrap();
    w.set_sources(ModelRc::new(VecModel::from(vec![SharedString::from(
        "clean-electric-guitar-loop",
    )])));
    w.set_outputs(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
    w.set_output_selected_index(-1);
    w.show().unwrap();

    assert_eq!(
        count_id(&w, "DiLoopPanel::out-sel-ta"),
        0,
        "no bound outputs → no output select"
    );
}
