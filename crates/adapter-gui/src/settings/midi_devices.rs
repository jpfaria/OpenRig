//! System / MIDI devices section wiring (#513). The adapter calls
//! `adapter_midi::list_input_ports()` directly for the refresh button
//! (no Command is necessary for a read-only query). User edits dispatch
//! `Command::SaveMidiDevices` immediately (when a project session is
//! loaded) and persist into `config.yaml` in the same callback — the
//! event still fans out, but persistence lives colocated with the user
//! action so the rows survive even when the launcher has no dispatcher.

use std::cell::RefCell;
use std::rc::Rc;

use slint::VecModel;

use application::command::Command;
use infra_filesystem::{FilesystemStorage, MidiDeviceSelection, MidiPortKey};

use crate::state::ProjectSession;
use crate::{AppWindow, MidiDeviceRow, ProjectSettingsWindow};

#[cfg(test)]
#[path = "midi_devices_tests.rs"]
mod midi_devices_tests;

/// Reconcile the persisted `MidiDeviceSelection` list with the freshly
/// enumerated `(name, instance)` pairs. Devices already in `persisted`
/// keep their alias and `enabled` flag; new devices seed with
/// `alias = name` (or `name (#instance)` for duplicates) and
/// `enabled = false`; devices that vanished are kept in the list but
/// force-disabled, so the user's choice isn't silently lost when a
/// controller is unplugged temporarily.
pub(crate) fn merge_enumeration(
    mut persisted: Vec<MidiDeviceSelection>,
    enumerated: Vec<(String, u32)>,
) -> Vec<MidiDeviceSelection> {
    let mut present: Vec<bool> = vec![false; persisted.len()];
    for (name, instance) in enumerated {
        let key = MidiPortKey {
            name: name.clone(),
            instance,
        };
        if let Some(i) = persisted.iter().position(|r| r.port_key == key) {
            present[i] = true;
        } else {
            persisted.push(MidiDeviceSelection {
                port_key: key.clone(),
                alias: if instance == 0 {
                    name.clone()
                } else {
                    format!("{name} (#{instance})")
                },
                enabled: false,
            });
            present.push(true);
        }
    }
    for (i, was_present) in present.iter().enumerate() {
        if !was_present {
            persisted[i].enabled = false;
        }
    }
    persisted
}

pub(crate) fn toggle_row(rows: &mut [MidiDeviceSelection], key: &MidiPortKey, enabled: bool) {
    if let Some(r) = rows.iter_mut().find(|r| r.port_key == *key) {
        r.enabled = enabled;
    }
}

pub(crate) fn edit_alias(rows: &mut [MidiDeviceSelection], key: &MidiPortKey, alias: &str) {
    if let Some(r) = rows.iter_mut().find(|r| r.port_key == *key) {
        r.alias = alias.to_string();
    }
}

/// Returns an owned copy of `rows` for submission to the dispatcher.
/// Kept as a named helper (instead of inlining `rows.to_vec()`) so the
/// test surface can assert the shape and so future call sites have a
/// single seam if we move to a non-cloning representation.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn devices_for_save(rows: &[MidiDeviceSelection]) -> Vec<MidiDeviceSelection> {
    rows.to_vec()
}

/// Install the section callbacks on the AppWindow. Each user edit
/// dispatches `Command::SaveMidiDevices` (when a session is loaded) and
/// persists the new list into `config.yaml` in the same callback. The
/// `Event::MidiDevicesSaved` fan-out still happens for any future
/// listener, but the persistence write is colocated so the launcher
/// path (no session, no dispatcher) still saves correctly.
pub fn install(
    win: &AppWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    rows: Rc<RefCell<Vec<MidiDeviceSelection>>>,
    model: Rc<VecModel<MidiDeviceRow>>,
) {
    let session_for_refresh = project_session.clone();
    let rows_for_refresh = rows.clone();
    let model_for_refresh = model.clone();
    win.on_refresh_midi_devices(move || {
        let infos = match adapter_midi::list_input_ports() {
            Ok(v) => v,
            Err(err) => {
                log::warn!("MIDI enumeration failed: {err}");
                return;
            }
        };
        let enumerated: Vec<(String, u32)> = infos
            .into_iter()
            .map(|i| (i.key.name, i.key.instance))
            .collect();
        let merged = merge_enumeration(rows_for_refresh.borrow().clone(), enumerated);
        *rows_for_refresh.borrow_mut() = merged.clone();
        replace_model(&model_for_refresh, &merged);
        dispatch_and_persist(&session_for_refresh, &merged);
        // #548: signal the profile daemon to attach any newly visible
        // port (e.g. a pedal paired AFTER the app started).
        adapter_midi::request_rescan();
    });

    let session_for_toggle = project_session.clone();
    let rows_for_toggle = rows.clone();
    let model_for_toggle = model.clone();
    win.on_toggle_midi_device(move |row_index, enabled| {
        let mut current = rows_for_toggle.borrow().clone();
        let key = match current.get(row_index as usize) {
            Some(r) => r.port_key.clone(),
            None => return,
        };
        toggle_row(&mut current, &key, enabled);
        *rows_for_toggle.borrow_mut() = current.clone();
        replace_model(&model_for_toggle, &current);
        dispatch_and_persist(&session_for_toggle, &current);
    });

    let session_for_alias = project_session;
    let rows_for_alias = rows;
    let model_for_alias = model;
    win.on_edit_midi_device_alias(move |row_index, alias| {
        let mut current = rows_for_alias.borrow().clone();
        let key = match current.get(row_index as usize) {
            Some(r) => r.port_key.clone(),
            None => return,
        };
        edit_alias(&mut current, &key, alias.as_str());
        *rows_for_alias.borrow_mut() = current.clone();
        replace_model(&model_for_alias, &current);
        dispatch_and_persist(&session_for_alias, &current);
    });
}

