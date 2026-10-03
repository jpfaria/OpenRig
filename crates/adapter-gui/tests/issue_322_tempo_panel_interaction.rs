//! HEADLESS proof that the chain header's tempo chip and the tempo panel it
//! opens respond to a pointer: instantiate the real `ProjectChainsPage`
//! (through `ProjectChainsHarness`), click the chip and the panel's controls,
//! and read back the `TempoPanel` / `MetronomeBridge` globals they report
//! through. The chip shows the global tempo (the metronome's); the panel nudges
//! and taps it, stores it on the chain's active preset or clears the preset's
//! own tempo, and flips the "use global tempo" lock.

use std::cell::RefCell;
use std::rc::Rc;

use adapter_gui::{
    ChainRigNav, MetronomeBridge, ProjectChainItem, ProjectChainsHarness, TempoPanel,
};
use i_slint_backend_testing::ElementHandle;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, ModelRc, SharedString, VecModel};

fn nav(has_rig: bool, preset_bpm: Option<f32>) -> ChainRigNav {
    ChainRigNav {
        has_rig,
        preset_labels: ModelRc::new(VecModel::from(if has_rig {
            vec![SharedString::from("Clean")]
        } else {
            Vec::new()
        })),
        active_preset_index: 0,
        scene: 1,
        scene_count: 1,
        has_preset_bpm: preset_bpm.is_some(),
        preset_bpm: preset_bpm.unwrap_or(0.0),
    }
}

/// The chain list with two chains; `navs` is their rig navigation.
fn page(navs: Vec<ChainRigNav>) -> ProjectChainsHarness {
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
    w.set_rig_nav(ModelRc::new(VecModel::from(navs)));
    MetronomeBridge::get(&w).set_bpm(97.0);
    w.show().unwrap();
    w
}

fn rig_page() -> ProjectChainsHarness {
    page(vec![nav(true, None), nav(true, Some(140.0))])
}

fn visible(w: &ProjectChainsHarness, id: &str) -> Vec<ElementHandle> {
    ElementHandle::find_by_element_id(w, id).collect()
}

fn click(w: &ProjectChainsHarness, el: &ElementHandle) {
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

fn click_id(w: &ProjectChainsHarness, id: &str) {
    let el = visible(w, id)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("{id} not found"));
    click(w, &el);
}

