//! Responsibility: maps every plugin catalog control to what it asks for.

use std::rc::Rc;

use slint::{ComponentHandle, Global};

use crate::plugins_ctx::PluginsCtx;
use crate::plugins_view::{OriginFilter, PluginsFilter};
use crate::{PluginsBridge, PluginsWindow};

/// Opens a plugin's info window from its id and effect type.
pub(crate) type OpenInfo = Rc<dyn Fn(&str, &str)>;
/// Opens the editor on a plugin, or on a new one for `None`; `Err` says why
/// the plugin cannot be edited.
pub(crate) type OpenEditor = Rc<dyn Fn(Option<&str>) -> Result<(), String>>;

/// Route every control of `window` through `ctx` and the two openers.
pub(crate) fn wire_plugins_intents(
    window: &PluginsWindow,
    ctx: &PluginsCtx,
    open_info: OpenInfo,
    open_editor: OpenEditor,
) {
    let bridge = PluginsBridge::get(window);
    let (weak, c) = (window.as_weak(), ctx.clone());
    bridge.on_filter(move || {
        if let Some(w) = weak.upgrade() {
            c.set_filter(filter_of(&PluginsBridge::get(&w)));
        }
    });
    let c = ctx.clone();
    bridge.on_type_query(move |query| c.set_type_query(&query));
    bridge.on_info(move |plugin_id, effect_type| open_info(&plugin_id, &effect_type));
    let (c, open) = (ctx.clone(), open_editor.clone());
    bridge.on_edit(move |plugin_id| {
        if let Err(why) = open(Some(&plugin_id)) {
            c.show_error(why);
        }
    });
    let c = ctx.clone();
    bridge.on_create(move || {
        if let Err(why) = open_editor(None) {
            c.show_error(why);
        }
    });
    let c = ctx.clone();
    bridge.on_redo(move |plugin_id| c.redo(&plugin_id));
    let c = ctx.clone();
    bridge.on_uninstall(move |plugin_id| c.uninstall(&plugin_id));
}

/// The filters the window's controls hold.
pub(crate) fn filter_of(bridge: &PluginsBridge) -> PluginsFilter {
    PluginsFilter {
        origin: OriginFilter::from_index(bridge.get_origin()),
        effect_type: bridge.get_type_key().to_string(),
        query: bridge.get_query().to_string(),
    }
}

#[cfg(test)]
#[path = "plugins_intents_tests.rs"]
mod tests;
