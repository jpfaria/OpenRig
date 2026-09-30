//! #1007 — HEADLESS proof that the mixer panel is actually operable.
//!
//! A render only proves layout. This dispatches REAL pointer events at the
//! strips and asserts the bridge callbacks fire with the strip's id.

use std::cell::RefCell;
use std::rc::Rc;

use adapter_gui::{MixerBridge, MixerHarness, MixerStripRow};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, ModelRc, VecModel};

fn row(id: &str, is_input: bool, muted: bool) -> MixerStripRow {
    MixerStripRow {
        id: id.into(),
        name: id.into(),
        detail: "CH 1".into(),
        is_input,
        position: 0.5,
        gain_label: "-12.0 dB".into(),
        muted,
    }
}

fn harness(inputs: Vec<MixerStripRow>, outputs: Vec<MixerStripRow>) -> MixerHarness {
    i_slint_backend_testing::init_no_event_loop();
    let w = MixerHarness::new().unwrap();
    let bridge = MixerBridge::get(&w);
    bridge.set_inputs(ModelRc::new(VecModel::from(inputs)));
    bridge.set_outputs(ModelRc::new(VecModel::from(outputs)));
    w.show().unwrap();
    w
}

fn element(w: &MixerHarness, id: &str, nth: usize) -> (LogicalPosition, slint::LogicalSize) {
    let el = i_slint_backend_testing::ElementHandle::find_by_element_id(w, id)
        .nth(nth)
        .unwrap_or_else(|| panic!("{id} #{nth} not found"));
    (el.absolute_position(), el.size())
}

fn press_release(w: &MixerHarness, at: LogicalPosition) {
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: at });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: at,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerReleased {
        position: at,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerExited);
}

fn center(w: &MixerHarness, id: &str, nth: usize) -> LogicalPosition {
    let (pos, size) = element(w, id, nth);
    LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0)
}

fn count(w: &MixerHarness, id: &str) -> usize {
    i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).count()
}

#[test]
fn one_strip_per_endpoint_on_each_side() {
    let w = harness(
        vec![row("in:0@d", true, false), row("in:1@d", true, false)],
        vec![row("out:0,1@d", false, false)],
    );
    assert_eq!(count(&w, "MixerStripView::mute-ta"), 3);
}

#[test]
fn mute_press_reports_the_strip() {
    let w = harness(
        vec![row("in:0@d", true, false)],
        vec![row("out:0,1@d", false, false)],
    );
    let got = Rc::new(RefCell::new(Vec::<String>::new()));
    let g = got.clone();
    MixerBridge::get(&w).on_mute_toggled(move |id| g.borrow_mut().push(id.to_string()));
    press_release(&w, center(&w, "MixerStripView::mute-ta", 1));
    assert_eq!(*got.borrow(), vec!["out:0,1@d".to_string()]);
}

#[test]
fn pressing_the_top_of_the_fader_asks_for_full_gain() {
    let w = harness(vec![row("in:0@d", true, false)], vec![]);
    let got = Rc::new(RefCell::new(Vec::<(String, f32)>::new()));
    let g = got.clone();
    MixerBridge::get(&w).on_fader_moved(move |id, p| g.borrow_mut().push((id.to_string(), p)));
    let (pos, size) = element(&w, "MixerStripView::fader-ta", 0);
    press_release(
        &w,
        LogicalPosition::new(pos.x + size.width / 2.0, pos.y + 1.0),
    );
    let got = got.borrow();
    assert!(
        !got.is_empty(),
        "a press on the fader must report a position"
    );
    assert_eq!(got[0].0, "in:0@d");
    assert!(
        got[0].1 > 0.99,
        "top of the fader is full gain, got {}",
        got[0].1
    );
}

#[test]
fn pressing_the_bottom_of_the_fader_asks_for_the_minimum() {
    let w = harness(vec![row("in:0@d", true, false)], vec![]);
    let got = Rc::new(RefCell::new(Vec::<f32>::new()));
    let g = got.clone();
    MixerBridge::get(&w).on_fader_moved(move |_, p| g.borrow_mut().push(p));
    let (pos, size) = element(&w, "MixerStripView::fader-ta", 0);
    press_release(
        &w,
        LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height - 1.0),
    );
    assert!(
        got.borrow().first().is_some_and(|p| *p < 0.01),
        "{:?}",
        got.borrow()
    );
}

#[test]
fn the_inline_close_button_reports_close() {
    let w = harness(vec![], vec![]);
    let closed = Rc::new(RefCell::new(false));
    let c = closed.clone();
    MixerBridge::get(&w).on_close_mixer(move || *c.borrow_mut() = true);
    press_release(&w, center(&w, "MixerPanel::close-ta", 0));
    assert!(*closed.borrow());
}
