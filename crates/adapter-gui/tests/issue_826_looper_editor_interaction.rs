//! #826 — HEADLESS proof that the waveform editor is actually usable.
//!
//! A render only proves layout (#749/#761): it says nothing about whether a
//! button can be clicked or a handle dragged. This dispatches REAL pointer
//! events at the editor's controls and asserts what each one asks the host for.

use adapter_gui::{LoopEditKind, LooperEditorHarness};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};
use std::cell::RefCell;
use std::rc::Rc;

fn element(w: &impl ComponentHandle, id: &str, nth: usize) -> Option<(LogicalPosition, f32)> {
    let el = i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).nth(nth)?;
    let pos = el.absolute_position();
    let sz = el.size();
    Some((
        LogicalPosition::new(pos.x + sz.width / 2.0, pos.y + sz.height / 2.0),
        sz.width,
    ))
}

fn click(w: &impl ComponentHandle, id: &str, nth: usize) -> bool {
    let Some((c, _)) = element(w, id, nth) else {
        return false;
    };
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

/// Press a handle and drag it `dx` logical pixels, releasing at the end — a
/// real drag, not a teleport, so the `moved` handler runs the way it does for
/// the user.
fn drag(w: &impl ComponentHandle, id: &str, dx: f32) -> bool {
    let Some((from, _)) = element(w, id, 0) else {
        return false;
    };
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: from });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: from,
        button: PointerEventButton::Left,
    });
    for step in 1..=8 {
        win.dispatch_event(WindowEvent::PointerMoved {
            position: LogicalPosition::new(from.x + dx * step as f32 / 8.0, from.y),
        });
    }
    let to = LogicalPosition::new(from.x + dx, from.y);
    win.dispatch_event(WindowEvent::PointerReleased {
        position: to,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerExited);
    true
}

/// (kind, sel-start, sel-end) of every apply the editor asked for.
type Applied = Rc<RefCell<Vec<(LoopEditKind, f32, f32)>>>;

fn harness() -> (LooperEditorHarness, Applied) {
    i_slint_backend_testing::init_no_event_loop();
    let w = LooperEditorHarness::new().unwrap();
    let applied: Applied = Rc::new(RefCell::new(Vec::new()));
    let a = applied.clone();
    w.on_apply(move |_chain, _uid, kind, from, to| a.borrow_mut().push((kind, from, to)));
    (w, applied)
}

#[test]
fn each_edit_button_asks_the_host_for_its_own_edit() {
    let (w, applied) = harness();
    w.show().unwrap();

    // The buttons are the overlay's own component, in declaration order:
    // trim, crop, cut, undo, redo, close.
    for nth in 0..3 {
        assert!(
            click(&w, "EditorButton::area", nth),
            "edit button {nth} must be hittable — a render proves nothing here"
        );
    }

    let calls = applied.borrow();
    assert_eq!(calls.len(), 3, "three edits were asked for");
    assert_eq!(calls[0].0, LoopEditKind::Trim);
    assert_eq!(calls[1].0, LoopEditKind::Crop);
    assert_eq!(calls[2].0, LoopEditKind::Cut);
}

#[test]
fn dragging_the_start_handle_moves_the_selection_the_host_is_told_about() {
    let (w, applied) = harness();
    w.show().unwrap();

    let before = w.global::<adapter_gui::LooperEditor>().get_sel_start();
    assert!(drag(&w, "WaveformView::start-handle", 120.0), "the handle must be draggable");
    let after = w.global::<adapter_gui::LooperEditor>().get_sel_start();

    assert!(
        after > before,
        "dragging the start handle right must move the selection start ({before} → {after})"
    );

    // And the edit the host is handed carries the DRAGGED bounds, not the
    // ones the editor opened with.
    assert!(click(&w, "EditorButton::area", 0), "trim must be hittable");
    let calls = applied.borrow();
    assert_eq!(calls.len(), 1);
    assert!(
        (calls[0].1 - after).abs() < 1e-4,
        "the trim carries the dragged selection start"
    );
}

#[test]
fn the_undo_button_asks_the_host_to_step_the_history() {
    let (w, _applied) = harness();
    let undone: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
    let u = undone.clone();
    w.on_undo(move |_chain, uid| u.borrow_mut().push(uid));
    w.show().unwrap();

    // The harness seeds `can-undo`, so the button is live; index 3 is undo.
    assert!(click(&w, "EditorButton::area", 3), "undo must be hittable");
    assert_eq!(*undone.borrow(), vec![1], "undo names the loop it steps");
}

#[test]
fn a_disabled_button_asks_for_nothing() {
    // The harness leaves `can-redo` false: a dead button must stay dead, or
    // the editor would dispatch a redo the store has to refuse.
    let (w, _applied) = harness();
    let redone: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
    let r = redone.clone();
    w.on_redo(move |_chain, uid| r.borrow_mut().push(uid));
    w.show().unwrap();

    click(&w, "EditorButton::area", 4);
    assert!(
        redone.borrow().is_empty(),
        "a disabled redo must not reach the host"
    );
}
