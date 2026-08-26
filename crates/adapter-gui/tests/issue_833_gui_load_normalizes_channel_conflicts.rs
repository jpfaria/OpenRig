//! #833: the GUI load path must apply the same channel-conflict normalization
//! the dispatcher's `LoadProject` applies — a project must never come up with
//! one physical input captured by two enabled chains.
//!
//! `load_project_session` reads the binding registry from the real per-machine
//! `config.yaml`, so an end-to-end assertion here would have to swap `$HOME`
//! (forbidden: it clobbers the owner's real config). The behavior itself is
//! covered by `project::input_channel_conflict` unit tests and by the
//! dispatcher integration test `chain_enable_channel_conflict.rs`; what this
//! test pins is the WIRING — that the GUI load path still calls it.

use std::path::PathBuf;

fn read_src(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn gui_load_path_disables_chains_that_contend_for_one_input() {
    let src = read_src("project_load_normalize.rs");
    assert!(
        src.contains("disable_conflicting_chains"),
        "issue #833: the GUI load-time normalization must call \
         `engine::runtime_endpoints::disable_conflicting_chains` against the \
         binding registry, like the dispatcher's LoadProject handler does. Without \
         it a project saved with two enabled chains on one device+channel opens with \
         the same guitar live in two runtimes."
    );
}

#[test]
fn compact_view_surfaces_the_rejected_toggle_instead_of_only_logging_it() {
    // The chains-screen row already toasts the dispatch error; the compact view
    // swallowed it into `log::error!`, so a refused enable looked like a dead
    // switch — the exact confusion #833 is about.
    let src = read_src("compact_chain_block_handlers.rs");
    let at = src
        .find("ToggleChainEnabled")
        .expect("the compact view toggles chains via the command bus");
    let dispatch_site = &src[at..(at + 600).min(src.len())];
    assert!(
        dispatch_site.contains("set_status_error"),
        "issue #833: a refused `ToggleChainEnabled` must reach the user in the \
         compact view too (toast via `set_status_error`), not just the log."
    );
}

#[test]
fn gui_load_path_runs_the_normalization() {
    // `load_project_session` moved to `project_session_load.rs` when
    // `project_ops.rs` was split by responsibility (#873); the contract is
    // unchanged — the loader still has to run the normalization.
    let src = read_src("project_session_load.rs");
    assert!(
        src.contains("normalize_loaded_project"),
        "issue #833: `load_project_session` must run \
         `project_load_normalize::normalize_loaded_project` on the freshly-read \
         project, or none of the load-time passes (#606 unavailable blocks, #833 \
         contended inputs) reach a GUI open."
    );
}
