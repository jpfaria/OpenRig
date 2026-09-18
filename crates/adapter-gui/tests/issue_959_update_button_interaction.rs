//! #959 — HEADLESS proof that the launcher's version label turns into a
//! clickable update button only when a newer release is offered, and that a
//! real pointer click on it fires `AppUpdate.install-update`.

use adapter_gui::{AppUpdate, AppWindow};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition};
use std::cell::Cell;
use std::rc::Rc;

const BUTTON: &str = "UpdateButton::touch";

fn count_id(w: &impl ComponentHandle, id: &str) -> usize {
    i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).count()
}

fn click_id(w: &impl ComponentHandle, id: &str) -> bool {
    let Some(el) = i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).next() else {
        return false;
    };
    let pos = el.absolute_position();
    let sz = el.size();
    let c = LogicalPosition::new(pos.x + sz.width / 2.0, pos.y + sz.height / 2.0);
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: c });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: c,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerReleased {
        position: c,
        button: PointerEventButton::Left,
    });
    true
}

fn launcher() -> AppWindow {
    i_slint_backend_testing::init_no_event_loop();
    let w = AppWindow::new().unwrap();
    w.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    w.set_app_version("0.4.5".into());
    w.set_show_project_launcher(true);
    w
}

#[test]
fn version_label_has_no_update_button_without_an_offer() {
    let w = launcher();
    assert_eq!(count_id(&w, BUTTON), 0);
}

#[test]
fn clicking_the_offered_update_fires_install_update() {
    let w = launcher();
    let fired = Rc::new(Cell::new(0));
    let f = fired.clone();
    AppUpdate::get(&w).on_install_update(move || f.set(f.get() + 1));

    AppUpdate::get(&w).set_latest_version("0.4.6".into());

    assert_eq!(count_id(&w, BUTTON), 1, "update button must appear");
    assert!(click_id(&w, BUTTON));
    assert_eq!(fired.get(), 1, "a click must request the install");
}
