//! #14 — HEADLESS proof that the metronome window's controls actually respond
//! to a pointer, not just that they render.
//!
//! A PNG render proves layout and nothing else: a control can sit in the right
//! place with a TouchArea that is covered, mis-sized or wired to nothing. So
//! this test instantiates the real `MetronomeWindow`, dispatches REAL pointer
//! events at the power footswitch, the three tempo pills, the count-in pill and
//! the three selector knobs, and asserts each one fires its callback with the
//! value the Rust wiring expects to receive.
//!
//! The output-device `Select` is covered separately: its dropdown is a
//! `PopupWindow`, a surface the testing backend cannot actuate (established in
//! #749 / #761), so what is provable here is that the field opens — the row
//! click is not, and is not pretended to be.

use adapter_gui::{MetronomeBridge, MetronomeWindow};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition};
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
fn every_metronome_control_fires_its_callback() {
    i_slint_backend_testing::init_no_event_loop();

    let w = MetronomeWindow::new().unwrap();
    // The panel reads its state from the MetronomeBridge global, so drive that.
    let bridge = MetronomeBridge::get(&w);
    // The window opens on the persisted settings; these are the defaults the
    // wiring pushes in (120 BPM, 4/4, subdivision off, click timbre).
    bridge.set_bpm(120.0);
    bridge.set_time_signature_index(2);
    bridge.set_subdivision_index(0);
    bridge.set_timbre_index(0);
    bridge.set_count_in(false);

    let powered: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
    let p = powered.clone();
    bridge.on_toggle_enabled(move |on| p.set(Some(on)));

    let bpm: Rc<RefCell<Vec<f32>>> = Rc::new(RefCell::new(Vec::new()));
    let b = bpm.clone();
    bridge.on_set_bpm(move |v| b.borrow_mut().push(v));

    let taps = Rc::new(Cell::new(0));
    let t = taps.clone();
    bridge.on_tap(move || t.set(t.get() + 1));

    let count_in: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
    let c = count_in.clone();
    bridge.on_set_count_in(move |on| c.set(Some(on)));

    let time_signature: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));
    let ts = time_signature.clone();
    bridge.on_set_time_signature(move |i| ts.set(Some(i)));

    let subdivision: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));
    let sd = subdivision.clone();
    bridge.on_set_subdivision(move |i| sd.set(Some(i)));

    let timbre: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));
    let tb = timbre.clone();
    bridge.on_set_timbre(move |i| tb.set(Some(i)));

    w.show().unwrap();

    // ── Power footswitch ────────────────────────────────────────────────
    assert!(
        click_id(&w, "PowerFootSwitch::ta", 0),
        "the power footswitch must be hittable"
    );
    assert_eq!(
        powered.get(),
        Some(true),
        "pressing POWER while off must ask to turn the click ON"
    );

    // ── Tempo row: −, TAP, +, and the count-in pill in the footer ───────
    assert_eq!(
        count_id(&w, "PillButton::ta"),
        4,
        "three tempo pills plus the count-in pill"
    );

    assert!(
        click_id(&w, "PillButton::ta", 0),
        "the − pill must be hittable"
    );
    assert!(
        click_id(&w, "PillButton::ta", 2),
        "the + pill must be hittable"
    );
    assert_eq!(
        *bpm.borrow(),
        vec![119.0, 121.0],
        "the nudges must ask for one BPM down and one BPM up from 120"
    );

    assert!(
        click_id(&w, "PillButton::ta", 1),
        "the TAP pill must be hittable"
    );
    assert_eq!(taps.get(), 1, "TAP must fire exactly one tap");

    assert!(
        click_id(&w, "PillButton::ta", 3),
        "the count-in pill must be hittable"
    );
    assert_eq!(
        count_in.get(),
        Some(true),
        "the count-in pill must toggle the current value"
    );

    // ── Knob row: time signature, subdivision, timbre ───────────────────
    assert_eq!(
        count_id(&w, "SelectorKnob::ta"),
        3,
        "three selector knobs (the volume knob is a PanelKnob, dragged not clicked)"
    );

    assert!(
        click_id(&w, "SelectorKnob::ta", 0),
        "time-signature knob hittable"
    );
    assert_eq!(
        time_signature.get(),
        Some(3),
        "a click advances the time signature from 4/4 (index 2) to 5/4 (index 3)"
    );

    assert!(
        click_id(&w, "SelectorKnob::ta", 1),
        "subdivision knob hittable"
    );
    assert_eq!(
        subdivision.get(),
        Some(1),
        "a click advances the subdivision from off (0) to eighths (1)"
    );

    assert!(click_id(&w, "SelectorKnob::ta", 2), "timbre knob hittable");
    assert_eq!(
        timbre.get(),
        Some(1),
        "a click advances the timbre from click (0) to wood (1)"
    );
}

/// Clicking the output field asks Rust for the endpoint list AND opens the
/// picker overlay. The overlay is a plain in-panel surface (not a PopupWindow),
/// so — unlike before — the whole flow is provable headlessly.
#[test]
fn opening_the_output_field_requests_the_endpoints_and_opens_the_picker() {
    i_slint_backend_testing::init_no_event_loop();

    let w = MetronomeWindow::new().unwrap();
    let bridge = MetronomeBridge::get(&w);
    let opened = Rc::new(Cell::new(0));
    let o = opened.clone();
    bridge.on_output_opened(move || o.set(o.get() + 1));
    w.show().unwrap();

    assert!(
        click_id(&w, "OutputField::ta", 0),
        "the output field must be hittable"
    );
    assert_eq!(
        opened.get(),
        1,
        "opening the field must ask Rust to publish the project's output endpoints"
    );
    assert!(
        bridge.get_output_picker_open(),
        "clicking the field must open the picker overlay"
    );
}

/// The whole point of dropping `PopupWindow`: a click on an endpoint row now
/// actually selects it. This is the path that could not be proven before
/// (#749/#761) and was the reported bug — the user could not pick an output.
#[test]
fn picking_an_output_row_selects_it_and_closes_the_picker() {
    i_slint_backend_testing::init_no_event_loop();

    let w = MetronomeWindow::new().unwrap();
    let bridge = MetronomeBridge::get(&w);
    bridge.set_output_options(slint::ModelRc::new(slint::VecModel::from(vec![
        adapter_gui::SelectOption {
            key: "main\u{1f}Out 1-2".into(),
            label: "Scarlett 2i2 · Out 1-2".into(),
        },
        adapter_gui::SelectOption {
            key: "monitor\u{1f}Phones".into(),
            label: "Headphones · Phones".into(),
        },
    ])));
    // Open the overlay directly (the field's open path is covered above).
    bridge.set_output_picker_open(true);

    let picked: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let p = picked.clone();
    bridge.on_pick_output(move |key| *p.borrow_mut() = Some(key.to_string()));
    w.show().unwrap();

    assert_eq!(
        count_id(&w, "OutputRow::ta"),
        2,
        "the open picker must list one row per output endpoint"
    );
    assert!(
        click_id(&w, "OutputRow::ta", 1),
        "the second endpoint row must be hittable"
    );
    assert_eq!(
        picked.borrow().as_deref(),
        Some("monitor\u{1f}Phones"),
        "clicking a row must select that endpoint by key"
    );
    assert!(
        !bridge.get_output_picker_open(),
        "picking must close the overlay"
    );
}
