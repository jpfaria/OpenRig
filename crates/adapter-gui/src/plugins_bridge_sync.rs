//! Responsibility: draws the plugin catalog view onto the window's bridge.

use std::collections::BTreeSet;

use slint::{ModelRc, SharedString, VecModel};

use crate::plugins_view::{PluginRowView, PluginsView, TypeOption};
use crate::{PluginRow, PluginsBridge, SelectOption};

/// What the catalog window shows besides the filtered list.
pub(crate) struct PluginsChrome<'a> {
    /// The plugins whose redo is running.
    pub busy: &'a BTreeSet<String>,
    pub error: &'a str,
    /// Narrows the type filter's choices.
    pub type_query: &'a str,
}

/// Publish the rows, the count and the type filter's choices.
pub(crate) fn set_plugins_view(bridge: &PluginsBridge, view: &PluginsView, chrome: &PluginsChrome) {
    let rows: Vec<PluginRow> = view
        .rows
        .iter()
        .map(|r| plugin_row(r, chrome.busy.contains(&r.plugin_id)))
        .collect();
    bridge.set_rows(ModelRc::new(VecModel::from(rows)));
    bridge.set_total(view.total as i32);
    bridge.set_error(chrome.error.into());
    let all = bridge.get_all_types_label();
    let types = type_options(&all, &view.types, chrome.type_query);
    bridge.set_types(ModelRc::new(VecModel::from(types)));
}

/// "Every type" first, then the types holding `query`, ignoring case.
pub(crate) fn type_options(all: &str, types: &[TypeOption], query: &str) -> Vec<SelectOption> {
    let query = query.trim().to_lowercase();
    std::iter::once(SelectOption {
        key: SharedString::new(),
        label: all.into(),
    })
    .chain(
        types
            .iter()
            .filter(|t| t.label.to_lowercase().contains(&query))
            .map(|t| SelectOption {
                key: t.key.as_str().into(),
                label: t.label.as_str().into(),
            }),
    )
    .collect()
}

fn plugin_row(row: &PluginRowView, busy: bool) -> PluginRow {
    PluginRow {
        plugin_id: row.plugin_id.as_str().into(),
        name: row.name.as_str().into(),
        brand: row.brand.as_str().into(),
        effect_type: row.effect_type.as_str().into(),
        type_label: row.type_label.as_str().into(),
        backend: row.backend.as_str().into(),
        arch: row.arch.as_str().into(),
        captures: row.captures as i32,
        tone3000: row.tone3000,
        editable: row.editable,
        busy,
    }
}

#[cfg(test)]
#[path = "plugins_bridge_sync_tests.rs"]
mod tests;
