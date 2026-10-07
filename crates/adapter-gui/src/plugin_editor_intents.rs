//! Responsibility: maps every plugin editor control to its change of the draft.
//!
//! A typed value only lands in the draft, so the field keeps its focus;
//! anything that changes the grid's shape redraws it. The name and brand of
//! a new plugin are read off the window before every change, so a redraw
//! never wipes what the user typed.

use std::path::PathBuf;
use std::rc::Rc;

use application::plugin_library::CaptureBackend;
use slint::{ComponentHandle, Global};

use crate::plugin_editor_ctx::PluginEditorCtx;
use crate::plugin_editor_draft::EditorDraft;
use crate::{PluginEditorBridge, PluginEditorWindow};

/// Asks the user for capture files that play through `backend`.
pub(crate) type PickFiles = Rc<dyn Fn(CaptureBackend) -> Vec<PathBuf>>;

/// Route every control of `window` through `ctx`.
pub(crate) fn wire_plugin_editor_intents(
    window: &PluginEditorWindow,
    ctx: &PluginEditorCtx,
    pick_files: PickFiles,
) {
    let bridge = PluginEditorBridge::get(window);
    let edit = Editor {
        window: window.as_weak(),
        ctx: ctx.clone(),
    };
    let e = edit.clone();
    bridge.on_set_cell(move |row, column, value| {
        if let (Some(r), Some(c)) = (index(row), index(column)) {
            e.apply(false, |d| d.set_cell(r, c, &value));
        }
    });
    let e = edit.clone();
    bridge.on_rename_column(move |column, name| {
        if let Some(c) = index(column) {
            e.apply(false, |d| d.rename_column(c, &name));
        }
    });
    let e = edit.clone();
    bridge.on_set_kind(move |column, kind| {
        if let Some(c) = index(column) {
            e.apply(true, |d| d.set_kind(c, kind));
        }
    });
    let e = edit.clone();
    bridge.on_add_column(move || e.apply(true, EditorDraft::add_column));
    let e = edit.clone();
    bridge.on_remove_column(move |column| {
        if let Some(c) = index(column) {
            e.apply(true, |d| d.remove_column(c));
        }
    });
    let e = edit.clone();
    bridge.on_add_files(move || {
        let backend = e.backend();
        let files = pick_files(backend);
        if !files.is_empty() {
            e.apply(true, |d| d.add_files(files));
        }
    });
    let e = edit.clone();
    bridge.on_remove_row(move |row| {
        if let Some(r) = index(row) {
            e.apply(true, |d| d.remove_row(r));
        }
    });
    let e = edit.clone();
    bridge.on_set_block_type(move |i| e.apply(true, |d| d.set_block_type(i)));
    let e = edit.clone();
    bridge.on_set_backend(move |i| e.apply(true, |d| d.set_backend(i)));
    let c = ctx.clone();
    bridge.on_restore(move |version| c.restore(&version));
    let c = ctx.clone();
    bridge.on_redo(move || c.redo());
    let e = edit.clone();
    bridge.on_save(move || {
        e.apply(false, |_| {});
        e.ctx.save();
    });
    let c = ctx.clone();
    bridge.on_cancel(move || c.cancel());
}

#[derive(Clone)]
struct Editor {
    window: slint::Weak<PluginEditorWindow>,
    ctx: PluginEditorCtx,
}

impl Editor {
    /// Take the typed name and brand, then apply `change`.
    fn apply(&self, republish: bool, change: impl FnOnce(&mut EditorDraft)) {
        let typed = self.window.upgrade().map(|w| {
            let bridge = PluginEditorBridge::get(&w);
            (
                bridge.get_display_name().to_string(),
                bridge.get_brand().to_string(),
            )
        });
        self.ctx.edit(republish, |d| {
            if let Some((name, brand)) = typed {
                d.create.display_name = name;
                d.create.brand = brand;
            }
            change(d);
        });
    }

    fn backend(&self) -> CaptureBackend {
        match self
            .window
            .upgrade()
            .map(|w| PluginEditorBridge::get(&w).get_backend())
        {
            Some(1) => CaptureBackend::Ir,
            _ => CaptureBackend::Nam,
        }
    }
}

fn index(i: i32) -> Option<usize> {
    usize::try_from(i).ok()
}

#[cfg(test)]
#[path = "plugin_editor_intents_tests.rs"]
mod tests;
