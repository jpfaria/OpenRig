//! HEADLESS proof of deleting a saved take from the DI panel.
//!
//! Only a saved take row carries the trash; a bundled loop and the "Choose
//! file…" entry never do. The trash is a two-step control — the first click
//! only arms it ("Delete?"), the second deletes — so a stray click on a crowded
//! list never loses a take, and neither click picks the row as the source.

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

type Calls = Rc<RefCell<Vec<(i32, String)>>>;

/// An expanded DI panel listing a bundled loop, two saved takes and the
/// sentinel. Returns the window plus what `delete-take` and `source-picked`
/// were called with.
fn expanded_panel() -> (DiLoopHarness, Calls, Calls) {
    i_slint_backend_testing::init_no_event_loop();
    let w = DiLoopHarness::new().unwrap();
    w.set_sources(ModelRc::new(VecModel::from(vec![
        SharedString::from("clean-electric-guitar-loop"),
        SharedString::from("riff.wav"),
        SharedString::from("verse.wav"),
        SharedString::from("Choose file…"),
    ])));
    w.set_take_rows(ModelRc::new(VecModel::from(vec![false, true, true, false])));
    w.set_selected_index(-1);

    let deleted: Calls = Rc::new(RefCell::new(Vec::new()));
    let d = deleted.clone();
    w.on_delete_take(move |i, s| d.borrow_mut().push((i, s.to_string())));
    let picked: Calls = Rc::new(RefCell::new(Vec::new()));
    let p = picked.clone();
    w.on_source_picked(move |i, s| p.borrow_mut().push((i, s.to_string())));

    w.show().unwrap();
    assert!(click_id(&w, "DiLoopPanel::sel-ta", 0), "select hittable");
    assert_eq!(count_id(&w, "DiLoopPanel::row-ta"), 4, "options expanded");
    (w, deleted, picked)
}

#[test]
fn only_saved_take_rows_carry_a_trash() {
    let (w, _, _) = expanded_panel();
    assert_eq!(
        count_id(&w, "DiLoopPanel::del-ta"),
        2,
        "a bundled loop and the Choose-file entry are never deletable"
    );
}

#[test]
fn the_first_click_only_arms_the_delete() {
    let (w, deleted, picked) = expanded_panel();
    assert_eq!(count_id(&w, "DiLoopPanel::del-confirm"), 0);

    assert!(click_id(&w, "DiLoopPanel::del-ta", 0));

    assert!(deleted.borrow().is_empty(), "one click never deletes");
    assert!(picked.borrow().is_empty(), "the trash never picks the row");
    assert_eq!(
        count_id(&w, "DiLoopPanel::del-confirm"),
        1,
        "the armed row asks to confirm"
    );
    assert_eq!(
        count_id(&w, "DiLoopPanel::row-ta"),
        4,
        "the list stays open to confirm"
    );
}

#[test]
fn the_second_click_deletes_that_take() {
    let (w, deleted, picked) = expanded_panel();

    assert!(click_id(&w, "DiLoopPanel::del-ta", 1));
    assert!(click_id(&w, "DiLoopPanel::del-ta", 1));

    assert_eq!(*deleted.borrow(), vec![(2, "verse.wav".to_string())]);
    assert!(picked.borrow().is_empty());
}

#[test]
fn arming_another_row_disarms_the_first() {
    let (w, deleted, _) = expanded_panel();

    assert!(click_id(&w, "DiLoopPanel::del-ta", 0));
    assert!(click_id(&w, "DiLoopPanel::del-ta", 1));

    assert!(deleted.borrow().is_empty(), "two different rows, no delete");
    assert_eq!(count_id(&w, "DiLoopPanel::del-confirm"), 1);
}

#[test]
fn closing_the_list_disarms_the_delete() {
    let (w, deleted, _) = expanded_panel();

    assert!(click_id(&w, "DiLoopPanel::del-ta", 0));
    assert!(click_id(&w, "DiLoopPanel::sel-ta", 0), "collapse");
    assert!(click_id(&w, "DiLoopPanel::sel-ta", 0), "expand again");

    assert_eq!(count_id(&w, "DiLoopPanel::del-confirm"), 0);
    assert!(click_id(&w, "DiLoopPanel::del-ta", 0));
    assert!(
        deleted.borrow().is_empty(),
        "an arm from before the list closed never carries over"
    );
}
