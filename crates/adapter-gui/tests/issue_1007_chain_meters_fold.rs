//! #1007 — HEADLESS proof that a chain card's IN / OUT meter section opens
//! hidden and that the header's meters icon shows it, only on a running
//! chain: instantiate the real `ProjectChainsPage` (through
//! `ProjectChainsHarness`), click the icon and read the flag back from the
//! row model the page writes.

use adapter_gui::{ProjectChainItem, ProjectChainsHarness};
use i_slint_backend_testing::ElementHandle;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Model, ModelRc, VecModel};
use std::rc::Rc;

fn page(enabled: bool) -> (ProjectChainsHarness, Rc<VecModel<ProjectChainItem>>) {
    i_slint_backend_testing::init_no_event_loop();
    let w = ProjectChainsHarness::new().unwrap();
    let chains = Rc::new(VecModel::from(vec![ProjectChainItem {
        title: "Guitar".into(),
        enabled,
        ..Default::default()
    }]));
    w.set_chains(ModelRc::from(chains.clone()));
    w.show().unwrap();
    (w, chains)
}

fn meters_button(w: &ProjectChainsHarness) -> Option<ElementHandle> {
    ElementHandle::find_by_element_id(w, "ChainMetersButton::ta").next()
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

fn expanded(chains: &VecModel<ProjectChainItem>) -> bool {
    chains.row_data(0).unwrap().meters_expanded
}

#[test]
fn a_chain_card_opens_with_its_meters_collapsed() {
    let (_w, chains) = page(true);
    assert!(!expanded(&chains));
}

#[test]
fn a_stopped_chain_has_no_meters_button() {
    let (w, _chains) = page(false);
    assert!(meters_button(&w).is_none());
}

#[test]
fn the_meters_icon_shows_the_bars_and_hides_them_again() {
    let (w, chains) = page(true);
    click(&w, &meters_button(&w).expect("meters button not found"));
    assert!(expanded(&chains));
    click(&w, &meters_button(&w).unwrap());
    assert!(!expanded(&chains));
}
