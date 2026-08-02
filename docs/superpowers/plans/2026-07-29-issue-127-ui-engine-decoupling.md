# UI/Engine Decoupling Implementation Plan (#127, folds #831)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Slint UI talks to the engine only through traits — `dyn CommandDispatcher` for writes and one `LiveSource` + core resolver for reads — so a non-local implementation can be swapped in without touching UI code.

**Architecture:** Writes already flow through `Command`/`CommandDispatcher`; this plan makes the GUI hold the trait object instead of `LocalDispatcher`, introduces `application::live_source::LiveSource` for state only a frontend hosts (meters, analyzers, DI, loopers, devices), moves the single `QueryKind` match into `application::read::resolve`, and converts the GUI's remaining direct `ProjectRuntimeController` control calls into `Command`s.

**Tech Stack:** Rust 2021, Slint (adapter-gui), cpal/JACK (infra-cpal), `application` crate as the transport-agnostic core.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-29-issue-127-ui-engine-decoupling-design.md`.
- Repo content in English (code, comments, commits, docs).
- Zero warnings: `cargo build` must stay clean; `cargo clippy` clean for touched crates.
- TDD red-first: every task starts with a test that fails before the implementation exists.
- No behavior change on the desktop; no change to the audio thread, DSP, routing, or stream isolation. No allocation/lock/IO added to any audio callback.
- PCM never crosses `LiveSource` — reduced readings only (dBFS, note/cents, band levels).
- Silent meters use `engine::output_meter::SILENT_DBFS`, never a literal `-120.0`.
- Test modules follow the repo convention: `#[cfg(test)] #[path = "<module>_tests.rs"] mod tests;` at the bottom of the module file.
- Commit after each task, push after each commit, then `gh issue comment 127` with hash + files + test result.
- Branch: `feature/issue-127`, workspace `.solvers/issue-127/`.

---

## File Structure

**New (crate `application`):**
- `crates/application/src/live_source.rs` — the `LiveSource` trait + `NoLiveSource`.
- `crates/application/src/read.rs` — `ReadContext` + `resolve(kind, ctx)`, the single `QueryKind` match.
- `crates/application/src/read_tests.rs` — transport-parity tests.
- `crates/application/src/dispatcher_object_safety_tests.rs` — proves the trait is usable as `dyn`.

**New (crate `adapter-gui`):**
- `crates/adapter-gui/src/gui_live_source.rs` — `GuiLiveSource`, the only place the GUI reads the runtime for transports.

**Modified:**
- `crates/application/src/dispatcher.rs` — trait gains the dispatcher-state reads and session attach methods.
- `crates/application/src/lib.rs` — declare `live_source`, `read`.
- `crates/adapter-gui/src/state.rs` — `dispatcher: Rc<dyn CommandDispatcher>`.
- `crates/adapter-gui/src/mcp_query_resolver.rs` — shrinks to building a `ReadContext`.
- `crates/adapter-console/src/main.rs` — calls `resolve` with `NoLiveSource`.
- GUI wiring modules that name `LocalDispatcher` or `ProjectRuntimeController` in signatures.
- `docs/architecture.md`, `docs/mcp.md`.

---

## Task 1: `CommandDispatcher` carries everything the GUI needs

The GUI calls 11 methods that exist only on the concrete `LocalDispatcher`: `selection_state`, `chain_snapshot`, `di_loop_for_chain`, `di_loop_source_for_chain`, `engine_sr`, `tone_report_json`, `attach_rig`, `attach_project_path`, `attach_presets_path`, `attach_config_path`, `attach_engine_sr`. Until they are on the trait, no trait object can serve the UI.

**Files:**
- Modify: `crates/application/src/dispatcher.rs`
- Modify: `crates/application/src/local_dispatcher_queries.rs`, `crates/application/src/local_dispatcher_attach.rs` (move the `pub fn`s into `impl CommandDispatcher for LocalDispatcher`)
- Test: `crates/application/src/dispatcher_object_safety_tests.rs` (new)

**Interfaces:**
- Produces: `trait CommandDispatcher` with the methods below; every later task consumes `&dyn CommandDispatcher`.

- [ ] **Step 1: Write the failing test**

`crates/application/src/dispatcher_object_safety_tests.rs`:

