use std::cell::RefCell;
use std::rc::Rc;

use application::command::Tone3000Command;
use application::tone3000::{Tone3000Architecture, Tone3000Format, Tone3000Gear, Tone3000Sort};
use slint::{Global, SharedString};

use super::wire_tone3000_intents;
use crate::{Tone3000Bridge, Tone3000Window};

type Log = Rc<RefCell<Vec<String>>>;

fn wired(picked: Option<Tone3000Architecture>) -> (Tone3000Window, Log, Log) {
    i_slint_backend_testing::init_no_event_loop();
    let w = Tone3000Window::new().unwrap();
    let sent: Log = Rc::new(RefCell::new(Vec::new()));
    let picks: Log = Rc::new(RefCell::new(Vec::new()));
    let s = sent.clone();
    let p = picks.clone();
    wire_tone3000_intents(
        &w,
        Rc::new(move |cmd: Tone3000Command| s.borrow_mut().push(format!("{cmd:?}"))),
        Rc::new(move |id, arch| p.borrow_mut().push(format!("{id} {arch:?}"))),
        Rc::new(move |_| picked),
    );
    (w, sent, picks)
}

fn debug(cmd: Tone3000Command) -> String {
    format!("{cmd:?}")
}

#[test]
fn a_search_carries_the_query_and_every_filter() {
    let (w, sent, _) = wired(None);
    let bridge = Tone3000Bridge::get(&w);
    bridge.set_query(SharedString::from("  plexi  "));
    bridge.set_format(SharedString::from("nam"));
    bridge.set_gear(SharedString::from("full-rig"));
    bridge.set_sort(SharedString::from("downloads-all-time"));
    bridge.invoke_search(3);
    assert_eq!(
        *sent.borrow(),
        vec![debug(Tone3000Command::SearchTone3000 {
            query: "plexi".into(),
            page: 3,
            format: Some(Tone3000Format::Nam),
            gear: Some(Tone3000Gear::FullRig),
            sort: Some(Tone3000Sort::DownloadsAllTime),
        })]
    );
}

#[test]
fn a_blank_filter_searches_everything() {
    let (w, sent, _) = wired(None);
    let bridge = Tone3000Bridge::get(&w);
    bridge.set_sort(SharedString::from(""));
    bridge.invoke_search(0);
    assert_eq!(
        *sent.borrow(),
        vec![debug(Tone3000Command::SearchTone3000 {
            query: String::new(),
            page: 1,
            format: None,
            gear: None,
            sort: None,
        })]
    );
}

#[test]
fn the_default_sort_is_best_match() {
    let (w, sent, _) = wired(None);
    Tone3000Bridge::get(&w).invoke_search(1);
    assert!(sent.borrow()[0].contains("BestMatch"));
}

#[test]
fn an_install_takes_the_architecture_the_user_picked() {
    let (w, sent, _) = wired(Some(Tone3000Architecture::A1));
    Tone3000Bridge::get(&w).invoke_install(SharedString::from("4521"));
    assert_eq!(
        *sent.borrow(),
        vec![debug(Tone3000Command::InstallTone3000 {
            tone_id: 4521,
            architecture: Some(Tone3000Architecture::A1),
            block_type: None,
        })]
    );
}

#[test]
fn an_install_without_a_pick_leaves_the_default_to_the_dispatcher() {
    let (w, sent, _) = wired(None);
    Tone3000Bridge::get(&w).invoke_install(SharedString::from("7"));
    assert!(sent.borrow()[0].contains("architecture: None"));
}

#[test]
fn picking_an_architecture_is_recorded_not_dispatched() {
    let (w, sent, picks) = wired(None);
    let bridge = Tone3000Bridge::get(&w);
    bridge.invoke_pick_arch(SharedString::from("12"), 1);
    bridge.invoke_pick_arch(SharedString::from("12"), 2);
    bridge.invoke_pick_arch(SharedString::from("12"), 0);
    assert_eq!(*picks.borrow(), vec!["12 A1", "12 A2"]);
    assert!(sent.borrow().is_empty());
}

#[test]
fn uninstall_names_the_package() {
    let (w, sent, _) = wired(None);
    Tone3000Bridge::get(&w).invoke_uninstall(SharedString::from("tone3000_7_a2"));
    assert_eq!(
        *sent.borrow(),
        vec![debug(Tone3000Command::UninstallTone3000 {
            plugin_id: "tone3000_7_a2".into(),
        })]
    );
}

#[test]
fn a_malformed_tone_id_sends_nothing() {
    let (w, sent, picks) = wired(None);
    let bridge = Tone3000Bridge::get(&w);
    bridge.invoke_install(SharedString::from("abc"));
    bridge.invoke_pick_arch(SharedString::from(""), 1);
    assert!(sent.borrow().is_empty());
    assert!(picks.borrow().is_empty());
}
