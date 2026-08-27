//! Responsibility: builds what the metronome window shows.
//! What the metronome window SHOWS (#14) — the knob vocabulary and the output
//! endpoints the click can play through. No state lives here.
//!
//! #127: this module used to own a `MetronomeSession` — the settings, the
//! chosen output and the tap history — which is why a footswitch or an MCP
//! client could flip `metronome_enabled` and hear nothing: only the GUI held
//! the settings, and only its own knob callbacks knew how to turn an event
//! into sound. That state now belongs to the dispatcher
//! (`application::metronome_state`), and this module renders whatever snapshot
//! it hands over.
//!
//! What remains is genuinely the window's: the index↔key↔label translation for
//! the three knobs (the knobs speak indices, the `Command`s speak strings like
//! `"eighths"`), and the project's output endpoints. Every conversion lives
//! here so no call site has to remember the order of a list.
//!
//! The beat lamps are NOT driven from here: the wiring's timer reads the click's
//! position through `LiveSource` — a phase, not a queue of events — so a slow
//! frame can never lose or double a beat.
pub use crate::metronome_outputs::{output_endpoints, resolve_output_endpoint, MetronomeOutput};
pub use crate::metronome_vocabulary::{
    subdivision_index, subdivision_key, subdivision_label, timbre_index, timbre_key, timbre_label,
    time_signature_beats, time_signature_index, time_signature_label,
};

#[cfg(test)]
#[path = "metronome_view_tests.rs"]
mod tests;
