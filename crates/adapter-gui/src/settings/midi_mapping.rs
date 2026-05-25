//! Project / MIDI mapping editor wiring (#513, #493). Lets the user
//! manage `project.midi.bindings` interactively: add a draft, MIDI
//! Learn the trigger, pick a `Command`, delete bindings. Each change
//! that yields a complete binding dispatches `Command::SaveMidiMapping`
//! with the full bindings list — the project save adapter persists
//! the project file on `Event::MidiMappingSaved`.
//!
//! Pattern mirrors `midi_devices.rs`: the install function takes the
//! optional `ProjectSession` so the section gracefully no-ops when no
//! project is loaded. Pure helpers (`add_draft`, `apply_learned_source`,
//! `finalize_draft`, `format_trigger`) are isolated from Slint so the
//! TDD red-first tests stay AppWindow-free.

use std::cell::RefCell;
use std::rc::Rc;

use slint::VecModel;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use project::midi::{Binding, Source};

use crate::state::ProjectSession;
use crate::{AppWindow, MidiBindingRow};

#[cfg(test)]
#[path = "midi_mapping_tests.rs"]
mod midi_mapping_tests;

/// Pending row in the editor that has not yet been promoted into the
/// persisted `project.midi.bindings` list. A draft is "complete" once
/// it carries both a `source` (filled by `apply_learned_source`) and a
/// `command` (picked from the dropdown). `learning = true` means the
/// daemon's single-shot capture is currently armed for THIS draft.
#[derive(Default, Debug, Clone)]
pub struct Draft {
    pub source: Option<Source>,
    pub command: Option<String>,
    pub learning: bool,
}

/// Human-readable label for a `Source` — drives the UI's trigger
/// column. Format is stable because tests assert on it; matches what
/// the `midi-map.yaml` reader produces for diagnostic logs.
pub fn format_trigger(src: &Source) -> String {
    match src {
        Source::NoteOn { channel, note } => format!("Note On ch={channel} #{note}"),
        Source::NoteOff { channel, note } => format!("Note Off ch={channel} #{note}"),
        Source::Cc {
            channel,
            controller,
        } => format!("CC ch={channel} #{controller}"),
        Source::ProgramChange { program } => format!("PC #{program}"),
    }
}

/// Append an empty draft in Learn mode. The bindings reference is taken
/// to keep a symmetric signature with the other helpers and to leave
/// room for future "auto-pick last used command" logic without touching
/// every call site.
pub fn add_draft(_bindings: &mut Vec<Binding>, drafts: &mut Vec<Draft>) {
    drafts.push(Draft {
        source: None,
        command: None,
        learning: true,
    });
}

/// Fill the source of the first draft currently in Learn mode and
/// auto-stop the learn — the daemon's single-shot mode (#513, Task 11)
/// already disarmed itself on this same event, but we mirror the flag
/// here so the UI repaint reflects it without another round-trip.
pub fn apply_learned_source(drafts: &mut [Draft], source: Source) {
    if let Some(d) = drafts.iter_mut().find(|d| d.learning) {
        d.source = Some(source);
        d.learning = false;
    }
}

/// Promote a draft into the persisted bindings list. Returns `false`
/// (and leaves `bindings` untouched) if the draft is missing either a
/// source or a command — that lets the caller decide whether to keep
/// the draft on screen.
pub fn finalize_draft(bindings: &mut Vec<Binding>, draft: Draft) -> bool {
    let (Some(source), Some(command)) = (draft.source, draft.command) else {
        return false;
    };
    bindings.push(Binding {
        source,
        command,
        args: serde_json::Value::Null,
        scale: None,
    });
    true
}

/// Refresh the Slint model the section binds to. Finalized bindings
/// render first, drafts after — that matches the visual `+` button
/// being at the top and rows growing downward.
pub fn repaint(model: &VecModel<MidiBindingRow>, bindings: &[Binding], drafts: &[Draft]) {
    let mut rows: Vec<MidiBindingRow> = bindings
        .iter()
        .map(|b| MidiBindingRow {
            trigger_label: format_trigger(&b.source).into(),
            command_label: b.command.clone().into(),
            learning: false,
        })
        .collect();
    rows.extend(drafts.iter().map(|d| {
        MidiBindingRow {
            trigger_label: d
                .source
                .as_ref()
                .map(format_trigger)
                .unwrap_or_else(|| String::from("(listening...)"))
                .into(),
            command_label: d.command.clone().unwrap_or_default().into(),
            learning: d.learning,
        }
    }));
    model.set_vec(rows);
}

