//! #827 — HEADLESS proof that a take can be saved from the waveform editor.
//!
//! A render proves layout only (#749/#761). This types a name into the REAL
//! field and presses the REAL button with pointer events, and asserts what the
//! editor asks the host for.

use adapter_gui::{LooperEditor, LooperEditorHarness};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};
use std::cell::RefCell;
use std::rc::Rc;

fn click(w: &impl ComponentHandle, id: &str, nth: usize) -> bool {
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

fn type_name(w: &impl ComponentHandle, text: &str) {
    let field = i_slint_backend_testing::ElementHandle::find_by_element_id(
        w,
        "LooperTakeSaveRow::name-edit",
    )
    .next()
    .expect("the take-name field must exist");
    field.set_accessible_value(text);
}

/// (chain-index, uid, name) of every save the editor asked for.
type Saved = Rc<RefCell<Vec<(i32, i32, String)>>>;

fn harness() -> (LooperEditorHarness, Saved) {
    i_slint_backend_testing::init_no_event_loop();
    let w = LooperEditorHarness::new().unwrap();
    let saved: Saved = Rc::new(RefCell::new(Vec::new()));
    let s = saved.clone();
    w.global::<LooperEditor>()
        .on_save_take(move |chain, uid, name| s.borrow_mut().push((chain, uid, name.into())));
    (w, saved)
}

/// Declaration order in the editor: fit, trim, crop, cut, play, undo, redo,
/// close — then the save row's button.
const SAVE_BUTTON: usize = 8;

#[test]
fn typing_a_name_and_pressing_save_asks_the_host_to_save_that_take() {
    let (w, saved) = harness();
    w.show().unwrap();

    type_name(&w, "verse riff");
    assert!(
        click(&w, "EditorButton::area", SAVE_BUTTON),
        "the save button must be hittable — a render proves nothing here"
    );

    assert_eq!(
        *saved.borrow(),
        vec![(0, 1, "verse riff".to_string())],
        "save names the loop and carries the typed name"
    );
}

#[test]
fn save_is_dead_until_the_take_has_a_name() {
    let (w, saved) = harness();
    w.show().unwrap();

    // Blank. A name of only spaces does reach the host, which refuses it with
    // its own message (`TAKE_NEEDS_NAME`) — Slint has no `trim`.
    type_name(&w, "");
    click(&w, "EditorButton::area", SAVE_BUTTON);

    assert!(
        saved.borrow().is_empty(),
        "a nameless save would only come back refused"
    );
}

#[test]
fn the_existing_editor_buttons_keep_their_places() {
    // The save row is added AFTER the edit buttons, so the #826 interaction
    // test's button indices still point at the same controls.
    let (w, _saved) = harness();
    let undone: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
    let u = undone.clone();
    w.on_undo(move |_chain, uid| u.borrow_mut().push(uid));
    w.show().unwrap();

    assert!(click(&w, "EditorButton::area", 5));
    assert_eq!(*undone.borrow(), vec![1]);
}
