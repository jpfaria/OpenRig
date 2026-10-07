use std::cell::RefCell;
use std::rc::Rc;

use slint::Global;

use super::*;
use crate::{AppWindow, ProjectSettingsWindow, SettingsBridge, Tone3000Bridge, Tone3000Window};

#[test]
fn open_settings_in_the_browser_opens_the_settings_on_integrations() {
    i_slint_backend_testing::init_no_event_loop();
    let main = AppWindow::new().unwrap();
    let settings = ProjectSettingsWindow::new().unwrap();
    let browser = Tone3000Window::new().unwrap();
    let opened = Rc::new(RefCell::new(0));
    let o = opened.clone();
    main.on_configure_project(move || *o.borrow_mut() += 1);

    wire_open_settings(&browser, &main, &settings);
    Tone3000Bridge::get(&browser).invoke_open_settings();

    assert_eq!(*opened.borrow(), 1);
    assert_eq!(
        settings.get_settings_selected_section(),
        INTEGRATIONS_SECTION
    );
}

#[test]
fn the_key_link_opens_the_tone3000_settings_page_from_both_settings_surfaces() {
    i_slint_backend_testing::init_no_event_loop();
    let main = AppWindow::new().unwrap();
    let settings = ProjectSettingsWindow::new().unwrap();
    let urls = Rc::new(RefCell::new(Vec::<String>::new()));
    let u = urls.clone();

    wire_keys_page(
        &main,
        &settings,
        Rc::new(move |url: &str| u.borrow_mut().push(url.to_owned())),
    );
    SettingsBridge::get(&main).invoke_open_tone3000_keys_page();
    SettingsBridge::get(&settings).invoke_open_tone3000_keys_page();

    assert_eq!(
        *urls.borrow(),
        vec![KEYS_PAGE.to_owned(), KEYS_PAGE.to_owned()]
    );
    assert_eq!(KEYS_PAGE, "https://www.tone3000.com/settings");
}
