//! Headless proof that the drum machine's controls respond to a pointer in its
//! own window, and that the compact chain view carries no drums section.
//! A PNG proves layout only; these press the real TouchAreas and check the
//! callback the Rust wiring listens to.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adapter_gui::{CompactChainViewWindow, DrumPickRow, DrumsBridge, DrumsWindow};
use i_slint_backend_testing::ElementHandle;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, ModelRc, VecModel};

fn click(w: &impl ComponentHandle, id: &str, nth: usize) -> bool {
    click_at(w, id, nth, 0.5)
}

/// Presses `id` at its horizontal centre, `down` of the way from its top.
fn click_at(w: &impl ComponentHandle, id: &str, nth: usize, down: f32) -> bool {
    let Some(el) = ElementHandle::find_by_element_id(w, id).nth(nth) else {
        return false;
    };
    let (pos, size) = (el.absolute_position(), el.size());
    let at = LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height * down);
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
    true
}

fn count(w: &impl ComponentHandle, id: &str) -> usize {
    ElementHandle::find_by_element_id(w, id).count()
}

fn row(key: &str, label: &str, header: bool) -> DrumPickRow {
    DrumPickRow {
        key: key.into(),
        label: label.into(),
        header,
    }
}

fn rows(items: Vec<DrumPickRow>) -> ModelRc<DrumPickRow> {
    ModelRc::new(VecModel::from(items))
}

fn window() -> DrumsWindow {
    i_slint_backend_testing::init_no_event_loop();
    let w = DrumsWindow::new().unwrap();
    let bridge = DrumsBridge::get(&w);
    bridge.set_kit_rows(rows(vec![
        row("black-pearl", "Black Pearl", false),
        row("red", "Red Zeppelin", false),
    ]));
    bridge.set_groove_rows(rows(vec![
        row("", "rock", true),
        row("rock-01", "Rock 1", false),
        row("", "jazz", true),
        row("jazz-01", "Jazz 1", false),
    ]));
    bridge.set_output_rows(rows(vec![row("main\u{1f}Out", "Main · Out", false)]));
    w.show().unwrap();
    w
}

const FOOTSWITCH: &str = "DrumFootSwitch::ta";
const FIELD: &str = "DrumPickerField::ta";
const ROW: &str = "DrumPickRowView::ta";

#[test]
fn power_is_the_only_transport_switch_and_fill_fires_while_playing() {
    let w = window();
    let bridge = DrumsBridge::get(&w);
    let powered = Rc::new(RefCell::new(Vec::new()));
    let p = powered.clone();
    bridge.on_toggle_enabled(move |on| p.borrow_mut().push(on));
    let fills = Rc::new(Cell::new(0));
    let f = fills.clone();
    bridge.on_fill(move || f.set(f.get() + 1));

    assert_eq!(count(&w, FOOTSWITCH), 1, "FILL is the only footswitch");
    assert!(click(&w, "PillSwitch::ta", 0));
    assert_eq!(*powered.borrow(), vec![true], "POWER while silent plays");
    bridge.set_playing(true);
    assert!(click(&w, "PillSwitch::ta", 0));
    assert_eq!(
        *powered.borrow(),
        vec![true, false],
        "POWER while playing turns it off"
    );

    // A fill only lands on a running groove.
    bridge.set_playing(false);
    assert!(click(&w, FOOTSWITCH, 0));
    assert_eq!(fills.get(), 0, "FILL is inert while stopped");
    bridge.set_playing(true);
    assert!(click(&w, FOOTSWITCH, 0));
    assert_eq!(fills.get(), 1);
}

#[test]
fn the_kit_field_opens_a_list_and_a_row_picks_that_kit() {
    let w = window();
    let bridge = DrumsBridge::get(&w);
    let picked = Rc::new(RefCell::new(None));
    let p = picked.clone();
    bridge.on_pick_kit(move |key| *p.borrow_mut() = Some(key.to_string()));

    assert_eq!(count(&w, FIELD), 3, "kit, groove and output fields");
    assert!(click(&w, FIELD, 0));
    assert_eq!(count(&w, ROW), 2);
    assert!(click(&w, ROW, 1));
    assert_eq!(picked.borrow().as_deref(), Some("red"));
    assert_eq!(count(&w, ROW), 0, "picking closes the list");
}

