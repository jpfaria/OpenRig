//! Headless proof that the backing-track player panel is operable: real
//! pointer events at its controls fire the bridge callbacks the wiring
//! dispatches from.

use std::cell::RefCell;
use std::rc::Rc;

use adapter_gui::{PlayerBridge, PlayerHarness, PlayerTrackRow};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, ModelRc, SharedString, VecModel};

fn track(name: &str, bundled: bool) -> PlayerTrackRow {
    PlayerTrackRow {
        name: name.into(),
        path: format!("/tracks/{name}.m4a").into(),
        bundled,
        category: "solo".into(),
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
    bridge.set_categories(ModelRc::new(VecModel::from(vec![
        SharedString::from("solo"),
        SharedString::from("rhythm"),
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

// The panel's only pill button is CHOOSE FILE; the transport is icons.
const CHOOSE_FILE: usize = 0;

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
fn double_clicking_a_track_plays_it() {
    let w = harness(false);
    let got = Rc::new(RefCell::new(Vec::<String>::new()));
    let g = got.clone();
    PlayerBridge::get(&w).on_play_track(move |path| g.borrow_mut().push(path.to_string()));
    let at = center(&w, "PlayerTrackItem::ta", 1);
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: at });
    for _ in 0..2 {
        win.dispatch_event(WindowEvent::PointerPressed {
            position: at,
            button: PointerEventButton::Left,
        });
        win.dispatch_event(WindowEvent::PointerReleased {
            position: at,
            button: PointerEventButton::Left,
        });
    }
    assert_eq!(*got.borrow(), vec!["/tracks/My Jam.m4a".to_string()]);
}

/// Every `set_playing` value the panel sends during the test.
fn playing_requests(w: &PlayerHarness) -> Rc<RefCell<Vec<bool>>> {
    let got = Rc::new(RefCell::new(Vec::new()));
    let g = got.clone();
    PlayerBridge::get(w).on_set_playing(move |playing| g.borrow_mut().push(playing));
    got
}

#[test]
fn play_starts_a_loaded_track() {
    let w = harness(true);
    let got = playing_requests(&w);
    press_release(&w, center(&w, "PlayerTransport::play-btn", 0));
    assert_eq!(*got.borrow(), vec![true]);
}

#[test]
fn pause_pauses_a_playing_track() {
    let w = harness(true);
    PlayerBridge::get(&w).set_playing(true);
    let got = playing_requests(&w);
    press_release(&w, center(&w, "PlayerTransport::pause-btn", 0));
    assert_eq!(*got.borrow(), vec![false]);
}

#[test]
fn play_does_nothing_without_a_track() {
    let w = harness(false);
    let got = playing_requests(&w);
    press_release(&w, center(&w, "PlayerTransport::play-btn", 0));
    assert!(got.borrow().is_empty());
}

#[test]
fn stop_stops_a_loaded_track() {
    let w = harness(true);
    let hits = presses(&w, |b, h| b.on_stop(move || *h.borrow_mut() += 1));
    press_release(&w, center(&w, "PlayerTransport::stop-btn", 0));
    assert_eq!(*hits.borrow(), 1);
}

#[test]
fn loop_marks_once_the_track_has_a_length() {
    let w = harness(true);
    let hits = presses(&w, |b, h| b.on_mark_loop(move || *h.borrow_mut() += 1));
    press_release(&w, center(&w, "PlayerTransport::loop-btn", 0));
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
    press_release(&w, center(&w, "ToolBarClose::close-ta", 0));
    assert_eq!(*hits.borrow(), 1);
}

#[test]
fn clicking_a_category_tab_picks_it() {
    let w = harness(false);
    let got = Rc::new(RefCell::new(Vec::<String>::new()));
    let g = got.clone();
    PlayerBridge::get(&w).on_pick_category(move |key| g.borrow_mut().push(key.to_string()));
    press_release(&w, center(&w, "PlayerCategoryTab::ta", 1));
    assert_eq!(*got.borrow(), vec!["rhythm".to_string()]);
}

#[test]
fn only_the_users_own_track_has_a_trash() {
    let w = harness(false);
    let trashes =
        i_slint_backend_testing::ElementHandle::find_by_element_id(&w, "PlayerTrackItem::del-ta")
            .count();
    assert_eq!(trashes, 1);
}

#[test]
fn the_trash_deletes_the_track_on_the_second_click() {
    let w = harness(false);
    let deleted = Rc::new(RefCell::new(Vec::<String>::new()));
    let d = deleted.clone();
    PlayerBridge::get(&w).on_delete_track(move |path| d.borrow_mut().push(path.to_string()));
    let picked = presses(&w, |b, h| b.on_pick_track(move |_| *h.borrow_mut() += 1));

    press_release(&w, center(&w, "PlayerTrackItem::del-ta", 0));
    assert!(deleted.borrow().is_empty(), "the first click only arms");
    press_release(&w, center(&w, "PlayerTrackItem::del-ta", 0));

    assert_eq!(*deleted.borrow(), vec!["/tracks/My Jam.m4a".to_string()]);
    assert_eq!(*picked.borrow(), 0, "the trash never loads the track");
}
