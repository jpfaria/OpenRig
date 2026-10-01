//! #1022 — the compact chain view keeps only power, title, volume, latency,
//! configure and delete in its header. Mixer, DI, Looper, Doctor and Meters
//! are accordion sections at the bottom of the view, the same shape as the
//! mixer (#1007): one header each, and only one section open at a time. None
//! of them floats over the view any more.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adapter_gui::{CompactChainViewWindow, CompactLooper, ToneDoctorState};
use i_slint_backend_testing::ElementHandle;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, ModelRc, SharedString, VecModel};

const CHAIN: i32 = 3;

fn window(enabled: bool) -> CompactChainViewWindow {
    i_slint_backend_testing::init_no_event_loop();
    let w = CompactChainViewWindow::new().unwrap();
    w.window().set_size(slint::LogicalSize::new(1100.0, 900.0));
    w.set_chain_index(CHAIN);
    w.set_chain_enabled(enabled);
    w.set_di_loop_sources(ModelRc::new(VecModel::from(vec![
        SharedString::from("clean-electric-guitar-loop"),
        SharedString::from("Choose file…"),
    ])));
    w.show().unwrap();
    w
}

fn count(w: &CompactChainViewWindow, id: &str) -> usize {
    ElementHandle::find_by_element_id(w, id).count()
}

fn click(w: &CompactChainViewWindow, id: &str) {
    let el = ElementHandle::find_by_element_id(w, id)
        .next()
        .unwrap_or_else(|| panic!("{id} not found"));
    let (pos, size) = (el.absolute_position(), el.size());
    let at = LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
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

const DI: &str = "CompactChainSections::di-toggle";
const LOOPER: &str = "CompactChainSections::looper-toggle";
const DOCTOR: &str = "CompactChainSections::doctor-toggle";
const METERS: &str = "CompactChainSections::meters-toggle";
const MIXER: &str = "CompactChainMixer::mixer-toggle";

#[test]
fn the_header_keeps_volume_and_latency_but_no_di_doctor_or_meters_button() {
    let w = window(true);
    for gone in [
        "ChainDiLoopButton::ta",
        "ChainToneDoctorButton::ta",
        "ChainMetersButton::ta",
    ] {
        assert_eq!(count(&w, gone), 0, "{gone} left the header");
    }
    assert_eq!(count(&w, "ChainVolumeButton::ta"), 1);
}

#[test]
fn every_section_header_is_on_screen_and_all_open_collapsed() {
    let w = window(true);
    for toggle in [MIXER, DI, LOOPER, DOCTOR, METERS] {
        assert_eq!(count(&w, toggle), 1, "{toggle}");
    }
    assert_eq!(count(&w, "DiLoopPanel::sel-ta"), 0);
    assert_eq!(count(&w, "LooperPanelView::add-ta"), 0);
    assert_eq!(count(&w, "ToneDoctorPanel::diag-ta"), 0);
}

#[test]
fn the_di_section_opens_the_di_panel_inline_and_it_stops_the_loop() {
    let w = window(true);
    w.set_di_loop_selected_index(0);
    w.set_di_loop_playing(true);
    let stopped = Rc::new(Cell::new(false));
    let s = stopped.clone();
    w.on_di_loop_stop(move || s.set(true));

    click(&w, DI);
    assert_eq!(count(&w, "DiLoopPanel::sel-ta"), 1);
    click(&w, "DiLoopPanel::play-ta");
    assert!(stopped.get(), "the inline panel's stop reaches the window");
}

#[test]
fn the_looper_section_shows_the_looper_panel_and_add_targets_this_chain() {
    let w = window(true);
    let added = Rc::new(RefCell::new(Vec::<i32>::new()));
    let a = added.clone();
    CompactLooper::get(&w).on_add(move |ci| a.borrow_mut().push(ci));

    click(&w, LOOPER);
    assert_eq!(count(&w, "LooperPanelView::add-ta"), 1);
    click(&w, "LooperPanelView::add-ta");
    assert_eq!(*added.borrow(), vec![CHAIN]);
}

#[test]
fn the_doctor_section_opens_clean_and_runs_on_this_chain() {
    let w = window(true);
    let st = ToneDoctorState::get(&w);
    st.set_has_result(true);
    st.set_can_diagnose(false);
    let runs = Rc::new(RefCell::new(Vec::<i32>::new()));
    let r = runs.clone();
    w.on_tone_doctor_run(move |ci| r.borrow_mut().push(ci));

    click(&w, DOCTOR);
    let st = ToneDoctorState::get(&w);
    assert!(!st.get_has_result(), "opening resets the last result");
    assert!(st.get_can_diagnose());
    assert_eq!(st.get_chain_index(), CHAIN);
    click(&w, "ToneDoctorPanel::diag-ta");
    assert_eq!(*runs.borrow(), vec![CHAIN]);
}

#[test]
fn opening_one_section_closes_the_one_that_was_open() {
    let w = window(true);
    click(&w, DI);
    click(&w, LOOPER);
    assert_eq!(count(&w, "DiLoopPanel::sel-ta"), 0);
    assert_eq!(count(&w, "LooperPanelView::add-ta"), 1);
    click(&w, MIXER);
    assert_eq!(count(&w, "LooperPanelView::add-ta"), 0);
    assert!(count(&w, "ParamTabBar::tab-ta") > 0, "the mixer is open");
    click(&w, LOOPER);
    assert_eq!(count(&w, "ParamTabBar::tab-ta"), 0, "the mixer closed");
}

#[test]
fn a_second_click_on_an_open_section_closes_it() {
    let w = window(true);
    click(&w, DOCTOR);
    click(&w, DOCTOR);
    assert_eq!(count(&w, "ToneDoctorPanel::diag-ta"), 0);
}

#[test]
fn a_stopped_chain_has_no_meters_section() {
    let w = window(false);
    assert_eq!(count(&w, METERS), 0);
    assert_eq!(count(&w, DI), 1, "the other sections stay");
}

#[test]
fn an_open_section_pushes_the_headers_above_it_up() {
    let w = window(true);
    let top = |w: &CompactChainViewWindow| {
        ElementHandle::find_by_element_id(w, MIXER)
            .next()
            .unwrap()
            .absolute_position()
            .y
    };
    let closed = top(&w);
    click(&w, METERS);
    assert!(
        top(&w) < closed,
        "the meter rows open below the mixer header"
    );
    click(&w, METERS);
    assert_eq!(top(&w), closed);
}

/// An open section body is the section itself, not the floating panel boxed
/// inside it: the panel spans the view (16px gutter each side) and drops its
/// own title, since the section header already names it.
#[test]
fn every_open_section_spans_the_view_without_its_own_box_title() {
    let w = window(true);
    for (toggle, panel, title) in [
        (DI, "CompactChainSections::di", "DiLoopPanel::title"),
        (
            LOOPER,
            "CompactChainSections::looper",
            "LooperPanelView::title",
        ),
        (
            DOCTOR,
            "CompactChainSections::doctor",
            "ToneDoctorPanel::title",
        ),
    ] {
        click(&w, toggle);
        let el = ElementHandle::find_by_element_id(&w, panel)
            .next()
            .unwrap_or_else(|| panic!("{panel} not found"));
        assert!(
            (el.size().width - (1100.0 - 32.0)).abs() < 1.0,
            "{panel} is {}px wide, not the view's width",
            el.size().width
        );
        assert_eq!(
            count(&w, title),
            0,
            "{title} still drawn inside the section"
        );
    }
}
