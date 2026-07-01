# Async Control Plane (issue #672) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **OpenRig RED-first micro-cycle (enforced by the `dev-rules` plugin hook):** reading/editing production `src/**` is blocked until a fresh failing test exists. So EVERY task here is: (a) write the failing test, (b) run it and see RED, (c) `touch .dev-rules/.red-first-unlocked`, (d) read the production code you need + implement, (e) green, (f) commit (the commit re-arms the guard). Each commit narrative must name the RED test.

**Goal:** A heavy command (chain activate/rebuild) never blocks the frontend thread; the new chain runtime is built off-thread and goes live via a wait-free atomic swap, with the old runtime dropped on the worker.

**Architecture:** Two planes joined by a lock-free hand-off. Audio plane stays dedicated RT and reads its live `ChainRuntimeState` through one wait-free atomic load per buffer. A single `ControlWorker` thread builds new runtimes and publishes them by swapping the atomic slot; the superseded runtime is dropped on the worker. The `!Send` dispatcher stays the single writer of `State` on the frontend thread and only enqueues a `Send` build request.

**Tech Stack:** Rust, `infra-cpal` (CPAL streams + `ProjectRuntimeController`), `engine` (`ChainRuntimeState`), `arc-swap` (wait-free RCU pointer), `application` (`Command`/bridge), Slint UI state.

**Invariant guardrails (every task must keep green):** #1 latency, #3 zero new xrun/dropout/click, #4 stream isolation, #6 callback jitter, #8 zero lock/alloc/syscall on the audio thread, #9 golden samples, #10 volume invariants. The audio read path may add at most one wait-free atomic load per buffer.

---

## File Structure

- `crates/infra-cpal/src/control_worker.rs` — **(done, Task 1)** the worker thread + `submit`.
- `crates/infra-cpal/src/live_runtime.rs` — **(new)** `LiveRuntimeSlot`: an `arc-swap`-backed holder of `Arc<ChainRuntimeState>` with a wait-free `load()` for the audio callback and a `publish()` for the worker.
- `crates/infra-cpal/src/active_runtime.rs` — `ActiveChainRuntime` gains the `LiveRuntimeSlot` handle(s) so streams read the live runtime instead of a fixed `Arc`.
- `crates/infra-cpal/src/stream_builder.rs` — audio callbacks capture the `LiveRuntimeSlot` and `load()` per buffer; build a `BuildRequest` + `build_chain_runtime` split out of `build_active_chain_runtime`.
- `crates/infra-cpal/src/controller.rs` — split `upsert_chain` into `build_chain_runtime(BuildRequest)` (worker-runnable) + `publish_chain_runtime(...)` (swap slot, queue old for drop); add an async rebuild entry that goes through `ControlWorker`.
- `crates/application/src/...` (dispatcher) — classify heavy vs cheap commands; heavy commands schedule a rebuild and post completion via `application::bridge`.
- Slint UI state — a per-chain `pending` flag set on schedule, cleared on completion.

---

## Task 1: `ControlWorker` off-thread primitive — DONE

Commit `942d95db`. `infra_cpal::ControlWorker::submit(build) -> Receiver<T>` runs off-thread; RED test `crates/infra-cpal/tests/control_worker_nonblocking.rs` is green.

- [x] Complete.

---

## Task 2: `LiveRuntimeSlot` — wait-free atomic runtime pointer

