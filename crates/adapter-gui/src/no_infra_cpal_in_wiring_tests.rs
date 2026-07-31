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
/// * `runtime_pipelines.rs` — the SAME seam, split by pipeline kind: the
///   bodies of the doors for the independent pipelines (invariant #4), the DI
///   loop and the metronome click. It left `runtime_lifecycle.rs` in Task 12b
///   because that file had reached its line cap, and it is an owner, not a
///   wiring module: nothing here is reached except through a `Command`.
/// * `runtime_teardown.rs` — the SAME seam again, for the two ways a rig STOPS
///   (Task 14): the whole rig (`stop_project_runtime`) and one chain
///   (`remove_live_chain_runtime`). It left `runtime_lifecycle.rs` because that
///   file had reached its line cap, and it is an owner, not a wiring module:
///   nothing in it is reachable except through a `Command`.
/// * `runtime_loopers.rs` — the same again for the looper doors (Task 13): the
///   store mutations, the playback reconcile that ends each of them, the PCM
///   export handed to the project save, and the restore that gives a
///   freshly-created runtime its loops back. Also an owner, for the same
///   reason: nothing in it is reachable except through a `Command` (or, for
///   the restore, through the controller's own creation).
/// * `runtime_taps.rs` — the implementation of the THIRD door, the
///   subscription seam (`application::audio_taps`, Task 16). It is the one
///   place that turns a `TapPoint` into a controller subscription, and the
///   `AudioTap` it hands back wraps the very rings the consumers used to hold
///   themselves. An owner for the `runtime_health.rs` reason: it is the
///   implementation of a seam, not a caller of one.
/// * `runtime_health.rs` — the same again for the frontend's own poll tick
///   (Task 15): installing a rebuild the control worker finished, and
///   reconnecting a backend that died. An owner for a slightly different
///   reason than the three above — neither door is reached through a
///   `Command`, because neither is one (nobody asks for a tick) — but the same
///   rule holds: it is the implementation of the seam, not a caller of it.
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
/// * a block's diagnostic stream (`poll_stream`):
///   `block_editor_window_setup.rs`, `select_chain_block_callback.rs`
/// * the meter tick's remaining per-chain reads and its looper reconcile:
///   `meter_wiring_poll.rs`
/// * pure hubs that only forward the handle to a module above:
///   `compact_chain_callbacks.rs` (which also polls the controller itself),
///   `desktop_app_block_wiring.rs`, `desktop_app_chain_wiring.rs`,
///   `block_choose_type_callback.rs` (hands it to `block_editor_window_setup`
///   when the ADD flow opens a detached editor, #815)
///
/// Task 11 emptied the EQ-viz bucket: the block editor's curve now takes its
/// rate from `CommandDispatcher::engine_sr`, so `eq.rs`,
/// `block_insert_callbacks.rs`, `block_parameter_wiring.rs`,
/// `block_editor_window_params.rs` and `block_editor_window_lifecycle.rs` left
/// this list. `block_choose_type_callback.rs` stayed for the OTHER reason
/// above — it still forwards the controller handle it is given.
///
/// Task 12 emptied the DI-arming bucket. `RuntimeControl` gained
/// `arm_di_stream` / `disarm_di_stream` / `refresh_di_stream`, applied by the
/// `SetChainDiLoopEnabled` / `SetChainDiLoopOutput` handlers and by
/// `attach_engine_sr` on a device-rate change, so `di_loop_wiring.rs`,
/// `di_output_select_wiring.rs` and `compact_chain_di_callbacks.rs` now only
/// dispatch. The engine-rate publish (`sync_engine_sr_from_runtime`) moved to
/// `runtime_lifecycle.rs`, and #808's `ensure_runtime` became the arm's
/// precondition instead of something a UI callback runs first.
///
/// Task 13 emptied the looper bucket: `looper_wiring.rs` and
/// `looper_persist.rs` are GONE (their contents moved to their owners), and
/// `looper_callbacks.rs` only dispatches and redraws. `RuntimeControl` gained
/// `create_looper` / `remove_looper` / `looper_transport` / `set_looper_param`
/// / `set_looper_input` / `set_looper_output`, applied by the `LooperCommand`
/// handlers, plus `export_chain_loops`, applied by `ProjectCommand::SaveProject`
/// — so a footswitch, an MCP client and the panel all record, play, clear and
/// SAVE the same loop. The drain's second copy of the store mutation is gone
/// with them. The panel re-reads the loops' transport state through
/// `LiveSource::chain_loopers` (`gui_live_source::LooperLiveSource`), the same
/// reading MCP gets; the recorded PCM never crosses that seam — it moves as an
/// `Arc<LoopPcm>` handle through the write door.
///
/// Task 14 emptied THREE buckets and cleared seven modules. `RuntimeControl`
/// gained `stop_project_runtime` (applied by the new
/// `ProjectCommand::StopProjectRuntime` AND by `CloseProject`, so a rig started
/// over MCP can be stopped over MCP), `sync_project` (applied by
/// `SettingsCommand::SaveAudioSettings`, so a device-settings change re-opens
/// the running graph on every transport instead of only from the settings
/// screen) and `remove_chain` (applied by `ChainCommand::RemoveChain`, so a
/// deleted chain stops sounding whoever deleted it). `remove_chain` is
/// deliberately NOT `sync_chain` for a missing chain: that lookalike validates
/// the whole project, so an unrelated invalid chain would abort the teardown.
///
/// So `back_to_launcher_wiring.rs`, `settings/audio.rs`, `chain_row_wiring.rs`
/// and `compact_chain_delete_wiring.rs` only dispatch now. The three modules
/// that OPEN projects — `project_file_dialog_wiring.rs`,
/// `recent_projects_wiring.rs` and `chain_rig_nav_wiring.rs` (the drain) —
/// still have to install the seam on a freshly built session, so they hold
/// `runtime_lifecycle::RuntimeAttach`: a capability whose handle is private and
/// whose only operation is `to_session`. A module holding one cannot start,
/// stop, sync or read the audio, which is the distinction this guard is about.
/// `chain_row_wiring.rs` additionally stopped forwarding the handle: the looper
/// panel takes an `Rc<dyn LiveSource>` built by `desktop_app`, and Tone
/// Doctor's main-page wiring moved to `desktop_app` too (it is a tap consumer,
/// Task 16).
///
/// Task 12b emptied that bucket: `metronome_wiring.rs` is gone from the list.
/// The click's whole lifecycle moved onto the bus — the dispatcher owns the
/// settings (`application::metronome_state`) and applies them through
/// `RuntimeControl::start_metronome` / `stop_metronome` /
/// `set_metronome_settings` / `refresh_metronome_output`, so a MIDI footswitch
/// and an MCP client start the same click the knob does. The beat lamps read
/// the click's position through `LiveSource::metronome` instead of the
/// controller's shared cell. The bodies of those doors live in
/// `runtime_pipelines.rs`, listed above as an owner.
///
/// Task 16 built the SUBSCRIPTION seam (`application::audio_taps`) — the third
/// door, for the one thing neither of the other two can express: a standing
/// tap. A consumer asks for a subscription BY STREAM IDENTITY (`TapPoint`) and
/// polls it on its own tick; `AudioTap::poll_peak_dbfs` is the reduced reading
/// any transport can carry, and `drain_channel` (raw window) defaults to
/// nothing, because samples are an in-process affordance — a remote frontend is
/// served the analyzers' RESULTS through `LiveSource::tuner` /
/// `LiveSource::spectrum` and never needs the PCM to travel. So `meter_wiring`,
/// `tuner_session`, `tuner_wiring`, `spectrum_session`, `spectrum_wiring` and
/// `tone_doctor_compact_wiring` are off this list, and `runtime_taps.rs` is on
/// it as the seam's owner.
///
/// Task 15 emptied the poll-tick bucket: `desktop_app_polling.rs` is off this
/// list. The tick was doing two different things through one handle, and they
/// are now split by what they ARE. The block errors and the backend's health
/// are READS (`LiveSource::block_errors` / `audio_health`, implemented by
/// `gui_live_source::HealthLiveSource`) — and the error read DRAINS, so it has
/// exactly one consumer and deliberately did not become a `QueryKind`.
/// Installing a rebuild the control worker finished (#672) and reconnecting a
/// dead backend are WRITES (`RuntimeControl::apply_finished_rebuilds` /
/// `reconnect_audio`, bodies in `runtime_health.rs`) — not `Command`s, because
/// a tick is nobody's request and a reconnect changes no project state.
const ALLOWED: &[&str] = &[
    "block_choose_type_callback.rs",
    "block_editor_window_setup.rs",
    "compact_chain_callbacks.rs",
    "desktop_app.rs",
    "desktop_app_block_wiring.rs",
    "desktop_app_chain_wiring.rs",
    "gui_live_source.rs",
    "mcp_query_resolver.rs",
    "meter_wiring_poll.rs",
    "runtime_health.rs",
    "runtime_lifecycle.rs",
    "runtime_loopers.rs",
    "runtime_pipelines.rs",
    "runtime_taps.rs",
    "runtime_teardown.rs",
    "select_chain_block_callback.rs",
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
