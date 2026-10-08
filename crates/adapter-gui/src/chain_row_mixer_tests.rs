//! Responsibility: proves a chain card's mixer button opens that chain's mixer inside the card
//!
//! The chain list draws a chain's mixer the way it draws its IN/OUT meters: at
//! the bottom of that chain's card, which grows to hold it. A second click on
//! the same button closes it, and only one chain's mixer is open at a time.
//! Driven through real pointer events on the chain list page. A tab of that
//! mixer takes a click even while the meter poll rewrites the chain's row
//! between the press and the release, as it does on every tick of a running
//! chain (#1099).

use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, LogicalSize, Model, ModelRc, VecModel};

use crate::{ChainMixerPanel, ProjectChainItem, ProjectChainsHarness};

/// One element on screen: top and height, in window coordinates.
#[derive(Clone, Copy, Debug)]
struct Span {
    y: f32,
    height: f32,
}

fn span(el: &ElementHandle) -> Span {
    Span {
        y: el.absolute_position().y,
        height: el.size().height,
    }
}

fn list() -> ProjectChainsHarness {
    i_slint_backend_testing::init_no_event_loop();
    let h = ProjectChainsHarness::new().expect("chain list harness");
    let chains: Vec<ProjectChainItem> = ["Guitar", "Bass"]
        .into_iter()
        .map(|title| ProjectChainItem {
            title: title.into(),
            ..Default::default()
        })
        .collect();
    h.set_chains(ModelRc::new(VecModel::from(chains)));
    h.show().expect("show");
    // Tall enough that both cards stay on screen with a mixer open: the list
    // only draws the cards in view.
    h.window().set_size(LogicalSize::new(1100.0, 1600.0));
    h
}

/// Every element of `type_name` on screen, top to bottom.
fn on_screen(h: &ProjectChainsHarness, type_name: &str) -> Vec<ElementHandle> {
    let mut els: Vec<ElementHandle> =
        ElementHandle::find_by_element_type_name(h, type_name).collect();
    els.sort_by(|a, b| a.absolute_position().y.total_cmp(&b.absolute_position().y));
    els
}

fn cards(h: &ProjectChainsHarness) -> Vec<Span> {
    on_screen(h, "ChainRow").iter().map(span).collect()
}

fn mixers(h: &ProjectChainsHarness) -> Vec<Span> {
    on_screen(h, "ChainRowMixer")
        .iter()
        .filter(|el| el.size().height > 0.0)
        .map(span)
        .collect()
}

/// Clicks the mixer button in the header of the chain card at `chain`.
fn click_mixer_button(h: &ProjectChainsHarness, chain: usize) {
    let buttons = on_screen(h, "ChainMixerButton");
    let el = buttons.get(chain).expect("a mixer button per chain card");
    let (pos, size) = (el.absolute_position(), el.size());
    let at = LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
    let win = h.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: at });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: at,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerReleased {
        position: at,
        button: PointerEventButton::Left,
    });
}

fn inside(inner: Span, outer: Span) -> bool {
    inner.y >= outer.y && inner.y + inner.height <= outer.y + outer.height + 0.5
}

#[test]
fn the_mixer_button_opens_the_mixer_at_the_bottom_of_its_card() {
    let h = list();
    let before = cards(&h);
    assert_eq!(before.len(), 2, "sanity: both chain cards are on screen");
    assert!(mixers(&h).is_empty(), "no mixer is open before a click");

    click_mixer_button(&h, 0);

    let after = cards(&h);
    let open = mixers(&h);
    assert_eq!(open.len(), 1, "one mixer opens, got {open:?}");
    assert!(
        inside(open[0], after[0]),
        "the mixer must sit inside the first card: mixer {:?}, card {:?}",
        open[0],
        after[0]
    );
    assert!(
        (open[0].y + open[0].height - (after[0].y + after[0].height)).abs() < 0.5,
        "the mixer must close the card at its bottom: mixer {:?}, card {:?}",
        open[0],
        after[0]
    );
    assert!(
        after[0].height >= before[0].height + open[0].height - 0.5,
        "the card grows to hold the mixer: {:?} -> {:?}",
        before[0],
        after[0]
    );
    assert!(
        (after[1].height - before[1].height).abs() < 0.5,
        "the other chain's card keeps its size"
    );
    assert!(
        on_screen(&h, "ChainMixerOverlay").is_empty(),
        "the chain list no longer covers itself with a mixer modal"
    );
}

