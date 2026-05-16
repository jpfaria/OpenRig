# #451 — Engine: N isolated input runtimes + preset switching — Plan

> Sub-issue de #436. Depende de #449 (modelo, na branch). **Sensível a áudio.**

**Goal:** Engine loads a `RigProject` and runs N totally isolated input
runtimes; per-input preset switch builds the new pipeline OFF the audio thread,
swaps lock-free, crossfades, and lets the old pipeline's tail decay.

## Hard constraints (CLAUDE.md invariants — regression = stop)

- Isolation #4: zero buffer/lock/route/tap shared across input runtimes.
- Zero alloc/lock/syscall in the audio callback.
- SPSC absolute: **exactly one producer per output ring**.
- Volume invariants (`volume_invariants_tests.rs`) unchanged.
- Latency/xrun/jitter not regressed.

## Architecture decision (low-risk, reuses proven machinery)

The engine only understands `project::chain::Chain`. **Do not change the audio
callback's contract or the engine's domain dependency.** Instead:

1. **Pure bridge** `rig_to_chains(&RigProject) -> Vec<Chain>`: each `RigInput` +
   its active-preset `RigPreset` + routed `RigOutput`s → one synthetic legacy
   `Chain` (Input block from `sources`, processing blocks from preset, Output
   blocks from `routing`→`outputs`). 1:1 with what the engine already builds.
   Pure, hardware-free, fully TDD-able. Lives in `crates/engine` (new
   `rig_runtime.rs`) — engine already depends on `project`.
2. **N isolated runtimes**: feed the synthetic chains through the **existing**
   `build_runtime_graph`/`build_per_input_runtimes`. Per-input isolation
   (invariant #4) is already enforced there (issue #350 machinery). No new
   isolation code, no new SPSC writer.
3. **Lock-free preset swap**: reuse the **proven** `update_chain_runtime_state`
   3-step pattern (brief lock → build off-lock → brief lock + `ArcSwap` store).
   Switching preset = rebuild that input's segment from the new synthetic chain
   via the existing in-place update path. The existing per-segment
   `fade_in_remaining` cosine ramp already makes the swap click-free.
4. **Crossfade + tail (the RT-sensitive piece), SPSC-safe**: the crossfade is
   composed **inside one segment** (one ring producer preserved). During the
   window the segment holds an optional `outgoing` `InputProcessingState`;
   `process_single_segment` runs new + old, equal-power crossfades, and keeps
   feeding only the old one its decaying tail (no new input) until its envelope
   reaches zero, then drops it. This mirrors the existing per-block
   `FadeState` dry/wet crossfade, lifted to segment granularity. Transient
   extra CPU during the window only (hierarchy: CPU < sound+stability; bounded).

## Tasks (each gated: `cargo test -p engine --lib` green before commit)

- [ ] T1 — `rig_to_chains` pure bridge + tests (no audio thread). Synthetic
  `Chain` per input; sources→InputBlock; preset blocks in order; routing→
  OutputBlock(s); deterministic; round-trips through `build_chain_runtime_state`
  without panicking; isolation: distinct `ChainId` per input.
- [ ] T2 — `RigRuntime` controller (engine-level, transport-agnostic, no Slint):
  build N runtimes from a `RigProject` via existing `build_runtime_graph`;
  `switch_preset(input_name, idx)` validates + rebuilds that input only.
  Tests: N inputs → N isolated `ChainRuntimeState`; switching one input does
  not touch another's state (isolation #4 assertion).
- [ ] T3 — lock-free preset swap via existing in-place update path; assert no
  alloc on the swap-apply (build happens before the brief lock). Volume
  invariants + stream_isolation green.
- [ ] T4 — in-segment crossfade + tail: extend `InputProcessingState` with an
  optional decaying `outgoing`; equal-power envelope reusing `FADE_*`; old gets
  silence-fed tail; dropped at envelope zero. ONE SPSC writer preserved.
  New golden-style test: switching preset produces no click and the old
  reverb/delay tail is audible during the window; volume invariants unchanged.
- [ ] T5 — docs (`project-openrig-format.md` runtime section + `audio-config.md`)
  + `./scripts/qa.sh` green + push + comment #451.

## Regression gate (run every task)

```
cargo test -p engine --lib volume_invariants
cargo test -p engine --lib stream_isolation
cargo test -p engine --lib audio_signal_integrity
cargo test -p engine --lib
```

Any volume-invariant break ⇒ the source is wrong, never the test (CLAUDE.md).
