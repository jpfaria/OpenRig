//! Headless proof that the backing-track player panel is operable: real
//! pointer events at its controls fire the bridge callbacks the wiring
//! dispatches from.

use std::cell::RefCell;
use std::rc::Rc;

use adapter_gui::{PlayerBridge, PlayerHarness, PlayerTrackRow};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, ModelRc, VecModel};

fn track(name: &str, bundled: bool) -> PlayerTrackRow {
    PlayerTrackRow {
        name: name.into(),
        path: format!("/tracks/{name}.m4a").into(),
        bundled,
    }
}

/// The panel with a two-track library and, when `loaded`, the first track
/// loaded with a known duration.
fn harness(loaded: bool) -> PlayerHarness {
    i_slint_backend_testing::init_no_event_loop();
    let w = PlayerHarness::new().unwrap();
    let bridge = PlayerBridge::get(&w);
    bridge.set_library(ModelRc::new(VecModel::from(vec![
        track("Slow Blues in A", true),
        track("My Jam", false),
    ])));
    if loaded {
        bridge.set_track_name("Slow Blues in A".into());
        bridge.set_track_path("/tracks/Slow Blues in A.m4a".into());
        bridge.set_duration(200.0);
    }
    w.show().unwrap();
    w
}

fn center(w: &PlayerHarness, id: &str, nth: usize) -> LogicalPosition {
    let el = i_slint_backend_testing::ElementHandle::find_by_element_id(w, id)
        .nth(nth)
        .unwrap_or_else(|| panic!("{id} #{nth} not found"));
    let (pos, size) = (el.absolute_position(), el.size());
    LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0)
}

fn press_release(w: &PlayerHarness, at: LogicalPosition) {
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

/// Count how many times `connect` sees its callback fire during `act`.
fn presses(
    w: &PlayerHarness,
    connect: impl FnOnce(&PlayerBridge, Rc<RefCell<u32>>),
) -> Rc<RefCell<u32>> {
    let hits = Rc::new(RefCell::new(0));
    connect(&PlayerBridge::get(w), hits.clone());
    hits
}

// Transport order in the panel: STOP, PLAY/PAUSE, LOOP, CLEAR, CHOOSE FILE.
const PLAY: usize = 1;
const LOOP: usize = 2;
const CHOOSE_FILE: usize = 4;

#[test]
fn clicking_a_track_picks_its_path() {
    let w = harness(false);
    let got = Rc::new(RefCell::new(Vec::<String>::new()));
    let g = got.clone();
    PlayerBridge::get(&w).on_pick_track(move |path| g.borrow_mut().push(path.to_string()));
    press_release(&w, center(&w, "PlayerTrackItem::ta", 1));
    assert_eq!(*got.borrow(), vec!["/tracks/My Jam.m4a".to_string()]);
}

#[test]
fn play_toggles_a_loaded_track() {
    let w = harness(true);
    let hits = presses(&w, |b, h| b.on_toggle_playing(move || *h.borrow_mut() += 1));
    press_release(&w, center(&w, "PillButton::ta", PLAY));
    assert_eq!(*hits.borrow(), 1);
}

#[test]
fn play_does_nothing_without_a_track() {
    let w = harness(false);
    let hits = presses(&w, |b, h| b.on_toggle_playing(move || *h.borrow_mut() += 1));
    press_release(&w, center(&w, "PillButton::ta", PLAY));
    assert_eq!(*hits.borrow(), 0);
}

#[test]
fn loop_marks_once_the_track_has_a_length() {
    let w = harness(true);
    let hits = presses(&w, |b, h| b.on_mark_loop(move || *h.borrow_mut() += 1));
    press_release(&w, center(&w, "PillButton::ta", LOOP));
    assert_eq!(*hits.borrow(), 1);
}

#[test]
fn choose_file_asks_for_a_file() {
    let w = harness(false);
    let hits = presses(&w, |b, h| b.on_choose_file(move || *h.borrow_mut() += 1));
    press_release(&w, center(&w, "PillButton::ta", CHOOSE_FILE));
    assert_eq!(*hits.borrow(), 1);
}

#[test]
fn clicking_the_middle_of_the_seek_bar_seeks_to_half_the_track() {
    let w = harness(true);
    let got = Rc::new(RefCell::new(Vec::<f32>::new()));
    let g = got.clone();
    PlayerBridge::get(&w).on_seek(move |seconds| g.borrow_mut().push(seconds));
    press_release(&w, center(&w, "PlayerSeekBar::ta", 0));
    let seeks = got.borrow();
    assert_eq!(seeks.len(), 1);
    assert!((seeks[0] - 100.0).abs() < 5.0, "seek to {}", seeks[0]);
}

#[test]
fn the_close_button_closes_the_player() {
    let w = harness(true);
    let hits = presses(&w, |b, h| b.on_close_player(move || *h.borrow_mut() += 1));
    press_release(&w, center(&w, "PlayerPanel::close-ta", 0));
    assert_eq!(*hits.borrow(), 1);
}