#[test]
fn a_second_click_on_the_same_button_closes_the_mixer() {
    let h = list();
    let before = cards(&h);

    click_mixer_button(&h, 0);
    assert_eq!(mixers(&h).len(), 1, "sanity: the first click opens it");
    click_mixer_button(&h, 0);

    assert!(mixers(&h).is_empty(), "the second click closes the mixer");
    assert!(!ChainMixerPanel::get(&h).get_open());
    assert!(
        (cards(&h)[0].height - before[0].height).abs() < 0.5,
        "the card goes back to its size"
    );
}

#[test]
fn opening_another_chains_mixer_moves_it_to_that_card() {
    let h = list();
    click_mixer_button(&h, 0);
    click_mixer_button(&h, 1);

    let after = cards(&h);
    let open = mixers(&h);
    assert_eq!(open.len(), 1, "one chain's mixer at a time, got {open:?}");
    assert!(
        inside(open[0], after[1]),
        "the mixer moves to the second card: mixer {:?}, card {:?}",
        open[0],
        after[1]
    );
    assert_eq!(ChainMixerPanel::get(&h).get_chain_index(), 1);
}

fn centre(el: &ElementHandle) -> LogicalPosition {
    let (pos, size) = (el.absolute_position(), el.size());
    LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0)
}

/// The open mixer's tab buttons, left to right: IN, OUT, ...
fn mixer_tabs(h: &ProjectChainsHarness) -> Vec<ElementHandle> {
    let bar = on_screen(h, "ParamTabBar")
        .into_iter()
        .next()
        .expect("the open mixer's tab bar");
    let mut tabs = bar
        .query_descendants()
        .match_accessible_role(AccessibleRole::Button)
        .find_all();
    tabs.sort_by(|a, b| a.absolute_position().x.total_cmp(&b.absolute_position().x));
    tabs
}

/// The words the open mixer shows below its tabs (the empty-tab text).
fn mixer_texts(h: &ProjectChainsHarness) -> Vec<String> {
    let mixer = on_screen(h, "ChainMixerTabs")
        .into_iter()
        .next()
        .expect("the open mixer");
    mixer
        .query_descendants()
        .match_accessible_role(AccessibleRole::Text)
        .find_all()
        .into_iter()
        .filter_map(|el| el.accessible_label().map(|l| l.to_string()))
        .collect()
}

/// The meter poll's tick on a running chain: the chain's row rewritten with
/// fresh meter levels.
fn meter_tick(h: &ProjectChainsHarness, chain: usize) {
    let chains = h.get_chains();
    let mut row = chains.row_data(chain).expect("the chain's row");
    row.meter_out_dbfs += 0.5;
    chains.set_row_data(chain, row);
}

#[test]
fn the_out_tab_takes_a_click_while_the_running_chain_rewrites_its_row() {
    let h = list();
    click_mixer_button(&h, 0);
    let on_in = mixer_texts(&h);

    let out = centre(&mixer_tabs(&h)[1]);
    let win = h.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: out });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: out,
        button: PointerEventButton::Left,
    });
    meter_tick(&h, 0);
    win.dispatch_event(WindowEvent::PointerReleased {
        position: out,
        button: PointerEventButton::Left,
    });

    assert_ne!(
        mixer_texts(&h),
        on_in,
        "the click on OUT must switch the mixer away from IN"
    );
}