/// A click on the backdrop's top-left corner: outside the panel, which sits
/// under its chip and covers the backdrop's centre.
fn click_outside_panel(w: &ProjectChainsHarness) {
    let backdrop = visible(w, "TempoOverlay::backdrop")
        .into_iter()
        .next()
        .expect("the panel's backdrop is up");
    let pos = backdrop.absolute_position();
    let at = LogicalPosition::new(pos.x + 4.0, pos.y + 4.0);
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

fn chips(w: &ProjectChainsHarness) -> Vec<ElementHandle> {
    visible(w, "TempoChip::ta")
}

fn open_panel_of(w: &ProjectChainsHarness, index: usize) {
    let chip = chips(w)
        .into_iter()
        .nth(index)
        .unwrap_or_else(|| panic!("tempo chip {index} not found"));
    click(w, &chip);
}

#[test]
fn every_chain_header_has_a_tempo_chip() {
    let w = rig_page();
    assert_eq!(chips(&w).len(), 2);
}

#[test]
fn a_chain_without_a_rig_still_has_its_tempo_chip() {
    let w = page(vec![nav(false, None), nav(false, None)]);
    assert_eq!(chips(&w).len(), 2);
}

#[test]
fn the_chip_shows_the_global_tempo() {
    let w = rig_page();
    let chip = visible(&w, "TempoChip::root").into_iter().next().unwrap();
    assert_eq!(chip.accessible_value().as_deref(), Some("97"));
}

#[test]
fn the_page_opens_with_no_tempo_panel() {
    let w = rig_page();
    assert!(!TempoPanel::get(&w).get_open());
    assert!(visible(&w, "TempoPanelView::tap-ta").is_empty());
}

#[test]
fn the_chip_opens_the_panel_of_its_own_chain() {
    let w = rig_page();
    open_panel_of(&w, 1);
    let panel = TempoPanel::get(&w);
    assert!(panel.get_open());
    assert_eq!(panel.get_chain_index(), 1);
    assert_eq!(visible(&w, "TempoPanelView::tap-ta").len(), 1);
}

#[test]
fn a_click_outside_the_panel_closes_it() {
    let w = rig_page();
    open_panel_of(&w, 0);
    click_outside_panel(&w);
    assert!(!TempoPanel::get(&w).get_open());
}

#[test]
fn tap_and_nudges_drive_the_metronome() {
    let w = rig_page();
    let taps = Rc::new(RefCell::new(0));
    let bpms = Rc::new(RefCell::new(Vec::<f32>::new()));
    let bridge = MetronomeBridge::get(&w);
    {
        let taps = taps.clone();
        bridge.on_tap(move || *taps.borrow_mut() += 1);
    }
    {
        let bpms = bpms.clone();
        bridge.on_set_bpm(move |bpm| bpms.borrow_mut().push(bpm));
    }
    open_panel_of(&w, 0);
    click_id(&w, "TempoPanelView::tap-ta");
    click_id(&w, "TempoPanelView::minus-ta");
    click_id(&w, "TempoPanelView::plus-ta");
    assert_eq!(*taps.borrow(), 1);
    assert_eq!(*bpms.borrow(), vec![96.0, 98.0]);
}

#[test]
fn save_stores_the_global_tempo_on_the_chains_preset() {
    let w = rig_page();
    let stored = Rc::new(RefCell::new(Vec::<(i32, f32)>::new()));
    {
        let stored = stored.clone();
        TempoPanel::get(&w).on_store_preset_bpm(move |ci, bpm| stored.borrow_mut().push((ci, bpm)));
    }
    open_panel_of(&w, 0);
    click_id(&w, "TempoPanelView::store-ta");
    assert_eq!(*stored.borrow(), vec![(0, 97.0)]);
}

#[test]
fn a_preset_with_its_own_tempo_shows_it_and_can_clear_it() {
    let w = rig_page();
    let cleared = Rc::new(RefCell::new(Vec::<i32>::new()));
    {
        let cleared = cleared.clone();
        TempoPanel::get(&w).on_clear_preset_bpm(move |ci| cleared.borrow_mut().push(ci));
    }
    // Chain 0's preset follows the global tempo: nothing to clear.
    open_panel_of(&w, 0);
    assert!(visible(&w, "TempoPanelView::clear-ta").is_empty());
    click_outside_panel(&w);
    // Chain 1's preset carries 140 BPM.
    open_panel_of(&w, 1);
    let value = visible(&w, "TempoPanelView::preset-value")
        .into_iter()
        .next()
        .expect("the preset tempo is shown");
    assert!(value.accessible_label().unwrap_or_default().contains("140"));
    click_id(&w, "TempoPanelView::clear-ta");
    assert_eq!(*cleared.borrow(), vec![1]);
}

#[test]
fn a_chain_without_a_rig_offers_no_preset_tempo() {
    let w = page(vec![nav(false, None), nav(false, None)]);
    open_panel_of(&w, 0);
    assert!(visible(&w, "TempoPanelView::store-ta").is_empty());
    // The global controls are still there.
    assert_eq!(visible(&w, "TempoPanelView::tap-ta").len(), 1);
}

#[test]
fn the_lock_toggle_asks_for_the_opposite_state() {
    let w = rig_page();
    let asked = Rc::new(RefCell::new(Vec::<bool>::new()));
    let bridge = MetronomeBridge::get(&w);
    {
        let asked = asked.clone();
        bridge.on_set_global_tempo_lock(move |on| asked.borrow_mut().push(on));
    }
    open_panel_of(&w, 0);
    click_id(&w, "TempoPanelView::lock-ta");
    bridge.set_global_tempo_lock(true);
    click_id(&w, "TempoPanelView::lock-ta");
    assert_eq!(*asked.borrow(), vec![true, false]);
}
