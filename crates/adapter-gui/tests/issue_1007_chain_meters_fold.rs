//! #1007 — HEADLESS proof that a chain card's IN / OUT meter section opens
//! collapsed and that its header expands it: instantiate the real
//! `ProjectChainsPage` (through `ProjectChainsHarness`), click the card's
//! meters header and read the flag back from the row model the page writes.

use adapter_gui::{ProjectChainItem, ProjectChainsHarness};
use i_slint_backend_testing::ElementHandle;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Model, ModelRc, VecModel};
use std::rc::Rc;

fn page() -> (ProjectChainsHarness, Rc<VecModel<ProjectChainItem>>) {
    i_slint_backend_testing::init_no_event_loop();
    let w = ProjectChainsHarness::new().unwrap();
    let chains = Rc::new(VecModel::from(vec![ProjectChainItem {
        title: "Guitar".into(),
        ..Default::default()
    }]));
    w.set_chains(ModelRc::from(chains.clone()));
    w.show().unwrap();
    (w, chains)
}

fn meters_toggle(w: &ProjectChainsHarness) -> ElementHandle {
    ElementHandle::find_by_element_id(w, "ChainRow::meters-toggle")
        .next()
        .expect("meters toggle not found")
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
    let (_w, chains) = page();
    assert!(!expanded(&chains));
}

#[test]
fn the_meters_header_expands_the_card_and_collapses_it_again() {
    let (w, chains) = page();
    click(&w, &meters_toggle(&w));
    assert!(expanded(&chains));
    click(&w, &meters_toggle(&w));
    assert!(!expanded(&chains));
}
