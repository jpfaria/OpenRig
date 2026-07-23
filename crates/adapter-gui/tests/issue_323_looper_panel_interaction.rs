//! #323 — HEADLESS proof that the looper panel is actually clickable.
//!
//! A render only proves layout. This dispatches REAL pointer events at the
//! transport buttons and asserts each callback fires, the way #749/#761 had to
//! prove the DI panel (a PopupWindow's content is unreachable — the panel is
//! inline precisely so these clicks land).

use adapter_gui::LooperHarness;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adapter_gui::LooperItem;

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

fn item(uid: i32, state_code: i32) -> LooperItem {
    LooperItem {
        uid,
        state_code,
        progress: 0.25,
        time_label: "0:02 / 0:08".into(),
        layers: 2,
        mix: 100,
        decay: 100,
        speed_index: 1,
        reverse: false,
        can_undo: true,
        can_redo: true,
    }
}

fn harness(items: Vec<LooperItem>) -> LooperHarness {
    let w = LooperHarness::new().unwrap();
    w.set_loopers(ModelRc::new(VecModel::from(items)));
    w
}

#[test]
fn an_empty_panel_offers_the_add_button_and_no_rows() {
    i_slint_backend_testing::init_no_event_loop();
    let w = harness(vec![]);
    let added = Rc::new(Cell::new(false));
    let a = added.clone();
    w.on_add(move || a.set(true));
    w.show().unwrap();

    assert_eq!(
        count_id(&w, "LooperRow::rec-btn"),
        0,
        "an empty chain shows no transport rows"
    );
    assert!(
        click_id(&w, "LooperPanelView::add-ta", 0),
        "the add button must be hittable"
    );
    assert!(added.get(), "clicking add must fire the callback");
}

#[test]
fn every_transport_button_fires_for_its_row() {
    i_slint_backend_testing::init_no_event_loop();
    let w = harness(vec![item(7, 2)]);

    let fired: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    macro_rules! record_call {
        ($setter:ident, $name:literal) => {{
            let f = fired.clone();
            w.$setter(move |uid| f.borrow_mut().push(format!("{}:{}", $name, uid)));
        }};
    }
    record_call!(on_record, "record");
    record_call!(on_play_stop, "play-stop");
    record_call!(on_undo, "undo");
    record_call!(on_redo, "redo");
    record_call!(on_clear, "clear");
    record_call!(on_remove, "remove");
    record_call!(on_toggle_drawer, "drawer");
    w.show().unwrap();

    for id in [
        "LooperRow::rec-btn",
        "LooperRow::play-btn",
        "LooperRow::undo-btn",
        "LooperRow::redo-btn",
        "LooperRow::clear-btn",
        "LooperRow::gear-btn",
        "LooperRow::remove-btn",
    ] {
        assert!(click_id(&w, id, 0), "{id} must be hittable");
    }

    assert_eq!(
        *fired.borrow(),
        vec![
            "record:7",
            "play-stop:7",
            "undo:7",
            "redo:7",
            "clear:7",
            "drawer:7",
            "remove:7",
        ]
    );
}

#[test]
fn a_disabled_button_does_not_fire() {
    i_slint_backend_testing::init_no_event_loop();
    // An empty looper: nothing to play, clear, undo or redo yet.
    let mut empty = item(3, 0);
    empty.can_undo = false;
    empty.can_redo = false;
    let w = harness(vec![empty]);

    let fired = Rc::new(Cell::new(0));
    for setter in 0..1 {
        let _ = setter;
    }
    let f = fired.clone();
    w.on_play_stop(move |_| f.set(f.get() + 1));
    let f2 = fired.clone();
    w.on_clear(move |_| f2.set(f2.get() + 1));
    let f3 = fired.clone();
    w.on_undo(move |_| f3.set(f3.get() + 1));
    w.show().unwrap();

    click_id(&w, "LooperRow::play-btn", 0);
    click_id(&w, "LooperRow::clear-btn", 0);
    click_id(&w, "LooperRow::undo-btn", 0);

    assert_eq!(
        fired.get(),
        0,
        "play / clear / undo are disabled on an empty looper and must not fire"
    );
}

#[test]
fn each_row_reports_its_own_uid() {
    i_slint_backend_testing::init_no_event_loop();
    let w = harness(vec![item(1, 2), item(2, 0), item(3, 4)]);

    let seen: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    w.on_record(move |uid| s.borrow_mut().push(uid));
    w.show().unwrap();

    for nth in 0..3 {
        assert!(click_id(&w, "LooperRow::rec-btn", nth));
    }
    assert_eq!(*seen.borrow(), vec![1, 2, 3]);
}

#[test]
fn the_drawer_controls_only_exist_when_the_row_is_expanded() {
    i_slint_backend_testing::init_no_event_loop();
    let w = harness(vec![item(5, 2)]);
    w.show().unwrap();
    assert_eq!(
        count_id(&w, "LooperSegmented::opt-ta"),
        0,
        "the parameter drawer is closed by default"
    );

    let w2 = harness(vec![item(5, 2)]);
    w2.set_expanded_uid(5);
    let picked = Rc::new(Cell::new(-1));
    let p = picked.clone();
    w2.on_speed_picked(move |_uid, index| p.set(index));
    w2.show().unwrap();

    assert_eq!(
        count_id(&w2, "LooperSegmented::opt-ta"),
        3,
        "half / normal / double are offered when the drawer is open"
    );
    assert!(click_id(&w2, "LooperSegmented::opt-ta", 2));
    assert_eq!(picked.get(), 2, "picking 2x must report index 2");
}