/// Install the section callbacks on the AppWindow. Mirrors the
/// `midi_devices` wiring pattern — takes the project session so the
/// section gracefully no-ops when no project is loaded.
pub fn install(
    win: &AppWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    bindings: Rc<RefCell<Vec<Binding>>>,
    drafts: Rc<RefCell<Vec<Draft>>>,
    model: Rc<VecModel<MidiBindingRow>>,
) {
    // + Add binding -> push a draft, start learn-mode in the daemon.
    let bindings_for_add = bindings.clone();
    let drafts_for_add = drafts.clone();
    let model_for_add = model.clone();
    let project_session_for_add = project_session.clone();
    win.on_add_midi_binding(move || {
        let mut b = bindings_for_add.borrow_mut();
        let mut d = drafts_for_add.borrow_mut();
        add_draft(&mut b, &mut d);
        repaint(&model_for_add, &b, &d);
        if let Some(session) = project_session_for_add.borrow().as_ref() {
            if let Err(e) = session.dispatcher.dispatch(Command::StartMidiLearn) {
                log::warn!("[midi_mapping] Command::StartMidiLearn failed: {e}");
            }
        }
    });

    // Cancel a draft: drop the row and stop learn-mode if it was active.
    // Slint passes the row index across both bindings + drafts; convert
    // here so the helper layer stays index-agnostic.
    let bindings_for_cancel = bindings.clone();
    let drafts_for_cancel = drafts.clone();
    let model_for_cancel = model.clone();
    let project_session_for_cancel = project_session.clone();
    win.on_cancel_midi_binding_draft(move |row_index| {
        let total = bindings_for_cancel.borrow().len();
        let row = row_index as usize;
        let Some(draft_idx) = row.checked_sub(total) else {
            return;
        };
        let mut d = drafts_for_cancel.borrow_mut();
        if draft_idx < d.len() {
            let was_learning = d[draft_idx].learning;
            d.remove(draft_idx);
            if was_learning {
                if let Some(session) = project_session_for_cancel.borrow().as_ref() {
                    if let Err(e) = session.dispatcher.dispatch(Command::StopMidiLearn) {
                        log::warn!("[midi_mapping] Command::StopMidiLearn failed: {e}");
                    }
                }
            }
        }
        let b = bindings_for_cancel.borrow();
        repaint(&model_for_cancel, &b, &d);
    });

    // Delete a finalized binding.
    let bindings_for_delete = bindings.clone();
    let drafts_for_delete = drafts.clone();
    let model_for_delete = model.clone();
    let project_session_for_delete = project_session.clone();
    win.on_delete_midi_binding(move |binding_index| {
        let mut b = bindings_for_delete.borrow_mut();
        let idx = binding_index as usize;
        if idx < b.len() {
            b.remove(idx);
        }
        let bindings_copy: Vec<Binding> = b.clone();
        let d = drafts_for_delete.borrow();
        repaint(&model_for_delete, &b, &d);
        drop(b);
        drop(d);
        if let Some(session) = project_session_for_delete.borrow().as_ref() {
            if let Err(e) = session.dispatcher.dispatch(Command::SaveMidiMapping {
                bindings: bindings_copy,
            }) {
                log::warn!("[midi_mapping] Command::SaveMidiMapping failed: {e}");
            }
        }
    });

    // Pick a command for a row. If the row points to an existing
    // binding, replace it inline and re-dispatch. If it points to a
    // draft and the draft now has both source + command, finalize and
    // dispatch SaveMidiMapping.
    let bindings_for_pick = bindings;
    let drafts_for_pick = drafts;
    let model_for_pick = model;
    let project_session_for_pick = project_session;
    win.on_pick_midi_binding_command(move |row_index, command_name| {
        let total_bindings = bindings_for_pick.borrow().len();
        let row = row_index as usize;
        if row < total_bindings {
            let mut b = bindings_for_pick.borrow_mut();
            b[row].command = command_name.to_string();
            let bindings_copy: Vec<Binding> = b.clone();
            let d = drafts_for_pick.borrow();
            repaint(&model_for_pick, &b, &d);
            drop(b);
            drop(d);
            if let Some(session) = project_session_for_pick.borrow().as_ref() {
                if let Err(e) = session.dispatcher.dispatch(Command::SaveMidiMapping {
                    bindings: bindings_copy,
                }) {
                    log::warn!("[midi_mapping] Command::SaveMidiMapping failed: {e}");
                }
            }
            return;
        }
        let draft_idx = row - total_bindings;
        let mut d = drafts_for_pick.borrow_mut();
        if draft_idx >= d.len() {
            return;
        }
        d[draft_idx].command = Some(command_name.to_string());
        if d[draft_idx].source.is_some() {
            let draft = d.remove(draft_idx);
            let mut b = bindings_for_pick.borrow_mut();
            finalize_draft(&mut b, draft);
            let bindings_copy: Vec<Binding> = b.clone();
            repaint(&model_for_pick, &b, &d);
            drop(b);
            drop(d);
            if let Some(session) = project_session_for_pick.borrow().as_ref() {
                if let Err(e) = session.dispatcher.dispatch(Command::SaveMidiMapping {
                    bindings: bindings_copy,
                }) {
                    log::warn!("[midi_mapping] Command::SaveMidiMapping failed: {e}");
                }
            }
        } else {
            let b = bindings_for_pick.borrow();
            repaint(&model_for_pick, &b, &d);
        }
    });
}
