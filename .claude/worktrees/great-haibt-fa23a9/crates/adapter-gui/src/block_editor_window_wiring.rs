//! Wiring for the standalone `BlockEditorWindow` callbacks.
//!
//! Owns the 13 callbacks registered on `BlockEditorWindow`:
//!
//! - 11 simple proxy-forwarders to the main `AppWindow::invoke_*` methods
//!   (drawer open/close/save/delete, parameter updates, file picker,
//!   VST3 editor opener).
//! - `on_show_plugin_info` — instantiates a `PluginInfoWindow` populated with
//!   metadata from `project::catalog` + `crate::plugin_info`, including
//!   homepage link and screenshot, then shows it as a child window.
//!
//! Stays out of `lib.rs` so changes to the standalone editor's UI bindings
//! don't collide with other features in parallel branches.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use project::catalog::{model_brand, model_display_name, model_type_label};

use crate::helpers::show_child_window;
use crate::plugin_info;
use crate::project_view::load_screenshot_image;
use crate::system_language;
use crate::{AppWindow, BlockEditorWindow, PluginInfoWindow};

pub(crate) struct BlockEditorWindowCtx {
    pub plugin_info_window: Rc<RefCell<Option<PluginInfoWindow>>>,
}

pub(crate) fn wire(
    window: &AppWindow,
    block_editor_window: &BlockEditorWindow,
    ctx: BlockEditorWindowCtx,
) {
    let BlockEditorWindowCtx { plugin_info_window } = ctx;

    // Drawer model picker — forwarder.
    {
        let weak_window = window.as_weak();
        block_editor_window.on_choose_block_model(move |index| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_choose_block_model(index);
            }
        });
    }
    // Drawer close — forwarder.
    {
        let weak_window = window.as_weak();
        block_editor_window.on_close_block_drawer(move || {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_close_block_drawer();
            }
        });
    }
    // Drawer save — forwarder.
    {
        let weak_window = window.as_weak();
        block_editor_window.on_save_block_drawer(move || {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_save_block_drawer();
            }
        });
    }
    // Drawer delete — forwarder.
    {
        let weak_window = window.as_weak();
        block_editor_window.on_delete_block_drawer(move || {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_delete_block_drawer();
            }
        });
    }
    // Plugin info popup — opens a child PluginInfoWindow with metadata.
    {
        let weak_window = window.as_weak();
        let plugin_info_window = plugin_info_window.clone();
        block_editor_window.on_show_plugin_info(move |effect_type, model_id| {
            let Some(window) = weak_window.upgrade() else {
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

            let info_win = match PluginInfoWindow::new() {
                Ok(w) => w,
                Err(e) => {
                    log::error!("Failed to create PluginInfoWindow: {}", e);
                    return;
                }
            };
            {
                use slint::Global;
                crate::Locale::get(&info_win)
                    .set_font_family(crate::i18n::font_for_persisted_runtime().into());
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

            {
                let homepage = meta.homepage.clone();
                info_win.on_open_homepage(move || {
                    plugin_info::open_homepage(&homepage);
                });
            }

            {
                let win_weak = info_win.as_weak();
                info_win.on_close_window(move || {
                    if let Some(w) = win_weak.upgrade() {
                        let _ = w.window().hide();
                    }
                });
            }

            *plugin_info_window.borrow_mut() = Some(info_win);
            if let Some(w) = plugin_info_window.borrow().as_ref() {
                show_child_window(window.window(), w.window());
            }
        });
    }
    // Drawer enable toggle — forwarder.
    {
        let weak_window = window.as_weak();
        block_editor_window.on_toggle_block_drawer_enabled(move || {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_toggle_block_drawer_enabled();
            }
        });
    }
    // Parameter updates — forwarders.
    {
        let weak_window = window.as_weak();
        block_editor_window.on_update_block_parameter_text(move |path, value| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_update_block_parameter_text(path, value);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        block_editor_window.on_update_block_parameter_number(move |path, value| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_update_block_parameter_number(path, value);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        block_editor_window.on_update_block_parameter_number_text(move |path, value| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_update_block_parameter_number_text(path, value);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        block_editor_window.on_update_block_parameter_bool(move |path, value| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_update_block_parameter_bool(path, value);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        block_editor_window.on_select_block_parameter_option(move |path, index| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_select_block_parameter_option(path, index);
            }
        });
    }
    // File picker — forwarder.
    {
        let weak_window = window.as_weak();
        block_editor_window.on_pick_block_parameter_file(move |path| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_pick_block_parameter_file(path);
            }
        });
    }
    // VST3 editor opener — forwarder.
    {
        let weak_window = window.as_weak();
        block_editor_window.on_open_vst3_editor(move |model_id| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_open_vst3_editor(model_id);
            }
        });
    }
}
