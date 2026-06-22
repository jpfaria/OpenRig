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

/// Name for a new binding: the typed name, or a sequential default ("I/O N")
/// when the field is empty, so the "+" always produces a visible, renamable
/// row instead of doing nothing.
fn binding_display_name(name: &str, cfg: &Rc<RefCell<AppConfig>>) -> String {
    let trimmed = name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    format!("I/O {}", cfg.borrow().io_bindings.len() + 1)
}

/// Issue #716 — mirror the edited `AppConfig.io_bindings` into the open
/// session so the live runtime resolves bound chains against the latest
/// registry on its next sync (a new/edited binding takes effect without a
/// project reopen).
fn mirror_bindings_to_session(
    ps: &Rc<RefCell<Option<ProjectSession>>>,
    cfg: &Rc<RefCell<AppConfig>>,
) {
    if let Some(session) = ps.borrow().as_ref() {
        *session.io_bindings.borrow_mut() = cfg.borrow().io_bindings.clone();
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
        .map(|c| {
            if c == ' ' {
                '-'
            } else {
                c.to_ascii_lowercase()
            }
        })
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
    use slint::{ModelRc, SharedString, VecModel};

    // ONE shared pair of models, set on BOTH windows. Every mutation
    // closure re-projects into these handles so the list actually updates
    // (issue #716: the list previously never refreshed after create).
    let (ids, names) = binding_rows(&app_config.borrow());
    let id_model: Rc<VecModel<SharedString>> = Rc::new(VecModel::from(ids));
    let name_model: Rc<VecModel<SharedString>> = Rc::new(VecModel::from(names));

    window.set_io_binding_ids(ModelRc::from(id_model.clone()));
    window.set_io_binding_names(ModelRc::from(name_model.clone()));
    project_settings_window.set_io_binding_ids(ModelRc::from(id_model.clone()));
    project_settings_window.set_io_binding_names(ModelRc::from(name_model.clone()));

    install_on_window(window, &project_session, &app_config, &id_model, &name_model);
    install_on_psw(
        project_settings_window,
        &project_session,
        &app_config,
        &id_model,
        &name_model,
    );
}

/// Flat (id, name) row vectors from the current config.
fn binding_rows(cfg: &AppConfig) -> (Vec<slint::SharedString>, Vec<slint::SharedString>) {
    use crate::ui_state::ui_bindings;
    use slint::SharedString;
    let models = ui_bindings(cfg);
    let ids = models
        .iter()
        .map(|m| SharedString::from(m.id.as_str()))
        .collect();
    let names = models
        .iter()
        .map(|m| SharedString::from(m.name.as_str()))
        .collect();
    (ids, names)
}

/// Re-project the binding list into the shared Slint models so the UI
/// reflects the current config after any mutation.
fn reproject(
    id_model: &Rc<slint::VecModel<slint::SharedString>>,
    name_model: &Rc<slint::VecModel<slint::SharedString>>,
    cfg: &Rc<RefCell<AppConfig>>,
) {
    let (ids, names) = binding_rows(&cfg.borrow());
    id_model.set_vec(ids);
    name_model.set_vec(names);
}

fn install_on_window(
    window: &AppWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    app_config: &Rc<RefCell<AppConfig>>,
    id_model: &Rc<slint::VecModel<slint::SharedString>>,
    name_model: &Rc<slint::VecModel<slint::SharedString>>,
) {
    // create-io-binding(name) -> id
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        let idm = Rc::clone(id_model);
        let nm = Rc::clone(name_model);
        window.on_create_io_binding(move |name| {
            let display = binding_display_name(name.as_str(), &cfg);
            let id = make_id(&display);
            let binding = IoBinding {
                id: id.clone(),
                name: display,
                inputs: vec![],
                outputs: vec![],
            };
            let cmd = build_create_command(binding.clone());
            dispatch_if_session(&ps, cmd);
            cfg.borrow_mut().io_bindings.push(binding);
            mirror_bindings_to_session(&ps, &cfg);
            reproject(&idm, &nm, &cfg);
            slint::SharedString::from(id)
        });
    }

    // delete-io-binding(id) -> error_message (empty = success)
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        let idm = Rc::clone(id_model);
        let nm = Rc::clone(name_model);
        window.on_delete_io_binding(move |id| {
            let msg = delete_reject_message(&ps, id.as_str());
            if msg.is_empty() {
                cfg.borrow_mut().io_bindings.retain(|b| b.id != id.as_str());
                mirror_bindings_to_session(&ps, &cfg);
                reproject(&idm, &nm, &cfg);
            }
            slint::SharedString::from(msg)
        });
    }

    // rename-io-binding(id, new_name)
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        let idm = Rc::clone(id_model);
        let nm = Rc::clone(name_model);
        window.on_rename_io_binding(move |id, new_name| {
            {
                let mut config = cfg.borrow_mut();
                if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                    b.name = new_name.to_string();
                    let cmd = Command::UpdateIoBinding { binding: b.clone() };
                    dispatch_if_session(&ps, cmd);
                }
            }
            mirror_bindings_to_session(&ps, &cfg);
            reproject(&idm, &nm, &cfg);
        });
    }

    // add-input-endpoint(binding_id, ep_name, device, mode, channels_label)
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        let idm = Rc::clone(id_model);
        let nm = Rc::clone(name_model);
        window.on_add_input_endpoint(move |id, ep_name, device, mode, channels| {
            let ep = make_endpoint(
                ep_name.as_str(),
                device.as_str(),
                mode.as_str(),
                channels.as_str(),
            );
            {
                let mut config = cfg.borrow_mut();
                if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                    b.inputs.push(ep);
                    dispatch_if_session(&ps, Command::UpdateIoBinding { binding: b.clone() });
                }
            }
            mirror_bindings_to_session(&ps, &cfg);
            reproject(&idm, &nm, &cfg);
        });
    }

    // add-output-endpoint(binding_id, ep_name, device, mode, channels_label)
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        let idm = Rc::clone(id_model);
        let nm = Rc::clone(name_model);
        window.on_add_output_endpoint(move |id, ep_name, device, mode, channels| {
            let ep = make_endpoint(
                ep_name.as_str(),
                device.as_str(),
                mode.as_str(),
                channels.as_str(),
            );
            {
                let mut config = cfg.borrow_mut();
                if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                    b.outputs.push(ep);
                    dispatch_if_session(&ps, Command::UpdateIoBinding { binding: b.clone() });
                }
            }
            mirror_bindings_to_session(&ps, &cfg);
            reproject(&idm, &nm, &cfg);
        });
    }

    // remove-endpoint(binding_id, ep_name, is_input: bool)
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        let idm = Rc::clone(id_model);
        let nm = Rc::clone(name_model);
        window.on_remove_endpoint(move |id, ep_name, is_input| {
            {
                let mut config = cfg.borrow_mut();
                if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                    if is_input {
                        b.inputs.retain(|e| e.name != ep_name.as_str());
                    } else {
                        b.outputs.retain(|e| e.name != ep_name.as_str());
                    }
                    dispatch_if_session(&ps, Command::UpdateIoBinding { binding: b.clone() });
                }
            }
            mirror_bindings_to_session(&ps, &cfg);
            reproject(&idm, &nm, &cfg);
        });
    }
}

