//! #398 — the project settings pages follow the approved mockup: the I/O
//! bindings page carries "New binding" on its title row, and the metadata
//! page's file field is as wide as the name field above it. A binding made
//! with "New binding" scrolls into view, open, however long the list is.

use adapter_gui::{IoBindingModel, ProjectSettingsWindow};
use i_slint_backend_testing::ElementHandle;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, LogicalSize, Model, ModelRc, VecModel};
use std::cell::Cell;
use std::rc::Rc;

fn window_on(section: i32) -> ProjectSettingsWindow {
    let w = ProjectSettingsWindow::new().unwrap();
    w.window().set_size(LogicalSize::new(1100.0, 700.0));
    w.set_settings_selected_section(section);
    w.show().unwrap();
    w
}

fn element(w: &ProjectSettingsWindow, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(w, id)
        .next()
        .unwrap_or_else(|| panic!("{id} is on screen"))
}

fn click(w: &ProjectSettingsWindow, el: &ElementHandle) {
    let p = el.absolute_position();
    let s = el.size();
    let at = LogicalPosition::new(p.x + s.width / 2.0, p.y + s.height / 2.0);
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
}

#[test]
fn new_binding_sits_on_the_title_row_and_still_creates_one() {
    i_slint_backend_testing::init_no_event_loop();
    let w = window_on(6);
    let title = element(&w, "SettingsPage::section-title");
    let button = element(&w, "SettingsPage::new-binding-btn");
    let t = title.absolute_position().y;
    let mid = button.absolute_position().y + button.size().height / 2.0;
    assert!(
        mid > t && mid < t + title.size().height,
        "the button's middle ({mid}) is on the title row ({t} .. {})",
        t + title.size().height
    );
    assert!(
        button.absolute_position().x > title.absolute_position().x + 300.0,
        "the button sits at the right end of the row"
    );

    let fired = Rc::new(Cell::new(false));
    let f = fired.clone();
    adapter_gui::SettingsBridge::get(&w).on_create_io_binding(move |_| {
        f.set(true);
        slint::SharedString::new()
    });
    click(&w, &button);
    assert!(fired.get(), "the button still creates a binding");
}

#[test]
fn the_file_field_is_as_wide_as_the_name_field() {
    i_slint_backend_testing::init_no_event_loop();
    let w = window_on(4);
    let name = element(&w, "SectionProjectMeta::name-edit").size().width;
    let file = element(&w, "SectionProjectMeta::file-field").size().width;
    assert!(
        (name - file).abs() < 0.5,
        "name {name} and file {file} line up"
    );
}

fn binding(id: &str) -> IoBindingModel {
    IoBindingModel {
        id: id.into(),
        name: id.to_uppercase().into(),
        inputs: ModelRc::default(),
        outputs: ModelRc::default(),
    }
}

#[test]
fn a_new_binding_at_the_end_of_a_long_list_scrolls_into_view() {
    i_slint_backend_testing::init_no_event_loop();
    let w = window_on(6);
    let list = Rc::new(VecModel::from(
        (0..12)
            .map(|i| binding(&format!("b{i}")))
            .collect::<Vec<_>>(),
    ));
    let bridge = adapter_gui::SettingsBridge::get(&w);
    bridge.set_io_bindings(ModelRc::from(list.clone()));
    let l = list.clone();
    bridge.on_create_io_binding(move |_| {
        l.push(binding("fresh"));
        "fresh".into()
    });

    click(&w, &element(&w, "SettingsPage::new-binding-btn"));
    assert_eq!(list.row_count(), 13);

    let pane = element(&w, "SettingsPage::section-scroll");
    let top = pane.absolute_position().y;
    let bottom = top + pane.size().height;
    let fresh = ElementHandle::find_by_accessible_label(&w, "FRESH")
        .next()
        .expect("the new binding is on screen");
    let y = fresh.absolute_position().y;
    assert!(
        y >= top && y + fresh.size().height <= bottom,
        "the new binding ({y}) is inside the visible pane ({top} .. {bottom})"
    );
}
