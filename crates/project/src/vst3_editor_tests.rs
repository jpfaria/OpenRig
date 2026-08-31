//! #913 — the VST3 facade the adapter layer is allowed to call.
//!
//! `adapter-gui` must not depend on `vst3-host` directly, so these three are
//! the whole surface. What must hold: marking the main thread and draining the
//! deferred teardowns are safe to call at any time and in any order — the
//! drain runs on every frontend tick, including before any plugin was ever
//! instantiated, and a drain that assumed work was queued would take the UI
//! thread down on the first tick (#778).

use super::{drain_deferred_vst3_teardowns, mark_main_thread};

#[test]
fn draining_before_any_plugin_existed_does_nothing() {
    drain_deferred_vst3_teardowns();
}

#[test]
fn marking_the_main_thread_is_idempotent() {
    mark_main_thread();
    mark_main_thread();
}

#[test]
fn the_frontend_tick_can_drain_repeatedly_after_marking() {
    mark_main_thread();
    for _ in 0..3 {
        drain_deferred_vst3_teardowns();
    }
}
