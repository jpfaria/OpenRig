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

use adapter_gui::MetronomeWindow;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};
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
    // The window opens on the persisted settings; these are the defaults the
    // wiring pushes in (120 BPM, 4/4, subdivision off, click timbre).
    w.set_bpm(120.0);
    w.set_time_signature_index(2);
    w.set_subdivision_index(0);
    w.set_timbre_index(0);
    w.set_count_in(false);

    let powered: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
    let p = powered.clone();
    w.on_toggle_enabled(move |on| p.set(Some(on)));

    let bpm: Rc<RefCell<Vec<f32>>> = Rc::new(RefCell::new(Vec::new()));
    let b = bpm.clone();
    w.on_set_bpm(move |v| b.borrow_mut().push(v));

    let taps = Rc::new(Cell::new(0));
    let t = taps.clone();
    w.on_tap(move || t.set(t.get() + 1));

    let count_in: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
    let c = count_in.clone();
    w.on_set_count_in(move |on| c.set(Some(on)));

    let time_signature: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));
    let ts = time_signature.clone();
    w.on_set_time_signature(move |i| ts.set(Some(i)));

    let subdivision: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));
    let sd = subdivision.clone();
    w.on_set_subdivision(move |i| sd.set(Some(i)));

    let timbre: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));
    let tb = timbre.clone();
    w.on_set_timbre(move |i| tb.set(Some(i)));

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

/// The output select's FIELD is a plain TouchArea, so opening it — the moment
/// the Rust side enumerates the devices and publishes the options — is
/// provable. The rows inside the dropdown are not: `PopupWindow` content is a
/// separate surface the testing backend cannot reach.
#[test]
fn opening_the_output_select_asks_rust_for_the_device_list() {
    i_slint_backend_testing::init_no_event_loop();

    let w = MetronomeWindow::new().unwrap();
    let opened = Rc::new(Cell::new(0));
    let o = opened.clone();
    w.on_output_opened(move || o.set(o.get() + 1));
    w.show().unwrap();

    assert!(
        click_id(&w, "Select::ta", 0),
        "the output select field must be hittable"
    );
    assert_eq!(
        opened.get(),
        1,
        "opening the select must ask Rust to publish the output devices"
    );
}

/// What the dropdown SHOWS is provable; what a click on one of its rows does is
/// not. Verified on this window: after opening, `Select::row-ta` enumerates both
/// device rows, but a pointer event dispatched at one never reaches it —
/// `PopupWindow` content is a separate surface the testing backend cannot
/// actuate, the same limitation found in #749 and #761. So this test pins the
/// list the user sees; the `picked(key)` → `SetMetronomeOutput` half stays
/// unproven headlessly and needs a click in the running app.
#[test]
fn the_open_dropdown_lists_every_output_device() {
    i_slint_backend_testing::init_no_event_loop();

    let w = MetronomeWindow::new().unwrap();
    w.set_output_options(slint::ModelRc::new(slint::VecModel::from(vec![
        adapter_gui::SelectOption {
            key: "dev:a".into(),
            label: "Scarlett 2i2".into(),
        },
        adapter_gui::SelectOption {
            key: "dev:b".into(),
            label: "MacBook Speakers".into(),
        },
    ])));
    w.show().unwrap();

    assert!(click_id(&w, "Select::ta", 0), "the select field must open");
    assert_eq!(
        count_id(&w, "Select::row-ta"),
        2,
        "the open dropdown must list one row per output device"
    );
}
