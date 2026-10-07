//! Responsibility: opens the plugin info window for one model.
//!
//! The block editor and the plugin catalog open the same window; a plugin
//! the user owns gets the button that opens the parameter editor.

use std::cell::RefCell;
use std::rc::Rc;

use project::catalog::{model_brand, model_display_name, model_type_label};
use slint::{ComponentHandle, Global};

use crate::helpers::{show_child_window, system_language};
use crate::plugin_info;
use crate::project_view::load_screenshot_image;
use crate::{AppWindow, PluginInfoWindow};

/// Build the info window of `model_id`, keep it in `slot` and show it.
pub(crate) fn open_plugin_info_window(
    window: &AppWindow,
    slot: &Rc<RefCell<Option<PluginInfoWindow>>>,
    effect_type: &str,
    model_id: &str,
) {
    let display_name = model_display_name(effect_type, model_id);
    let brand = model_brand(effect_type, model_id);
    let type_label = model_type_label(effect_type, model_id);

    let lang = system_language();
    let meta = plugin_info::plugin_metadata(&lang, model_id);

    let (screenshot_img, has_screenshot) = load_screenshot_image(effect_type, model_id);

    let info_win = match PluginInfoWindow::new() {
        Ok(w) => w,
        Err(e) => {
            log::error!("Failed to create PluginInfoWindow: {}", e);
            return;
        }
    };
    {
        crate::Locale::get(&info_win)
            .set_font_family(crate::i18n::font_for_persisted_runtime().into());
        crate::settings::appearance_followers::follow(&info_win);
    }

    info_win.set_plugin_name(display_name.into());
    info_win.set_brand(brand.into());
    info_win.set_type_label(type_label.into());
    info_win.set_description(meta.description.into());
    info_win.set_license(meta.license.into());
    info_win.set_has_homepage(!meta.homepage.is_empty());
    info_win.set_homepage(meta.homepage.clone().into());
    info_win.set_screenshot(screenshot_img);
    info_win.set_has_screenshot(has_screenshot);
    info_win.set_editable(crate::plugin_editor_link::is_editable(model_id));

    {
        let homepage = meta.homepage.clone();
        info_win.on_open_homepage(move || {
            plugin_info::open_homepage(&homepage);
        });
    }

    {
        let model_id = model_id.to_string();
        info_win.on_edit_parameters(move || crate::plugin_editor_link::open(&model_id));
    }

    {
        let win_weak = info_win.as_weak();
        info_win.on_close_window(move || {
            if let Some(w) = win_weak.upgrade() {
                let _ = w.window().hide();
            }
        });
    }

    *slot.borrow_mut() = Some(info_win);
    if let Some(w) = slot.borrow().as_ref() {
        show_child_window(window.window(), w.window());
    }
}
