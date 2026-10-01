//! #1007 — HEADLESS proof that the compact chain view carries the chain's own
//! mixer and that it is operable: instantiate the real
//! `CompactChainViewWindow`, feed its `MixerBridge` (the global strips) and
//! its `ChainMixerBridge` (the chain's own faders) and press them.
//!
//! Tabs IN | OUT | DI | LOOPER; OUT opens with the chain's MASTER. IN / OUT show one dual strip per
//! endpoint: the GLOBAL fader on the left (reports through `MixerBridge`, the
//! same bridge as the Mixer window), the chain's own fader on the right
//! (reports through `ChainMixerBridge`). No SOLO in the compact view.

use std::cell::RefCell;
use std::rc::Rc;

use adapter_gui::{ChainMixerBridge, CompactChainViewWindow, MixerBridge, MixerStripRow};
use i_slint_backend_testing::ElementHandle;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, ModelRc, VecModel};

fn row(id: &str, is_input: bool) -> MixerStripRow {
    MixerStripRow {
        id: id.into(),
        name: id.into(),
        detail: if is_input { "IN 1" } else { "OUT 1,2" }.into(),
        is_input,
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

/// The compact view as it opens: the mixer section collapsed.
fn collapsed(inputs: Vec<MixerStripRow>, outputs: Vec<MixerStripRow>) -> CompactChainViewWindow {
    i_slint_backend_testing::init_no_event_loop();
    let w = CompactChainViewWindow::new().unwrap();
    w.window().set_size(slint::LogicalSize::new(900.0, 900.0));
    let bridge = MixerBridge::get(&w);
    bridge.set_inputs(model(inputs.clone()));
    bridge.set_outputs(model(outputs.clone()));
    let chain = ChainMixerBridge::get(&w);
    chain.set_inputs(model(inputs));
    chain.set_outputs(model(outputs));
    chain.set_di(model(vec![row("di", false)]));
    chain.set_loopers(model(vec![row("looper:7", false), row("looper:9", false)]));
    chain.set_master(model(vec![row("master", false)]));
    // A running chain with its DI loop playing shows every tab.
    w.set_chain_enabled(true);
    w.set_di_loop_playing(true);
    w.show().unwrap();
    w
}

/// The mixer section's header toggle (collapse / expand).
fn toggle(w: &CompactChainViewWindow) {
    let header = ElementHandle::find_by_element_id(w, "CompactChainMixer::mixer-toggle")
        .next()
        .expect("mixer toggle not found");
    click_at(w, &header);
}

/// The compact view with its mixer section expanded.
fn window(inputs: Vec<MixerStripRow>, outputs: Vec<MixerStripRow>) -> CompactChainViewWindow {
    let w = collapsed(inputs, outputs);
    toggle(&w);
    w
}

/// Every instance of `id` on screen (a strip without SOLO/MUTE has none).
fn visible(w: &CompactChainViewWindow, id: &str) -> Vec<ElementHandle> {
    ElementHandle::find_by_element_id(w, id).collect()
}

fn click_at(w: &CompactChainViewWindow, el: &ElementHandle) {
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

/// The mixer tabs, in order: IN, OUT, DI, LOOPER. Found by element
/// id, not label, so the tests hold in any locale the bundled catalog picks.
fn tabs(w: &CompactChainViewWindow) -> Vec<ElementHandle> {
    ElementHandle::find_by_element_id(w, "ParamTabBar::tab-ta").collect()
}

fn open_tab(w: &CompactChainViewWindow, index: usize) {
    let tab = tabs(w)
        .into_iter()
        .nth(index)
        .unwrap_or_else(|| panic!("tab {index} not found"));
    click_at(w, &tab);
}

const OUT_TAB: usize = 1;
const DI_TAB: usize = 2;
const LOOPER_TAB: usize = 3;

#[test]
fn the_in_tab_is_open_first_with_one_dual_strip_per_input() {
    let w = window(
        vec![row("in:0@d", true), row("in:1@d", true)],
        vec![row("out:0,1@d", false)],
    );
    // Two inputs, two faders each (global + chain), each with its MUTE.
    assert_eq!(visible(&w, "MixerStripView::mute-ta").len(), 4);
}

#[test]
fn the_out_tab_shows_the_outputs() {
    let w = window(vec![row("in:0@d", true)], vec![row("out:0,1@d", false)]);
    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let h = hits.clone();
    MixerBridge::get(&w).on_mute_toggled(move |id| h.borrow_mut().push(id.to_string()));
    open_tab(&w, OUT_TAB);
    let mutes = visible(&w, "MixerStripView::mute-ta");
    assert_eq!(mutes.len(), 2);
    click_at(&w, &mutes[0]);
    assert_eq!(*hits.borrow(), vec!["out:0,1@d".to_string()]);
}

#[test]
fn a_chain_without_endpoints_shows_no_dual_strip() {
    let w = window(vec![], vec![]);
    assert!(visible(&w, "MixerStripView::mute-ta").is_empty());
}

#[test]
fn the_left_mute_is_global_and_the_right_mute_is_the_chains() {
    let w = window(vec![row("in:0@d", true)], vec![]);
    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let h = hits.clone();
    MixerBridge::get(&w).on_mute_toggled(move |id| h.borrow_mut().push(format!("global {id}")));
    let h = hits.clone();
    ChainMixerBridge::get(&w)
        .on_chain_mute_toggled(move |id| h.borrow_mut().push(format!("chain {id}")));
    let mutes = visible(&w, "MixerStripView::mute-ta");
    assert!(mutes[0].absolute_position().x < mutes[1].absolute_position().x);
    click_at(&w, &mutes[0]);
    click_at(&w, &mutes[1]);
    assert_eq!(
        *hits.borrow(),
        vec!["global in:0@d".to_string(), "chain in:0@d".to_string()]
    );
}

#[test]
fn the_chain_fader_reports_through_the_chain_bridge_by_strip_id() {
    let w = window(vec![row("in:0@d", true)], vec![]);
    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let h = hits.clone();
    MixerBridge::get(&w).on_fader_moved(move |id, _| h.borrow_mut().push(format!("global {id}")));
    let h = hits.clone();
    ChainMixerBridge::get(&w)
        .on_chain_fader_moved(move |id, _| h.borrow_mut().push(format!("chain {id}")));
    let faders = visible(&w, "MixerStripView::fader-ta");
    click_at(&w, &faders[1]);
    assert_eq!(*hits.borrow(), vec!["chain in:0@d".to_string()]);
}

#[test]
fn the_compact_view_has_no_solo() {
    let w = window(vec![row("in:0@d", true)], vec![row("out:0,1@d", false)]);
    assert!(visible(&w, "MixerStripView::solo-ta").is_empty());
}

#[test]
fn the_looper_tab_shows_one_fader_per_looper() {
    let w = window(vec![row("in:0@d", true)], vec![]);
    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let h = hits.clone();
    ChainMixerBridge::get(&w)
        .on_single_fader_moved(move |id, _| h.borrow_mut().push(id.to_string()));
    open_tab(&w, LOOPER_TAB);
    let faders = visible(&w, "MixerStripView::fader-ta");
    assert_eq!(faders.len(), 2);
    assert!(visible(&w, "MixerStripView::mute-ta").is_empty());
    click_at(&w, &faders[1]);
    assert_eq!(*hits.borrow(), vec!["looper:9".to_string()]);
}

#[test]
fn the_di_tab_and_an_empty_out_tab_show_one_fader_each() {
    let w = window(vec![], vec![]);
    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let h = hits.clone();
    ChainMixerBridge::get(&w)
        .on_single_fader_moved(move |id, _| h.borrow_mut().push(id.to_string()));
    // Without outputs, OUT holds just the chain's MASTER.
    for tab in [DI_TAB, OUT_TAB] {
        open_tab(&w, tab);
        let faders = visible(&w, "MixerStripView::fader-ta");
        assert_eq!(faders.len(), 1, "{tab}");
        click_at(&w, &faders[0]);
    }
    assert_eq!(*hits.borrow(), vec!["di".to_string(), "master".to_string()]);
}

#[test]
fn the_mixer_opens_collapsed_and_its_header_toggles_it() {
    let w = collapsed(vec![row("in:0@d", true)], vec![]);
    assert!(tabs(&w).is_empty());
    assert!(visible(&w, "MixerStripView::fader-ta").is_empty());
    toggle(&w);
    assert_eq!(tabs(&w).len(), 4);
    assert_eq!(visible(&w, "MixerStripView::fader-ta").len(), 2);
    toggle(&w);
    assert!(visible(&w, "MixerStripView::fader-ta").is_empty());
}

#[test]
fn master_is_the_first_fader_of_the_out_tab() {
    let w = window(vec![], vec![row("out:0,1@d", false)]);
    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let h = hits.clone();
    ChainMixerBridge::get(&w)
        .on_single_fader_moved(move |id, _| h.borrow_mut().push(id.to_string()));
    open_tab(&w, OUT_TAB);
    let faders = visible(&w, "MixerStripView::fader-ta");
    assert_eq!(faders.len(), 3);
    let leftmost = faders
        .iter()
        .min_by(|a, b| a.absolute_position().x.total_cmp(&b.absolute_position().x))
        .unwrap();
    click_at(&w, leftmost);
    assert_eq!(*hits.borrow(), vec!["master".to_string()]);
}

/// #1022: the IN / OUT meters section header, under the mixer.
fn meters_button(w: &CompactChainViewWindow) -> Option<ElementHandle> {
    ElementHandle::find_by_element_id(w, "CompactChainSections::meters-toggle").next()
}

/// Where the mixer section starts: the sections below it push it up.
fn mixer_top(w: &CompactChainViewWindow) -> f32 {
    ElementHandle::find_by_element_id(w, "CompactChainMixer::mixer-toggle")
        .next()
        .expect("mixer toggle not found")
        .absolute_position()
        .y
}

#[test]
fn the_meters_open_hidden_and_their_section_shows_them() {
    let w = collapsed(vec![row("in:0@d", true)], vec![]);
    let hidden = mixer_top(&w);
    click_at(&w, &meters_button(&w).expect("meters button not found"));
    let shown = mixer_top(&w);
    assert!(
        shown < hidden,
        "the meter rows must push the mixer up: {shown} vs {hidden}"
    );
    click_at(&w, &meters_button(&w).unwrap());
    assert_eq!(mixer_top(&w), hidden);
}

#[test]
fn a_stopped_chain_has_no_meters_section() {
    let w = collapsed(vec![row("in:0@d", true)], vec![]);
    w.set_chain_enabled(false);
    assert!(meters_button(&w).is_none());
}

#[test]
fn a_chain_without_a_playing_di_or_a_looper_shows_only_in_and_out() {
    let w = collapsed(vec![row("in:0@d", true)], vec![]);
    w.set_di_loop_playing(false);
    ChainMixerBridge::get(&w).set_loopers(model(vec![]));
    toggle(&w);
    assert_eq!(tabs(&w).len(), 2);
}

#[test]
fn without_a_playing_di_the_third_tab_is_the_looper() {
    let w = collapsed(vec![row("in:0@d", true)], vec![]);
    w.set_di_loop_playing(false);
    toggle(&w);
    assert_eq!(tabs(&w).len(), 3);
    open_tab(&w, 2);
    assert_eq!(visible(&w, "MixerStripView::fader-ta").len(), 2);
}

#[test]
fn without_loopers_the_di_tab_stays() {
    let w = collapsed(vec![row("in:0@d", true)], vec![]);
    ChainMixerBridge::get(&w).set_loopers(model(vec![]));
    toggle(&w);
    assert_eq!(tabs(&w).len(), 3);
    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let h = hits.clone();
    ChainMixerBridge::get(&w)
        .on_single_fader_moved(move |id, _| h.borrow_mut().push(id.to_string()));
    open_tab(&w, DI_TAB);
    click_at(&w, &visible(&w, "MixerStripView::fader-ta")[0]);
    assert_eq!(*hits.borrow(), vec!["di".to_string()]);
}

#[test]
fn every_fader_has_its_own_reset() {
    let w = window(vec![row("in:0@d", true)], vec![]);
    let hits = Rc::new(RefCell::new(Vec::<String>::new()));
    let h = hits.clone();
    MixerBridge::get(&w).on_fader_reset(move |id| h.borrow_mut().push(format!("global {id}")));
    let h = hits.clone();
    ChainMixerBridge::get(&w)
        .on_chain_fader_reset(move |id| h.borrow_mut().push(format!("chain {id}")));
    let h = hits.clone();
    ChainMixerBridge::get(&w)
        .on_single_fader_reset(move |id| h.borrow_mut().push(format!("single {id}")));
    let resets = visible(&w, "MixerStripView::reset-ta");
    assert_eq!(resets.len(), 2, "the global fader and the chain fader");
    click_at(&w, &resets[0]);
    click_at(&w, &resets[1]);
    open_tab(&w, LOOPER_TAB);
    let resets = visible(&w, "MixerStripView::reset-ta");
    assert_eq!(resets.len(), 2, "one per looper");
    click_at(&w, &resets[1]);
    assert_eq!(
        *hits.borrow(),
        vec![
            "global in:0@d".to_string(),
            "chain in:0@d".to_string(),
            "single looper:9".to_string()
        ]
    );
}
