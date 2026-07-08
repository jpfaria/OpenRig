# DI Loop: Isolated Pre-Rendered Stream + Per-Chain Output Selector — Implementation Plan (#771)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An armed DI loop plays on its own isolated, pre-rendered stream — output-clocked (no drift), routed to the chain's chosen bound output only — leaving the guitar's live signal and meters untouched; the DI meter row reads the DI's own levels; a second select in the DI panel picks the output.

**Architecture:** Arming pre-renders the loop through a fresh copy of the chain's block graph off-thread (two full cycles, keep the second so wrap tails are loop-consistent), producing an immutable stereo buffer at the chosen output's rate. The chosen output device's existing callback mixes that buffer at a cursor it advances itself (output-clocked → no drift, no NAM in the callback). The guitar injection path (`set_chain_di_loop`) is removed from the audible flow. The free-running `DiWorker` (meters-only today) is retired; DI meters come from peaks the playback mix computes with atomics.

**Tech Stack:** Rust (engine, infra-cpal, application, adapter-gui), Slint (UI), cpal.

**Issue:** #771 (supersedes the clock decision in `docs/superpowers/specs/2026-07-01-issue-717-di-stream-clock-decision.md` — the worker-clocked Candidate B was tried in `ea900b0c4`, drifted against the device clock, and was reverted in `f1131725e`; #771 mandates the pre-render fallback).

## Global Constraints