```rust
use super::*;
use domain::ids::ChainId;

/// A dispatcher that is NOT `LocalDispatcher`. If the GUI-facing surface
/// lives on the trait, this compiles and answers through `dyn`.
struct FakeDispatcher {
    engine_sr: u32,
}

impl CommandDispatcher for FakeDispatcher {
    fn dispatch(&self, _cmd: Command) -> Result<Vec<Event>> {
        Ok(Vec::new())
    }
    fn engine_sr(&self) -> u32 {
        self.engine_sr
    }
    fn selection_state(&self) -> std::sync::Arc<std::sync::RwLock<crate::selection_state::SelectionState>> {
        std::sync::Arc::new(std::sync::RwLock::new(
            crate::selection_state::SelectionState::default(),
        ))
    }
}

#[test]
fn gui_surface_is_reachable_through_a_trait_object() {
    let dispatcher: std::rc::Rc<dyn CommandDispatcher> =
        std::rc::Rc::new(FakeDispatcher { engine_sr: 44_100 });

    assert_eq!(dispatcher.engine_sr(), 44_100);
    // Defaulted reads answer "nothing here" instead of forcing every
    // implementation to carry local-only state.
    assert!(dispatcher.chain_snapshot(&ChainId("missing".into())).is_none());
    assert!(dispatcher.di_loop_for_chain(&ChainId("missing".into())).is_none());
    assert_eq!(dispatcher.tone_report_json(&ChainId("missing".into())), "{}");
    // Attach is local session setup: a no-op default, never a panic.
    dispatcher.attach_presets_path(std::path::PathBuf::from("/tmp/presets"));
    assert!(dispatcher.attach_engine_sr(48_000).is_empty());
}
```

Wire it at the bottom of `crates/application/src/dispatcher.rs`:

```rust
#[cfg(test)]
#[path = "dispatcher_object_safety_tests.rs"]
mod tests;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p application --lib dispatcher::tests`
Expected: FAIL — compile error, `no method named engine_sr found for reference &dyn CommandDispatcher` (the methods still live on the inherent `impl LocalDispatcher`).

- [ ] **Step 3: Move the surface onto the trait**

In `crates/application/src/dispatcher.rs`, extend the trait (keep the existing `dispatch` and `poll_async_results` as they are):

```rust
    /// Engine sample rate the dispatcher last saw. `0` until the audio
    /// runtime reports one.
    fn engine_sr(&self) -> u32 {
        0
    }

    /// Shared UI selection state. Every implementation owns one — the
    /// selection is frontend state, not engine state.
    fn selection_state(&self) -> Arc<RwLock<SelectionState>>;

    /// Immutable copy of one chain, or `None` when the id is unknown.
    fn chain_snapshot(&self, _chain: &ChainId) -> Option<project::chain::Chain> {
        None
    }

    /// Decoded DI loop bound to a chain, when the implementation holds one.
    fn di_loop_for_chain(&self, _chain: &ChainId) -> Option<Arc<DiPcm>> {
        None
    }

    /// Where a chain's DI loop came from (file, looper, …).
    fn di_loop_source_for_chain(&self, _chain: &ChainId) -> Option<DiLoopSource> {
        None
    }

    /// Last Tone Doctor run for a chain, already serialized. `{}` when the
    /// implementation has never run one.
    fn tone_report_json(&self, _chain: &ChainId) -> String {
        "{}".to_string()
    }

    // --- session attach: local setup, no-op by default ---
    fn attach_rig(&self, _rig: Rc<RefCell<RigProject>>) {}
    fn attach_presets_path(&self, _path: PathBuf) {}
    fn attach_project_path(&self, _path: PathBuf) {}
    fn attach_config_path(&self, _path: Option<PathBuf>) {}
    /// Returns the chains whose resolved rate changed.
    fn attach_engine_sr(&self, _sr: u32) -> Vec<ChainId> {
        Vec::new()
    }
```

Then move each `pub fn` body from `local_dispatcher_queries.rs` / `local_dispatcher_attach.rs` out of `impl LocalDispatcher` and into `impl CommandDispatcher for LocalDispatcher` (same file, same bodies — signatures already match). Keep the doc comments with the bodies.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p application` then `cargo build -p adapter-gui`
Expected: PASS. `adapter-gui` may need `use application::dispatcher::CommandDispatcher;` added where a moved method is called — add the import, change nothing else.

- [ ] **Step 5: Commit**

```bash
git add crates/application/src/dispatcher.rs crates/application/src/dispatcher_object_safety_tests.rs \
        crates/application/src/local_dispatcher_queries.rs crates/application/src/local_dispatcher_attach.rs
git commit -m "refactor(#127): move the GUI-facing dispatcher surface onto the trait"
git push
```

---

## Task 2: The GUI holds `Rc<dyn CommandDispatcher>`

**Files:**
- Modify: `crates/adapter-gui/src/state.rs` (`ProjectSession.dispatcher`)
- Modify: every wiring module whose signature names `application::local_dispatcher::LocalDispatcher` (`compact_chain_callbacks.rs`, `di_loop_wiring.rs`, and any the compiler flags)
- Test: `crates/adapter-gui/src/state_dyn_dispatcher_tests.rs` (new)

**Interfaces:**
- Consumes: `trait CommandDispatcher` from Task 1.
- Produces: `ProjectSession { dispatcher: Rc<dyn CommandDispatcher>, .. }` and `ProjectSession::with_dispatcher(project, dispatcher, …)` for tests.

- [ ] **Step 1: Write the failing test**

`crates/adapter-gui/src/state_dyn_dispatcher_tests.rs`:

```rust
use super::*;
use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use std::cell::RefCell as StdRefCell;

/// Records what the UI dispatched, proving the session is not bound to
/// `LocalDispatcher`.
struct RecordingDispatcher {
    seen: StdRefCell<Vec<String>>,
    selection: std::sync::Arc<std::sync::RwLock<application::selection_state::SelectionState>>,
}

