//! #323 phase 2 — HEADLESS proof that the looper preset picker is reachable.
//!
//! A render only proves layout. This dispatches REAL pointer events: open the
//! drawer's preset button, then click an option in the modal, and assert the
//! `preset-picked` callback fires with the right (chain, uid, option-index) —
//! the #749/#761 lesson that a root overlay's content must be a live in-tree
//! TouchArea, never a PopupWindow.

use adapter_gui::{LooperItem, LooperOverlayHarness};
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

fn item(uid: i32, preset_index: i32) -> LooperItem {
    LooperItem {
        uid,
        state_code: 2, // playing — has material, drawer controls live
        progress: 0.25,
        time_label: "0:02 / 0:08".into(),
        layers: 2,
        mix: 100,
        decay: 100,
        speed_index: 1,
        reverse: false,
        can_undo: true,
        can_redo: true,
        can_record: true,
        input_index: 0,
        output_index: 0,
        preset_index,
    }
}

fn harness(preset_index: i32) -> LooperOverlayHarness {
    let w = LooperOverlayHarness::new().unwrap();
    w.set_loopers(ModelRc::new(VecModel::from(vec![item(7, preset_index)])));
    w.set_preset_options(ModelRc::new(VecModel::from(vec![
        SharedString::from("Clean"),
        SharedString::from("Lead Drive"),
    ])));
    // The harness's `init` opens the panel on chain 0 with the row's drawer
    // expanded (uid 7), so the preset button is on screen.
    w
}

#[test]
fn the_preset_button_opens_the_modal_and_picking_an_option_fires_the_callback() {
    i_slint_backend_testing::init_no_event_loop();
    let w = harness(0);
    let picked: Rc<RefCell<Vec<(i32, i32, i32)>>> = Rc::new(RefCell::new(Vec::new()));
    let p = picked.clone();
    w.on_preset_picked(move |ci, uid, idx| p.borrow_mut().push((ci, uid, idx)));
    w.show().unwrap();

    // The modal is closed until the preset button is pressed.
    assert_eq!(
        count_id(&w, "LooperOverlay::opt-ta"),
        0,
        "the picker modal is closed by default"
    );

    // Press the drawer's preset button → the modal opens.
    assert!(
        click_id(&w, "LooperRow::preset-ta", 0),
        "the preset button must be hittable"
    );
    assert_eq!(
        count_id(&w, "LooperOverlay::opt-ta"),
        2,
        "the modal lists the chain's two bank presets"
    );

    // Pick the SECOND bank preset (Lead Drive) → option index 2 (0 = follow).
    assert!(click_id(&w, "LooperOverlay::opt-ta", 1));
    assert_eq!(
        *picked.borrow(),
        vec![(0, 7, 2)],
        "picking bank slot 2 reports (chain 0, uid 7, option 2)"
    );
}

#[test]
fn the_follow_option_clears_the_link_with_option_zero() {
    i_slint_backend_testing::init_no_event_loop();
    // A loop currently linked to bank slot 1 (option index 1).
    let w = harness(1);
    let picked: Rc<RefCell<Vec<(i32, i32, i32)>>> = Rc::new(RefCell::new(Vec::new()));
    let p = picked.clone();
    w.on_preset_picked(move |ci, uid, idx| p.borrow_mut().push((ci, uid, idx)));
    w.show().unwrap();

    assert!(click_id(&w, "LooperRow::preset-ta", 0), "open the modal");
    // The "follow the chain" row clears the fixed link → option index 0.
    assert!(click_id(&w, "LooperOverlay::follow-ta", 0));
    assert_eq!(*picked.borrow(), vec![(0, 7, 0)]);
}
