//! #880 — the channel picker must be a bounded, VERTICALLY scrolling list.
//!
//! The owner's interface has 30 inputs and 30 outputs. The I/O bindings form
//! laid the channels out as one unwrapped `HorizontalLayout`, so every channel
//! past the right edge of the window was rendered off-screen and could not be
//! clicked at all — there was no wrap and no scrollbar to reach it.
//!
//! These are REAL pointer events dispatched at real element geometry through
//! the headless testing backend: a render PNG proves layout, only this proves
//! the channel is reachable (see #749/#761).

use adapter_gui::{ChannelOptionItem, IoBindingModel, IoEndpointModel, ProjectSettingsWindow};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, LogicalSize, ModelRc, VecModel};
use std::cell::Cell;
use std::rc::Rc;

const WIN_W: f32 = 1100.0;
const WIN_H: f32 = 1000.0;
const CHANNEL_COUNT: i32 = 30;

/// Every materialised channel cell, wherever it is declared. The cell moved
/// from `SectionSystemIoBindings` into the shared `ChannelPicker` component
/// when this issue was fixed; the test asks about the CELLS, not about which
/// file declares them, so it keeps working across that move.
fn channel_cells(w: &ProjectSettingsWindow) -> Vec<i_slint_backend_testing::ElementHandle> {
    ["ChannelPicker::chan-cell", "SectionSystemIoBindings::chan-cell"]
        .iter()
        .flat_map(|id| i_slint_backend_testing::ElementHandle::find_by_element_id(w, id))
        .collect()
}

fn click_element(w: &impl ComponentHandle, id: &str) -> bool {
    let Some(el) = i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).next() else {
        return false;
    };
    click_at(w, center_of(&el));
    true
}

fn center_of(el: &i_slint_backend_testing::ElementHandle) -> LogicalPosition {
    let pos = el.absolute_position();
    let sz = el.size();
    LogicalPosition::new(pos.x + sz.width / 2.0, pos.y + sz.height / 2.0)
}

fn click_at(w: &impl ComponentHandle, at: LogicalPosition) {
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

fn scroll_at(w: &impl ComponentHandle, at: LogicalPosition, delta_y: f32) {
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: at });
    win.dispatch_event(WindowEvent::PointerScrolled {
        position: at,
        delta_x: 0.0,
        delta_y,
    });
}

fn binding() -> IoBindingModel {
    IoBindingModel {
        id: "b1".into(),
        name: "B1".into(),
        inputs: ModelRc::new(VecModel::from(Vec::<IoEndpointModel>::new())),
        outputs: ModelRc::new(VecModel::from(Vec::<IoEndpointModel>::new())),
    }
}

/// A window on the I/O bindings section with a 30-channel device selected and
/// the add-input form open.
fn window_with_open_input_form() -> ProjectSettingsWindow {
    let w = ProjectSettingsWindow::new().unwrap();
    w.window().set_size(LogicalSize::new(WIN_W, WIN_H));
    w.set_io_bindings(ModelRc::new(VecModel::from(vec![binding()])));
    w.set_settings_selected_section(6);
    let chans: Vec<ChannelOptionItem> = (0..CHANNEL_COUNT)
        .map(|i| ChannelOptionItem {
            index: i,
            label: format!("{}", i + 1).into(),
            selected: false,
            available: true,
        })
        .collect();
    w.set_io_binding_channel_options(ModelRc::new(VecModel::from(chans)));
    w.show().unwrap();

    assert!(
        click_element(&w, "SectionSystemIoBindings::chev-ta"),
        "binding expand chevron not found"
    );
    assert!(
        click_element(&w, "SectionSystemIoBindings::add-input-btn"),
        "add-input button not found"
    );
    w
}

#[test]
fn thirty_channels_stay_inside_the_window_and_the_last_one_is_reachable() {
    i_slint_backend_testing::init_no_event_loop();

    let w = window_with_open_input_form();

    // ── 1. No channel cell may sit outside the window ────────────────────────
    //    The horizontal row put channel ~14 onwards past the right edge: the
    //    user could see neither the cell nor a scrollbar leading to it.
    let cells = channel_cells(&w);
    assert!(
        !cells.is_empty(),
        "no channel cell materialised — the add-input form did not open"
    );
    let outside: Vec<String> = cells
        .iter()
        .filter(|el| el.absolute_position().x + el.size().width > WIN_W)
        .map(|el| format!("x={} w={}", el.absolute_position().x, el.size().width))
        .collect();
    assert!(
        outside.is_empty(),
        "{} of {} channel cells are rendered past the window's right edge \
         ({WIN_W}px) and cannot be clicked: {outside:?} — the picker must be a \
         bounded VERTICAL list, not one unwrapped horizontal row",
        outside.len(),
        cells.len()
    );

    // ── 2. The cells stack VERTICALLY — same column, one row each ───────────
    let xs: Vec<f32> = cells.iter().map(|el| el.absolute_position().x).collect();
    let ys: Vec<f32> = cells.iter().map(|el| el.absolute_position().y).collect();
    assert!(
        xs.windows(2).all(|p| (p[0] - p[1]).abs() < 0.5),
        "channel cells sit at different x ({xs:?}) — they are laid out side by \
         side instead of stacked in one scrollable column"
    );
    assert!(
        ys.windows(2).all(|p| p[1] > p[0]),
        "channel cells do not advance downwards ({ys:?}) — the picker is not a \
         vertical list"
    );

    // ── 3. The list is bounded: it shows a window of channels, not all 30 ────
    assert!(
        cells.len() < CHANNEL_COUNT as usize,
        "all {CHANNEL_COUNT} channels are materialised at once — the picker \
         grows with the channel count instead of scrolling inside a fixed area"
    );

    // ── 4. Scrolling the list reaches the LAST channel, and it toggles ───────
    let over_list = center_of(&cells[0]);
    for _ in 0..30 {
        scroll_at(&w, over_list, -120.0);
    }

    let fired: Rc<Cell<(i32, bool)>> = Rc::new(Cell::new((-1, false)));
    let f = fired.clone();
    w.on_toggle_endpoint_channel(move |idx, sel, _mode| f.set((idx, sel)));

    let cells = channel_cells(&w);
    let last = cells
        .last()
        .expect("channel cells vanished after scrolling the list");
    click_at(&w, center_of(last));

    let (idx, sel) = fired.get();
    assert_eq!(
        idx,
        CHANNEL_COUNT - 1,
        "after scrolling to the bottom of the channel list, the last visible \
         cell must be channel {CHANNEL_COUNT} (index {}) — got index {idx}",
        CHANNEL_COUNT - 1
    );
    assert!(sel, "clicking an unselected channel must toggle it ON");
}
