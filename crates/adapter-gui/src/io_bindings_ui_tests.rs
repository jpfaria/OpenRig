//! #716 Slint interaction tests for the I/O bindings settings screen.
//!
//! These instantiate the REAL `ProjectSettingsWindow` headlessly (via the
//! slint testing backend) and dispatch REAL pointer events at element
//! geometry. They catch `.slint` structural bugs that pure `WireCtx` tests
//! (no AppWindow, per LAW 2) cannot see — e.g. a `TouchArea` that does not
//! cover its cell, or a callback that never reaches the window. This is the
//! gap that let "green" unit tests ship a screen where you couldn't click a
//! channel, create a binding, or open the rename field without crashing.
//!
//! Everything runs in ONE test function on a single thread: the slint testing
//! backend is per-thread, so a second `#[test]` on another thread would fall
//! back to the real (winit) backend and fail.

use crate::{ChannelOptionItem, IoBindingModel, IoEndpointModel, ProjectSettingsWindow};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, LogicalSize, ModelRc, VecModel};
use std::cell::Cell;
use std::rc::Rc;

/// Dispatch a left press+release at the centre of the element with `id`.
/// Returns false if no such element is currently materialised.
fn click_element(w: &ProjectSettingsWindow, id: &str) -> bool {
    let Some(el) = i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).next() else {
        return false;
    };
    let pos = el.absolute_position();
    let sz = el.size();
    let center = LogicalPosition::new(pos.x + sz.width / 2.0, pos.y + sz.height / 2.0);
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: center });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: center,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerReleased {
        position: center,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerExited);
    true
}

fn new_window(bindings: Vec<IoBindingModel>) -> ProjectSettingsWindow {
    let w = ProjectSettingsWindow::new().unwrap();
    w.window().set_size(LogicalSize::new(1100.0, 1000.0));
    w.set_io_bindings(ModelRc::new(VecModel::from(bindings)));
    // Navigate to the I/O bindings section (index 6) so SectionSystemIoBindings
    // is materialised; otherwise the window shows the default (audio) section.
    w.set_settings_selected_section(6);
    w.show().unwrap();
    w
}

fn empty_binding() -> IoBindingModel {
    IoBindingModel {
        id: "b1".into(),
        name: "B1".into(),
        inputs: ModelRc::new(VecModel::from(Vec::<IoEndpointModel>::new())),
        outputs: ModelRc::new(VecModel::from(Vec::<IoEndpointModel>::new())),
    }
}

#[test]
fn io_bindings_ui_interactions() {
    i_slint_backend_testing::init_no_event_loop();

    // ── 1. "Nova ligação" button is hittable and fires create-io-binding ──
    {
        let w = new_window(vec![]);
        let fired = Rc::new(Cell::new(false));
        let f = fired.clone();
        w.on_create_io_binding(move |_name| {
            f.set(true);
            slint::SharedString::new()
        });
        assert!(
            click_element(&w, "SectionSystemIoBindings::new-binding-btn"),
            "new-binding button element not found"
        );
        assert!(
            fired.get(),
            "clicking 'Nova ligação' did not fire create-io-binding — not hittable"
        );
    }

    // ── 2. Clicking a channel cell fires toggle-endpoint-channel ──
    {
        let w = new_window(vec![empty_binding()]);
        let chans = vec![
            ChannelOptionItem { index: 0, label: "1".into(), selected: false, available: true },
            ChannelOptionItem { index: 1, label: "2".into(), selected: false, available: true },
        ];
        w.set_io_binding_channel_options(ModelRc::new(VecModel::from(chans)));

        // Open the add-input form so the channel cells materialise.
        assert!(
            click_element(&w, "SectionSystemIoBindings::add-input-btn"),
            "add-input button element not found"
        );

        let fired = Rc::new(Cell::new(false));
        let f = fired.clone();
        w.on_toggle_endpoint_channel(move |_idx, _sel, _mode| f.set(true));

        assert!(
            click_element(&w, "SectionSystemIoBindings::chan-cell"),
            "channel cell not found — add-endpoint form did not open"
        );
        assert!(
            fired.get(),
            "clicking the channel cell did not fire toggle-endpoint-channel — \
             the TouchArea does not cover the cell (radio/checkbox unclickable)"
        );
    }

    // ── 3. Opening the inline rename field must not crash ──
    {
        let w = new_window(vec![empty_binding()]);
        assert!(
            click_element(&w, "SectionSystemIoBindings::rename-btn"),
            "rename pencil element not found"
        );
        // Materialise the inline editor (and its auto-focus). A focus() that
        // runs while the conditional is still being laid out recurses on
        // item_geometry and aborts the process; reaching this assertion proves
        // it does not.
        let found = i_slint_backend_testing::ElementHandle::find_by_element_id(
            &w,
            "SectionSystemIoBindings::name-input",
        )
        .next()
        .is_some();
        assert!(found, "inline rename TextInput did not appear after clicking the pencil");
    }
}
