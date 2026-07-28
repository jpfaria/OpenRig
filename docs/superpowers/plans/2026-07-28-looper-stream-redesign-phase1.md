# Looper Stream Redesign — Phase 1 (stability) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move looper state out of the volatile `ChainRuntimeState` into the controller so record/stop/clear/remove stop racing the chain's runtime lifecycle.

**Architecture:** Reuse the proven `engine::looper::LooperSlot` state machine unchanged, but relocate its OWNERSHIP from the per-runtime `LooperBank` to a controller-owned `LooperStore` keyed by `(ChainId, uid)`. Recording is driven by draining the chain's existing lock-free `InputTap` rings off the audio thread (on the GUI/meter tick) into the slot, instead of the audio-thread `bank.process` tick. Control ops (stop/clear/undo/redo/play) call slot methods on the store directly — deterministic, no queue to a runtime that may not be processing. Playback stays the isolated stream (`IsolatedSource::Looper`), now sourcing the store's exported mixdown. The guitar hot path (`runtime_process_segment`) loses its looper branch entirely — untouched otherwise.

**Tech Stack:** Rust; `SpscRing` (lock-free), `arc_swap`, cpal isolated streams, existing `LooperSlot`.

## Global Constraints

- Zero allocation/lock/syscall/log on the audio thread (invariant #8). Recording drains rings off-thread only.
- Guitar path quality/latency/isolation must not regress (invariants #1–#9). The looper branch is REMOVED from the audio segment path, so the guitar path gets strictly simpler.
- Every state-changing operation stays a `Command` (parity: MCP tool-per-variant guard stays green).
- Repo content in English; docs updated same commit.
- TDD red-first: every production change is preceded by a test seen failing.

## File Structure

- `crates/infra-cpal/src/looper_store.rs` (Create) — `LooperStore`: owns `LooperSlot` per `(ChainId, uid)` + each loop's input/output `EndpointRef` and its input-tap ring handles. All control + record-drain + export methods.
- `crates/infra-cpal/src/controller_loopers.rs` (Modify) — the controller's looper facade delegates to `LooperStore` instead of `push_chain_looper_op`/runtime banks. `sync_looper_streams` reads store state (not published runtime status); `sync_looper_slots` deleted.
- `crates/engine/src/runtime.rs` (Modify) — remove the `looper_bank.drain_ops/process/publish` calls and the `loopers` argument threading.
- `crates/engine/src/runtime_process_segment.rs` (Modify) — remove the `loopers: Option<&mut LooperBank>` parameter and the `bank.process` call.
- `crates/adapter-gui/src/looper_wiring.rs` (Modify) — `apply_looper_event` calls `LooperStore` control methods (via the controller) directly; drop the suppression/`push_chain_looper_op` paths.
- `crates/adapter-gui/src/looper_callbacks.rs` (Modify) — drop `sync_looper_slots`; keep `ensure_runtime` + `sync_looper_streams`.
- `crates/adapter-gui/src/meter_wiring_poll.rs` (Modify) — each tick: drain record + `sync_looper_streams`; drop `sync_looper_slots`.
- Tests: `crates/infra-cpal/src/looper_store_tests.rs` (Create), plus the existing `issue_323_controller_loopers.rs` / `issue_323_looper_wiring.rs` updated to the store.

---

### Task 1: `LooperStore` owns the slots and control

**Files:**
- Create: `crates/infra-cpal/src/looper_store.rs`
- Create: `crates/infra-cpal/src/looper_store_tests.rs`
- Modify: `crates/infra-cpal/src/lib.rs` (add `mod looper_store;`)

**Interfaces:**
- Consumes: `engine::looper::LooperSlot`, `engine::{LooperState, LooperStatus, LooperOp}`, `domain::ids::ChainId`, `project::chain::EndpointRef`.
- Produces:
  - `struct LooperStore { slots: HashMap<(ChainId, u64), LoopEntry> }` where `LoopEntry { slot: LooperSlot, input: Option<EndpointRef>, output: Option<EndpointRef>, rings: Vec<Arc<SpscRing<f32>>> }`.
  - `fn create(&mut self, chain: &ChainId, uid: u64)` — idempotent (keeps an existing slot).
  - `fn remove(&mut self, chain: &ChainId, uid: u64)`.
  - `fn tap_record(&mut self, chain: &ChainId, uid: u64)` — start/close recording (mirrors `LooperSlot::tap_record`, allocating the layer here off the audio thread).
  - `fn stop/play/clear/undo/redo(&mut self, chain, uid)`.
  - `fn set_mix/decay/speed/reverse(...)`.
  - `fn status(&self, chain, uid) -> Option<LooperStatus>` and `fn statuses(&self, chain) -> Vec<LooperStatus>`.
  - `fn export(&self, chain, uid) -> Option<Vec<f32>>` (interleaved stereo mixdown).

- [ ] **Step 1: Write the failing test** (`looper_store_tests.rs`)

```rust
use super::*;
use domain::ids::ChainId;
use engine::LooperState;

fn cid() -> ChainId { ChainId("c".into()) }

#[test]
fn record_close_play_stop_clear_are_deterministic_without_any_runtime() {
    let mut store = LooperStore::default();
    store.create(&cid(), 1);
    assert_eq!(store.status(&cid(), 1).unwrap().state, LooperState::Empty);

    // Record: start, feed 3 frames of dry audio, close.
    store.tap_record(&cid(), 1);
    store.record_frames(&cid(), 1, &[0.2, 0.2, 0.3, 0.3, 0.4, 0.4]); // 3 stereo frames
    assert_eq!(store.status(&cid(), 1).unwrap().state, LooperState::Recording);
    store.tap_record(&cid(), 1); // close → Playing
    let s = store.status(&cid(), 1).unwrap();
    assert_eq!(s.state, LooperState::Playing);
    assert_eq!(s.len_frames, 3);

    // Stop / clear act with NO runtime in sight — the whole point.
    store.stop(&cid(), 1);
    assert_eq!(store.status(&cid(), 1).unwrap().state, LooperState::Stopped);
    store.clear(&cid(), 1);
    assert_eq!(store.status(&cid(), 1).unwrap().state, LooperState::Empty);

    store.remove(&cid(), 1);
    assert!(store.status(&cid(), 1).is_none());
}
```

- [ ] **Step 2: Run it, verify it fails** — `cargo test -p infra-cpal --lib looper_store` → FAIL (module missing).

- [ ] **Step 3: Implement `LooperStore`** wrapping `LooperSlot`. `record_frames` writes interleaved stereo into the recording layer via the slot's per-frame `tick`; `tap_record` allocates the layer buffer (`vec![0.0; max*2]`) here (off-thread) and hands it to `slot.tap_record(Some(buf))`. `status/export` delegate to the slot. Reuse `LooperSlot` verbatim — expose a `record_frame(dry: [f32;2])` on the slot if not already public (it is `tick`).

- [ ] **Step 4: Run, verify PASS.**

- [ ] **Step 5: Commit** — `git add crates/infra-cpal/src/looper_store.rs crates/infra-cpal/src/looper_store_tests.rs crates/infra-cpal/src/lib.rs && git commit -m "feat(#323): LooperStore owns loop state off the runtime"`

---

### Task 2: Recording drains the input tap into the store

**Files:**
- Modify: `crates/infra-cpal/src/looper_store.rs`
- Modify: `crates/infra-cpal/src/controller_loopers.rs`
- Test: `crates/infra-cpal/tests/issue_323_controller_loopers.rs`

**Interfaces:**
- Consumes: the chain runtime's `InputTap` subscription (`engine::input_tap::InputTap::new` via the controller's existing tap plumbing), `resolve_input_segment` for the chosen input's channels.
- Produces: `ProjectRuntimeController::drain_looper_recording(&self, chain: &Chain)` — for each Recording loop, drains its subscribed rings and appends to the slot; called on the meter tick. `arm_looper_recording` subscribes the chosen input's channels when recording starts.

- [ ] **Step 1: Write the failing test** — a controller with a live runtime: `AddChainLooper`-equivalent (`store.create`), start recording, push samples through the runtime's input tap, call `drain_looper_recording`, assert the loop captured them (len grows). (Model on the existing `tick` helper; assert `chain_looper_status(...).len_frames > 0` while Recording.)

- [ ] **Step 2: Run, verify FAIL.**

- [ ] **Step 3: Implement** the tap subscription on record-start (channels from the resolved input segment) and the off-thread drain that appends ring samples to the slot's recording layer.

- [ ] **Step 4: Run, verify PASS.**

- [ ] **Step 5: Commit.**

---

### Task 3: Controller facade + wiring delegate to the store

**Files:**
- Modify: `crates/infra-cpal/src/controller_loopers.rs` — `chain_looper_status(es)`, `export_chain_looper`, control entry points read/write the store; `sync_looper_streams` arms/disarms from store transport state; delete `sync_looper_slots`, `disarm_looper_playback` suppression, `looper_suppressed`, `looper_armed`-vs-status re-arm races (arm keyed on store state + content revision only).
- Modify: `crates/adapter-gui/src/looper_wiring.rs` — `apply_looper_event` calls `controller.looper_<op>()` store methods; no `push_chain_looper_op`, no suppression.
- Modify: `crates/adapter-gui/src/looper_callbacks.rs` / `meter_wiring_poll.rs` — drop `sync_looper_slots`; add `drain_looper_recording` on the tick.
- Test: `crates/adapter-gui/tests/issue_323_looper_wiring.rs` (existing tests updated: stop/clear/remove with no runtime already assert the win).

- [ ] **Step 1:** Update the wiring tests to the store API; keep `stop_disarms_the_stream_even_when_the_chain_is_not_streaming` and `play_after_stop_re_arms...` — they must pass by construction now.
- [ ] **Step 2: Run, verify they compile-fail / assert-fail against the old path.**
- [ ] **Step 3: Implement** the delegation; remove the suppression/slot-sync band-aids.
- [ ] **Step 4: Run — all looper wiring + controller tests PASS.**
- [ ] **Step 5: Commit.**

---

### Task 4: Remove `LooperBank` from the audio hot path

**Files:**
- Modify: `crates/engine/src/runtime.rs` — drop `looper_bank.drain_ops/process/publish` and the field if now unused by the segment path.
- Modify: `crates/engine/src/runtime_process_segment.rs` — remove the `loopers` parameter + `bank.process` call.
- Modify: `crates/engine/src/runtime.rs` callers of `process_single_segment`.
- Test: existing engine golden/segment tests must stay green (guitar path unchanged, minus the looper branch).

- [ ] **Step 1:** Run the engine golden + segment tests to capture GREEN baseline.
- [ ] **Step 2:** Remove the looper branch; keep `LooperBank`/`LooperShared` only if still referenced by the store (the store uses `LooperSlot`, not the bank — so the bank may be deleted or left unused; delete if orphaned to avoid dead code / warnings).
- [ ] **Step 3:** Build with zero warnings; run engine tests — GREEN.
- [ ] **Step 4: Commit.**

---

### Task 5: Queries / persistence / docs

**Files:**
- Modify: `crates/adapter-gui/src/desktop_app.rs` (the `ChainLoopers` query already reads controller statuses — now from the store; verify).
- Modify: `crates/adapter-gui/src/looper_persist.rs` — restore/persist reads/writes the store (loops load into the store on open, save from it).
- Modify: `docs/screens.md` — the looper is a controller-owned stream; REC needs the source running; stop/clear/remove are immediate.
- Test: `issue_323_looper_panel_interaction.rs` and `looper_view` tests stay green; `looper_persist` tests updated to the store.

- [ ] Steps: update persistence to the store (red-first on the reopen test), refresh docs, run the full looper suite.
- [ ] **Commit.**

---

### Task 6: End-to-end MCP acceptance

**Files:** none (verification only).

- [ ] **Step 1:** Build the workspace; run `cargo test --workspace --lib` for the touched crates — GREEN.
- [ ] **Step 2:** Launch `openrig --mcp` (owner runs it) and, via curl to `127.0.0.1:4123`: `AddChainLooper`, `SetChainLooperTransport{Record}` (while playing), `{Stop}`, `{Clear}`, `RemoveChainLooper`, reading `openrig://chains/<id>/loopers` between each — the reported state must match each driven op, on a chain toggled between ops.
- [ ] **Step 3:** Hand off with the validation checklist + `git checkout` block.

## Self-Review

- **Spec coverage:** state off the runtime (T1,T3,T4) ✓; tap recording off-thread (T2) ✓; deterministic control (T1,T3) ✓; playback sources store (T3) ✓; parity + MCP acceptance (T5,T6) ✓; guitar path untouched (T4) ✓. Preset reference is Phase 2 (out of scope here) ✓.
- **Placeholders:** none — each task names exact files, methods, and the red test.
- **Type consistency:** `LooperStore` method names (`create/remove/tap_record/stop/play/clear/undo/redo/set_*/status/statuses/export/record_frames`) are used consistently across T1–T5; `drain_looper_recording`/`arm_looper_recording` on the controller in T2–T3.
