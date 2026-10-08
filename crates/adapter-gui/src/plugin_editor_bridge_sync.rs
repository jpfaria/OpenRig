//! Responsibility: draws the plugin editor draft onto the window's bridge.

use application::plugin_library::CaptureBackend;
use slint::{ModelRc, SharedString, VecModel};

use crate::plugin_editor_draft::{EditorDraft, EditorMode, COLUMN_KINDS, CREATE_BLOCK_TYPES};
use crate::{EditorColumn, EditorRow, PluginEditorBridge, SelectOption};

/// What the editor shows besides the draft.
pub(crate) struct EditorChrome<'a> {
    /// The plugin's name, or the tone's while it waits for names.
    pub subject: &'a str,
    pub tone3000: bool,
    pub busy: bool,
    pub error: &'a str,
}

/// Publish the whole draft. A cell edit never calls this, so the field the
/// user types in keeps its focus.
pub(crate) fn set_editor_view(
    bridge: &PluginEditorBridge,
    draft: &EditorDraft,
    chrome: &EditorChrome,
) {
    bridge.set_mode(mode_index(&draft.mode));
    bridge.set_subject(chrome.subject.into());
    bridge.set_tone3000(chrome.tone3000);
    bridge.set_busy(chrome.busy);
    bridge.set_error(chrome.error.into());
    let columns: Vec<EditorColumn> = draft
        .grid
        .columns
        .iter()
        .map(|c| EditorColumn {
            name: c.name.as_str().into(),
            kind: COLUMN_KINDS.iter().position(|k| *k == c.kind).unwrap_or(0) as i32,
        })
        .collect();
    bridge.set_columns(ModelRc::new(VecModel::from(columns)));
    let rows: Vec<EditorRow> = draft
        .grid
        .rows
        .iter()
        .map(|r| EditorRow {
            source_name: r.source_name.as_str().into(),
            file: r.file.display().to_string().into(),
            cells: ModelRc::new(VecModel::from(
                r.cells
                    .iter()
                    .map(|c| SharedString::from(c.as_str()))
                    .collect::<Vec<_>>(),
            )),
        })
        .collect();
    bridge.set_rows(ModelRc::new(VecModel::from(rows)));
    bridge.set_versions(ModelRc::new(VecModel::from(version_options(
        &draft.versions,
    ))));
    bridge.set_display_name(draft.create.display_name.as_str().into());
    bridge.set_brand(draft.create.brand.as_str().into());
    bridge.set_block_type(
        CREATE_BLOCK_TYPES
            .iter()
            .position(|t| *t == draft.create.block_type)
            .unwrap_or(0) as i32,
    );
    bridge.set_backend(match draft.create.backend {
        CaptureBackend::Nam => 0,
        CaptureBackend::Ir => 1,
    });
}

/// The window's mode code: 0 edit, 1 create, 2 naming.
pub(crate) fn mode_index(mode: &EditorMode) -> i32 {
    match mode {
        EditorMode::Edit { .. } => 0,
        EditorMode::Create => 1,
        EditorMode::Naming { .. } => 2,
    }
}

/// The kept versions, newest first, keyed by their number.
pub(crate) fn version_options(versions: &[u32]) -> Vec<SelectOption> {
    versions
        .iter()
        .rev()
        .map(|n| SelectOption {
            key: n.to_string().into(),
            label: rust_i18n::t!("option-plugin-editor-version", n = n)
                .as_ref()
                .into(),
        })
        .collect()
}

#[cfg(test)]
#[path = "plugin_editor_bridge_sync_tests.rs"]
mod tests;
