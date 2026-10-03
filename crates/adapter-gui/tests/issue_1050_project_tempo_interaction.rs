//! HEADLESS proof of the project tempo (#1050): one BPM for the whole
//! project, shown once in the chains page top bar with −/+ and TAP, and no
//! tempo control on any chain header.

use std::cell::RefCell;
use std::rc::Rc;

use adapter_gui::{ChainRigNav, MetronomeBridge, ProjectChainItem, ProjectChainsHarness};
use i_slint_backend_testing::ElementHandle;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, ModelRc, SharedString, VecModel};

fn nav() -> ChainRigNav {
    ChainRigNav {
        has_rig: true,
        preset_labels: ModelRc::new(VecModel::from(vec![SharedString::from("Clean")])),
        active_preset_index: 0,
        scene: 1,
        scene_count: 1,
    }
}

fn page() -> ProjectChainsHarness {
    i_slint_backend_testing::init_no_event_loop();
    let w = ProjectChainsHarness::new().unwrap();
    let chain = |title: &str| ProjectChainItem {
        title: title.into(),
        enabled: true,
        ..Default::default()
    };
    w.set_chains(ModelRc::new(VecModel::from(vec![
        chain("Guitar"),
        chain("Bass"),
    ])));
    w.set_rig_nav(ModelRc::new(VecModel::from(vec![nav(), nav()])));
    MetronomeBridge::get(&w).set_bpm(97.0);
    w.show().unwrap();
    w
}

fn find(w: &ProjectChainsHarness, id: &str) -> Vec<ElementHandle> {
    ElementHandle::find_by_element_id(w, id).collect()
}

fn click_id(w: &ProjectChainsHarness, id: &str) {
    let el = find(w, id)
        .into_iter()
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

#[test]
fn the_top_bar_shows_the_project_tempo_once() {
    let w = page();
    let shown = find(&w, "ProjectTempo::bpm-text");
    assert_eq!(shown.len(), 1, "one tempo for the project");
    assert_eq!(
        shown[0].accessible_label().map(|s| s.to_string()),
        Some("97".to_string())
    );
}

#[test]
fn no_chain_header_has_a_tempo_control() {
    let w = page();
    assert!(find(&w, "TempoChip::ta").is_empty());
}

#[test]
fn minus_plus_and_tap_drive_the_metronome() {
    let w = page();
    let bridge = MetronomeBridge::get(&w);
    let set = Rc::new(RefCell::new(Vec::new()));
    let taps = Rc::new(RefCell::new(0));
    {
        let set = Rc::clone(&set);
        bridge.on_set_bpm(move |bpm| set.borrow_mut().push(bpm));
    }
    {
        let taps = Rc::clone(&taps);
        bridge.on_tap(move || *taps.borrow_mut() += 1);
    }
    click_id(&w, "ProjectTempo::minus");
    click_id(&w, "ProjectTempo::plus");
    click_id(&w, "ProjectTempo::tap");
    assert_eq!(*set.borrow(), vec![96.0, 98.0]);
    assert_eq!(*taps.borrow(), 1);
}
