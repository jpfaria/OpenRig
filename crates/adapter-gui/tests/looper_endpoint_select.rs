//! HEADLESS proof that a looper's record-from and play-to endpoints are picked
//! from a select (a field that opens a list), never from a horizontal row of
//! buttons: a chain can have many endpoints with long names.
//!
//! Real pointer events: press the field, click an option in the modal, assert
//! the callback reports (chain, uid, option-index) and the modal closes.

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

fn labels(names: &[&str]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        names
            .iter()
            .map(|n| SharedString::from(*n))
            .collect::<Vec<_>>(),
    ))
}

fn harness() -> LooperOverlayHarness {
    let w = LooperOverlayHarness::new().unwrap();
    w.set_loopers(ModelRc::new(VecModel::from(vec![LooperItem {
        uid: 7,
        state_code: 2,
        progress: 0.25,
        time_label: "0:02 / 0:08".into(),
        layers: 1,
        mix: 100,
        decay: 100,
        speed_index: 1,
        reverse: false,
        can_undo: true,
        can_redo: true,
        can_record: true,
        can_edit: false,
        input_index: 0,
        output_index: 0,
        preset_index: 0,
    }])));
    w.set_input_options(labels(&["In 1", "In 2", "In 3"]));
    w.set_output_options(labels(&["Out 1/2", "Out 3/4"]));
    // The harness's `init` opens the panel on chain 0 with uid 7's drawer open.
    w
}

type Picks = Rc<RefCell<Vec<(i32, i32, i32)>>>;

fn recorder() -> Picks {
    Rc::new(RefCell::new(Vec::new()))
}

#[test]
fn the_endpoints_are_not_a_horizontal_row_of_buttons() {
    i_slint_backend_testing::init_no_event_loop();
    let w = harness();
    w.show().unwrap();
    // Only the playback speed (half / normal / double) is a segmented row; the
    // three inputs and two outputs are not laid out as buttons.
    assert_eq!(count_id(&w, "LooperSegmented::opt-ta"), 3);
    assert_eq!(count_id(&w, "LooperSelectField::field-ta"), 2);
}

#[test]
fn the_record_from_field_opens_a_list_and_picking_reports_the_input() {
    i_slint_backend_testing::init_no_event_loop();
    let w = harness();
    let (inputs, outputs) = (recorder(), recorder());
    let i = inputs.clone();
    w.on_input_picked(move |ci, uid, v| i.borrow_mut().push((ci, uid, v)));
    let o = outputs.clone();
    w.on_output_picked(move |ci, uid, v| o.borrow_mut().push((ci, uid, v)));
    w.show().unwrap();

    assert_eq!(
        count_id(&w, "LooperEndpointPicker::opt-ta"),
        0,
        "closed by default"
    );
    assert!(
        click_id(&w, "LooperSelectField::field-ta", 0),
        "record-from field"
    );
    assert_eq!(
        count_id(&w, "LooperEndpointPicker::opt-ta"),
        3,
        "lists the inputs"
    );

    assert!(click_id(&w, "LooperEndpointPicker::opt-ta", 2));
    assert_eq!(*inputs.borrow(), vec![(0, 7, 2)]);
    assert!(outputs.borrow().is_empty());
    assert_eq!(
        count_id(&w, "LooperEndpointPicker::opt-ta"),
        0,
        "closes on pick"
    );
}

#[test]
fn the_play_to_field_opens_a_list_and_picking_reports_the_output() {
    i_slint_backend_testing::init_no_event_loop();
    let w = harness();
    let (inputs, outputs) = (recorder(), recorder());
    let i = inputs.clone();
    w.on_input_picked(move |ci, uid, v| i.borrow_mut().push((ci, uid, v)));
    let o = outputs.clone();
    w.on_output_picked(move |ci, uid, v| o.borrow_mut().push((ci, uid, v)));
    w.show().unwrap();

    assert!(
        click_id(&w, "LooperSelectField::field-ta", 1),
        "play-to field"
    );
    assert_eq!(
        count_id(&w, "LooperEndpointPicker::opt-ta"),
        2,
        "lists the outputs"
    );

    assert!(click_id(&w, "LooperEndpointPicker::opt-ta", 1));
    assert_eq!(*outputs.borrow(), vec![(0, 7, 1)]);
    assert!(inputs.borrow().is_empty());
}
