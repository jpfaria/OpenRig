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
    new_window_sized(bindings, 1100.0, 1000.0)
}

/// Like `new_window` but with an explicit window size, so a short window can be
/// used to force the section content to overflow (scroll test).
fn new_window_sized(bindings: Vec<IoBindingModel>, w_px: f32, h_px: f32) -> ProjectSettingsWindow {
    let w = ProjectSettingsWindow::new().unwrap();
    w.window().set_size(LogicalSize::new(w_px, h_px));
    w.set_io_bindings(ModelRc::new(VecModel::from(bindings)));
    // Navigate to the I/O bindings section (index 6) so SectionSystemIoBindings
    // is materialised; otherwise the window shows the default (audio) section.
    w.set_settings_selected_section(6);
    w.show().unwrap();
    w
}

fn empty_binding() -> IoBindingModel {
    binding_named("b1", "B1")
}

fn binding_named(id: &str, name: &str) -> IoBindingModel {
    IoBindingModel {
        id: id.into(),
        name: name.into(),
        inputs: ModelRc::new(VecModel::from(Vec::<IoEndpointModel>::new())),
        outputs: ModelRc::new(VecModel::from(Vec::<IoEndpointModel>::new())),
    }
}

/// Absolute Y of the Nth (0-based) materialised element with `id`, if present.
fn nth_abs_y(w: &ProjectSettingsWindow, id: &str, n: usize) -> Option<f32> {
    i_slint_backend_testing::ElementHandle::find_by_element_id(w, id)
        .nth(n)
        .map(|el| el.absolute_position().y as f32)
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

    // ── 3. Rename FLOW: pencil → editor with a confirm button; pencil &
    //       trash disappear; confirm commits. (Also: no focus-recursion crash.)
    {
        let w = new_window(vec![empty_binding()]);

        // Resting state: pencil + trash are present.
        assert!(exists(&w, "SectionSystemIoBindings::rename-btn"), "pencil missing at rest");
        assert!(exists(&w, "SectionSystemIoBindings::delete-btn"), "trash missing at rest");

        let fired = Rc::new(Cell::new(false));
        let f = fired.clone();
        w.on_rename_io_binding(move |_id, _name| f.set(true));

        assert!(
            click_element(&w, "SectionSystemIoBindings::rename-btn"),
            "rename pencil not clickable"
        );

        // Editing state: the inline editor appears (no crash) and the
        // pencil/trash give way to a confirm action.
        assert!(
            exists(&w, "SectionSystemIoBindings::name-input"),
            "inline rename TextInput did not appear"
        );
        assert!(
            !exists(&w, "SectionSystemIoBindings::rename-btn"),
            "pencil still visible while editing — edit/delete must give way to confirm"
        );
        assert!(
            !exists(&w, "SectionSystemIoBindings::delete-btn"),
            "trash still visible while editing — edit/delete must give way to confirm"
        );
        assert!(
            exists(&w, "SectionSystemIoBindings::confirm-rename-btn"),
            "no confirm button while editing the name — nothing commits the edit"
        );

        // Confirm commits the rename and leaves edit mode.
        assert!(
            click_element(&w, "SectionSystemIoBindings::confirm-rename-btn"),
            "confirm button not clickable"
        );
        assert!(fired.get(), "clicking confirm did not commit the rename");
        assert!(
            exists(&w, "SectionSystemIoBindings::rename-btn"),
            "pencil did not come back after confirming"
        );
    }

    // ── 4. SCROLL: with 2 bindings in a SHORT window the content overflows the
    //       settings panel; a wheel scroll must move the lower card up. The
    //       panel's ScrollView only scrolls when its conditional section
    //       children are wrapped in a single layout — otherwise the viewport
    //       never auto-sizes and the second card is unreachable.
    {
        let w = new_window_sized(
            vec![binding_named("b1", "First"), binding_named("b2", "Second")],
            1100.0,
            420.0,
        );

        // First card is on-screen; the second overflows the short panel and is
        // not materialised in the rendered tree (off-viewport cull) — proving
        // the content is taller than the panel.
        let y0 = nth_abs_y(&w, "SectionSystemIoBindings::rename-btn", 0)
            .expect("first binding's pencil must be on-screen");
        assert!(
            nth_abs_y(&w, "SectionSystemIoBindings::rename-btn", 1).is_none(),
            "second binding should start BELOW the viewport (content overflows)"
        );

        // Wheel down over the content area: the (overflowing) content must move
        // up. Before the fix the ScrollView viewport was pinned to the panel
        // height (no scrollable extent) so the content never moved.
        let pos = LogicalPosition::new(550.0, 200.0);
        let win = w.window();
        win.dispatch_event(WindowEvent::PointerMoved { position: pos });
        win.dispatch_event(WindowEvent::PointerScrolled {
            position: pos,
            delta_x: 0.0,
            delta_y: -100.0,
        });
        let y1 = nth_abs_y(&w, "SectionSystemIoBindings::rename-btn", 0)
            .expect("first binding's pencil still on-screen after a small scroll");
        assert!(
            y1 < y0 - 50.0,
            "scrolling did not move the content up (y0={y0}, y1={y1}) — \
             the settings ScrollView is not scrolling (no scrollbar)"
        );
    }
}

/// True if an element with `id` is currently materialised in `w`.
fn exists(w: &ProjectSettingsWindow, id: &str) -> bool {
    i_slint_backend_testing::ElementHandle::find_by_element_id(w, id)
        .next()
        .is_some()
}
