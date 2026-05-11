//! Inline plugin info overlay wiring on the main `AppWindow`.
//!
//! In fullscreen / touch modes the secondary `PluginInfoWindow` does not
//! surface (Orange Pi / kiosk), so the AppWindow exposes its own inline
//! overlay driven by `show_plugin_info` + a flat set of `plugin_info_*`
//! properties. This module wires the three callbacks that drive it:
//!
//! * `on_show_plugin_info(effect_type, model_id)` — populates the metadata
//!   properties from the catalog + plugin_info store, loads the screenshot,
//!   then flips `show_plugin_info` to `true`.
//! * `on_close_plugin_info` — flips it back to `false`.
//! * `on_open_plugin_info_homepage` — opens the homepage stored at the last
//!   `show` invocation in the system browser.
//!
//! See issue #307. Wired once from `desktop_app::run_desktop_app`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use project::catalog::{model_brand, model_display_name, model_type_label};

use crate::plugin_info;
use crate::project_view::load_screenshot_image;
use crate::runtime_lifecycle::system_language;
use crate::{AppWindow, PluginInfoData};

pub(crate) fn wire(window: &AppWindow) {
    let homepage_store: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

    {
        let weak = window.as_weak();
        let homepage_store = homepage_store.clone();
        window.on_show_plugin_info(move |effect_type, model_id| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let effect_type = effect_type.to_string();
            let model_id = model_id.to_string();

            let display_name = model_display_name(&effect_type, &model_id);
            let brand = model_brand(&effect_type, &model_id);
            let type_label = model_type_label(&effect_type, &model_id);
            let lang = system_language();
            let meta = plugin_info::plugin_metadata(&lang, &model_id);
            let (screenshot_img, has_screenshot) = load_screenshot_image(&effect_type, &model_id);

            *homepage_store.borrow_mut() = meta.homepage.clone();

            window.set_plugin_info_data(PluginInfoData {
                screenshot: screenshot_img,
                has_screenshot,
                plugin_name: display_name.into(),
                brand: brand.into(),
                type_label: type_label.into(),
                description: meta.description.into(),
                license: meta.license.into(),
                homepage: meta.homepage.clone().into(),
                has_homepage: !meta.homepage.is_empty(),
            });
            window.set_plugin_info_visible(true);
        });
    }

    {
        let weak = window.as_weak();
        window.on_close_plugin_info(move || {
            if let Some(window) = weak.upgrade() {
                window.set_plugin_info_visible(false);
            }
        });
    }

    {
        let homepage_store = homepage_store.clone();
        window.on_open_plugin_info_homepage(move || {
            let homepage = homepage_store.borrow().clone();
            if !homepage.is_empty() {
                plugin_info::open_homepage(&homepage);
            }
        });
    }
}
