//! System / I/O bindings section wiring (#716).
//!
//! Pure wiring functions that translate Slint callback events into
//! `Command` values for the shared dispatcher. No `AppWindow` is
//! constructed in tests — every exported function is a pure transformation.
//!
//! The Slint section (`section_io_bindings.slint`) surfaces callbacks that
//! carry flat string / int arguments so this layer never depends on a
//! nested Slint struct model. Bindings are identified by their `id` string;
//! endpoints by their `name`. The wiring maintains the in-memory
//! `AppConfig` snapshot (same pattern as `settings::integrations`).

use std::cell::RefCell;
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_filesystem::AppConfig;

use crate::state::ProjectSession;
use crate::{AppWindow, ProjectSettingsWindow};

#[cfg(test)]
#[path = "io_bindings_tests.rs"]
mod io_bindings_tests;

// ── Pure helpers (testable without AppWindow) ─────────────────────────────────

/// Build a `Command::CreateIoBinding` for a new binding.
pub(crate) fn build_create_command(binding: IoBinding) -> Command {
    Command::CreateIoBinding { binding }
}

/// Build a `Command::UpdateIoBinding` with `new_ep` appended to
/// `binding.outputs`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn build_update_with_output_endpoint(
    mut binding: IoBinding,
    new_ep: IoEndpoint,
) -> Command {
    binding.outputs.push(new_ep);
    Command::UpdateIoBinding { binding }
}

/// Convert a dispatcher reject `Err` into a display string for the UI.
/// Leaves `list` unchanged — the delete was rejected, so no mutation.
///
/// Returns the human-readable error message to surface inline.
pub(crate) fn surface_delete_error(err: &anyhow::Error, _list: &mut Vec<IoBinding>) -> String {
    err.to_string()
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn channel_mode_from_str(s: &str) -> ChannelMode {
    match s {
        "stereo" => ChannelMode::Stereo,
        "dual_mono" => ChannelMode::DualMono,
        _ => ChannelMode::Mono,
    }
}

/// Parse a 1-based channel label like "1, 2" into 0-based channel indices.
fn channels_from_label(label: &str) -> Vec<usize> {
    label
        .split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok().map(|n| n.saturating_sub(1)))
        .collect()
}

fn make_endpoint(name: &str, device: &str, mode: &str, channels_label: &str) -> IoEndpoint {
    IoEndpoint {
        name: name.to_string(),
        device_id: DeviceId(device.to_string()),
        mode: channel_mode_from_str(mode),
        channels: channels_from_label(channels_label),
    }
}

fn dispatch_if_session(ps: &Rc<RefCell<Option<ProjectSession>>>, cmd: Command) {
    if let Some(session) = ps.borrow().as_ref() {
        let _ = session.dispatcher.dispatch(cmd);
    }
}

fn delete_reject_message(ps: &Rc<RefCell<Option<ProjectSession>>>, id: &str) -> String {
    let cmd = Command::DeleteIoBinding { id: id.to_string() };
    if let Some(session) = ps.borrow().as_ref() {
        match session.dispatcher.dispatch(cmd) {
            Ok(_) => String::new(),
            Err(e) => {
                let mut dummy: Vec<IoBinding> = Vec::new();
                surface_delete_error(&e, &mut dummy)
            }
        }
    } else {
        // No dispatcher: treat as success (launcher, no project open).
        String::new()
    }
}

/// Generate a slug-style id from the binding name + a small hash.
fn make_id(name: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    let mut h = DefaultHasher::new();
    name.hash(&mut h);
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
        .hash(&mut h);

    let slug: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .map(|c| if c == ' ' { '-' } else { c.to_ascii_lowercase() })
        .take(24)
        .collect();

    format!("{slug}-{:x}", h.finish() & 0xffff)
}

// ── Installer ─────────────────────────────────────────────────────────────────

/// Wire the I/O bindings section callbacks on both window surfaces.
///
/// Seeds the initial binding list from `app_config` and keeps the
/// in-memory snapshot in sync on every mutation — same pattern as
/// `settings::integrations::wire`.
pub fn wire(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    app_config: Rc<RefCell<AppConfig>>,
) {
    seed_both(window, project_settings_window, &app_config);
    install_on_window(window, &project_session, &app_config);
    install_on_psw(project_settings_window, &project_session, &app_config);
}

/// Seed both surfaces from the in-memory `AppConfig` snapshot.
fn seed_both(
    window: &AppWindow,
    psw: &ProjectSettingsWindow,
    app_config: &Rc<RefCell<AppConfig>>,
) {
    use crate::ui_state::ui_bindings;
    use slint::{SharedString, VecModel};

    let cfg = app_config.borrow();
    let models = ui_bindings(&cfg);

    // Build a flat list of (id, name) pairs for the section.
    let rows: Vec<(SharedString, SharedString)> = models
        .iter()
        .map(|m| {
            (
                SharedString::from(m.id.as_str()),
                SharedString::from(m.name.as_str()),
            )
        })
        .collect();

    let id_model = slint::ModelRc::new(VecModel::from(
        rows.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
    ));
    let name_model = slint::ModelRc::new(VecModel::from(
        rows.iter().map(|(_, n)| n.clone()).collect::<Vec<_>>(),
    ));

    window.set_io_binding_ids(id_model.clone());
    window.set_io_binding_names(name_model.clone());
    psw.set_io_binding_ids(id_model);
    psw.set_io_binding_names(name_model);
}

