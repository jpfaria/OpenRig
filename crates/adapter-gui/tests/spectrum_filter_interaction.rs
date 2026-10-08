//! HEADLESS proof that the spectrum's output filter answers a real pointer:
//! the OUTPUTS button opens the checklist, a row click reports which output was
//! unchecked, and a click outside closes it. A window with nothing to filter
//! shows no button.

use adapter_gui::{AnalyzerBridge, ChannelOptionItem, SpectrumRow, SpectrumWindow};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

fn click_at(w: &impl ComponentHandle, at: LogicalPosition) {
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

fn click_id(w: &impl ComponentHandle, id: &str, nth: usize) -> bool {
    let Some(el) = i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).nth(nth)
    else {
        return false;
    };
    let pos = el.absolute_position();
    let size = el.size();
    click_at(
        w,
        LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0),
    );
    true
}

fn count_id(w: &impl ComponentHandle, id: &str) -> usize {
    i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).count()
}

fn item(index: i32, label: &str) -> ChannelOptionItem {
    ChannelOptionItem {
        index,
        label: label.into(),
        selected: true,
        available: true,
    }
}

fn window_with_two_outputs() -> SpectrumWindow {
    let w = SpectrumWindow::new().unwrap();
    w.window().set_size(slint::LogicalSize::new(720.0, 480.0));
    let bridge = AnalyzerBridge::get(&w);
    bridge.set_spectrum_enabled(true);
    bridge.set_spectrum_rows(ModelRc::from(Rc::new(VecModel::from(vec![SpectrumRow {
        label: "DIGITAL  ·  IN 1  →  OUT 1,2  ·  L".into(),
        output: "DIGITAL  ·  IN 1  →  OUT 1,2".into(),
        ..Default::default()
    }]))));
    bridge.set_spectrum_filter(ModelRc::from(Rc::new(VecModel::from(vec![
        item(0, "DIGITAL  ·  IN 1  →  OUT 1,2"),
        item(1, "DIGITAL  ·  IN 1  →  OUT 25,26"),
    ]))));
    bridge.set_spectrum_filter_shown(2);
    w
}

#[test]
fn the_outputs_button_opens_a_checklist_of_every_output() {
    i_slint_backend_testing::init_no_event_loop();
    let w = window_with_two_outputs();
    assert_eq!(count_id(&w, "ChannelPicker::chan-cell"), 0);

    assert!(
        click_id(&w, "SpectrumPanel::filter-button", 0),
        "no OUTPUTS button"
    );

    assert_eq!(count_id(&w, "ChannelPicker::chan-cell"), 2);
}

#[test]
fn unchecking_a_row_reports_that_output() {
    i_slint_backend_testing::init_no_event_loop();
    let w = window_with_two_outputs();
    let got: Rc<RefCell<Option<(i32, bool)>>> = Rc::new(RefCell::new(None));
    let sink = got.clone();
    AnalyzerBridge::get(&w).on_toggle_spectrum_filter(move |row, on| {
        *sink.borrow_mut() = Some((row, on));
    });
    click_id(&w, "SpectrumPanel::filter-button", 0);

    assert!(click_id(&w, "ChannelPicker::chan-cell", 1));

    assert_eq!(*got.borrow(), Some((1, false)));
}

#[test]
fn a_click_outside_the_checklist_closes_it() {
    i_slint_backend_testing::init_no_event_loop();
    let w = window_with_two_outputs();
    click_id(&w, "SpectrumPanel::filter-button", 0);

    click_at(&w, LogicalPosition::new(5.0, 470.0));

    assert_eq!(count_id(&w, "ChannelPicker::chan-cell"), 0);
}

#[test]
fn nothing_to_filter_shows_no_button() {
    i_slint_backend_testing::init_no_event_loop();
    let w = SpectrumWindow::new().unwrap();

    assert_eq!(count_id(&w, "SpectrumPanel::filter-button"), 0);
}