**Why:** the audio callback currently holds a fixed `Arc<ChainRuntimeState>`. To publish a rebuilt runtime without stopping the stream, the callback must read through a swappable slot with a wait-free load (no lock, no alloc — invariant #8).

**Files:**
- Create: `crates/infra-cpal/src/live_runtime.rs`
- Modify: `crates/infra-cpal/src/lib.rs` (add `mod live_runtime;` + `pub(crate) use`/`pub use` as needed)
- Add dep: `crates/infra-cpal/Cargo.toml` → `arc-swap = "1"`

- [ ] **Step 1: Write the failing test** — `crates/infra-cpal/tests/live_runtime_slot.rs`

```rust
//! Issue #672 — LiveRuntimeSlot publishes a new runtime that the audio-side
//! load() observes wait-free, while the old Arc is handed back for off-thread drop.
use std::sync::Arc;
use engine::runtime::build_chain_runtime_state;
use infra_cpal::LiveRuntimeSlot;
use project::chain::Chain;
use domain::ids::ChainId;

fn empty_chain(id: &str) -> Chain {
    Chain { id: ChainId(id.into()), description: None,
        instrument: "electric_guitar".into(), enabled: true, volume: 100.0, blocks: vec![] }
}

#[test]
fn publish_swaps_runtime_and_returns_old_for_drop() {
    let first = Arc::new(build_chain_runtime_state(&empty_chain("c"), 48_000.0, &[1024]).unwrap());
    let slot = LiveRuntimeSlot::new(Arc::clone(&first));

    // Audio-side load returns the current runtime (wait-free).
    let loaded = slot.load();
    assert!(Arc::ptr_eq(&loaded, &first), "load() must see the published runtime");
    drop(loaded);

    let second = Arc::new(build_chain_runtime_state(&empty_chain("c"), 48_000.0, &[1024]).unwrap());
    let second_addr = Arc::as_ptr(&second) as usize;
    let old = slot.publish(second);

    assert!(Arc::ptr_eq(&old, &first), "publish must return the previous runtime for off-thread drop");
    assert_eq!(Arc::as_ptr(&slot.load()) as usize, second_addr, "load() now sees the new runtime");
}
```

- [ ] **Step 2: Run it, see RED**

Run: `cargo test -p infra-cpal --test live_runtime_slot`
Expected: FAIL — `no LiveRuntimeSlot in the root`.

- [ ] **Step 3: `touch .dev-rules/.red-first-unlocked`, then implement**

```rust
// crates/infra-cpal/src/live_runtime.rs
use std::sync::Arc;
use arc_swap::ArcSwap;
use engine::runtime::ChainRuntimeState;

/// Wait-free swappable holder of a chain's live runtime.
///
/// The audio callback calls [`load`] once per buffer (wait-free, zero lock /
/// alloc / syscall — invariant #8). The control worker calls [`publish`] to
/// install a rebuilt runtime and receives the previous `Arc` back so it can be
/// dropped on the worker thread, never on the audio thread.
pub struct LiveRuntimeSlot(Arc<ArcSwap<ChainRuntimeState>>);

impl LiveRuntimeSlot {
    #[must_use]
    pub fn new(initial: Arc<ChainRuntimeState>) -> Self {
        Self(Arc::new(ArcSwap::from(initial)))
    }
    /// Audio-thread read: wait-free load of the current runtime.
    #[must_use]
    pub fn load(&self) -> Arc<ChainRuntimeState> {
        self.0.load_full()
    }
    /// Worker-thread publish: install `next`, return the previous runtime so the
    /// caller drops it off the audio thread.
    #[must_use]
    pub fn publish(&self, next: Arc<ChainRuntimeState>) -> Arc<ChainRuntimeState> {
        self.0.swap(next)
    }
    /// Cheap clone of the handle (audio callbacks and the worker share one slot).
    #[must_use]
    pub fn handle(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
```
Wire `mod live_runtime; pub use live_runtime::LiveRuntimeSlot;` into `lib.rs`; add `arc-swap` to `Cargo.toml`.

- [ ] **Step 4: green** — `cargo test -p infra-cpal --test live_runtime_slot` → PASS.
- [ ] **Step 5: Commit** — `feat(issue-672): LiveRuntimeSlot wait-free runtime swap`.

---

## Task 3: Audio callbacks read the live slot (no behaviour change yet)

**Why:** before anything swaps, the streams must read `ChainRuntimeState` *through* a `LiveRuntimeSlot` instead of a captured fixed `Arc`. With a single published runtime this is a pure refactor — output samples must be byte-identical (golden test proves it).

**Files:**
- Modify: `crates/infra-cpal/src/active_runtime.rs` (hold a `LiveRuntimeSlot` per chain),
  `crates/infra-cpal/src/stream_builder.rs` (callback captures the slot, `load()` per buffer),
  `crates/infra-cpal/src/controller.rs` (construct the slot when building the chain).

- [ ] **Step 1: Write the failing test** — `crates/infra-cpal/tests/audio_reads_live_slot.rs`

Assert the rendered output of a chain driven through the slot equals the existing golden/offline render for the same input (reuse `engine::offline::render_chain` or the existing golden fixture). Drive one buffer, compare within tolerance to the runtime read directly. (Exact fixture wiring pinned once `stream_builder.rs` is readable in Step 3.)

- [ ] **Step 2: RED** — fails because the slot read path does not exist / output diverges.
- [ ] **Step 3: unlock + implement** — replace the captured `Arc<ChainRuntimeState>` in the input/output callbacks with a captured `LiveRuntimeSlot` and a `let runtime = slot.load();` at the top of each callback. Build the slot in the controller's chain-build path. No swap yet.
- [ ] **Step 4: green** + run `cargo test -p engine` golden/volume suites to prove no regression.
- [ ] **Step 5: Commit** — `refactor(issue-672): audio callbacks read ChainRuntimeState via LiveRuntimeSlot`.

---

## Task 4: Split `upsert_chain` into build (worker-runnable) + publish

**Why:** the heavy work (`build_active_chain_runtime` / `build_chain_runtime_state`, CPAL stream build) must be callable from the worker thread producing a `Send` result; publishing must be the cheap slot swap + queue-old-for-drop.

**Files:** Modify `crates/infra-cpal/src/controller.rs`, `crates/infra-cpal/src/stream_builder.rs`.

- [ ] **Step 1: Write the failing test** — `crates/infra-cpal/tests/build_publish_split.rs`

Build a chain runtime via the new `build_chain_runtime(&BuildRequest) -> Result<BuiltRuntime>` without an existing controller, then `publish_chain_runtime` into a controller's slot and assert `load()` sees it and the old runtime is returned. (Mirror the construction in `controller_pause_chain_tests.rs`.)

- [ ] **Step 2: RED** — `build_chain_runtime` / `BuildRequest` don't exist.
- [ ] **Step 3: unlock + implement** — extract a pure `build_chain_runtime(&BuildRequest) -> Result<BuiltRuntime>` (no `&self`, all inputs owned/`Send`) from `upsert_chain`'s rebuild arm; add `publish_chain_runtime(&mut self, chain_id, BuiltRuntime) -> Option<Arc<ChainRuntimeState>>` that swaps the slot and returns the old runtime. Keep `upsert_chain` working by composing the two (still synchronous for now → no behaviour change, existing tests stay green).
- [ ] **Step 4: green** — existing `controller_pause_chain_tests`, `tests_regression`, `tests_signatures` stay green.
- [ ] **Step 5: Commit** — `refactor(issue-672): split chain rebuild into build_chain_runtime + publish`.

---

## Task 5: Route heavy rebuild through `ControlWorker`; drop old runtime on the worker

**Why:** this is the slice that removes the freeze — the rebuild runs on the worker and the controller call returns immediately; the old runtime's `Drop` runs on the worker.

**Files:** Modify `crates/infra-cpal/src/controller.rs` (own a `ControlWorker`, add `schedule_chain_rebuild`).

- [ ] **Step 1: Write the failing test** — `crates/infra-cpal/tests/rebuild_is_offthread.rs`

```rust
// schedule_chain_rebuild returns before a deliberately-slow build finishes,
// and the superseded runtime is dropped on the worker thread (not the caller).
// Capture the dropping thread id via a guard injected into BuildRequest, assert
// it != std::thread::current().id() and the schedule call returned first
// (same channel-park technique as Task 1).
```

- [ ] **Step 2: RED** — `schedule_chain_rebuild` doesn't exist.
- [ ] **Step 3: unlock + implement** — `schedule_chain_rebuild` builds a `Send` `BuildRequest`, `worker.submit(move || build_chain_runtime(&req))`, and on the completion path `publish_chain_runtime` + move the returned old `Arc` into a drop performed on the worker (or a dedicated drop queue drained by the worker). Coalesce: a newer rebuild for the same chain supersedes an in-flight one.
- [ ] **Step 4: green** + golden/volume/`pause_chain` suites stay green; manual: activating a chain no longer freezes the UI.
- [ ] **Step 5: Commit** — `feat(issue-672): build+drop chain runtimes on the control worker (no UI freeze)`.

---

## Task 6: Async completion + UI pending state; classify heavy commands

**Why:** the frontend must not await the rebuild. Heavy `Command`s schedule + mark the chain `pending`; the worker posts completion through `application::bridge`, which clears `pending` → `live`.

**Files:** Modify `crates/application/src/...` (dispatcher + command classification + bridge completion), Slint UI state (`pending` flag), the chains screen `.slint`.

- [ ] **Step 1: Write the failing test** — application-layer test: dispatching the heavy `Command` returns immediately with the chain marked `pending`, and feeding the worker-completion event clears it to `live`. Pure `State`/`Event` test (no `AppWindow`, per architecture law).
- [ ] **Step 2: RED.**
- [ ] **Step 3: unlock + implement** — single source of truth for heavy-vs-cheap classification next to `Command`; heavy → `schedule_chain_rebuild` + set `pending`; bridge completion `Event` clears it. Cheap commands (incl. #522 toggle fast-path) unchanged.
- [ ] **Step 4: green.**
- [ ] **Step 5: Commit** — `feat(issue-672): async heavy commands with pending UI state`.

---

## Task 7: Continuity / failure / isolation invariant tests

**Files:** `crates/infra-cpal/tests/` + reuse `crates/engine/src/volume_invariants_tests.rs`, golden suites.

- [ ] Audio continuity across a publish swap: no dropout/discontinuity beyond golden tolerance.
- [ ] Build-failure path: build returns `Err` on the worker → no swap, old runtime stays live, error `SideEffect` surfaced.
- [ ] Isolation #4: a rebuild/swap on chain A leaves chain B's slot and samples untouched.
- [ ] Coalescing: rapid repeated rebuilds for one chain collapse to the latest published runtime.
- [ ] Full `cargo test --workspace` green; `cargo build --workspace` zero warnings.
- [ ] Commit — `test(issue-672): swap continuity, failure, isolation, coalescing`.

---

## Docs to update in the same commits (CLAUDE.md law)

- `docs/architecture.md` — `infra-cpal` entry: control worker + `LiveRuntimeSlot` hand-off.
- `docs/audio-config.md` — note the wait-free per-buffer slot load on the audio path.
- `docs/mcp.md` — heavy commands now apply asynchronously (UI no longer blocks during rebuild).

## Out of scope

- Any change to audio-path concurrency beyond the single wait-free slot load.
- Generalising the worker to commands other than chain rebuild (follow-ups once the slice is proven).