fn install_on_window(
    window: &AppWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    app_config: &Rc<RefCell<AppConfig>>,
) {
    // create-io-binding(name) -> id
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        window.on_create_io_binding(move |name| {
            let id = make_id(name.as_str());
            let binding = IoBinding {
                id: id.clone(),
                name: name.to_string(),
                inputs: vec![],
                outputs: vec![],
            };
            let cmd = build_create_command(binding.clone());
            dispatch_if_session(&ps, cmd);
            cfg.borrow_mut().io_bindings.push(binding);
            slint::SharedString::from(id)
        });
    }

    // delete-io-binding(id) -> error_message (empty = success)
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        window.on_delete_io_binding(move |id| {
            let msg = delete_reject_message(&ps, id.as_str());
            if msg.is_empty() {
                cfg.borrow_mut().io_bindings.retain(|b| b.id != id.as_str());
            }
            slint::SharedString::from(msg)
        });
    }

    // rename-io-binding(id, new_name)
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        window.on_rename_io_binding(move |id, new_name| {
            let mut config = cfg.borrow_mut();
            if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                b.name = new_name.to_string();
                let cmd = Command::UpdateIoBinding { binding: b.clone() };
                dispatch_if_session(&ps, cmd);
            }
        });
    }

    // add-input-endpoint(binding_id, ep_name, device, mode, channels_label)
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        window.on_add_input_endpoint(move |id, ep_name, device, mode, channels| {
            let ep = make_endpoint(ep_name.as_str(), device.as_str(), mode.as_str(), channels.as_str());
            let mut config = cfg.borrow_mut();
            if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                b.inputs.push(ep);
                dispatch_if_session(&ps, Command::UpdateIoBinding { binding: b.clone() });
            }
        });
    }

    // add-output-endpoint(binding_id, ep_name, device, mode, channels_label)
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        window.on_add_output_endpoint(move |id, ep_name, device, mode, channels| {
            let ep = make_endpoint(ep_name.as_str(), device.as_str(), mode.as_str(), channels.as_str());
            let mut config = cfg.borrow_mut();
            if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                b.outputs.push(ep);
                dispatch_if_session(&ps, Command::UpdateIoBinding { binding: b.clone() });
            }
        });
    }

    // remove-endpoint(binding_id, ep_name, is_input: bool)
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        window.on_remove_endpoint(move |id, ep_name, is_input| {
            let mut config = cfg.borrow_mut();
            if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                if is_input {
                    b.inputs.retain(|e| e.name != ep_name.as_str());
                } else {
                    b.outputs.retain(|e| e.name != ep_name.as_str());
                }
                dispatch_if_session(&ps, Command::UpdateIoBinding { binding: b.clone() });
            }
        });
    }
}

fn install_on_psw(
    psw: &ProjectSettingsWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    app_config: &Rc<RefCell<AppConfig>>,
) {
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        psw.on_create_io_binding(move |name| {
            let id = make_id(name.as_str());
            let binding = IoBinding {
                id: id.clone(),
                name: name.to_string(),
                inputs: vec![],
                outputs: vec![],
            };
            let cmd = build_create_command(binding.clone());
            dispatch_if_session(&ps, cmd);
            cfg.borrow_mut().io_bindings.push(binding);
            slint::SharedString::from(id)
        });
    }
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        psw.on_delete_io_binding(move |id| {
            let msg = delete_reject_message(&ps, id.as_str());
            if msg.is_empty() {
                cfg.borrow_mut().io_bindings.retain(|b| b.id != id.as_str());
            }
            slint::SharedString::from(msg)
        });
    }
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        psw.on_rename_io_binding(move |id, new_name| {
            let mut config = cfg.borrow_mut();
            if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                b.name = new_name.to_string();
                let cmd = Command::UpdateIoBinding { binding: b.clone() };
                dispatch_if_session(&ps, cmd);
            }
        });
    }
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        psw.on_add_input_endpoint(move |id, ep_name, device, mode, channels| {
            let ep = make_endpoint(ep_name.as_str(), device.as_str(), mode.as_str(), channels.as_str());
            let mut config = cfg.borrow_mut();
            if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                b.inputs.push(ep);
                dispatch_if_session(&ps, Command::UpdateIoBinding { binding: b.clone() });
            }
        });
    }
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        psw.on_add_output_endpoint(move |id, ep_name, device, mode, channels| {
            let ep = make_endpoint(ep_name.as_str(), device.as_str(), mode.as_str(), channels.as_str());
            let mut config = cfg.borrow_mut();
            if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                b.outputs.push(ep);
                dispatch_if_session(&ps, Command::UpdateIoBinding { binding: b.clone() });
            }
        });
    }
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        psw.on_remove_endpoint(move |id, ep_name, is_input| {
            let mut config = cfg.borrow_mut();
            if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                if is_input {
                    b.inputs.retain(|e| e.name != ep_name.as_str());
                } else {
                    b.outputs.retain(|e| e.name != ep_name.as_str());
                }
                dispatch_if_session(&ps, Command::UpdateIoBinding { binding: b.clone() });
            }
        });
    }
}
