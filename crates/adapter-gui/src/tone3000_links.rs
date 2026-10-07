//! Responsibility: routes the TONE3000 "get a key" links to where the key is made or set.
//!
//! The browser's "Open settings" lands on the Integrations section, where the
//! Secret Key field lives; the settings card's link opens the TONE3000 page
//! that creates the key (#879).

use std::rc::Rc;

use slint::{ComponentHandle, Global};

use crate::{AppWindow, ProjectSettingsWindow, SettingsBridge, Tone3000Bridge, Tone3000Window};

/// The TONE3000 page that shows and creates the user's API keys.
pub(crate) const KEYS_PAGE: &str = "https://www.tone3000.com/settings";

/// The settings sidebar index of System > Integrations (`pages/settings.slint`).
pub(crate) const INTEGRATIONS_SECTION: i32 = 5;

/// "Open settings" in the browser without a key: the settings, on Integrations.
pub(crate) fn wire_open_settings(
    browser: &Tone3000Window,
    main: &AppWindow,
    settings: &ProjectSettingsWindow,
) {
    let (main, settings) = (main.as_weak(), settings.as_weak());
    Tone3000Bridge::get(browser).on_open_settings(move || {
        let (Some(main), Some(settings)) = (main.upgrade(), settings.upgrade()) else {
            return;
        };
        settings.set_settings_selected_section(INTEGRATIONS_SECTION);
        main.invoke_configure_project();
    });
}

/// The key card's link, on both settings surfaces, opens `KEYS_PAGE` through
/// `open` (the browser in the app, a recorder in tests).
pub(crate) fn wire_keys_page(
    main: &AppWindow,
    settings: &ProjectSettingsWindow,
    open: Rc<dyn Fn(&str)>,
) {
    let for_main = open.clone();
    SettingsBridge::get(main).on_open_tone3000_keys_page(move || for_main(KEYS_PAGE));
    SettingsBridge::get(settings).on_open_tone3000_keys_page(move || open(KEYS_PAGE));
}

#[cfg(test)]
#[path = "tone3000_links_tests.rs"]
mod tests;