impl CommandDispatcher for RecordingDispatcher {
    fn dispatch(&self, cmd: Command) -> anyhow::Result<Vec<Event>> {
        self.seen.borrow_mut().push(format!("{cmd:?}"));
        Ok(Vec::new())
    }
    fn selection_state(
        &self,
    ) -> std::sync::Arc<std::sync::RwLock<application::selection_state::SelectionState>> {
        std::sync::Arc::clone(&self.selection)
    }
}

#[test]
fn session_accepts_a_non_local_dispatcher() {
    let recorder = std::rc::Rc::new(RecordingDispatcher {
        seen: StdRefCell::new(Vec::new()),
        selection: std::sync::Arc::new(std::sync::RwLock::new(Default::default())),
    });
    let session = ProjectSession::with_dispatcher(
        Project::default(),
        std::rc::Rc::clone(&recorder) as std::rc::Rc<dyn CommandDispatcher>,
        None,
        None,
        std::path::PathBuf::from("/tmp/presets"),
    );

    session
        .dispatcher
        .dispatch(Command::SaveProject)
        .expect("dispatch");

    assert_eq!(recorder.seen.borrow().len(), 1);
}
```

Wire it at the bottom of `crates/adapter-gui/src/state.rs`:

```rust
#[cfg(test)]
#[path = "state_dyn_dispatcher_tests.rs"]
mod dyn_dispatcher_tests;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p adapter-gui --lib state::dyn_dispatcher_tests`
Expected: FAIL — compile error: `ProjectSession::with_dispatcher` not found, and `expected Rc<LocalDispatcher>, found Rc<dyn CommandDispatcher>`.

- [ ] **Step 3: Change the field type and add the constructor**

In `state.rs`:

```rust
pub(crate) dispatcher: Rc<dyn CommandDispatcher>,
```

`ProjectSession::new` keeps building a `LocalDispatcher` and coerces it (`as Rc<dyn CommandDispatcher>`), preserving today's attach calls. Add:

```rust
    /// Build a session around a dispatcher the caller chose. `new` is this
    /// with a `LocalDispatcher`; tests and future transports use it directly.
    pub(crate) fn with_dispatcher(
        project: Project,
        dispatcher: Rc<dyn CommandDispatcher>,
        project_path: Option<PathBuf>,
        config_path: Option<PathBuf>,
        presets_path: PathBuf,
    ) -> Self { /* same body as `new` minus dispatcher construction */ }
```

Then walk the compiler errors: every wiring signature `&application::local_dispatcher::LocalDispatcher` becomes `&dyn CommandDispatcher`. Change only the type — no logic edits.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p adapter-gui && cargo build --workspace`
Expected: PASS, zero warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/adapter-gui/src
git commit -m "refactor(#127): the GUI session holds dyn CommandDispatcher"
git push
```

---

## Task 3: `LiveSource` trait + `NoLiveSource`

**Files:**
- Create: `crates/application/src/live_source.rs`
- Modify: `crates/application/src/lib.rs` (`pub mod live_source;`)
- Test: `crates/application/src/live_source_tests.rs` (new)

**Interfaces:**
- Produces:

```rust
pub trait LiveSource {
    fn chain_meters(&self) -> Option<Vec<ChainMeterReading>>;
    fn tuner(&self) -> Option<Vec<TunerReading>>;
    fn spectrum(&self) -> Option<Vec<SpectrumReading>>;
    fn di_loop(&self) -> Option<Vec<DiLoopReading>>;
    fn chain_loopers(&self, chain: &ChainId) -> Option<(Vec<LooperStatus>, u32)>;
    fn devices(&self) -> Option<Vec<String>>;
    fn sample_rate(&self) -> Option<u32>;
}
pub struct ChainMeterReading { pub chain: ChainId, pub in_dbfs: f32, pub out_dbfs: f32 }
pub struct NoLiveSource;
```

- [ ] **Step 1: Write the failing test**

`crates/application/src/live_source_tests.rs`:

```rust
use super::*;
use domain::ids::ChainId;

#[test]
fn a_frontend_that_hosts_nothing_answers_none_everywhere() {
    let live = NoLiveSource;
    assert!(live.chain_meters().is_none());
    assert!(live.tuner().is_none());
    assert!(live.spectrum().is_none());
    assert!(live.di_loop().is_none());
    assert!(live.chain_loopers(&ChainId("guitar".into())).is_none());
    assert!(live.devices().is_none());
    assert!(live.sample_rate().is_none());
}

