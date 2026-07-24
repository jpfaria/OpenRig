//! Single-shot MIDI learn-mode flag shared between the adapter wiring and
//! the daemon callback (#513 / #493).
//!
//! While `is_active()` is true, the daemon publishes each raw incoming MIDI
//! event as `MidiCommand::PublishMidiEvent` through the existing
//! [`application::bridge::CommandBridge`] instead of resolving it through the
//! binding map, then calls [`LearnState::on_event_captured`] to auto-disarm
//! after the first event.
//!
//! The state is held behind a process-wide `LazyLock<Arc<LearnState>>` exposed
//! by [`learn_state`]. The choice is deliberate: there is exactly one MIDI
//! daemon per process, the daemon may not be spawned (the `--midi` opt-in is
//! absent) yet the GUI still has to call `start()` / `stop()` safely from the
//! Slint thread, and the daemon callback runs on a `midir` thread that has no
//! handle to the GUI's session struct. A single `Arc` is the smallest API
//! both sides can share without plumbing a new channel.
//!
//! Hot path is lock-free: a single `AtomicBool` with `SeqCst` ordering. No
//! `Mutex`, no allocation, no syscall — same invariants as the rest of the
//! `midir` callback (see CLAUDE.md "Invariantes que NUNCA podem piorar").

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

/// Atomic single-shot flag. See module docs.
#[derive(Default)]
pub struct LearnState {
    active: AtomicBool,
}

impl LearnState {
    /// Arm learn-mode. Idempotent — calling twice without an event between
    /// is a no-op.
    pub fn start(&self) {
        self.active.store(true, Ordering::SeqCst);
    }

    /// Disarm learn-mode. Idempotent.
    pub fn stop(&self) {
        self.active.store(false, Ordering::SeqCst);
    }

    /// Is the daemon currently publishing raw events instead of resolving
    /// them through the binding map?
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Called once after a single MIDI event has been captured; auto-resets
    /// the flag so subsequent events fall back to the normal resolution
    /// path. Equivalent to `stop()` — named separately so the daemon
    /// callback site documents intent.
    pub fn on_event_captured(&self) {
        self.stop();
    }
}

/// Process-wide `Arc<LearnState>`. Both the GUI wiring (which flips the flag
/// in response to `Event::MidiLearnStarted` / `Event::MidiLearnStopped`) and
/// the daemon callback (which consults the flag on every incoming MIDI byte
/// slice) clone this `Arc`.
pub fn learn_state() -> Arc<LearnState> {
    static STATE: LazyLock<Arc<LearnState>> = LazyLock::new(|| Arc::new(LearnState::default()));
    STATE.clone()
}
