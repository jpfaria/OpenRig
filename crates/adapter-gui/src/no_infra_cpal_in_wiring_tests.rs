//! Issue #127: the invariant the whole issue exists to establish — **the UI
//! must not name the audio backend outside the modules that own it.**
//!
//! Tasks 1–8 built the two doors a wiring module is supposed to use:
//!
//! * writes → a `Command` on the bus (`application::command`), applied by the
//!   dispatcher through `application::runtime_control::RuntimeControl`;
//! * reads  → `application::live_source::LiveSource`, implemented for the GUI
//!   by `gui_live_source::GuiLiveSource` and resolved in `application::read`.
//!
//! A module that names `infra_cpal::ProjectRuntimeController` has neither: it
//! reaches the audio engine directly, which is exactly how a capability ends up
//! working in the GUI and silently doing nothing over MCP/gRPC. So the set of
//! modules allowed to name it is an explicit list, and this test pins it in BOTH
//! directions:
//!
//! * a module outside the list that names the backend fails as an offender —
//!   the boundary cannot be crossed by accident;
//! * a listed module that no longer names it fails as stale — the list can only
//!   ever shrink, so it is a ratchet and not a graveyard.
//!
//! Every entry is justified in the ledger below. "It compiles" is not a
//! justification; "this module owns a controller API that has no `Command` and
//! no `LiveSource` reading yet" is.

use std::path::{Path, PathBuf};

/// The identifier no wiring module may name.
const BACKEND: &str = "ProjectRuntimeController";

/// The one-chain runtime sync sequence. Only the module that owns the
/// controller may call it; everyone else asks for it on the bus.
const SYNC_SEQUENCE: &str = "sync_live_chain_runtime(";

/// The modules allowed to name the audio backend, by path relative to
/// `crates/adapter-gui/src`.
///
/// ── Owns the runtime ────────────────────────────────────────────────────────
/// * `runtime_lifecycle.rs` — CREATES, syncs and drops the controller, and
///   hosts `GuiRuntimeControl`, the `RuntimeControl` impl every command handler
///   reaches the audio through. This is the module the invariant exists to
///   concentrate everything into.
/// * `desktop_app.rs` — allocates the one `Rc<RefCell<Option<..>>>` the app
///   shares (`let project_runtime = Rc::new(RefCell::new(None))`) and hands it
///   to the modules below. Someone has to say the type once.
///
/// ── Reads it as the frontend's `LiveSource` ─────────────────────────────────
/// * `gui_live_source.rs` — the read side of the same seam.
/// * `mcp_query_resolver.rs` — builds `GuiLiveSource`, so it names the handle
///   type it is handed.
///
/// ── Controller APIs with no `Command` and no `LiveSource` reading yet ───────
/// Each of these calls a `ProjectRuntimeController` method that has no door on
/// the bus. They are the remaining work, named module by module in the Task 9
/// report, NOT a licence:
/// * audio taps (`subscribe_*_tap`, `poll_stream`): `meter_wiring.rs`,
///   `meter_wiring_poll.rs`, `tuner_session.rs`, `tuner_wiring.rs`,
///   `spectrum_session.rs`, `spectrum_wiring.rs`, `block_editor_window_setup.rs`,
///   `select_chain_block_callback.rs`, `tone_doctor_compact_wiring.rs`
/// * looper transport/PCM store: `looper_wiring.rs`, `looper_callbacks.rs`,
///   `looper_persist.rs`
/// * DI stream arming + engine-rate sync: `di_loop_wiring.rs`,
///   `di_output_select_wiring.rs`, `compact_chain_di_callbacks.rs`
/// * metronome + `ensure_runtime` (#808): `metronome_wiring.rs`
/// * live EQ-viz sample rate (`eq::eq_viz_sample_rate`): `eq.rs`,
///   `block_choose_type_callback.rs`, `block_insert_callbacks.rs`,
///   `block_parameter_wiring.rs`, `block_editor_window_params.rs`,
///   `block_editor_window_lifecycle.rs`
/// * runtime health / stream errors on the poll tick: `desktop_app_polling.rs`
/// * whole-project sync (`sync_project_runtime`): `settings/audio.rs`
/// * runtime teardown (`stop_project_runtime`) and the session-install attach:
///   `back_to_launcher_wiring.rs`, `project_file_dialog_wiring.rs`,
///   `recent_projects_wiring.rs`
/// * `remove_live_chain_runtime` on chain delete: `chain_row_wiring.rs`,
///   `compact_chain_delete_wiring.rs`
/// * pure hubs that only forward the handle to a module above:
///   `chain_rig_nav_wiring.rs`, `compact_chain_callbacks.rs`,
///   `desktop_app_block_wiring.rs`, `desktop_app_chain_wiring.rs`
const ALLOWED: &[&str] = &[
    "back_to_launcher_wiring.rs",
    "block_choose_type_callback.rs",
    "block_editor_window_lifecycle.rs",
    "block_editor_window_params.rs",
    "block_editor_window_setup.rs",
    "block_insert_callbacks.rs",
    "block_parameter_wiring.rs",
    "chain_rig_nav_wiring.rs",
    "chain_row_wiring.rs",
    "compact_chain_callbacks.rs",
    "compact_chain_delete_wiring.rs",
    "compact_chain_di_callbacks.rs",
    "desktop_app.rs",
    "desktop_app_block_wiring.rs",
    "desktop_app_chain_wiring.rs",
    "desktop_app_polling.rs",
    "di_loop_wiring.rs",
    "di_output_select_wiring.rs",
    "eq.rs",
    "gui_live_source.rs",
    "looper_callbacks.rs",
    "looper_persist.rs",
    "looper_wiring.rs",
    "mcp_query_resolver.rs",
    "meter_wiring.rs",
    "meter_wiring_poll.rs",
    "metronome_wiring.rs",
    "project_file_dialog_wiring.rs",
    "recent_projects_wiring.rs",
    "runtime_lifecycle.rs",
    "select_chain_block_callback.rs",
    "settings/audio.rs",
    "spectrum_session.rs",
    "spectrum_wiring.rs",
    "tone_doctor_compact_wiring.rs",
    "tuner_session.rs",
    "tuner_wiring.rs",
];