#[test]
fn a_frontend_overrides_only_what_it_hosts() {
    struct MetersOnly;
    impl LiveSource for MetersOnly {
        fn chain_meters(&self) -> Option<Vec<ChainMeterReading>> {
            Some(vec![ChainMeterReading {
                chain: ChainId("guitar".into()),
                in_dbfs: -12.0,
                out_dbfs: -6.0,
            }])
        }
    }
    let live = MetersOnly;
    assert_eq!(live.chain_meters().unwrap().len(), 1);
    assert!(live.tuner().is_none(), "unimplemented reads stay None");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p application --lib live_source`
Expected: FAIL — `unresolved module live_source`.

- [ ] **Step 3: Write the module**

Every method gets a `{ None }` default so a frontend implements only what it hosts; `NoLiveSource` is the empty implementation (`impl LiveSource for NoLiveSource {}`). Document on the trait: **readings only — PCM taps never cross this boundary.**

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p application --lib live_source`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/application/src/live_source.rs crates/application/src/live_source_tests.rs crates/application/src/lib.rs
git commit -m "feat(#127): LiveSource — the one trait for frontend-hosted live reads"
git push
```

---

## Task 4: `read::resolve` — one `QueryKind` match, with the parity test

This is the #831 verification. The match moves here whole; the two adapters keep their copies until Tasks 5 and 6 delete them.

**Files:**
- Create: `crates/application/src/read.rs`
- Create: `crates/application/src/read_tests.rs`
- Modify: `crates/application/src/lib.rs`

**Interfaces:**
- Consumes: `LiveSource` (Task 3), `CommandDispatcher` (Task 1), `QueryKind` (`bridge.rs`).
- Produces: `ReadContext<'a>` and `pub fn resolve(kind: &QueryKind, ctx: &ReadContext<'_>) -> Result<String, String>`.

- [ ] **Step 1: Write the failing test**

`crates/application/src/read_tests.rs` — the parity test enumerates every `QueryKind` and asserts both transports produce the same *shape*:

```rust
use super::*;
use crate::bridge::QueryKind;
use domain::ids::{BlockId, ChainId};

/// Every variant, so a new one cannot be added without deciding what an
/// empty frontend answers. Kept exhaustive by `match_all_kinds` below.
fn all_kinds() -> Vec<QueryKind> {
    let chain = ChainId("guitar".to_string());
    vec![
        QueryKind::ProjectYaml,
        QueryKind::Devices,
        QueryKind::Ids,
        QueryKind::ChainMeters,
        QueryKind::TunerReadings,
        QueryKind::SpectrumReadings,
        QueryKind::DiLoopState,
        QueryKind::ChainLoopers { chain: chain.clone() },
        QueryKind::ChainLatency { chain: chain.clone() },
        QueryKind::ListChainPresets { chain: chain.clone() },
        QueryKind::ListProjectPresets,
        QueryKind::ListPluginCatalog,
        QueryKind::GetPlugin { id: "x".to_string() },
        QueryKind::FindPlugins { query: String::new() },
        QueryKind::GetPluginParams { plugin_id: "x".to_string() },
        QueryKind::GetBlockParams { chain: chain.clone(), block: BlockId("b".to_string()) },
        QueryKind::Paths,
        QueryKind::ChainQualityReport { chain: chain.clone() },
        QueryKind::ChainToneReport { chain },
    ]
}

#[test]
fn every_kind_answers_on_a_frontend_that_hosts_nothing() {
    let project = test_project_with_one_chain();
    let dispatcher = crate::local_dispatcher::LocalDispatcher::new(rc_project(&project));
    let ctx = ReadContext {
        project: &project,
        rig: None,
        io_bindings: &[],
        dispatcher: &dispatcher,
        live: &crate::live_source::NoLiveSource,
    };
    for kind in all_kinds() {
        let out = resolve(&kind, &ctx);
        assert!(
            out.is_ok(),
            "{kind:?} refused on a frontend that hosts nothing — every resource \
             must stay addressable with the documented empty shape"
        );
    }
}

#[test]
fn silent_meters_use_the_engine_constant_not_a_literal() {
    let project = test_project_with_one_chain();
    let dispatcher = crate::local_dispatcher::LocalDispatcher::new(rc_project(&project));
    let ctx = ReadContext { /* … same, NoLiveSource … */ };
    let meters = resolve(&QueryKind::ChainMeters, &ctx).unwrap();
    let expected = format!("guitar\t{:.1}\t{:.1}\n",
        engine::output_meter::SILENT_DBFS, engine::output_meter::SILENT_DBFS);
    assert_eq!(meters, expected);
}

#[test]
fn a_hosting_frontend_returns_the_same_shape_with_live_values() {
    // Same field layout, different numbers — a client cannot tell which
    // adapter served it apart from the values.
    let hosted = resolve_with_meters(vec![ChainMeterReading {
        chain: ChainId("guitar".to_string()), in_dbfs: -12.0, out_dbfs: -6.0,
    }]);
    let empty = resolve_empty(QueryKind::ChainMeters);
    assert_eq!(hosted.lines().count(), empty.lines().count());
    assert_eq!(hosted.split('\t').count(), empty.split('\t').count());
    assert!(hosted.starts_with("guitar\t-12.0\t-6.0"));
}
```

(Helpers `test_project_with_one_chain`, `rc_project`, `resolve_with_meters`, `resolve_empty` are defined in the same test file; build the project with a single chain id `guitar` following the pattern in `crates/application/src/query_tests.rs`.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p application --lib read`
Expected: FAIL — `unresolved module read`.

