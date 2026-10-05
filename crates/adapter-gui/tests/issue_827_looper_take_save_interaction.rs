//! #827 — HEADLESS proof that a take is saved from the looper's own row.
//!
//! Each looper row carries a floppy; it opens a name dialog, and Save there
//! asks the host to keep THAT loop as a named take. A render proves layout
//! only (#749/#761): this clicks the REAL floppy, types into the REAL field
//! and presses the REAL button with pointer events, and asserts what the
//! panel asks the host for.

use adapter_gui::{LooperEditorHarness, LooperItem, LooperOverlayHarness, LooperTake};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

const FLOPPY: &str = "LooperRow::save-btn";
const NAME_FIELD: &str = "LooperTakeSaveDialog::name-edit";
const SAVE: &str = "LooperTakeSaveDialog::save-btn";
const CANCEL: &str = "LooperTakeSaveDialog::cancel-btn";

// state_code: 0 empty · 1 recording · 2 playing · 3 overdubbing · 4 stopped
const EMPTY: i32 = 0;
const RECORDING: i32 = 1;
const PLAYING: i32 = 2;
const STOPPED: i32 = 4;

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

fn count(w: &impl ComponentHandle, id: &str) -> usize {
    i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).count()
}

fn type_name(w: &impl ComponentHandle, text: &str) {
    let field = i_slint_backend_testing::ElementHandle::find_by_element_id(w, NAME_FIELD)
        .next()
        .expect("the take-name field must exist once the dialog is open");
    field.set_accessible_value(text);
}

fn item(uid: i32, state_code: i32) -> LooperItem {
    LooperItem {
        uid,
        state_code,
        progress: 0.25,
        time_label: "0:02 / 0:08".into(),
        layers: 1,
        mix: 100,
        decay: 100,
        speed_index: 1,
        reverse: false,
        can_undo: false,
        can_redo: false,
        can_record: true,
        can_edit: state_code == STOPPED,
        input_index: 0,
        output_index: 0,
        preset_index: 0,
    }
}

/// (chain-index, uid, name) of every save the panel asked for.
type Saved = Rc<RefCell<Vec<(i32, i32, String)>>>;

fn harness(items: Vec<LooperItem>) -> (LooperOverlayHarness, Saved) {
    i_slint_backend_testing::init_no_event_loop();
    let w = LooperOverlayHarness::new().unwrap();
    // The harness opens the panel on chain 0.
    w.set_loopers(ModelRc::new(VecModel::from(items)));
    let saved: Saved = Rc::new(RefCell::new(Vec::new()));
    let s = saved.clone();
    w.global::<LooperTake>()
        .on_save(move |chain, uid, name| s.borrow_mut().push((chain, uid, name.into())));
    (w, saved)
}

#[test]
fn every_looper_row_has_a_floppy() {
    let (w, _saved) = harness(vec![item(3, STOPPED), item(7, PLAYING)]);
    w.show().unwrap();

    assert_eq!(count(&w, FLOPPY), 2, "one floppy beside each looper");
    assert_eq!(
        count(&w, NAME_FIELD),
        0,
        "no dialog until a floppy is pressed"
    );
}

#[test]
fn the_floppy_asks_for_a_name_and_save_keeps_that_loop() {
    let (w, saved) = harness(vec![item(3, STOPPED), item(7, STOPPED)]);
    w.show().unwrap();

    assert!(
        click(&w, FLOPPY, 1),
        "the second row's floppy must be hittable"
    );
    type_name(&w, "verse riff");
    assert!(click(&w, SAVE, 0), "the dialog's Save must be hittable");

    assert_eq!(
        *saved.borrow(),
        vec![(0, 7, "verse riff".to_string())],
        "save names the chain and the loop whose floppy was pressed, with the typed name"
    );
}

#[test]
fn save_is_dead_until_the_take_has_a_name() {
    let (w, saved) = harness(vec![item(7, STOPPED)]);
    w.show().unwrap();

    click(&w, FLOPPY, 0);
    // Blank. A name of only spaces does reach the host, which refuses it with
    // its own message (`TAKE_NEEDS_NAME`) — Slint has no `trim`.
    type_name(&w, "");
    click(&w, SAVE, 0);

    assert!(
        saved.borrow().is_empty(),
        "a nameless save would only come back refused"
    );
}

#[test]
fn a_playing_loop_can_be_saved() {
    let (w, saved) = harness(vec![item(7, PLAYING)]);
    w.show().unwrap();

    click(&w, FLOPPY, 0);
    type_name(&w, "chorus");
    click(&w, SAVE, 0);

    assert_eq!(*saved.borrow(), vec![(0, 7, "chorus".to_string())]);
}

#[test]
fn the_floppy_is_dead_on_a_loop_with_nothing_to_keep() {
    // Empty: nothing recorded. Recording: the take has no end yet.
    let (w, _saved) = harness(vec![]);
    w.show().unwrap();
    for state in [EMPTY, RECORDING] {
        w.set_loopers(ModelRc::new(VecModel::from(vec![item(7, state)])));

        click(&w, FLOPPY, 0);

        assert_eq!(
            count(&w, NAME_FIELD),
            0,
            "state {state}: the floppy must not open the dialog"
        );
    }
}

#[test]
fn cancel_closes_the_dialog_without_saving() {
    let (w, saved) = harness(vec![item(7, STOPPED)]);
    w.show().unwrap();

    click(&w, FLOPPY, 0);
    type_name(&w, "verse riff");
    assert!(click(&w, CANCEL, 0), "the dialog's Cancel must be hittable");

    assert_eq!(count(&w, NAME_FIELD), 0, "cancel closes the dialog");
    assert!(saved.borrow().is_empty(), "cancel never saves");
}

#[test]
fn opening_the_dialog_starts_clean() {
    // The outcome and name of the previous save must not sit under a new one.
    let (w, _saved) = harness(vec![item(7, STOPPED)]);
    let take = w.global::<LooperTake>();
    take.set_status(1);
    take.set_name("old".into());
    w.show().unwrap();

    click(&w, FLOPPY, 0);

    let take = w.global::<LooperTake>();
    assert_eq!(take.get_status(), 0);
    assert_eq!(take.get_name(), "");
    assert_eq!(take.get_dialog_uid(), 7);
}

#[test]
fn the_waveform_editor_no_longer_saves_takes() {
    // The save moved to the row: the editor keeps only its own buttons —
    // fit, trim, crop, cut, play, undo, redo, close.
    i_slint_backend_testing::init_no_event_loop();
    let w = LooperEditorHarness::new().unwrap();
    w.show().unwrap();

    assert_eq!(count(&w, "EditorButton::area"), 8);
    assert_eq!(count(&w, NAME_FIELD), 0);
}