#[test]
fn the_groove_list_shows_genre_headers_that_are_not_choices() {
    let w = window();
    let bridge = DrumsBridge::get(&w);
    let picked = Rc::new(RefCell::new(None));
    let p = picked.clone();
    bridge.on_pick_groove(move |key| *p.borrow_mut() = Some(key.to_string()));

    assert!(click(&w, FIELD, 1));
    assert_eq!(
        count(&w, "DrumPickRowView::genre"),
        2,
        "rock and jazz headers"
    );
    assert_eq!(count(&w, ROW), 2, "only the grooves are clickable");
    assert!(click(&w, ROW, 1));
    assert_eq!(picked.borrow().as_deref(), Some("jazz-01"));
}

#[test]
fn the_output_field_asks_for_endpoints_and_a_row_picks_one() {
    let w = window();
    let bridge = DrumsBridge::get(&w);
    let opened = Rc::new(Cell::new(0));
    let o = opened.clone();
    bridge.on_output_opened(move || o.set(o.get() + 1));
    let picked = Rc::new(RefCell::new(None));
    let p = picked.clone();
    bridge.on_pick_output(move |key| *p.borrow_mut() = Some(key.to_string()));

    assert!(click(&w, FIELD, 2));
    assert_eq!(opened.get(), 1);
    assert!(click(&w, ROW, 0));
    assert_eq!(picked.borrow().as_deref(), Some("main\u{1f}Out"));
}

#[test]
fn a_click_outside_the_list_closes_it_without_picking() {
    let w = window();
    let bridge = DrumsBridge::get(&w);
    let picked = Rc::new(Cell::new(false));
    let p = picked.clone();
    bridge.on_pick_kit(move |_| p.set(true));

    assert!(click(&w, FIELD, 0));
    // The list box sits near the top; the scrim's bottom edge is outside it.
    assert!(click_at(&w, "DrumsPanel::scrim", 0, 0.97));
    assert_eq!(count(&w, ROW), 0);
    assert!(!picked.get());
}

#[test]
fn the_compact_view_has_no_drums_section() {
    i_slint_backend_testing::init_no_event_loop();
    let w = CompactChainViewWindow::new().unwrap();
    w.window().set_size(slint::LogicalSize::new(1100.0, 900.0));
    w.set_chain_enabled(true);
    w.show().unwrap();

    assert_eq!(count(&w, "CompactChainSections::drums-toggle"), 0);
    assert_eq!(count(&w, "DrumsPanel::power"), 0);
}

#[test]
fn opening_a_list_clears_its_search_and_typing_narrows_it() {
    let w = window();
    let bridge = DrumsBridge::get(&w);
    bridge.set_query("stale".into());
    let queries = Rc::new(RefCell::new(Vec::new()));
    let q = queries.clone();
    bridge.on_query_changed(move |text| q.borrow_mut().push(text.to_string()));

    assert_eq!(count(&w, "DrumsPanel::search"), 0, "no search while closed");
    assert!(click(&w, FIELD, 1));
    assert_eq!(bridge.get_query(), "", "a list opens unfiltered");
    assert!(
        click(&w, "DrumsPanel::search", 0),
        "the open list has a search box"
    );
    for ch in "jazz".chars() {
        let text = slint::SharedString::from(ch.to_string().as_str());
        w.window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        w.window().dispatch_event(WindowEvent::KeyReleased { text });
    }
    assert_eq!(bridge.get_query(), "jazz");
    assert_eq!(queries.borrow().last().map(String::as_str), Some("jazz"));
    assert_eq!(queries.borrow().first().map(String::as_str), Some(""));
}
