//! #913 — the refresh is a no-op until the wiring registers its handles.
//!
//! `Command::RefreshAudioDevices` can arrive from MCP before the window is
//! wired (the drain timer runs from startup) and after it closed. Both reach
//! `refresh_now`, so the unregistered case must return quietly — the earlier
//! shape, where the work lived inside the Slint callbacks, is exactly what left
//! the command with nothing to run (#614/#829).

use super::refresh_now;

#[test]
fn a_refresh_before_the_wiring_registered_does_nothing() {
    refresh_now(false);
}

#[test]
fn a_refresh_that_also_targets_the_settings_window_is_equally_safe_unregistered() {
    refresh_now(true);
}

#[test]
fn repeated_refreshes_before_wiring_stay_quiet() {
    for _ in 0..3 {
        refresh_now(false);
    }
}