fn install_on_psw(
    psw: &ProjectSettingsWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    app_config: &Rc<RefCell<AppConfig>>,
    id_model: &Rc<slint::VecModel<slint::SharedString>>,
    name_model: &Rc<slint::VecModel<slint::SharedString>>,
) {
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        let idm = Rc::clone(id_model);
        let nm = Rc::clone(name_model);
        psw.on_create_io_binding(move |name| {
            let display = binding_display_name(name.as_str(), &cfg);
            let id = make_id(&display);
            let binding = IoBinding {
                id: id.clone(),
                name: display,
                inputs: vec![],
                outputs: vec![],
            };
            let cmd = build_create_command(binding.clone());
            dispatch_if_session(&ps, cmd);
            cfg.borrow_mut().io_bindings.push(binding);
            mirror_bindings_to_session(&ps, &cfg);
            reproject(&idm, &nm, &cfg);
            slint::SharedString::from(id)
        });
    }
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        let idm = Rc::clone(id_model);
        let nm = Rc::clone(name_model);
        psw.on_delete_io_binding(move |id| {
            let msg = delete_reject_message(&ps, id.as_str());
            if msg.is_empty() {
                cfg.borrow_mut().io_bindings.retain(|b| b.id != id.as_str());
                mirror_bindings_to_session(&ps, &cfg);
                reproject(&idm, &nm, &cfg);
            }
            slint::SharedString::from(msg)
        });
    }
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        let idm = Rc::clone(id_model);
        let nm = Rc::clone(name_model);
        psw.on_rename_io_binding(move |id, new_name| {
            {
                let mut config = cfg.borrow_mut();
                if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                    b.name = new_name.to_string();
                    let cmd = Command::UpdateIoBinding { binding: b.clone() };
                    dispatch_if_session(&ps, cmd);
                }
            }
            mirror_bindings_to_session(&ps, &cfg);
            reproject(&idm, &nm, &cfg);
        });
    }
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        let idm = Rc::clone(id_model);
        let nm = Rc::clone(name_model);
        psw.on_add_input_endpoint(move |id, ep_name, device, mode, channels| {
            let ep = make_endpoint(
                ep_name.as_str(),
                device.as_str(),
                mode.as_str(),
                channels.as_str(),
            );
            {
                let mut config = cfg.borrow_mut();
                if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                    b.inputs.push(ep);
                    dispatch_if_session(&ps, Command::UpdateIoBinding { binding: b.clone() });
                }
            }
            mirror_bindings_to_session(&ps, &cfg);
            reproject(&idm, &nm, &cfg);
        });
    }
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        let idm = Rc::clone(id_model);
        let nm = Rc::clone(name_model);
        psw.on_add_output_endpoint(move |id, ep_name, device, mode, channels| {
            let ep = make_endpoint(
                ep_name.as_str(),
                device.as_str(),
                mode.as_str(),
                channels.as_str(),
            );
            {
                let mut config = cfg.borrow_mut();
                if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                    b.outputs.push(ep);
                    dispatch_if_session(&ps, Command::UpdateIoBinding { binding: b.clone() });
                }
            }
            mirror_bindings_to_session(&ps, &cfg);
            reproject(&idm, &nm, &cfg);
        });
    }
    {
        let ps = Rc::clone(project_session);
        let cfg = Rc::clone(app_config);
        let idm = Rc::clone(id_model);
        let nm = Rc::clone(name_model);
        psw.on_remove_endpoint(move |id, ep_name, is_input| {
            {
                let mut config = cfg.borrow_mut();
                if let Some(b) = config.io_bindings.iter_mut().find(|b| b.id == id.as_str()) {
                    if is_input {
                        b.inputs.retain(|e| e.name != ep_name.as_str());
                    } else {
                        b.outputs.retain(|e| e.name != ep_name.as_str());
                    }
                    dispatch_if_session(&ps, Command::UpdateIoBinding { binding: b.clone() });
                }
            }
            mirror_bindings_to_session(&ps, &cfg);
            reproject(&idm, &nm, &cfg);
        });
    }
}
