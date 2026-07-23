//! Issue #731 — a config write enqueued under one `$HOME` must never
//! land in a different `$HOME`.
//!
//! Recurrence of #701: `SaveAudioSettings` persists `config.yaml` on the
//! async persist worker. If the worker resolves `app_config_path()` at
//! WRITE time instead of at DISPATCH time, then any test that swaps
//! `$HOME` to a tempdir and restores it before the worker drains (the
//! `with_tmp_home` pattern, which does NOT flush inside the swap) leaks
//! the queued write onto the user's REAL `~/Library/Application
//! Support/OpenRig/config.yaml`. The symptom looks like "settings vanish
//! on every open".
//!
//! Contract under test: the destination path is bound at dispatch time.
//! Swap `$HOME` from A to B AFTER dispatch but BEFORE flush — the write
//! must land in A and B must stay untouched.
#![cfg(unix)]

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use project::project::Project;

/// True if any `config.yaml` exists anywhere under `root`.
fn contains_config_yaml(root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if contains_config_yaml(&path) {
                return true;
            }
        } else if path.file_name().is_some_and(|n| n == "config.yaml") {
            return true;
        }
    }
    false
}

#[test]
fn issue_731_save_audio_settings_write_does_not_leak_to_swapped_home() {
    let home_a = tempfile::TempDir::new().expect("home_a");
    let home_b = tempfile::TempDir::new().expect("home_b");
    let prev = std::env::var_os("HOME");
    // On Linux `dirs::config_dir()` (used by `app_config_path()`) honours
    // $XDG_CONFIG_HOME over $HOME/.config, and CI runners may set it — so a
    // HOME-only swap would leak to the runner's real config dir, not the
    // tempdir. Track XDG alongside HOME so the config path follows the swap.
    let prev_xdg = std::env::var_os("XDG_CONFIG_HOME");

    // Dispatch under HOME = A: the write targets A's config.yaml.
    std::env::set_var("HOME", home_a.path());
    std::env::set_var("XDG_CONFIG_HOME", home_a.path().join(".config"));
    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    })));
    let _ = dispatcher.dispatch(Command::SaveAudioSettings {
        input_devices: Vec::new(),
        output_devices: Vec::new(),
    });

    // Swap to HOME = B before the worker drains — exactly what
    // `with_tmp_home` does when it restores `$HOME` post-closure.
    std::env::set_var("HOME", home_b.path());
    std::env::set_var("XDG_CONFIG_HOME", home_b.path().join(".config"));
    application::persist_worker::flush();

    // Restore the real $HOME/$XDG before asserting (never leave them dangling).
    match prev {
        Some(p) => std::env::set_var("HOME", p),
        None => std::env::remove_var("HOME"),
    }
    match prev_xdg {
        Some(p) => std::env::set_var("XDG_CONFIG_HOME", p),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
    }

    assert!(
        !contains_config_yaml(home_b.path()),
        "config write enqueued under HOME=A leaked into HOME=B — the \
         persist worker resolved app_config_path() at write time instead \
         of binding it at dispatch (issue #731 / #701)"
    );
    assert!(
        contains_config_yaml(home_a.path()),
        "config write enqueued under HOME=A never reached A — the write \
         was misrouted (issue #731)"
    );
}
