//! Wires the System / Integrations toggles (issue #712): per-machine
//! master switches for the MIDI/BLE-MIDI adapter and the MCP server.
//!
//! Enablement is system config (ADR 0003), not a CLI flag — packaged
//! builds launch the binary with no arguments, so `--midi`/`--mcp` never
//! reached the installed app. The toggles dispatch `SetMidiEnabled` /
//! `SetMcpEnabled` through the shared dispatcher (GUI/MCP/gRPC parity);
//! the handler persists `config.yaml`. With no open project (launcher),
//! there is no dispatcher, so the wiring persists directly — the same
//! fallback `settings::language` uses. The change takes effect on next
//! launch (the subsystem is wired at bootstrap); the section shows a
//! restart hint.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use infra_filesystem::FilesystemStorage;

use crate::state::ProjectSession;
use crate::{AppWindow, ProjectSettingsWindow};

pub fn wire(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
) {
    // Seed both surfaces from the persisted per-machine config so the
    // toggles render in their stored state.
    let config = FilesystemStorage::load_app_config().unwrap_or_default();
    window.set_midi_enabled(config.midi_enabled);
    window.set_mcp_enabled(config.mcp_enabled);
    project_settings_window.set_midi_enabled(config.midi_enabled);
    project_settings_window.set_mcp_enabled(config.mcp_enabled);

    install_midi(window, project_settings_window, &project_session);
    install_mcp(window, project_settings_window, &project_session);
}

/// Build the shared handler for one master switch. Mirrors the new value
/// onto both surfaces (so the toggle reflects immediately), then routes
/// the state change through the command bus when a project is open, or
/// persists directly from the launcher where no dispatcher exists.
fn make_handler(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    set_on_both: impl Fn(&AppWindow, &ProjectSettingsWindow, bool) + 'static,
    make_command: impl Fn(bool) -> Command + 'static,
    persist: impl Fn(bool) + Send + Copy + 'static,
) -> impl Fn(bool) {
    let weak = window.as_weak();
    let weak_settings = project_settings_window.as_weak();
    let session = project_session.clone();
    move |enabled: bool| {
        if let (Some(w), Some(s)) = (weak.upgrade(), weak_settings.upgrade()) {
            set_on_both(&w, &s, enabled);
        }
        if let Some(session) = session.borrow().as_ref() {
            // Dispatch persists (handle_set_*_enabled) + emits the event.
            if let Err(e) = session.dispatcher.dispatch(make_command(enabled)) {
                log::warn!("[integrations] subsystem toggle dispatch failed: {e}");
            }
        } else {
            // Launcher: no dispatcher → persist directly (same fallback as
            // the language selector).
            application::persist_worker::run(move || persist(enabled));
        }
    }
}

fn install_midi(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) {
    let handler = Rc::new(make_handler(
        window,
        project_settings_window,
        project_session,
        |w, s, on| {
            w.set_midi_enabled(on);
            s.set_midi_enabled(on);
        },
        |enabled| Command::SetMidiEnabled { enabled },
        |enabled| {
            let mut config = FilesystemStorage::load_app_config().unwrap_or_default();
            config.midi_enabled = enabled;
            if let Err(e) = FilesystemStorage::save_app_config(&config) {
                log::error!("failed to persist midi_enabled: {e}");
            }
        },
    ));
    let for_app = handler.clone();
    window.on_set_midi_enabled(move |on| for_app(on));
    let for_settings = handler;
    project_settings_window.on_set_midi_enabled(move |on| for_settings(on));
}

fn install_mcp(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) {
    let handler = Rc::new(make_handler(
        window,
        project_settings_window,
        project_session,
        |w, s, on| {
            w.set_mcp_enabled(on);
            s.set_mcp_enabled(on);
        },
        |enabled| Command::SetMcpEnabled { enabled },
        |enabled| {
            let mut config = FilesystemStorage::load_app_config().unwrap_or_default();
            config.mcp_enabled = enabled;
            if let Err(e) = FilesystemStorage::save_app_config(&config) {
                log::error!("failed to persist mcp_enabled: {e}");
            }
        },
    ));
    let for_app = handler.clone();
    window.on_set_mcp_enabled(move |on| for_app(on));
    let for_settings = handler;
    project_settings_window.on_set_mcp_enabled(move |on| for_settings(on));
}
