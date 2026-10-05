//! #1007 — HEADLESS proof that every chain card in the chain list carries a
//! mixer button that opens THAT chain's mixer at the bottom of its card: instantiate the
//! real `ProjectChainsPage` (through `ProjectChainsHarness`), click the button
//! and read back the `ChainMixerPanel` global. The open mixer shows the same
//! tabs as the compact view: one dual strip per endpoint, the GLOBAL fader
//! reporting through `MixerBridge` and the chain's own through
//! `ChainMixerBridge`.

use std::cell::RefCell;
use std::rc::Rc;

use adapter_gui::{
    ChainMixerBridge, ChainMixerPanel, MixerBridge, MixerStripRow, ProjectChainItem,
    ProjectChainsHarness,
};
use i_slint_backend_testing::ElementHandle;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, ModelRc, VecModel};

fn row(id: &str) -> MixerStripRow {
    MixerStripRow {
        id: id.into(),
        name: id.into(),
        detail: "IN 1".into(),
        is_input: true,
        position: 0.5,
        gain_label: "-12.0 dB".into(),
        muted: false,
        soloed: false,
        solo_silenced: false,
    }
}

fn model(rows: Vec<MixerStripRow>) -> ModelRc<MixerStripRow> {
    ModelRc::new(VecModel::from(rows))
}

/// The chain list with two chains, both running or both stopped.
fn page(enabled: bool) -> ProjectChainsHarness {
    i_slint_backend_testing::init_no_event_loop();
    let w = ProjectChainsHarness::new().unwrap();
    let chain = |title: &str| ProjectChainItem {
        title: title.into(),
        enabled,
        ..Default::default()
    };
    w.set_chains(ModelRc::new(VecModel::from(vec![
        chain("Guitar"),
        chain("Bass"),
    ])));
    w.show().unwrap();
    w
}

fn visible(w: &ProjectChainsHarness, id: &str) -> Vec<ElementHandle> {
    ElementHandle::find_by_element_id(w, id).collect()
}

fn mixer_buttons(w: &ProjectChainsHarness) -> Vec<ElementHandle> {
    visible(w, "ChainMixerButton::ta")
}

/// The chain cards' mixer sections that are open (a closed one has no height).
fn open_mixers(w: &ProjectChainsHarness) -> usize {
    ElementHandle::find_by_element_type_name(w, "ChainRowMixer")
        .filter(|el| el.size().height > 0.0)
        .count()
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

/// Click chain `index`'s mixer button.
fn open_mixer_of(w: &ProjectChainsHarness, index: usize) {
    let button = mixer_buttons(w)
        .into_iter()
        .nth(index)
        .unwrap_or_else(|| panic!("mixer button {index} not found"));
    click(w, &button);
}

#[test]
fn every_chain_card_has_a_mixer_button() {
    let w = page(true);
    assert_eq!(mixer_buttons(&w).len(), 2);
}

#[test]
fn a_stopped_chain_keeps_its_mixer_button() {
    let w = page(false);
    assert_eq!(mixer_buttons(&w).len(), 2);
}

#[test]
fn the_chain_list_opens_with_no_mixer_shown() {
    let w = page(true);
    assert!(!ChainMixerPanel::get(&w).get_open());
    assert_eq!(open_mixers(&w), 0);
}

#[test]
fn the_mixer_button_opens_the_mixer_of_its_own_chain() {
    let w = page(true);
    let opened = Rc::new(RefCell::new(Vec::<i32>::new()));
    let o = opened.clone();
    ChainMixerPanel::get(&w).on_opened(move |ci| o.borrow_mut().push(ci));
    open_mixer_of(&w, 1);
    let panel = ChainMixerPanel::get(&w);
    assert!(panel.get_open());
    assert_eq!(panel.get_chain_index(), 1);
    assert_eq!(*opened.borrow(), vec![1]);
    assert_eq!(open_mixers(&w), 1);
}

#[test]
fn the_open_mixer_pairs_each_endpoint_with_the_chains_own_fader() {
    let w = page(true);
    ChainMixerPanel::get(&w).set_global_inputs(model(vec![row("in:0@d")]));
    ChainMixerBridge::get(&w).set_inputs(model(vec![row("in:0@d")]));
    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let h = hits.clone();
    MixerBridge::get(&w).on_fader_moved(move |id, _| h.borrow_mut().push(format!("global {id}")));
    let h = hits.clone();
    ChainMixerBridge::get(&w)
        .on_chain_fader_moved(move |id, _| h.borrow_mut().push(format!("chain {id}")));
    open_mixer_of(&w, 0);
    let faders = visible(&w, "MixerStripView::fader-ta");
    assert_eq!(faders.len(), 2, "one dual strip: global + chain fader");
    click(&w, &faders[0]);
    click(&w, &faders[1]);
    assert_eq!(
        *hits.borrow(),
        vec!["global in:0@d".to_string(), "chain in:0@d".to_string()]
    );
}

#[test]
fn a_second_click_on_the_mixer_button_hides_the_mixer() {
    let w = page(true);
    open_mixer_of(&w, 0);
    open_mixer_of(&w, 0);
    assert!(!ChainMixerPanel::get(&w).get_open());
    assert_eq!(open_mixers(&w), 0);
}
