//! Match pipeline (issue #548 Phase 4).
//!
//! Given a raw incoming MIDI message + the MIDI port it came from + the
//! set of profiles the user has active, returns every binding that
//! fires. The daemon then converts each hit into a `Command` via
//! `slots::slot_to_command` and dispatches.
//!
//! Match rules (see `docs/superpowers/specs/2026-05-26-midi-profiles-design.md`):
//! - `profile.source: Some(s)` matches when `s.is_substring(&port_name)`.
//! - `profile.source: None` matches any port.
//! - `kind` and `channel` always match by value.
//! - Per-kind value (`note` / `controller` / `program`) is exact when
//!   `Some(v)`, wildcard when `None` (matches any byte).
//! - Two profiles binding the same message both fire — no exclusivity.

use application::bridge::CommandBridge;
use application::dispatcher::CommandDispatcher;
use application::SelectionState;

use crate::profile::{MatchExpr, MidiProfile};
use crate::slots::{slot_to_command, IncomingMessage};

/// One profile binding that fired for an incoming message.
#[derive(Debug, Clone)]
pub struct SlotHit {
    /// The slot name from the matched binding (e.g. `"prev_preset"`).
    pub slot: String,
    /// The original message, passed through so the slot can read its
    /// value byte (used by `jump_preset_n` / continuous CC slots).
    pub message: IncomingMessage,
}

fn matches(expr: &MatchExpr, msg: &IncomingMessage) -> bool {
    match (expr, msg) {
        (
            MatchExpr::NoteOn {
                channel,
                note: filter,
            },
            IncomingMessage::NoteOn {
                channel: c,
                note,
                ..
            },
        ) => *channel == *c && filter.map(|f| f == *note).unwrap_or(true),
        (
            MatchExpr::NoteOff {
                channel,
                note: filter,
            },
            IncomingMessage::NoteOff { channel: c, note },
        ) => *channel == *c && filter.map(|f| f == *note).unwrap_or(true),
        (
            MatchExpr::ControlChange {
                channel,
                controller: filter,
            },
            IncomingMessage::ControlChange {
                channel: c,
                controller,
                ..
            },
        ) => *channel == *c && filter.map(|f| f == *controller).unwrap_or(true),
        (
            MatchExpr::ProgramChange {
                channel,
                program: filter,
            },
            IncomingMessage::ProgramChange {
                channel: c,
                program,
            },
        ) => *channel == *c && filter.map(|f| f == *program).unwrap_or(true),
        _ => false,
    }
}

fn port_matches(profile_source: Option<&str>, port_name: &str) -> bool {
    profile_source
        .map(|s| port_name.contains(s))
        .unwrap_or(true)
}

/// Walk every active profile, collect each binding that fires.
pub fn match_message(
    active_profiles: &[&MidiProfile],
    port_name: &str,
    msg: &IncomingMessage,
) -> Vec<SlotHit> {
    let mut hits = Vec::new();
    for profile in active_profiles {
        if !port_matches(profile.source.as_deref(), port_name) {
            continue;
        }
        for binding in &profile.bindings {
            if matches(&binding.when, msg) {
                hits.push(SlotHit {
                    slot: binding.action.clone(),
                    message: *msg,
                });
            }
        }
    }
    hits
}

/// End-to-end connector: raw message + active profiles + selection
/// snapshot → dispatch every command this message produces. Daemon
/// (real MIDI input) calls this once per incoming message; tests use it
/// to prove the full pipeline without spinning up a real MIDI port.
///
/// Each `SlotHit` independently produces (or not) a Command via
/// `slot_to_command`; slots that need an active id without one return
/// `None` and dispatch nothing. Dispatch errors are silently dropped —
/// MIDI input is best-effort by design; the audio path keeps running.
pub fn dispatch_midi_message(
    active_profiles: &[&MidiProfile],
    port_name: &str,
    msg: &IncomingMessage,
    selection: &SelectionState,
    dispatcher: &dyn CommandDispatcher,
) {
    for hit in match_message(active_profiles, port_name, msg) {
        if let Some(cmd) = slot_to_command(&hit.slot, &hit.message, selection) {
            let _ = dispatcher.dispatch(cmd);
        }
    }
}

/// Same as [`dispatch_midi_message`] but submits over the
/// `CommandBridge` instead of calling a `CommandDispatcher` directly.
/// The MIDI daemon runs on a separate `midir`-callback thread; a
/// `LocalDispatcher` is `!Send`, so the callback owns a
/// `CommandBridge::clone` (which is `Send`) and uses this connector.
/// The frontend thread drains the bridge and dispatches normally.
pub fn dispatch_midi_message_to_bridge(
    active_profiles: &[&MidiProfile],
    port_name: &str,
    msg: &IncomingMessage,
    selection: &SelectionState,
    bridge: &CommandBridge,
) {
    for hit in match_message(active_profiles, port_name, msg) {
        if let Some(cmd) = slot_to_command(&hit.slot, &hit.message, selection) {
            let _ = bridge.submit(cmd);
        }
    }
}