- [ ] **Step 3: Write the resolver**

`ReadContext` per the spec. `resolve` is one exhaustive match. Arm rules:

- Pure reads call the existing `application::query*` helpers verbatim (`ProjectYaml`, `Ids`, `ListPluginCatalog`, `GetPlugin`, `FindPlugins`, `GetPluginParams`, `GetBlockParams`, `Paths`, `ChainQualityReport`, `ChainToneReport` via `ctx.dispatcher.tone_report_json`).
- `ChainLatency` uses `query_latency::chain_latency_report(project, io_bindings, chain, ctx.dispatcher.engine_sr() as f32)`.
- `ListChainPresets` / `ListProjectPresets` use `ctx.rig`, `Err("no rig attached to the session")` when `None` — one error string for every transport.
- Live arms read `ctx.live` and fall back to the documented empty shape when it answers `None`:
  - `ChainMeters` → one line per project chain, `SILENT_DBFS` for both columns.
  - `TunerReadings` / `SpectrumReadings` → `query_analyzers::*_json(running: false, …, &[])`.
  - `DiLoopState` → one row per chain, not playing, `SILENT_DBFS`, `source` from `ctx.dispatcher.di_loop_source_for_chain`.
  - `ChainLoopers` → `query_loopers::loopers_json(chain, &[], rate)`, where `rate` is `ctx.live.sample_rate()` or the rate resolved from `io_bindings` (the console's `resolve_project_chain_sample_rates` path — move that helper into `application` so both callers share it).
  - `Devices` → `ctx.live.devices()` when hosted, otherwise `infra_cpal::list_devices()`. **Note:** `application` must not depend on `infra_cpal`; keep the local enumeration on the caller side by having `GuiLiveSource`/console supply `devices()`, and return `Ok(String::new())` only if neither hosts one.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p application`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/application/src/read.rs crates/application/src/read_tests.rs crates/application/src/lib.rs
git commit -m "feat(#127): single QueryKind resolver in the core with transport-parity tests"
git push
```

---

## Task 5: `adapter-console` calls `resolve`

**Files:**
- Modify: `crates/adapter-console/src/main.rs:181-260` (delete the match, keep the closure)
- Test: `crates/adapter-console/src/console_read_tests.rs` (new)

**Interfaces:**
- Consumes: `read::resolve`, `NoLiveSource`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn console_serves_meters_with_the_engine_silent_constant() {
    let project = one_chain_project();
    let dispatcher = application::local_dispatcher::LocalDispatcher::new(rc(&project));
    let out = console_resolve(&application::bridge::QueryKind::ChainMeters, &project, &dispatcher);
    assert!(
        !out.contains("-120.0") || engine::output_meter::SILENT_DBFS == -120.0,
        "console must not hardcode the silent value"
    );
    assert_eq!(
        out,
        format!("guitar\t{:.1}\t{:.1}\n",
            engine::output_meter::SILENT_DBFS, engine::output_meter::SILENT_DBFS)
    );
}

#[test]
fn console_answers_reads_it_does_not_host_instead_of_refusing() {
    let out = console_resolve(&application::bridge::QueryKind::TunerReadings, /* … */);
    assert!(out.contains("\"running\":false"), "got: {out}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p adapter-console`
Expected: FAIL — the second test fails on the current `Err("not available on the console adapter")`.

- [ ] **Step 3: Replace the match with `resolve`**

```rust
drain.serve_queries(
    |kind| {
        let project = shared.borrow();
        application::read::resolve(
            kind,
            &application::read::ReadContext {
                project: &project,
                rig: None,
                io_bindings: &io_bindings,
                dispatcher: dispatcher.inner(),
                live: &ConsoleLiveSource,
            },
        )
    },
    …
);
```

`ConsoleLiveSource` implements only `devices()` (via `infra_cpal::list_devices`) and `sample_rate()`. Delete the whole `QueryKind` match, the `-120.0` literal, and the "not available on the console adapter" arm.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p adapter-console && cargo build -p adapter-console`
Expected: PASS, zero warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/adapter-console/src
git commit -m "refactor(#127): console reads through the core resolver (closes the -120.0 drift)"
git push
```

---

## Task 6: `GuiLiveSource` + `adapter-gui` calls `resolve`

**Files:**
- Create: `crates/adapter-gui/src/gui_live_source.rs`
- Modify: `crates/adapter-gui/src/mcp_query_resolver.rs` (match deleted; builds `ReadContext`)
- Test: `crates/adapter-gui/src/gui_live_source_tests.rs` (new)

**Interfaces:**
- Consumes: `LiveSource`, `ReadContext`, `resolve`.
- Produces: `GuiLiveSource<'a>` reading the meter rows, tuner/spectrum sessions and the runtime.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn gui_meters_report_the_values_the_rows_are_showing() {
    // The row the IN/OUT bars drew this tick.
    let rows = rows_with(&[("guitar", -12.0, -6.0)]);
    let live = GuiLiveSource { chain_rows: &rows, tuner: &none(), spectrum: &none(), runtime: &no_runtime(), dispatcher: &dispatcher };
    let meters = live.chain_meters().expect("GUI hosts meters");
    assert_eq!(meters[0].in_dbfs, -12.0);
    assert_eq!(meters[0].out_dbfs, -6.0);
}

#[test]
fn gui_without_a_tuner_session_reports_none_not_a_fabricated_row() {
    let live = GuiLiveSource { /* tuner: &none() */ };
    assert!(live.tuner().is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p adapter-gui --lib gui_live_source`
Expected: FAIL — `unresolved module gui_live_source`.

- [ ] **Step 3: Implement**

`GuiLiveSource` moves the bodies currently in `mcp_query_resolver.rs` (`chain_meters`, `tuner_readings`, `spectrum_readings`, `di_loop_state`, loopers) but returns **data, not JSON**: `Vec<ChainMeterReading>`, `Vec<TunerReading>`, `Vec<SpectrumReading>`, `Vec<DiLoopReading>`, `(Vec<LooperStatus>, u32)`. Serialization now happens once, in `read::resolve`.

`mcp_query_resolver.rs` shrinks to: borrow session state → build `ReadContext { live: &GuiLiveSource { … } }` → `application::read::resolve(kind, &ctx)`. The `QueryKind` match is deleted.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p adapter-gui && cargo test -p application && cargo build --workspace`
Expected: PASS, zero warnings. **#831 is closed at this commit** — one match, one payload shape.

- [ ] **Step 5: Commit**

```bash
git add crates/adapter-gui/src
git commit -m "refactor(#127): GUI reads through GuiLiveSource + the core resolver (closes #831)"
git push
```

---

## Task 7: Runtime control becomes `Command`s — output mute and I/O bindings

**Files:**
- Modify: `crates/application/src/command.rs` (`SetOutputMuted { muted: bool }`, `SetIoBindings { bindings: Vec<IoBinding> }`)
- Modify: `crates/application/src/local_dispatcher_output.rs` / `local_dispatcher_io_binding.rs`
- Modify: the GUI call sites of `set_output_muted` / `set_io_bindings`
- Test: `crates/application/src/local_dispatcher_output_tests.rs` (extend)

**Interfaces:**
- Consumes: `Command`, `CommandDispatcher`.
- Produces: two new `Command` variants + their events.

- [ ] **Step 1: Write the failing test** — dispatch `Command::SetOutputMuted { muted: true }` and assert the resulting `Event` plus the state the dispatcher records; assert the runtime hook is invoked through the dispatcher's runtime handle, not from the UI.

- [ ] **Step 2: Run it** — `cargo test -p application --lib local_dispatcher_output`; expected FAIL (`no variant named SetOutputMuted`).

- [ ] **Step 3: Implement** the variants, their handlers, and the `command_schema` entries (the schema test enumerates variants — update it in the same commit).

- [ ] **Step 4: Run** `cargo test -p application && cargo build --workspace`; expected PASS.

- [ ] **Step 5: Commit** `feat(#127): output mute and I/O bindings as Commands`.

---

## Task 8: Runtime control becomes `Command`s — block enable and chain sync

**Files:**
- Modify: `crates/application/src/command.rs`, the matching `local_dispatcher_*` handler
- Modify: `crates/adapter-gui/src/runtime_lifecycle.rs`, `crates/adapter-gui/src/project_ops.rs`
- Test: `crates/application/src/local_dispatcher_block_lifecycle.rs` tests (extend)

- [ ] **Step 1: Write the failing test** — dispatching the enable/disable command produces the same events the GUI path produces today, and a chain sync request reaches the dispatcher rather than the UI calling `sync_project` directly.

- [ ] **Step 2: Run it** — expected FAIL.

- [ ] **Step 3: Implement**, routing the runtime effect through the dispatcher's runtime handle.

- [ ] **Step 4: Run** `cargo test -p application -p adapter-gui && cargo build --workspace`; expected PASS.

- [ ] **Step 5: Commit** `feat(#127): block enable and chain sync as Commands`.

---

## Task 9: `infra_cpal` leaves the wiring signatures

**Files:**
- Modify: every `adapter-gui` wiring module that still names `ProjectRuntimeController` in a struct field or function parameter
- Keep: `runtime_lifecycle.rs` (owns the runtime) and `gui_live_source.rs` (reads it)
- Test: `crates/adapter-gui/src/no_infra_cpal_in_wiring_tests.rs` (new)

- [ ] **Step 1: Write the failing test**

```rust
/// The UI may not name the audio backend outside the two modules that own
/// it. This is the invariant #127 exists to establish.
#[test]
fn wiring_modules_do_not_name_the_audio_backend() {
    const ALLOWED: &[&str] = &["runtime_lifecycle.rs", "gui_live_source.rs", "desktop_app.rs"];
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".rs") || name.ends_with("_tests.rs") || ALLOWED.contains(&name.as_str()) {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        if src.contains("ProjectRuntimeController") {
            offenders.push(name);
        }
    }
    assert!(offenders.is_empty(), "still bound to infra_cpal: {offenders:?}");
}
```

- [ ] **Step 2: Run it** — `cargo test -p adapter-gui --lib no_infra_cpal`; expected FAIL listing the remaining modules.

- [ ] **Step 3: Remove them** module by module: a wiring function that needed the controller now dispatches a `Command` (Tasks 7–8) or reads through `GuiLiveSource`. Where a struct only passed the handle through, delete the field.

- [ ] **Step 4: Run** `cargo test -p adapter-gui && cargo build --workspace && cargo clippy -p adapter-gui -- -D warnings`; expected PASS with an empty offender list.

- [ ] **Step 5: Commit** `refactor(#127): infra_cpal out of the UI wiring`.

---

## Task 10: Documentation

**Files:**
- Modify: `docs/architecture.md` (read bus section), `docs/mcp.md` (one shape per resource)

- [ ] **Step 1:** Document in `docs/architecture.md`: writes go through `dyn CommandDispatcher`, reads through `LiveSource` + `read::resolve`, and the rule that a frontend implements only the reads it hosts while the core owns every payload shape.
- [ ] **Step 2:** In `docs/mcp.md`, state that every resource has exactly one shape regardless of the mounted adapter, and that a frontend without a source returns the documented empty shape.
- [ ] **Step 3:** Run `cargo test --workspace` one final time, plus the hardware battery: `OPENRIG_HW_TESTS=1 cargo test -p infra-cpal` on an idle machine.
- [ ] **Step 4: Commit** `docs(#127): the read bus and one shape per resource` and push.
- [ ] **Step 5:** `gh issue comment 127` with the validation checklist for the user (checkout command + `- [ ]` items covering: meters/tuner/spectrum unchanged on screen, DI loop and looper tiles unchanged, MCP resources return the same payloads, no xruns or added latency).

---

## Self-Review

**Spec coverage:** phase 1 → Tasks 1–2; phase 2 → Tasks 3–6 (closes #831 at Task 6); phase 3 → Tasks 7–9; phase 4 → Task 10. Meter parity ("same values as the rows") → Task 6 test. `SILENT_DBFS` → Tasks 4 and 5. Hardware battery → Task 10.

**Open decision carried into Task 4:** `application` must not depend on `infra_cpal`, so device enumeration is supplied by each frontend's `LiveSource::devices()` rather than called from the core. The console keeps calling `infra_cpal::list_devices` inside its own implementation.

---

# Phase 5 — the missing doors (Tasks 11-16)

Task 9 established the invariant and the ratchet, but landed with 34 of 47 GUI
modules still naming `ProjectRuntimeController`. A review spot-checked ten of
them and confirmed none could be converted with the mechanisms Tasks 7-8 built:
what is missing is not effort, it is **doors**. Phase 5 builds them, in order of
rising risk, and each task ends by shrinking the guard's allowlist — the ratchet
test fails if a listed module stops naming the backend, so the list cannot
silently stay too long.

Rules that bind every task in this phase:

- A new `RuntimeControl` method is a *write* door. Ask, per method, whether the
  operation should also be a `Command` (MCP/gRPC parity) or is genuinely a
  frontend-local effect. State the answer in the report — "MCP would start
  recording" is a real consequence, not a footnote.
- A new `LiveSource` method is a *read* door and returns finished readings.
  PCM never crosses it.
- Stream isolation is absolute: every door carries stream/device identity.
  Never select or group runtimes by rate or "all matching".
- Live paths stay live: no rebuild introduced where a live operation exists.
- Nothing new on the audio thread.

## Task 11: the EQ-viz sample rate

Clears `eq.rs`, `block_choose_type_callback.rs`, `block_insert_callbacks.rs`,
`block_parameter_wiring.rs`, `block_editor_window_params.rs`,
`block_editor_window_lifecycle.rs` (6 modules; the biggest single lever).

`eq_viz_sample_rate` (`crates/adapter-gui/src/eq.rs:179`) reads one number off
the controller: the live device rate, else `EQ_VIZ_REFERENCE_SAMPLE_RATE`. The
dispatcher already tracks it (`CommandDispatcher::engine_sr`, kept in lock-step
by `di_loop_wiring::sync_engine_sr_from_runtime`).

- [ ] Swap the source to `dispatcher.engine_sr()`.
- [ ] **Fix the honest bug this exposes:** `engine_sr` is never reset when the
      runtime stops, so after stopping a 44.1 kHz rig the curve would be drawn
      at 44 100. Reset it on `stop_project_runtime` so "nothing running" means
      the reference rate again. The owner has accepted this behavior change.
- [ ] Check the other readers of `engine_sr` for the same staleness
      (`latency_probe.rs`, `read.rs`'s `ChainLatency` and looper arms) and state
      in the report whether the reset changes what they report.
- [ ] Red-first: a test that draws the curve after stopping a non-48k rig and
      asserts the reference rate, failing against today's stale value.
- [ ] Shrink the allowlist by the modules this clears; the ratchet must pass.

## Task 12: DI arming and metronome doors

Clears `di_loop_wiring.rs`, `di_output_select_wiring.rs`,
`compact_chain_di_callbacks.rs`, `metronome_wiring.rs` (4 modules).

`RuntimeControl` gains the DI stream doors (`arm_di_stream`, `disarm_di_stream`,
plus the `di_stream_active` read via `LiveSource` if the UI needs it), the
metronome door, and `ensure_runtime` (#808 — the UI currently starts the runtime
on demand).

- [ ] Decide per door: `Command` (parity) or frontend-local effect. DI arming
      and the metronome are user-visible state changes — default to `Command`
      unless the report argues otherwise.
- [ ] Red-first per door: command → event → runtime effect.
- [ ] Shrink the allowlist.

## Task 13: looper transport and PCM store

Clears `looper_wiring.rs`, `looper_callbacks.rs`, `looper_persist.rs`
(3 modules).

Record / play / clear and the per-loop PCM save/restore become doors. This is
the case where parity has teeth: an MCP client issuing record WILL start
recording audio.

- [ ] State the parity decision explicitly in the report, per operation.
- [ ] PCM save/restore moves as a handle (`Arc<DiPcm>`-shaped), never as samples
      crossing a read seam.
- [ ] Red-first per door; the looper tests already in the tree must stay green.
- [ ] Shrink the allowlist.

## Task 14: teardown, whole-project sync, and chain removal

Clears `back_to_launcher_wiring.rs`, `project_file_dialog_wiring.rs`,
`recent_projects_wiring.rs`, `settings/audio.rs`, `chain_row_wiring.rs`,
`compact_chain_delete_wiring.rs` (6 modules).

- [ ] A "stop the audio" door (`stop_project_runtime`) — a `Command`, since a
      remote client must be able to stop the rig.
- [ ] A project-scoped sync variant for `settings/audio.rs`'s device-settings
      rebuild. It must stay a whole-graph rebuild without becoming a
      rate-grouped selection — carry explicit identity, not "all matching".
- [ ] A `remove_chain` door on `RuntimeControl` for chain delete. Task 9
      recorded the trap: dispatching `SyncChainRuntime` for a deleted chain
      *looks* equivalent but also runs `validate_project` (which can abort the
      removal) and tears the runtime down when the last chain goes. Build the
      real door; do not reuse the lookalike.
- [ ] Red-first per door, including a test pinning that chain removal does not
      tear down other chains' runtimes.
- [ ] Shrink the allowlist. The four pure hubs
      (`chain_rig_nav_wiring.rs`, `compact_chain_callbacks.rs`,
      `desktop_app_block_wiring.rs`, `desktop_app_chain_wiring.rs`) become
      deletable as their last consumer clears — remove each the moment it can go.

## Task 15: runtime health on the poll tick

Clears `desktop_app_polling.rs` (1 module).

The GUI tick drains block errors, advances pending rebuilds
(`poll_pending_rebuilds` — a mutation, not a read), and checks
`is_healthy` / `try_reconnect`.

- [ ] Split it honestly: the error drain and health state are reads
      (`LiveSource`); advancing rebuilds and reconnecting are writes
      (`RuntimeControl`). Do not model a mutation as a read.
- [ ] Red-first: errors surface through the read door, and a pending rebuild is
      advanced through the write door.
- [ ] Shrink the allowlist.

## Task 16: tap subscription (highest risk — do last)

Clears `meter_wiring.rs`, `meter_wiring_poll.rs`, `tuner_session.rs`,
`tuner_wiring.rs`, `spectrum_session.rs`, `spectrum_wiring.rs`,
`block_editor_window_setup.rs`, `select_chain_block_callback.rs`,
`tone_doctor_compact_wiring.rs` (9 modules).

This is the one that needs a design decision, not a mechanical move:
`LiveSource` returns finished readings and a `Command` cannot return an SPSC
ring, so tap *subscription* has no door. `MeterTapApi` is implemented on the
controller itself.

- [ ] Design the subscription seam first and write it in the report before
      coding: a frontend asks for a subscription by stream/device identity and
      receives something it can poll each tick. Locally that is backed by the
      existing ring; the shape must not assume a shared address space, since the
      whole point is that a remote frontend could implement the same seam by
      polling reduced readings.
- [ ] PCM stays on the audio side of the seam. The subscription hands out
      readings, not samples, unless the local implementation can do so with the
      exact same ring it uses today and no extra work on the audio thread.
- [ ] Meter parity (spec) still holds: what the transport reports is what the
      bars draw.
- [ ] Red-first, and run the real-hardware battery (`OPENRIG_HW_TESTS=1`) before
      the final push — this task touches the meter path end to end.
- [ ] Shrink the allowlist to the modules that genuinely own the runtime.

## Phase 5 exit criteria

- The guard's allowlist contains only modules that own or construct the runtime
  handle, each justified in the test's ledger.
- `cargo test --workspace` green, `cargo build --workspace` zero warnings,
  volume invariants pass with their count recorded.
- No new xruns, no added latency, nothing new on the audio thread.
