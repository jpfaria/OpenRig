//! #881 — the owner's rule: "sempre que a gente alterar a chain tem que matar a
//! antiga e criar uma nova stream do zero — só que você pode matar depois que a
//! outra levantar".
//!
//! The controller honours the second half already: an activation builds its
//! streams off-thread, starts them, and installing the result drops the previous
//! set at that moment. The GUI defeated it by calling `remove_chain` at the
//! REQUEST, before the new build even started, so every topology edit (bypassing
//! the insert, re-binding an E/S) opened a hole of silence: "quando eu ativo e
//! desativo o bloco o som morre e depois volta".
//!
//! The live path is real streams and a real controller; the crate's
//! source-presence convention (see `issue_622_model_pick_resizes_window`) pins
//! the policy where an assertion cannot reach.

use std::path::PathBuf;

fn read_src(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn the_enable_path_never_drops_the_streams_before_rebuilding() {
    let src = read_src("runtime_lifecycle.rs");
    let enable_branch = src
        .split_once("LiveSyncAction::Enable")
        .expect("runtime_lifecycle must handle LiveSyncAction::Enable")
        .1;
    let enable_branch = enable_branch
        .split_once("LiveSyncAction::")
        .map(|(branch, _)| branch)
        .unwrap_or(enable_branch);

    assert!(
        !enable_branch.contains("remove_chain"),
        "#881: an edit must not tear the live streams down before the new ones \
         exist. `schedule_chain_activation` builds off-thread and the install \
         swaps the sets — new up, then old down. Calling `remove_chain` here is \
         the silence the owner hears on every block toggle:\n{enable_branch}"
    );
}