/// A module's CODE, with `//` comments stripped.
///
/// This is not defensive tidiness. The first version of the sibling guard in
/// `tests/issue_127_drain_reaches_the_runtime_on_the_bus.rs` searched the RAW
/// source, and the comment explaining the call it was looking for contained the
/// identifier — so deleting the real call still passed. An assertion about code
/// must never be satisfiable by prose, in either direction: a module cleaned of
/// the backend must not be kept "clean" only in a comment, and a module that
/// merely *mentions* the backend in a doc comment must not be forced onto the
/// allowlist.
fn code_only(src: &str) -> String {
    src.lines()
        .map(|line| match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every non-test module under `src`, recursively, as
/// (path relative to `src`, code with comments stripped).
///
/// Recursive on purpose: a flat `read_dir` would let `settings/audio.rs` — a
/// real offender — walk straight past the guard.
fn wiring_modules() -> Vec<(String, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    collect(&root, &root, &mut found);
    found.sort();
    found
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect(root, &path, out);
            continue;
        }
        let name = path
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .to_string();
        // `*_tests.rs` may drive the controller directly — a test is allowed to
        // stand in for the frontend that hosts it.
        if !name.ends_with(".rs") || name.ends_with("_tests.rs") {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .expect("under src")
            .to_string_lossy()
            .replace('\\', "/");
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        out.push((rel, code_only(&src)));
    }
}

/// The invariant: outside the allowlist, no `adapter-gui` module names the
/// audio backend. A wiring module that needs the engine dispatches a `Command`
/// or reads through `GuiLiveSource`.
#[test]
fn wiring_modules_do_not_name_the_audio_backend() {
    let offenders: Vec<String> = wiring_modules()
        .into_iter()
        .filter(|(rel, code)| !ALLOWED.contains(&rel.as_str()) && code.contains(BACKEND))
        .map(|(rel, _)| rel)
        .collect();
    assert!(
        offenders.is_empty(),
        "these modules still name `{BACKEND}` — a wiring module reaches the \
         audio runtime through a `Command` (writes) or `GuiLiveSource` (reads), \
         never through `infra_cpal` directly: {offenders:#?}"
    );
}

/// The other direction, so the list is a ratchet and not a graveyard: an
/// allowlisted module that no longer names the backend must leave the list in
/// the same commit that cleans it. Without this, the list keeps its size
/// forever and the invariant quietly stops being about anything.
#[test]
fn the_allowlist_carries_nothing_it_no_longer_needs() {
    let modules = wiring_modules();
    let stale: Vec<&str> = ALLOWED
        .iter()
        .copied()
        .filter(|allowed| {
            match modules.iter().find(|(rel, _)| rel == allowed) {
                Some((_, code)) => !code.contains(BACKEND),
                // A listed module that no longer exists is stale too.
                None => true,
            }
        })
        .collect();
    assert!(
        stale.is_empty(),
        "these modules no longer name `{BACKEND}` (or no longer exist) and must \
         be removed from ALLOWED: {stale:#?}"
    );
}

/// The behavioural half of the same invariant, and the one that is fully
/// closed: **there is exactly ONE road from the UI to a chain's live runtime.**
///
/// `sync_live_chain_runtime` is the sequence that resolves devices, schedules
/// activation and rebuilds a chain's DSP. Before #127 it was called from ~24 UI
/// callbacks AND from the external-event drain, so the GUI reached the audio by
/// one road and MCP/MIDI by another — the split that lets one road quietly gain
/// or lose a step. Every one of those call sites now dispatches
/// `ChainCommand::SyncChainRuntime`, which the dispatcher applies through
/// `RuntimeControl`, whose GUI impl (`GuiRuntimeControl::sync_chain`, in
/// `runtime_lifecycle.rs`) is the single remaining caller.
///
/// This is asserted on CODE, so a comment naming the function — several still
/// do, and should — cannot make it pass or fail.
#[test]
fn only_the_runtime_owner_calls_the_chain_sync_sequence() {
    let callers: Vec<String> = wiring_modules()
        .into_iter()
        .filter(|(rel, code)| rel != "runtime_lifecycle.rs" && code.contains(SYNC_SEQUENCE))
        .map(|(rel, _)| rel)
        .collect();
    assert!(
        callers.is_empty(),
        "these modules call `{SYNC_SEQUENCE}` directly instead of asking for the \
         sync on the bus (`runtime_sync_policy::request_chain_sync` → \
         `ChainCommand::SyncChainRuntime`) — that is two roads to the same audio \
         again: {callers:#?}"
    );
}