/// Mirror of `install` for the standalone `ProjectSettingsWindow`
/// (issue #513). The master-detail SettingsPage lives inside this
/// secondary Window too — without re-registering the callbacks on this
/// window the MIDI Devices section silently no-ops when Settings opens
/// in the standalone surface. Same Rc state as the main window so both
/// surfaces stay in sync.
pub fn install_secondary(
    win: &ProjectSettingsWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    rows: Rc<RefCell<Vec<MidiDeviceSelection>>>,
    model: Rc<VecModel<MidiDeviceRow>>,
) {
    let session_for_refresh = project_session.clone();
    let rows_for_refresh = rows.clone();
    let model_for_refresh = model.clone();
    win.on_refresh_midi_devices(move || {
        let infos = match adapter_midi::list_input_ports() {
            Ok(v) => v,
            Err(err) => {
                log::warn!("MIDI enumeration failed: {err}");
                return;
            }
        };
        let enumerated: Vec<(String, u32)> = infos
            .into_iter()
            .map(|i| (i.key.name, i.key.instance))
            .collect();
        let merged = merge_enumeration(rows_for_refresh.borrow().clone(), enumerated);
        *rows_for_refresh.borrow_mut() = merged.clone();
        replace_model(&model_for_refresh, &merged);
        dispatch_and_persist(&session_for_refresh, &merged);
        // #548: signal the profile daemon to attach any newly visible
        // port (e.g. a pedal paired AFTER the app started).
        adapter_midi::request_rescan();
    });

    let session_for_toggle = project_session.clone();
    let rows_for_toggle = rows.clone();
    let model_for_toggle = model.clone();
    win.on_toggle_midi_device(move |row_index, enabled| {
        let mut current = rows_for_toggle.borrow().clone();
        let key = match current.get(row_index as usize) {
            Some(r) => r.port_key.clone(),
            None => return,
        };
        toggle_row(&mut current, &key, enabled);
        *rows_for_toggle.borrow_mut() = current.clone();
        replace_model(&model_for_toggle, &current);
        dispatch_and_persist(&session_for_toggle, &current);
    });

    let session_for_alias = project_session;
    let rows_for_alias = rows;
    let model_for_alias = model;
    win.on_edit_midi_device_alias(move |row_index, alias| {
        let mut current = rows_for_alias.borrow().clone();
        let key = match current.get(row_index as usize) {
            Some(r) => r.port_key.clone(),
            None => return,
        };
        edit_alias(&mut current, &key, alias.as_str());
        *rows_for_alias.borrow_mut() = current.clone();
        replace_model(&model_for_alias, &current);
        dispatch_and_persist(&session_for_alias, &current);
    });
}

fn dispatch_and_persist(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    rows: &[MidiDeviceSelection],
) {
    if let Some(session) = project_session.borrow().as_ref() {
        use application::dispatcher::CommandDispatcher;
        if let Err(e) = session.dispatcher.dispatch(Command::SaveMidiDevices {
            devices: rows.to_vec(),
        }) {
            log::warn!("[midi_devices] Command::SaveMidiDevices failed: {e}");
        }
    }
    if let Err(e) = persist(rows) {
        log::warn!("[midi_devices] persist failed: {e}");
    }
}

/// Pushes the row state into the Slint model the section binds to.
pub(crate) fn replace_model(model: &VecModel<MidiDeviceRow>, rows: &[MidiDeviceSelection]) {
    model.set_vec(
        rows.iter()
            .map(|r| MidiDeviceRow {
                name: r.port_key.name.clone().into(),
                instance: r.port_key.instance as i32,
                alias: r.alias.clone().into(),
                enabled: r.enabled,
            })
            .collect::<Vec<_>>(),
    );
}

/// Persist the row list into `config.yaml`, preserving every other
/// AppConfig field. Read-modify-write because `AppConfig` carries
/// audio devices, language, recent projects and asset paths too.
pub fn persist(rows: &[MidiDeviceSelection]) -> anyhow::Result<()> {
    let mut config = FilesystemStorage::load_app_config().unwrap_or_default();
    config.midi_devices = rows.to_vec();
    FilesystemStorage::save_app_config(&config)
}
