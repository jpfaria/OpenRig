//! Responsibility: wires the windows that manage the user's plugins.
//!
//! Everything the callbacks do lives in `PluginsCtx` and `PluginEditorCtx`,
//! which are tested; this file only hands them the windows, the openers the
//! info windows reach through `plugin_editor_link`, the listener that
//! receives library events from every drain, and the tick.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{CloseRequestResponse, ComponentHandle, Global, Timer, TimerMode};

use crate::capture_file_chooser::choose_capture_files;
use crate::helpers::show_child_window;
use crate::plugin_editor_ctx::{PluginEditorCtx, WindowToggle};
use crate::plugin_editor_intents::wire_plugin_editor_intents;
use crate::plugin_editor_link::{self, EditorLink};
use crate::plugin_info_window_open::open_plugin_info_window;
use crate::plugin_library_session::plugin_entries;
use crate::plugins_ctx::PluginsCtx;
use crate::plugins_intents::{wire_plugins_intents, OpenEditor, OpenInfo};
use crate::tone3000_ctx::ForwardEvents;
use crate::tone3000_session::SessionCell;
use crate::{AppWindow, PluginEditorWindow, PluginInfoWindow, PluginsBridge, PluginsWindow};

/// A create or a redo takes seconds; a quarter second reads as live.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

pub(crate) struct PluginWindows<'a> {
    pub catalog: &'a PluginsWindow,
    pub editor: &'a PluginEditorWindow,
    /// Where the info window the catalog opens is kept.
    pub info: Rc<RefCell<Option<PluginInfoWindow>>>,
}

/// Wire the top-bar button, both windows and the tick. Call once.
pub(crate) fn wire_plugins(
    window: &AppWindow,
    windows: PluginWindows<'_>,
    project_session: &SessionCell,
    forward: ForwardEvents,
) {
    let main = window.as_weak();
    let editor = editor_ctx(window, windows.editor, project_session, forward.clone());
    let catalog = PluginsCtx::new(project_session.clone(), windows.catalog.as_weak(), forward);
    wire_plugin_editor_intents(windows.editor, &editor, Rc::new(choose_capture_files));
    let e = editor.clone();
    windows.editor.window().on_close_requested(move || {
        e.cancel();
        CloseRequestResponse::HideWindow
    });
    let e = editor.clone();
    let open_editor: OpenEditor = Rc::new(move |plugin_id| match plugin_id {
        Some(id) => e.open_edit(id),
        None => {
            e.open_create();
            Ok(())
        }
    });
    let (m, info) = (main.clone(), windows.info);
    let open_info: OpenInfo = Rc::new(move |plugin_id, effect_type| {
        if let Some(w) = m.upgrade() {
            open_plugin_info_window(&w, &info, effect_type, plugin_id);
        }
    });
    wire_plugins_intents(windows.catalog, &catalog, open_info, open_editor);
    register_link(project_session, &editor);
    let (c, e) = (catalog.clone(), editor.clone());
    crate::plugin_library_events::register(Rc::new(move |events| {
        c.on_events(events);
        e.on_events(events);
    }));
    let c = catalog.clone();
    PluginsBridge::get(window).on_open_plugins_window(move || {
        let (Some(main_w), Some(pw)) = (main.upgrade(), c.window.upgrade()) else {
            return;
        };
        c.render();
        show_child_window(main_w.window(), pw.window());
    });
    // The closure owns its timer, so the tick lives as long as the app.
    let timer = Rc::new(Timer::default());
    let owned = timer.clone();
    timer.start(TimerMode::Repeated, POLL_INTERVAL, move || {
        let _ = &owned;
        catalog.tick();
        editor.tick();
    });
}

fn editor_ctx(
    window: &AppWindow,
    editor_window: &PluginEditorWindow,
    project_session: &SessionCell,
    forward: ForwardEvents,
) -> PluginEditorCtx {
    let (main, ew) = (window.as_weak(), editor_window.as_weak());
    let shown = ew.clone();
    let show: WindowToggle = Rc::new(move || {
        if let (Some(m), Some(e)) = (main.upgrade(), shown.upgrade()) {
            show_child_window(m.window(), e.window());
        }
    });
    let hidden = ew.clone();
    let hide: WindowToggle = Rc::new(move || {
        if let Some(e) = hidden.upgrade() {
            let _ = e.hide();
        }
    });
    PluginEditorCtx::new(project_session.clone(), ew, forward, show, hide)
}

/// Let every info window ask whether a plugin is editable and open it.
fn register_link(project_session: &SessionCell, editor: &PluginEditorCtx) {
    let session = project_session.clone();
    let e = editor.clone();
    plugin_editor_link::register(EditorLink {
        editable: Rc::new(move |plugin_id| {
            plugin_entries(&session)
                .unwrap_or_default()
                .iter()
                .any(|entry| entry.plugin_id == plugin_id && entry.editable)
        }),
        open: Rc::new(move |plugin_id| {
            if let Err(why) = e.open_edit(plugin_id) {
                log::warn!("plugin editor: {why}");
            }
        }),
    });
}