- Audio-thread invariants (CLAUDE.md): zero alloc/lock/syscall/IO in any callback; zero xruns; stream isolation (#4) — the DI playback shares no buffer/route/tap/DSP state with the guitar runtime; internal stereo (#5); volume invariants pinned in `crates/engine/src/volume_invariants_tests.rs` untouched.
- TDD red-first: every production change preceded by a test whose ASSERTION was seen failing (not just a compile error) — stub first if the API is new, then watch the behavioral RED.
- Existing tests that pin the OLD injection behavior (`issue_614_di_loop_play_applies_to_runtime.rs`) are updated to pin the NEW owner-specified behavior — that is the feature, not test-gaming; every other test stays untouched.
- Every state change is a `Command` (already true: `SetChainDiLoopOutput` landed in #717); MCP parity test (`COMMAND_VARIANT_COUNT = 73`) stays green — no new Command is needed.
- Repo content in English; new `@tr` keys → `scripts/extract-translations.sh` + all 9 `.po` in the same commit; docs in the same commit as behavior.
- Work only under `.solvers/issue-771/`, branch `feature/issue-771`, push after each commit, `gh issue comment` after each push.
- UI tasks: invoke `ui-ux-pro-max` + `slint-best-practices` first; render headless PNG via `tools/slint-render` before claiming layout done; the DI panel select stays INLINE (PopupWindow content is unreachable by the testing backend — #749/#761).

## File Structure

| File | Responsibility |
|---|---|
| `crates/engine/src/di_render.rs` (new) | Off-line pre-render: chain copy + loop → `DiRenderedLoop` buffer |
| `crates/engine/src/di_output_resolve.rs` (new) | Pure `DiOutputRef` → output index resolution |
| `crates/infra-cpal/src/di_playback.rs` (new) | RT-safe playback cell: `DiPlayback` (frames + cursor + peaks), mix fn |
| `crates/infra-cpal/src/di_stream.rs` (rewrite) | arm = resolve output + spawn render + install playback; disarm = clear; `DiWorker` deleted |
| `crates/infra-cpal/src/stream_builder.rs` | Output callback mixes the installed playback after `process_output_buffer` |
| `crates/infra-cpal/src/active_runtime.rs` + `controller.rs` | Per-(chain, output_index) playback cells; `di_streams` map becomes arm bookkeeping |
| `crates/adapter-gui/src/di_loop_wiring.rs` | Play/stop arm ONLY the isolated stream (guitar injection removed) |
| `crates/adapter-gui/src/di_output_options.rs` (new) | Bound-output-endpoint option list + index↔`DiOutputRef` mapping |
| `crates/adapter-gui/src/compact_chain_callbacks.rs` + `meter_wiring.rs` | DI meter row reads playback peaks, not the chain mirror |
| `crates/adapter-gui/ui/components/di_loop_panel.slint` + `di_panel_globals.slint` | Second inline select (output endpoint) |
| `docs/screens.md`, `docs/audio-config.md` | Behavior docs |

---

### Task 1: Pure resolution — `DiOutputRef` → output index

**Files:**
- Create: `crates/engine/src/di_output_resolve.rs` (+ `pub mod` in `crates/engine/src/lib.rs`)
- Test: inline `#[cfg(test)]` in the same file

**Interfaces:**
- Consumes: `project::chain::{Chain, DiOutputRef}`, `domain::io_binding::IoBinding`, `engine::runtime_endpoints::resolve_chain_io_by_binding` (`crates/engine/src/runtime_endpoints.rs:84`, returns `Vec<BindingIo>` with `binding_id` + per-binding outputs in registry order — the SAME deterministic order `resolve_chain_io` uses to number output streams).
- Produces: `pub fn resolve_di_output_index(chain: &Chain, registry: &[IoBinding], di_output: Option<&DiOutputRef>) -> usize` — flat output index into the chain's resolved outputs; `None`, unknown binding, or unknown endpoint name → `0` (the chain's main/first output, so legacy projects and stale refs keep today's default).

- [ ] **Step 1: Write the failing test** (stub the fn returning `usize::MAX` first so the RED is behavioral):

```rust
// crates/engine/src/di_output_resolve.rs
#[cfg(test)]
mod tests {
    use super::*;
    // Build a chain bound to one IoBinding with two output endpoints
    // ("out_main", "out_fx") and assert:
    #[test]
    fn none_resolves_to_first_output() {
        assert_eq!(resolve_di_output_index(&chain, &registry, None), 0);
    }
    #[test]
    fn named_endpoint_resolves_to_its_flat_index() {
        let r = DiOutputRef { binding_id: "io".into(), endpoint: "out_fx".into() };
        assert_eq!(resolve_di_output_index(&chain, &registry, Some(&r)), 1);
    }
    #[test]
    fn stale_ref_falls_back_to_first_output() {
        let r = DiOutputRef { binding_id: "gone".into(), endpoint: "x".into() };
        assert_eq!(resolve_di_output_index(&chain, &registry, Some(&r)), 0);
    }
}
```

Fixture construction copies the `Chain`/`IoBinding` literals used in `crates/infra-cpal/tests/issue_717_di_dedicated_runtime.rs` (same fields, `di_output: None` etc.).

- [ ] **Step 2: Run — expect behavioral FAIL.** `cargo test -p engine di_output_resolve` → assertions fail (`usize::MAX != 0/1`). Paste the FAILED lines; touch `.claude/.red-first-unlocked`.
- [ ] **Step 3: Implement** — walk `chain.io_binding_ids` in order, for each binding in the registry enumerate `binding.outputs` (names from `IoEndpoint.name`), counting a flat index; return the index whose `(binding.id, endpoint.name)` matches; fallback `0`.
- [ ] **Step 4: Run — expect PASS.** Also `cargo build -p engine` warning-free.
- [ ] **Step 5: Commit + push.** `git add crates/engine/src/di_output_resolve.rs crates/engine/src/lib.rs && git commit -m "feat(#771): resolve a chain's DiOutputRef to its flat output index"` — then `git push` + `gh issue comment 771`.

---

### Task 2: Off-line pre-render — `render_di_loop`

**Files:**
- Create: `crates/engine/src/di_render.rs` (+ `pub mod` in `lib.rs`)
- Test: `crates/engine/tests/issue_771_di_render.rs`

**Interfaces:**
- Consumes: `build_chain_runtime_state(chain, sample_rate, &[], registry)` (`crates/engine/src/runtime_graph.rs:291`), `DiPcm::to_loop_at` (`crates/engine/src/di_loop.rs:123`), `ChainRuntimeState::set_di_loop` (`runtime_state.rs:677`), `process_input_f32` + `process_output_f32` (`crates/engine/src/runtime.rs`).
- Produces:

```rust
pub struct DiRenderedLoop {
    /// Interleaved stereo frames at `sample_rate`, exactly one loop period long.
    pub frames: Vec<[f32; 2]>,
    pub sample_rate: u32,
}
pub fn render_di_loop(
    chain: &Chain,
    registry: &[IoBinding],
    output_rate: u32,
    pcm: &DiPcm,
) -> anyhow::Result<DiRenderedLoop>
```

- [ ] **Step 1: Write the failing test** (stub returns an empty `frames` vec so the RED is behavioral):

```rust
// crates/engine/tests/issue_771_di_render.rs
// Chain fixture: passthrough chain (no blocks) bound to one output binding,
// DiPcm = 1s 440Hz mono sine at 44100 (generated in the test, no files).
#[test]
fn rendered_loop_has_one_period_at_output_rate_and_signal() {
    let rendered = render_di_loop(&chain, &registry, 48_000, &pcm).unwrap();
    let expected_len = pcm.to_loop_at(48_000).len();
    assert_eq!(rendered.frames.len(), expected_len);       // one loop period at 48k
    let peak = rendered.frames.iter().map(|f| f[0].abs().max(f[1].abs())).fold(0.0f32, f32::max);
    assert!(peak > 0.1, "render must carry the loop signal, got peak {peak}");
}
#[test]
fn render_does_not_touch_any_shared_state() {
    // build the guitar runtime for the same chain BEFORE rendering;
    // after render_di_loop, the guitar runtime still has no di_loop.
    assert!(!guitar_runtime.has_di_loop());
}
```

- [ ] **Step 2: Run — expect behavioral FAIL** (`0 != expected_len`). `cargo test -p engine --test issue_771_di_render`. Paste FAILED lines; unlock red-first.
- [ ] **Step 3: Implement**: build a fresh `ChainRuntimeState` at `output_rate`, `set_di_loop(Some(Arc::new(pcm.to_loop_at(output_rate))))`, then step it in 256-frame blocks feeding silence through `process_input_f32` (the armed loop substitutes the input — same mechanism today's `DiWorker` uses, `di_stream.rs:44-49`) while draining the output route via `process_output_f32` into a scratch. Render **two full loop periods; keep only the second** — reverb/delay tails from cycle 1 flow into cycle 2, so the kept cycle loops seamlessly. Downmix the drained route frames to `[f32;2]` (they are already stereo, invariant #5). This function runs OFF the audio thread — allocation is fine here.
- [ ] **Step 4: Run — expect PASS**; `cargo test -p engine` full crate still green (golden/volume invariants untouched).
- [ ] **Step 5: Commit + push + issue comment.** `feat(#771): pre-render the DI loop through a copy of the chain graph`

---

### Task 3: RT-safe playback cell + mix — `DiPlayback`

**Files:**
- Create: `crates/infra-cpal/src/di_playback.rs` (+ `mod` in `lib.rs`)
- Test: inline `#[cfg(test)]`

**Interfaces:**
- Consumes: `DiRenderedLoop` (Task 2), `output_limiter` (`crates/engine/src/runtime_dsp.rs:77`).
- Produces:

```rust
pub(crate) struct DiPlayback {
    frames: Arc<DiRenderedLoop>,
    cursor: AtomicUsize,
    /// Peaks of the last mixed buffer, read by the meter poll (Task 6).
    out_peak: AtomicU32,             // f32 bits
    in_peak: AtomicU32,              // raw loop peak at the same cursor window
    raw: Arc<DiLoop>,                // the un-processed loop, for the IN meter
}
pub(crate) type DiPlaybackCell = Arc<ArcSwapOption<DiPlayback>>;

/// Called from the output callback AFTER process_output_buffer.
/// Zero alloc/lock: Arc load + slice reads + atomics.
pub(crate) fn mix_di_playback(
    cell: &DiPlaybackCell,
    out: &mut [f32],
    output_total_channels: usize,
    dest_channels: &[usize],         // the resolved output's channel offsets
) 
```

`mix_di_playback` loads the cell (`ArcSwapOption::load`, wait-free); if `Some`, for each output frame adds `frames[cursor]` L/R into `dest_channels`, applies `output_limiter` to the summed samples, advances `cursor` modulo `frames.len()`, and stores the window's peaks.

- [ ] **Step 1: Write the failing tests** (mix into a silent buffer → samples appear on the right channels and cursor wraps; mix over an existing signal → sum is limited; empty cell → buffer untouched):

```rust
#[test]
fn mix_writes_loop_frames_on_dest_channels_and_wraps() { /* 8-frame loop, 12-frame buffer: assert wrap */ }
#[test]
fn mix_sums_over_existing_signal_with_limiter() { /* 0.9 + 0.9 stays <= 1.0 */ }
#[test]
fn empty_cell_leaves_buffer_untouched() { }
```

- [ ] **Step 2: Run — expect behavioral FAIL** (stub `mix_di_playback` as no-op → first two tests fail on assertions). `cargo test -p infra-cpal di_playback`. Paste FAILED lines; unlock.
- [ ] **Step 3: Implement** as specified. `arc_swap` is already a workspace dependency (used by `runtime_state.rs` `ArcSwapOption<DiLoop>`).
- [ ] **Step 4: Run — expect PASS.**
- [ ] **Step 5: Commit + push + issue comment.** `feat(#771): output-clocked DI playback cell with RT-safe mix`

---

### Task 4: Wire the cell into the output stream + rewrite arm/disarm

**Files:**
- Modify: `crates/infra-cpal/src/stream_builder.rs` (`build_output_stream_for_output` `:352-357`, F32 callback `:389-409`), `crates/infra-cpal/src/active_runtime.rs` (hold `Vec<(usize, DiPlaybackCell)>` per output), `crates/infra-cpal/src/controller.rs:109` (`di_streams` value type), `crates/infra-cpal/src/di_stream.rs` (rewrite arm/disarm; DELETE `DiWorker`)
- Test: `crates/infra-cpal/tests/issue_771_di_playback_routing.rs`; update `crates/infra-cpal/tests/issue_717_di_dedicated_runtime.rs` + `issue_717_di_stream_meters.rs` to the new machinery

**Interfaces:**
- Consumes: `resolve_di_output_index` (Task 1), `render_di_loop` (Task 2), `DiPlaybackCell`/`mix_di_playback` (Task 3), `ResolvedOutputDevice`/output rate from `chain_resolve.rs:196-250`.
- Produces (same public API as today, new semantics):
  - `arm_di_stream(&self, chain: &Chain, pcm: Arc<DiPcm>) -> Result<()>` — resolves `chain.di_output` → output index (Task 1) → that output's rate; spawns a short-lived render thread (`render_di_loop`), then stores `Some(DiPlayback)` into that output's cell; records the armed entry in `di_streams`.
  - `disarm_di_stream(&self, chain_id)` — stores `None` in the cell, drops the entry.
  - `di_stream_active(&self, chain_id) -> bool` — reads `di_streams` (armed immediately, even while the render is still running).
  - `di_playback_peaks(&self, chain_id) -> Option<(f32, f32)>` — (in, out) dBFS-ready linear peaks for Task 6.

- [ ] **Step 1: Write the failing routing test**:

```rust
// crates/infra-cpal/tests/issue_771_di_playback_routing.rs
// Chain bound to TWO output endpoints; di_output = Some(second).
#[test]
fn playback_lands_on_the_chosen_output_cell_only() {
    controller.arm_di_stream(&chain, pcm).unwrap();
    wait_for_render(&controller, &chain.id);          // poll cell != None, timeout 10s
    assert!(cell_for(&controller, &chain.id, 1).load().is_some());  // chosen
    assert!(cell_for(&controller, &chain.id, 0).load().is_none());  // NOT the main output
}
#[test]
fn playback_is_rendered_at_the_chosen_outputs_rate() {
    // reuses the #749 length-per-rate assertion on DiRenderedLoop.sample_rate/len
}
#[test]
fn guitar_runtime_is_never_touched() {
    assert!(!guitar_runtime.has_di_loop());
    // and the guitar runtime's output route stays fed by live input only
}
```

(Headless: use the same no-device runtime fixtures `issue_717_di_dedicated_runtime.rs` already uses; the cells are testable without opening cpal streams.)

- [ ] **Step 2: Run — expect behavioral FAIL** (today `arm_di_stream` spawns the worker, installs no playback → `cell.load().is_some()` fails). Paste FAILED lines; unlock.
- [ ] **Step 3: Implement.** Create one `DiPlaybackCell` per chain output at stream/runtime build (`active_runtime.rs`), pass the matching cell into `build_output_stream_for_output`; in the F32 callback call `mix_di_playback(&cell, data, channels, dest)` right after `process_output_buffer` (inside the existing `catch_unwind`; the cell Arc is cloned at build time — no alloc in the callback). Rewrite `di_stream.rs`: delete `DiWorker` + the silence-feeding loop; arm resolves index+rate, `std::thread::spawn` the render (thread name `"di-render"`), store the playback on completion (if disarmed meanwhile, drop the result). Keep `di_stream_loop_len` semantics from the rendered loop. Update the two #717 test files to assert the new machinery (armed ⇒ active flag true, guitar untouched — same intent, new observables); delete `di_subscribe_stream_tap`/`di_stream_count` if nothing consumes them after this task (check `meter_wiring.rs` first).
- [ ] **Step 4: Run — expect PASS**: `cargo test -p infra-cpal`, plus `cargo test -p engine` and a full `cargo build` (zero warnings).
- [ ] **Step 5: Commit + push + issue comment.** `feat(#771): DI plays pre-rendered on the chosen output's clock — off the guitar stream`

---

### Task 5: adapter-gui — play/stop drive ONLY the isolated stream

**Files:**
- Modify: `crates/adapter-gui/src/di_loop_wiring.rs:117-130` (enable path calls `arm_di_stream` only — remove `rt.set_chain_di_loop(...)` guitar injection; disable path symmetric) and `:189-220` (rate-change re-arm re-renders via the new arm)
- Test: update `crates/adapter-gui/tests/issue_614_di_loop_play_applies_to_runtime.rs` (pins the OLD injection — flip it to pin the new isolation), extend with:

```rust
#[test]
fn play_leaves_the_guitar_runtime_unarmed() {
    handle_chain_di_loop_enabled_changed(/* enabled=true */);
    assert!(!guitar_runtime.has_di_loop());        // RED today: injection sets it
    assert!(controller.di_stream_active(&chain.id));
}
```

- [ ] **Step 1: Write the failing test** above.
- [ ] **Step 2: Run — expect behavioral FAIL** (`has_di_loop()` is true today). `cargo test -p adapter-gui --test issue_614_di_loop_play_applies_to_runtime`. Paste FAILED line; unlock.
- [ ] **Step 3: Rewire**: enable → `rt.arm_di_stream(&chain_def, pcm)` only; disable → `rt.disarm_di_stream(&chain.id)` only; drop the now-dead `set_chain_di_loop`/`arm_di_loop_per_output_stream` audible path from `controller_taps.rs:313-381` **if** no other caller remains (grep first; the latency beep #723 has its own path — do not touch it).
- [ ] **Step 4: Run — expect PASS**: `cargo test -p adapter-gui`, `cargo test -p infra-cpal`, full build zero warnings.
- [ ] **Step 5: Commit + push + issue comment.** `feat(#771): play/stop arm only the isolated DI stream — guitar path clean`

---

### Task 6: DI meter row reads the DI's own levels

**Files:**
- Modify: `crates/adapter-gui/src/compact_chain_callbacks.rs:519-525` (replace the chain-mirror with `controller.di_playback_peaks(&cid)` → dBFS), `crates/adapter-gui/src/meter_wiring.rs` (same replacement where the row is fed)
- Test: `crates/adapter-gui/tests/issue_771_di_meter_own_levels.rs`

**Interfaces:**
- Consumes: `di_playback_peaks` (Task 4); the dBFS conversion already used by `StreamMeterReading`.

- [ ] **Step 1: Write the failing test** — with a playback installed whose peaks are known and the chain row meters set to a DIFFERENT value, the DI meter update function yields the playback's dBFS, not the row's:

```rust
#[test]
fn di_meter_reads_playback_peaks_not_the_chain_mirror() {
    // pure-function test on the row-update helper (extract it if inline)
    assert_eq!(di_meter.in_dbfs, expected_from_playback_in);
    assert_ne!(di_meter.out_dbfs, chain_row.meter_out_dbfs);
}
```

- [ ] **Step 2: Run — expect behavioral FAIL** (today it copies `row.meter_*_dbfs`). Paste FAILED line; unlock.
- [ ] **Step 3: Implement** — extract the DI-row update into a pure helper fed by `(playback_peaks, playing)`, call it from the 80ms timer.
- [ ] **Step 4: Run — expect PASS.**
- [ ] **Step 5: Commit + push + issue comment.** `feat(#771): DI meter row reads the DI stream's own peaks`

---

### Task 7: Output selector in the DI panel (UI)

**Files:**
- Create: `crates/adapter-gui/src/di_output_options.rs` — `build_di_output_options(chain, registry) -> Vec<DiOutputOption>` (`{ di_ref: DiOutputRef, label: String }`, label = `IoEndpoint.name`, order = the SAME flat order as Task 1), `di_output_selected_index(chain, options) -> i32`
- Modify: `crates/adapter-gui/ui/components/di_panel_globals.slint` (add `outputs: [string]`, `output-selected-index: int`), `crates/adapter-gui/ui/components/di_loop_panel.slint` (second INLINE select — clone the `select-field`/`row-ta` pattern lines 45-90/125-167, element ids `out-sel-ta`/`out-row-ta`, new callback `output-picked(int, string)`), `chain_di_loop_button.slint` (seed outputs), `app-window.slint` + `secondary_windows_block.slint` overlays (forward the callback), `chain_row_wiring.rs` + `compact_chain_callbacks.rs` (wire `on_di_loop_output_selected` → `Command::SetChainDiLoopOutput`), `meter_wiring.rs` (populate `row.di_loop_outputs` like `di_loop_sources` at `:565/:620`), `runtime_sync_policy.rs:16-21` (add `ChainDiLoopOutputChanged` to the no-rebuild exclusions), `di_loop_wiring.rs` (on `ChainDiLoopOutputChanged` while playing → disarm + re-arm so the sound moves to the new output), translations.
- Test: `crates/adapter-gui/tests/issue_771_di_output_select.rs` (interaction, #749 pattern) + unit tests for `di_output_options.rs`; PNG render via `di_loop_test_harness.slint`.

**Interfaces:**
- Consumes: `Command::SetChainDiLoopOutput { chain, output: DiOutputRef }` (`command.rs:577-580`), `resolve_chain_io_by_binding`, `DiPanel` global, `arm/disarm` (Task 4).

- [ ] **Step 1: Invoke `ui-ux-pro-max` + `slint-best-practices` skills.**
- [ ] **Step 2: Write the failing tests**:

```rust
// unit — options + mapping
#[test]
fn options_list_the_chains_bound_output_endpoints_in_flat_order() { }
#[test]
fn picked_index_maps_to_the_matching_di_output_ref() { }
// interaction — i_slint_backend_testing::init_no_event_loop, click_id helper
// copied from issue_749_di_loop_popup_interaction.rs
#[test]
fn clicking_an_output_row_fires_output_picked_with_that_index() { }
// wiring — dispatching path
#[test]
fn output_selected_dispatches_set_chain_di_loop_output() { }
#[test]
fn output_change_while_playing_rearms_to_the_new_output() { }
#[test]
fn output_changed_event_does_not_trigger_runtime_rebuild() {
    // extends runtime_sync_policy tests: ChainDiLoopOutputChanged excluded
}
```

- [ ] **Step 3: Run — expect behavioral FAIL** on each (stub Slint props first so tests compile where needed). Paste FAILED lines; unlock.
- [ ] **Step 4: Implement** — options builder; second inline select under the source select (same width, `@tr("di-output-select-label")` etc. — NO hardcoded strings); overlay forwarding; Rust wiring dispatches the Command and re-arms while playing; policy exclusion. Run `scripts/extract-translations.sh` and fill the new keys in all 9 `.po` (English reference).
- [ ] **Step 5: Run — expect PASS** (`cargo test -p adapter-gui`), then render: `cargo run -p slint-render -- crates/adapter-gui/ui/components/di_loop_test_harness.slint DiLoopHarness <scratchpad>/di_panel_771.png 600 560` — READ the PNG, verify both selects visible/aligned, iterate until right.
- [ ] **Step 6: Commit + push + issue comment.** `feat(#771): per-chain DI output selector in the DI panel`

---

### Task 8: Docs + full verification + hardware battery

**Files:**
- Modify: `docs/screens.md` (DI panel: source + output selects, isolated stream, own meters), `docs/audio-config.md` (DI = pre-rendered output-clocked player; chain edits apply on next play — pre-render is a deliberate trade-off per #771), `docs/superpowers/specs/2026-07-01-issue-717-di-stream-clock-decision.md` (append a short "superseded by #771" note pointing at the drift revert `f1131725e`)

- [ ] **Step 1: Update the docs.**
- [ ] **Step 2: Full check**: `cargo build` (zero warnings) + `cargo test --workspace` green; golden + volume invariants untouched.
- [ ] **Step 3: Hardware battery** (idle machine): `OPENRIG_HW_TESTS=1 cargo test -p infra-cpal` — guitar + DI simultaneously, zero xruns; per `docs/testing.md` "Real-hardware battery".
- [ ] **Step 4: Commit + push + issue comment** with the full verification evidence. PR only when the owner asks.

---

## Self-Review

- **Issue coverage:** isolated pre-rendered stream, output-clocked cursor (T2-T4) · guitar stream/meters untouched (T4 test 3, T5) · mixed into the CHOSEN output only via endpoint correlation (T1, T4 test 1 — the `ResolvedOutputDevice` identity gap is bypassed by correlating at arm time through the deterministic binding order, no struct change needed) · DI meter row reads own levels (T6) · output selector dispatching the existing Command (T7) · docs + HW proof (T8). Covered.
- **Placeholders:** none — every step names files/lines/commands; fixtures reference existing test files to copy from.
- **Type consistency:** `DiRenderedLoop` (T2) is what `DiPlayback.frames` (T3) holds and `arm_di_stream` (T4) installs; `resolve_di_output_index` (T1) and `build_di_output_options` (T7) share the same flat-order contract (pinned by tests in both).
- **Known behavior change ratified by the issue:** DI no longer live-follows chain edits while playing (pre-render trade-off, stated in #771 itself); documented in T8.
