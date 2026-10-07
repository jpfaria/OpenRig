//! Responsibility: turns the plugin library into the rows the catalog window draws.
//!
//! Pure: the user's plugins plus the filters in, rows out. Plugins the app
//! ships never reach it, so every row can be uninstalled.

use application::block_factory::block_type_to_effect_type;
use application::plugin_library::entries::PluginLibraryEntry;
use application::plugin_library::PluginOrigin;
use plugin_loader::manifest::NamArchitecture;
use project::catalog::supported_block_types;

/// Which origin the catalog shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum OriginFilter {
    #[default]
    All,
    PluginsFolder,
    Tone3000,
}

impl OriginFilter {
    /// The segment order of the window's origin control.
    pub(crate) fn from_index(index: i32) -> Self {
        match index {
            1 => Self::PluginsFolder,
            2 => Self::Tone3000,
            _ => Self::All,
        }
    }

    fn admits(self, origin: PluginOrigin) -> bool {
        match self {
            Self::All => true,
            Self::PluginsFolder => origin == PluginOrigin::PluginsFolder,
            Self::Tone3000 => origin == PluginOrigin::Tone3000,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PluginsFilter {
    pub origin: OriginFilter,
    /// An effect type (`preamp`, `cab`, …); empty for every type.
    pub effect_type: String,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PluginRowView {
    pub plugin_id: String,
    pub name: String,
    pub brand: String,
    pub effect_type: String,
    pub type_label: String,
    /// `NAM`, `IR`, `LV2`, …
    pub backend: String,
    /// `A1`, `A2`, or empty.
    pub arch: String,
    pub captures: u32,
    pub tone3000: bool,
    pub editable: bool,
}

/// One choice of the type filter.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TypeOption {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PluginsView {
    pub rows: Vec<PluginRowView>,
    /// Every plugin, before the filters.
    pub total: usize,
    /// The types the user's plugins have, in catalog order.
    pub types: Vec<TypeOption>,
}

pub(crate) fn plugins_view(entries: &[PluginLibraryEntry], filter: &PluginsFilter) -> PluginsView {
    let query = filter.query.trim().to_lowercase();
    let rows = entries
        .iter()
        .map(row)
        .filter(|row| filter.effect_type.is_empty() || row.effect_type == filter.effect_type)
        .zip(entries)
        .filter(|(_, entry)| filter.origin.admits(entry.origin))
        .map(|(row, _)| row)
        .filter(|row| query.is_empty() || matches(row, &query))
        .collect();
    PluginsView {
        rows,
        total: entries.len(),
        types: type_options(entries),
    }
}

fn matches(row: &PluginRowView, query: &str) -> bool {
    row.name.to_lowercase().contains(query) || row.brand.to_lowercase().contains(query)
}

fn row(entry: &PluginLibraryEntry) -> PluginRowView {
    let effect_type = block_type_to_effect_type(entry.block_type);
    PluginRowView {
        plugin_id: entry.plugin_id.clone(),
        name: entry.display_name.clone(),
        brand: entry.brand.clone().unwrap_or_default(),
        effect_type: effect_type.to_string(),
        type_label: type_label(effect_type),
        backend: entry.backend.to_uppercase(),
        arch: match entry.architecture {
            Some(NamArchitecture::A1) => "A1".into(),
            Some(NamArchitecture::A2) => "A2".into(),
            None => String::new(),
        },
        captures: entry.captures as u32,
        tone3000: entry.origin == PluginOrigin::Tone3000,
        editable: entry.editable,
    }
}

fn type_label(effect_type: &str) -> String {
    supported_block_types()
        .into_iter()
        .find(|t| t.effect_type == effect_type)
        .map(|t| t.display_label.to_string())
        .unwrap_or_else(|| effect_type.to_uppercase())
}

fn type_options(entries: &[PluginLibraryEntry]) -> Vec<TypeOption> {
    let present: Vec<&str> = entries
        .iter()
        .map(|e| block_type_to_effect_type(e.block_type))
        .collect();
    supported_block_types()
        .into_iter()
        .filter(|t| present.contains(&t.effect_type))
        .map(|t| TypeOption {
            key: t.effect_type.to_string(),
            label: t.display_label.to_string(),
        })
        .collect()
}

#[cfg(test)]
#[path = "plugins_view_tests.rs"]
mod tests;
