//! The wizard footer, driven by real pointer events: each button reaches its
//! callback, back is absent on the first step and skip only on optional steps.

use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};

use crate::SetupWizardHarness;

fn click(w: &impl ComponentHandle, el: &i_slint_backend_testing::ElementHandle) {
    let (pos, size) = (el.absolute_position(), el.size());
    let p = LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: p });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: p,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerReleased {
        position: p,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerExited);
}

fn harness(step: i32, skippable: bool) -> SetupWizardHarness {
    i_slint_backend_testing::init_no_event_loop();
    let h = SetupWizardHarness::new().expect("harness");
    h.set_step(step);
    h.set_step_skippable(skippable);
    h.show().expect("show");
    h
}

fn find(h: &SetupWizardHarness, id: &str) -> Vec<i_slint_backend_testing::ElementHandle> {
    i_slint_backend_testing::ElementHandle::find_by_element_id(h, &format!("SetupWizardPage::{id}"))
        .collect()
}

#[test]
fn next_reaches_its_callback() {
    let h = harness(1, false);
    click(&h, &find(&h, "next-btn")[0]);
    assert_eq!(h.get_next_count(), 1);
}

#[test]
fn back_reaches_its_callback() {
    let h = harness(2, false);
    click(&h, &find(&h, "back-btn")[0]);
    assert_eq!(h.get_back_count(), 1);
}

#[test]
fn skip_reaches_its_callback() {
    let h = harness(3, true);
    click(&h, &find(&h, "skip-btn")[0]);
    assert_eq!(h.get_skip_count(), 1);
}

#[test]
fn the_first_step_has_no_back() {
    let h = harness(0, false);
    assert!(find(&h, "back-btn").is_empty());
}

#[test]
fn a_required_step_has_no_skip() {
    let h = harness(1, false);
    assert!(find(&h, "skip-btn").is_empty());
}
