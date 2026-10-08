//! #1100 — any text the UI cuts short ("Master…", "ValhallaSuperm…") shows its
//! full content when the pointer rests on it; a text that fits shows nothing,
//! and the label never takes the click away from the control under it.

use adapter_gui::{BlockParameterItem, ElidedTextHarness, SelectOption};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, VecModel};

const LONG: &str = "Valhalla Supermassive Long Preset Name";

fn harness(field_label: &str, param_label: &str) -> ElidedTextHarness {
    i_slint_backend_testing::init_no_event_loop();
    let w = ElidedTextHarness::new().unwrap();
    w.set_field_label(field_label.into());
    w.set_options(ModelRc::new(VecModel::from(vec![SelectOption {
        key: "k".into(),
        label: LONG.into(),
    }])));
    w.set_param(BlockParameterItem {
        label: param_label.into(),
        widget_kind: "bool".into(),
        tab_slot: -1,
        strip_line: -1,
        ..Default::default()
    });
    w.show().unwrap();
    w
}

fn hover(w: &ElidedTextHarness, x: f32, y: f32) {
    w.window().dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(x, y),
    });
    slint::platform::update_timers_and_animations();
}

fn click(w: &ElidedTextHarness, x: f32, y: f32) {
    let position = LogicalPosition::new(x, y);
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position });
    win.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
    slint::platform::update_timers_and_animations();
}

#[test]
fn hovering_a_cut_select_value_shows_the_whole_value() {
    let w = harness(LONG, "Gain");
    hover(&w, 50.0, 29.0);
    assert!(w.get_tip_shown(), "the cut value shows its hover label");
    assert_eq!(w.get_tip_text(), LONG);
}

#[test]
fn hovering_a_value_that_fits_shows_nothing() {
    let w = harness("Clean", "Gain");
    hover(&w, 50.0, 29.0);
    assert!(!w.get_tip_shown(), "a value that fits has no hover label");
}

#[test]
fn a_cut_select_value_still_opens_the_select() {
    let w = harness(LONG, "Gain");
    click(&w, 50.0, 29.0);
    assert_eq!(w.get_opened_count(), 1, "the click reaches the select");
}

#[test]
fn hovering_a_cut_row_in_the_open_list_shows_the_whole_row() {
    let w = harness("Clean", "Gain");
    click(&w, 50.0, 29.0);
    // Field 10..48, popup at 52, rows start 54 px into it (34 px each).
    hover(&w, 50.0, 52.0 + 54.0 + 17.0);
    assert!(w.get_tip_shown(), "the cut row shows its hover label");
    assert_eq!(w.get_tip_text(), LONG);
}

#[test]
fn hovering_a_cut_parameter_name_shows_the_whole_name() {
    let w = harness("Clean", "Master sum");
    hover(&w, 290.0, 19.0);
    assert!(
        w.get_tip_shown(),
        "the cut parameter name shows its hover label"
    );
    assert_eq!(w.get_tip_text(), "Master sum");
}
