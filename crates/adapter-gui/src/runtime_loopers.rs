//! Responsibility: implements the looper doors of the runtime seam.
//! #127/#323: the looper doors' bodies — how a `LooperCommand` reaches the
//! controller's looper store, and how a project's loops leave and re-enter it.
//!
//! Looper state lives in the controller-owned store, NOT in the project: the
//! project only remembers that a looper exists and where its knobs are. So
//! every command has a runtime half, and before #127 that half was applied by
//! the GUI callback that had just dispatched (with the external-event drain
//! running a second, parallel copy). A looper driven over MCP or from a MIDI
//! footswitch mutated nothing.
//!
//! Everything here is reached through `application::runtime_control::RuntimeControl`
//! — a wiring module never calls it. Three rules hold across the file:
//!
//! * **isolation.** Every door is handed ONE chain and touches only that
//!   chain's store entries and its own isolated playback stream. Nothing is
//!   ever selected by sample rate or by "all that match" (`CLAUDE.md` LAW).
//! * **the reconcile belongs to the door.** Every mutation ends by syncing
//!   THAT chain's playback streams, so a closed loop sounds and a removed one
//!   goes quiet on the user's action, not on the next ~15 Hz meter tick.
//! * **#808: only a start may wake audio.** The wake itself is not here — it
//!   is the `ensure_runtime` precondition `GuiRuntimeControl` runs before
//!   [`create`] and before a Record / Play / PlayStop [`transport`], and
//!   before nothing else. A looper the user cannot record into is not a
//!   looper, and the panel arms REC only against a live store; stopping,
//!   clearing, undoing or turning a knob is never a reason to open a device.
//!   Everything in this file is a store mutation that no-ops when nothing is
//!   running.
//!
//! Nothing here runs on the audio thread: the store is mutated on the
//! dispatching thread and the callback reads it lock-free.

pub use crate::looper_commands::{
    apply_edit, create, redo_edit, remove, set_input, set_output, set_param, transport,
    transport_may_start_audio, undo_edit,
};
pub use crate::looper_restore::export_chain_loops;
pub(crate) use crate::looper_restore::{reconcile_chain_loopers, restore_project_loops};

// The test modules hang off this path and reach these through `super::`.
#[cfg(test)]
pub(crate) use crate::looper_restore::restore_chain_loops;

#[cfg(test)]
#[path = "runtime_loopers_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "runtime_loopers_808_tests.rs"]
mod tests_808;

#[cfg(test)]
#[path = "runtime_loopers_826_tests.rs"]
mod tests_826;
