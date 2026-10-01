# #328 Part 3: Engine: Split -> Mix and path alignment — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a `Split` block with `end: Mix` play inside one segment of the engine: bus → path A ∥ path B (time-aligned) → mixer → bus, with zero allocation on the audio thread, and make every processor report the processing latency the alignment needs.

**Architecture:** Every processor gains `latency_samples()` (block-core trait default `0`; real values for the IR convolver, the 2× oversampler round trip, the brick wall limiter's look-ahead, VST3 and LV2). In the engine a new `RuntimeProcessor::Split(SplitRuntimeState)` holds both path node lists, a path-B buffer preallocated to the segment frame capacity, two preallocated alignment delay lines and the knobs as atomics. A pure math module (`runtime_split_mix.rs`) does split/mix per sample; `runtime_split_process.rs` runs one callback; `runtime_split_builder.rs` builds the node from the model, reusing path processors by id across rebuilds. Walkers (block toggle, bypass mirror, offline faulted list, probe summary, route convolution, and infra-cpal's #779 "chain contains a VST3" live-rebuild check) descend into both paths.

**Tech Stack:** Rust (workspace crates `block-core`, `block-mod`, `block-dyn`, `ir`, `vst3-host`, `plugin-loader`, `lv2`, `project` tests, `engine`, `infra-cpal`), `crossbeam-queue`, `hound` (tests only), `vst3` 0.3 bindings.

**Spec:** `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328/docs/superpowers/specs/2026-09-28-issue-328-chain-split-graph-design.md` — covers §4.1 (Split → Mix DSP inside one segment), §4.3 (path alignment), the Split/mixer knob semantics of §1.2 as the engine reads them, the engine-side invariants of §6 and the engine rows of §7 (engine math, engine structure "no allocation after warm-up with a split", hardware battery dual amp). §4.2 (Y → A/B segments, `SegmentPaths`, JACK) is Part 4.

**Depends on:** Part 1 (model): `AudioBlockKind::Split(SplitBlock)`, `SplitBlock { end, params, a, b }`, `SplitEnd { Mix, Y }`, `project::block::split_params::{SPLIT_MODE, LEVEL_TO_A, LEVEL_TO_B, BALANCE_A, BALANCE_B, MIX_LEVEL_A, MIX_LEVEL_B, MIX_PAN_A, MIX_PAN_B, MIX_B_POLARITY, MIX_MASTER, MIX_MASTER_SUM, SPLIT_MODE_DUAL_MONO, POLARITY_INVERT, default_split_params()}` (Part 1 plan, Task 1), and the minimal arm Part 1 adds to `runtime_block_builders.rs::build_block_runtime_node` (`AudioBlockKind::Split(_) => bypass_runtime_node(…)`, a pass-through) — Task 14 replaces it. This part adds no `Command`, so the orchestrator's `chain_id` field-name note does not apply here.

**Workspace:** every command runs from the solver clone, never the main folder:

```bash
W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328
cd "$W"
```

Commit messages are `feat(#328): …` in English with **no `Co-Authored-By` trailer** (`.claude/skills/openrig-code-quality/SKILL.md:446` and `docs/development/gitflow.md:39`: "Commits in English, no `Co-Authored-By` trailers" — the project rule; all six parts follow it). Every commit is pushed right away, after the push gate; after each push post the hash, files and tests run on the issue (`gh issue comment 328 --repo jpfaria/OpenRig --body "…"`) — every "Commit and push" step below carries the gate, the push and the comment. If a push is rejected: `git -C "$W" pull --rebase origin feature/issue-328`, re-run the gate, push again.

## Global Constraints

- Spec §6: "Chains without a split must produce bit-identical output; `volume_invariants_tests.rs` stays unchanged. A Split → Mix with default knobs and identical paths is unity." This part never edits `crates/engine/src/volume_invariants_tests.rs`.
- Spec §4.1: "The mixer is DSP inside the segment's processing, never a sum of two segments or two runtimes (stream-isolation law)."
- Spec §4.1: "Zero allocation, lock, syscall or I/O on the audio thread. `audio_alloc_invariant_tests.rs` gets a Split → Mix case."
- Spec §4.3: "The longer path is not delayed, so the chain's total latency does not change." Delay line "capped at 16384 samples. Above the cap, it clamps and logs at build time (never on the audio thread)."
- Spec §6: "File caps. New logic goes into new files, each with its `//! Responsibility:` / `// Responsibility:` header."
- CLAUDE.md (translated): every production file does ONE thing and declares it in its header as `//! Responsibility: <one sentence>`; the sentence may not contain "and", "e", "plus", "also", ",", ";", "/", "&" or "+". Line cap `.rs` 600 (smoke alarm only); `lib.rs`/`mod.rs` routers stay thin (< 100 LOC). Test files have no cap. No inline `#[cfg(test)] mod tests { … }` in production files — tests live in `<module>_tests.rs` attached with `#[path]`.
- CLAUDE.md invariant 8 (translated): zero allocation, lock, syscall or I/O on the audio thread, no exception.
- CLAUDE.md invariant 4 (translated): each stream is a fully isolated runtime; no buffer, lock, route or tap shared between streams; mixing between streams only in the backend.
- CLAUDE.md invariant 10 (translated): per-stream volume is immutable without an explicit request; if `volume_invariants_tests.rs` breaks, the source is wrong, never the test.
- CLAUDE.md (translated): TDD red-first is mandatory — no production change without a test seen failing first; show the behavioural red (the assertion line), not only a compile error; no `#[ignore]`.
- CLAUDE.md (translated): zero warnings (`cargo build --workspace`); before EVERY push ("Antes de TODO push: `cargo test --workspace`, nunca por crate") run `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo build --workspace` (zero warnings) and `VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates` — the one-line gate in every "Commit and push" step; Task 21 is the part's final full gate.
- CLAUDE.md (translated): repo content in English; Linux/JACK-only behaviour behind `cfg(all(target_os = "linux", feature = "jack"))` — this part adds none.
- Builds: `nice -n 19 cargo … -j 2`, one build at a time.
- Run `cargo fmt --all` before every `git add` — a few snippets below are longer than rustfmt's 100 columns, and the push gate (Task 21) runs `cargo fmt --all -- --check`.
- Temporary `#[allow(dead_code)]` on a `runtime_split` sub-module declaration is allowed only until the task named next to it removes it; Task 21 proves none is left.

## Review Focus

1. A split the user just added, with both paths still empty — the signal must pass at unity, not go silent. Pinned by `a_new_split_with_empty_paths_passes_the_signal_at_unity` (Task 14).
2. A block switched off/on inside a path (the amp-B footswitch) — it must fade like any block, never report "not found", and the paths must stay aligned. Pinned by `toggling_a_block_inside_a_path_fades_it_out` and `switching_the_ir_in_path_a_off_realigns_the_paths` (Task 18).
3. A knob move or a drag of a block between lanes while playing — no amp reload, no gap in the delayed path. Pinned by `a_mixer_knob_edit_keeps_the_path_processors`, `moving_a_block_from_path_a_to_path_b_keeps_its_processor`, `moving_a_block_out_of_a_path_keeps_its_processor` (Task 15) and `a_knob_edit_keeps_the_alignment_history` (Task 16).
4. A device buffer larger than the preallocated 1024 frames — every frame must still be split and mixed. Pinned by `a_callback_larger_than_the_preallocated_buffer_still_mixes_every_frame` (Task 11).
5. Paths whose latency differs by more than 16384 samples (a long look-ahead plugin) — clamp to the cap, log at build, never panic. Pinned by `a_difference_above_the_cap_is_clamped` (Task 7) and `alignment_above_the_cap_is_clamped` (Task 10).
6. A VST3 inside a path on a live edit — it must take the #779 in-place update, never a fresh off-thread `createInstance` while the audio thread is inside the old instance. Pinned by `a_vst3_inside_a_split_path_takes_the_in_place_rebuild` (Task 19b).

## Cross-part notes (read before starting)

- **Spec §4.1 "knobs as atomics … in-place param update path".** Every block param edit in this engine goes through `update_chain_runtime_state` (the existing in-place path: old nodes reused by id, `try_in_place_param_update` for model blocks). A `Split` has no `model_ref()`, so its in-place path is Tasks 15–16: the split node is re-made off the audio thread with a fresh `SplitKnobs` (atomics, read once per callback), while every path processor is reused by id and both delay lines continue from their history. No amp reloads, no gap — pinned by `a_mixer_knob_edit_keeps_the_path_processors` and `a_knob_edit_keeps_the_alignment_history`.
- **Lane drags vs. stream rebuilds (Part 1 / Part 4 owners).** Part 1 makes `SplitBlock`'s `model_identity()` encode the path ids and models, and `infra-cpal/src/io_topology.rs:93-111` (`chain_structure_signature`) puts `model_identity()` of every block in the stream signature. So a block dragged between lanes (or added to a path) on a Split → Mix changes the signature and takes the full stream rebuild, not `update_chain_runtime_state`. The engine-side reuse of Task 15 is correct either way, but the "no gap on a lane drag" promise of Review Focus 3 only holds in the live app if Part 4 keeps path contents of a Mix split out of the stream signature (only Y path SETS change streams). Report this on #328 when Task 15 lands.
- **`crates/engine/src/lib.rs` line budget.** It is 97 lines; this part adds exactly two module lines (Task 6 `mod runtime_split;`, Task 13 `mod runtime_block_reuse;`) → 99. Part 4 also adds module lines and already carries a fallback (`#[path]` declaration inside another file) — whichever part lands second checks `wc -l crates/engine/src/lib.rs` and keeps it under 100.
- **Left as is:** `crates/engine/src/runtime_graph.rs:437` prints a split as `?` in the chain-build log (`_ => "?"`). It is a log label with no behaviour and no test hook; not changed here. Switching `split_mode` between `same` and `dual_mono` on a stereo source flips the paths' #588 mono-content flag (Task 14 builder), so the DualMono models in the paths rebuild once on that switch — every other knob keeps them.

## File Structure

| File | Status | Responsibility | Lines now |
|---|---|---|---|
| `crates/block-core/src/processor.rs` | modify | declares the contract every block processor implements | 51 |
| `crates/block-core/src/output_gain.rs` | modify | applies a plugin's manifest output gain after its processing | 77 |
| `crates/block-core/src/traits_tests.rs` | modify (test) | — | 44 |
| `crates/block-core/src/dsp/oversampling.rs` | modify | runs a stage at twice the sample rate | 163 |
| `crates/block-core/src/dsp/oversampling_tests.rs` | modify (test) | — | 48 |
| `crates/block-mod/src/native_ring_modulator.rs` | modify | (existing ring modulator model) | 196 |
| `crates/block-mod/src/native_ring_modulator_tests.rs` | modify (test) | — | 52 |
| `crates/ir/src/fft_convolver.rs` | modify | (existing partitioned convolver) | 234 |
| `crates/ir/src/ir_processors.rs` | modify | exposes the convolver as a block processor | 70 |
| `crates/ir/src/from_package.rs` | modify | builds an impulse response processor out of a loaded plugin package | 156 |
| `crates/ir/src/from_package_tests.rs` | modify (test) | — | 64 |
| `crates/ir/tests/issue_328_ir_reports_latency.rs` | create (test) | — | 0 |
| `crates/block-dyn/src/native_limiter_brickwall/lookahead.rs` | modify | delays the signal while tracking the peak that is coming | 88 |
| `crates/block-dyn/src/native_limiter_brickwall/mono.rs` | modify | limits a single channel | 46 |
| `crates/block-dyn/src/native_limiter_brickwall/stereo.rs` | modify | limits two channels under one shared gain reduction | 56 |
| `crates/block-dyn/src/native_limiter_brickwall/mono_tests.rs`, `stereo_tests.rs` | modify (test) | — | 99, 87 |
| `crates/vst3-host/src/host.rs` | modify | drives one loaded VST3 plugin | 463 |
| `crates/vst3-host/src/host_load.rs` | modify | loads a VST3 bundle into an instantiated plugin | 320 |
| `crates/vst3-host/src/processor.rs` | modify | runs a VST3 plugin over a mono signal | 110 |
| `crates/vst3-host/src/stereo.rs` | modify | runs a VST3 plugin over a stereo signal | 145 |
| `crates/vst3-host/tests/issue_328_vst3_reports_latency.rs` | create (test) | — | 0 |
| `crates/plugin-loader/src/lv2_ports.rs` | modify | reads an LV2 bundle's ports out of its Turtle files | 314 |
| `crates/plugin-loader/src/dispatch_lv2_parse.rs` | modify | parses an LV2 bundle's ports out of its TTL | 208 |
| `crates/plugin-loader/src/dispatch_tests.rs` | modify (test) | — | 494 |
| `crates/project/src/block/disk_audio_mode_tests.rs` | modify (test fixture) | — | 53 |
| `crates/lv2/src/from_package.rs` | modify | instantiates an LV2 plugin out of a loaded package | 374 |
| `crates/lv2/src/from_package_tests.rs` | modify (test) | — | 147 |
| `crates/lv2/src/processor.rs` | modify | runs an LV2 plugin over a mono signal | 179 |
| `crates/lv2/src/processor_tests.rs` | create (test) | — | 0 |
| `crates/lv2/src/stereo_processor.rs` | modify | runs an LV2 plugin over a stereo signal | 196 |
| `crates/lv2/tests/issue_328_lv2_reports_latency.rs` | create (test) | — | 0 |
| `crates/engine/src/lib.rs` | modify | routes the engine crate's public surface | 97 |
| `crates/engine/src/runtime_split.rs` | create | routes the modules that run a chain split | 0 |
| `crates/engine/src/runtime_split_mix.rs` | create | computes the per-sample gains a split applies to its paths | 0 |
| `crates/engine/src/runtime_split_align.rs` | create | delays one split path so both paths meet in time | 0 |
| `crates/engine/src/runtime_split_knobs.rs` | create | holds a split's knob values where the audio thread reads them | 0 |
| `crates/engine/src/runtime_split_latency.rs` | create | measures how many samples of delay a path of runtime nodes adds | 0 |
| `crates/engine/src/runtime_split_state.rs` | create | describes the state a split keeps between callbacks | 0 |
| `crates/engine/src/runtime_split_process.rs` | create | runs one callback of a split over the segment's bus | 0 |
| `crates/engine/src/runtime_split_builder.rs` | create | builds the runtime node a split block turns into | 0 |
| `crates/engine/src/runtime_split_walk.rs` | create | visits every runtime node including those inside split paths | 0 |
| `crates/engine/src/runtime_block_reuse.rs` | create | decides whether a rebuilt block keeps its existing runtime node | 0 |
| `crates/engine/src/runtime_split_*_tests.rs`, `runtime_split_test_support.rs`, `runtime_split_dispatch_tests.rs`, `issue_328_split_mix_tests.rs` | create (tests) | — | 0 |
| `crates/engine/src/runtime_state.rs` | modify | describes the state a chain runtime keeps between callbacks | 309 |
| `crates/engine/src/runtime_block_builders.rs` | modify | builds the runtime node a block turns into | 520 |
| `crates/engine/src/runtime_process_segment.rs` | modify | processes one segment of a chain | 469 |
| `crates/engine/src/offline.rs` | modify | drives a chain through its DSP with no audio device | 258 |
| `crates/engine/src/probe.rs` | modify | measures how long a chain takes to process | 150 |
| `crates/engine/src/runtime_block_toggle.rs` | modify | flips a block's enabled flag without rebuilding the chain | 167 |
| `crates/engine/src/runtime_graph_assemble.rs` | modify | assembles the runtime graph of a chain | 402 |
| `crates/engine/src/route_convolution.rs` | modify | tells whether a convolution block feeds an output route | 60 |
| `crates/engine/src/route_convolution_tests.rs` | modify (test) | — | 169 |
| `crates/engine/src/runtime.rs` | modify (test registration only) | feeds the input callback into the chain runtimes it belongs to | 537 |
| `crates/engine/src/audio_alloc_invariant_tests.rs` | modify (test) | — | 334 |
| `crates/infra-cpal/src/controller_offthread_live_rebuild.rs` | modify | rebuilds a live chain off the audio thread | 253 |
| `crates/infra-cpal/src/controller_offthread_live_rebuild_tests.rs` | create (test) | — | 0 |
| `crates/infra-cpal/tests/issue_328_dual_amp_split.rs` | create (test) | — | 0 |
| `docs/blocks-catalog.md` | modify | block catalog | 154 |
| `docs/architecture.md` | modify | architecture | 477 |
| `docs/testing.md` | modify | tests | 371 |

`crates/engine/src/volume_invariants_tests.rs` (259) is read-only for this part.

---

### Task 0: Preflight

**Files:** none changed.

**Interfaces:**
- Consumes: Part 1 surface listed under **Depends on**.
- Produces: the list of Part 1 engine placeholder arms (used by Tasks 12 and 14) and the owner's answer for Task 2.

- [ ] **Step 1: Confirm the workspace is the solver clone on the feature branch**

Run: `test -d "$W/.git" && git -C "$W" rev-parse --abbrev-ref HEAD && git -C "$W" pull --ff-only && git -C "$W" rev-parse HEAD`
Expected: `feature/issue-328`, pull succeeds. Post the printed hash on #328 as "Part 3 base: <hash>" — Task 21 calls it `P3_BASE`.

- [ ] **Step 2: Confirm Part 1 landed**

Run:
```bash
grep -n "Split(SplitBlock)" crates/project/src/block/types.rs
grep -n "SplitBlock\|SplitEnd\|split_params" crates/project/src/block/mod.rs
grep -n "pub const SPLIT_MODE:\|pub const MIX_B_POLARITY\|pub const MIX_MASTER_SUM\|pub const SPLIT_MODE_DUAL_MONO\|pub const POLARITY_INVERT\|pub fn default_split_params" crates/project/src/block/split_params.rs
```
Expected: one hit per pattern; `project::block::{SplitBlock, SplitEnd}` and `project::block::split_params` are public. If any is missing, STOP: this part cannot start before Part 1.

- [ ] **Step 3: List Part 1's engine placeholders**

Run: `grep -n "Split" crates/engine/src/*.rs | grep -v "_tests.rs\|split_mono\|split_chain\|split_positions\|SPLIT"`
Expected: the arm(s) Part 1 added so `engine` compiles with `AudioBlockKind::Split` (typically in `runtime_block_builders.rs::build_block_runtime_node`). Write them down; Task 14 replaces every one of them.

- [ ] **Step 4: Baseline**

Run: `nice -n 19 cargo test -p engine -j 2`
Expected: all green (record the count of passed/ignored tests).

- [ ] **Step 5: Ask the owner the Task 2 question now (it gates Task 2 only)**

Ask in chat, one line: "The 2× oversampler reports 7 samples of latency but its measured round trip is 14.5 samples (a test pins 7). Fix it to 15 and update that test so a ring modulator in a split path stays aligned?" Record the answer in Task 2.

---

### Task 1: Processors report their latency (block-core)

**Files:**
- Modify: `crates/block-core/src/processor.rs:22-29` (MonoProcessor), `:44-51` (StereoProcessor)
- Modify: `crates/block-core/src/output_gain.rs:14-33` (GainScaledMono), `:41-54` (GainScaledStereo)
- Test: `crates/block-core/src/traits_tests.rs` (append)

**Interfaces:**
- Consumes: nothing.
- Produces: `fn latency_samples(&self) -> usize` with default `0` on both `block_core::MonoProcessor` and `block_core::StereoProcessor`; the output-gain wrappers forward it.

- [ ] **Step 1: Write the failing test** (append to `crates/block-core/src/traits_tests.rs`)

```rust
/// #328: a processor that delays its output by a known amount.
struct Delayed(usize);

impl MonoProcessor for Delayed {
    fn process_sample(&mut self, input: f32) -> f32 {
        input
    }

    fn latency_samples(&self) -> usize {
        self.0
    }
}

impl StereoProcessor for Delayed {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        input
    }

    fn latency_samples(&self) -> usize {
        self.0
    }
}

/// #328: the manifest output-gain wrapper sits around IR and LV2 processors;
/// a chain split aligns its paths from what the OUTER processor reports, so
/// the wrapper must pass the inner latency through.
#[test]
fn output_gain_keeps_the_wrapped_processors_latency() {
    let mono = wrap_with_output_gain_db(BlockProcessor::Mono(Box::new(Delayed(64))), Some(-6.0));
    let stereo =
        wrap_with_output_gain_db(BlockProcessor::Stereo(Box::new(Delayed(64))), Some(-6.0));
    match (mono, stereo) {
        (BlockProcessor::Mono(mono), BlockProcessor::Stereo(stereo)) => {
            assert_eq!(mono.latency_samples(), 64, "mono gain wrapper");
            assert_eq!(stereo.latency_samples(), 64, "stereo gain wrapper");
        }
        _ => panic!("the gain wrapper keeps the processor layout"),
    }
}
```

- [ ] **Step 2: Run it — expected compile failure**

Run: `nice -n 19 cargo test -p block-core -j 2 output_gain_keeps`
Expected: FAIL to compile — `method 'latency_samples' is not a member of trait 'MonoProcessor'` (E0407).

- [ ] **Step 3: Expose the trait method with its default** — in `processor.rs`, inside `MonoProcessor` after `try_in_place_update` (before the closing `}` at line 29):

```rust

    /// Samples of delay this processor adds between a sample going in and the
    /// same sample coming out: processing latency (block convolution,
    /// oversampling filters, plugin look-ahead), never the musical delay of a
    /// delay effect. A chain split sums it per path to line the two paths up
    /// (#328). Called at build time and, after a block toggle, on the audio
    /// thread: return a stored value — no allocation, no lock, no FFI call.
    fn latency_samples(&self) -> usize {
        0
    }
```

and inside `StereoProcessor` after its `try_in_place_update` (before line 51):

```rust

    /// Processing latency in samples — same contract as
    /// [`MonoProcessor::latency_samples`] (#328).
    fn latency_samples(&self) -> usize {
        0
    }
```

- [ ] **Step 4: Run it — expected behavioural failure**

Run: `nice -n 19 cargo test -p block-core -j 2 output_gain_keeps`
Expected: FAIL — `assertion 'left == right' failed: mono gain wrapper` with `left: 0`, `right: 64`.

- [ ] **Step 5: Forward through the wrappers** — in `output_gain.rs`, add to `impl MonoProcessor for GainScaledMono` (after `try_in_place_update`, line 32):

```rust

    fn latency_samples(&self) -> usize {
        self.inner.latency_samples()
    }
```

and to `impl StereoProcessor for GainScaledStereo` (after `process_block`, line 53):

```rust

    fn latency_samples(&self) -> usize {
        self.inner.latency_samples()
    }
```

- [ ] **Step 6: Run — expected PASS**

Run: `nice -n 19 cargo test -p block-core -j 2`
Expected: PASS (all block-core tests).

- [ ] **Step 7: Commit and push**

```bash
git -C "$W" add crates/block-core/src/processor.rs crates/block-core/src/output_gain.rs crates/block-core/src/traits_tests.rs
git -C "$W" commit -m "feat(#328): processors report the latency they add"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 2: The oversampler reports its measured round trip; the ring modulator reports it (GATED)

**Gate:** Task 0 Step 5. If the owner said NO, skip this task entirely (the ring modulator keeps reporting 0) and write "ring modulator: 0 — not reported (owner decision, #328 Task 2)" in the latency table of Task 14's docs.

**Files:**
- Modify: `crates/block-core/src/dsp/oversampling.rs:153-158`
- Modify: `crates/block-core/src/dsp/oversampling_tests.rs:44-48` (the old pin `latency_is_constant`, owner-approved) + append
- Modify: `crates/block-mod/src/native_ring_modulator.rs:114-134`, `:164-171`
- Test: `crates/block-mod/src/native_ring_modulator_tests.rs` (append)

**Interfaces:**
- Consumes: `MonoProcessor::latency_samples` / `StereoProcessor::latency_samples` (Task 1).
- Produces: `Oversampler2x::latency_samples() == 15`; `RingModulator` and its stereo wrapper report it.

- [ ] **Step 1: Write the failing tests**

Append to `crates/block-core/src/dsp/oversampling_tests.rs`:

```rust
/// #328: a chain split aligns its paths with what a block reports, so the
/// report must match the delay the impulse actually comes out with.
#[test]
fn reported_latency_matches_the_measured_round_trip_delay() {
    let mut os = Oversampler2x::new();
    let response: Vec<f32> = (0..64)
        .map(|n| {
            let [a, b] = os.up(if n == 0 { 1.0 } else { 0.0 });
            os.down([a, b])
        })
        .collect();
    let energy: f32 = response.iter().map(|v| v * v).sum();
    let centroid: f32 = response
        .iter()
        .enumerate()
        .map(|(n, v)| n as f32 * v * v)
        .sum::<f32>()
        / energy;
    let reported = Oversampler2x::new().latency_samples() as f32;
    assert!(
        (reported - centroid).abs() <= 0.5 + 1e-3,
        "reported {reported} samples, the impulse comes out at {centroid:.2}"
    );
}
```

Append to `crates/block-mod/src/native_ring_modulator_tests.rs`:

```rust
/// #328: the ring modulator runs through the 2× oversampler, so its wet
/// signal comes out one oversampler round trip late — and says so.
#[test]
fn reports_the_oversampler_round_trip_as_its_latency() {
    let expected = Oversampler2x::new().latency_samples();
    let mono = RingModulator::new(220.0, 1.0, 48_000.0);
    assert_eq!(mono.latency_samples(), expected, "mono ring modulator");

    let mut params = ParameterSet::default();
    params.insert(
        "carrier_hz",
        domain::value_objects::ParameterValue::Float(220.0),
    );
    params.insert("mix", domain::value_objects::ParameterValue::Float(100.0));
    match build(&params, 48_000.0, block_core::AudioChannelLayout::Stereo)
        .expect("stereo ring modulator builds")
    {
        block_core::BlockProcessor::Stereo(stereo) => {
            assert_eq!(stereo.latency_samples(), expected, "stereo ring modulator")
        }
        block_core::BlockProcessor::Mono(_) => {
            panic!("a stereo layout builds a stereo processor")
        }
    }
}
```

- [ ] **Step 2: Run — expected FAIL**

Run: `nice -n 19 cargo test -p block-core -j 2 reported_latency_matches; nice -n 19 cargo test -p block-mod -j 2 reports_the_oversampler_round_trip`
Expected: block-core FAIL — `reported 7 samples, the impulse comes out at 14.50`; block-mod FAIL — `mono ring modulator`, `left: 0`, `right: 7`.

- [ ] **Step 3: Implement** — replace `oversampling.rs:153-158` with:

```rust
    /// Round-trip group delay of [`Self::up`] + [`Self::down`], in
    /// original-rate samples — what a caller reports as its processing
    /// latency. Each linear-phase FIR of length N delays (N−1)/2 up-rate
    /// samples, so up and down together delay (N−1)/2 base-rate samples;
    /// keeping the odd phase on decimation moves the peak half a sample
    /// earlier (14.5 for N = 31, measured by
    /// `reported_latency_matches_the_measured_round_trip_delay`). The whole
    /// sample reported is 15 (#328).
    pub const fn latency_samples(&self) -> usize {
        (HBF_LEN - 1) / 2
    }
```

Replace the old pin in `oversampling_tests.rs:44-48` (owner-approved in Task 0 Step 5):

```rust
#[test]
fn latency_is_constant() {
    let os = Oversampler2x::new();
    assert_eq!(os.latency_samples(), 15);
}
```

In `native_ring_modulator.rs`, add to `impl MonoProcessor for RingModulator` (after `process_sample`, line 133):

```rust

    /// The wet signal is one oversampler round trip late; below 100 % mix
    /// the dry part inside this block is not delayed (#328).
    fn latency_samples(&self) -> usize {
        self.oversampler.latency_samples()
    }
```

and to `impl block_core::StereoProcessor for StereoRingMod` (after `process_frame`, line 170):

```rust

                fn latency_samples(&self) -> usize {
                    self.left.latency_samples()
                }
```

- [ ] **Step 4: Run — expected PASS**

Run: `nice -n 19 cargo test -p block-core -j 2 && nice -n 19 cargo test -p block-mod -j 2`
Expected: PASS.

- [ ] **Step 5: Commit and push**

```bash
git -C "$W" add crates/block-core/src/dsp/oversampling.rs crates/block-core/src/dsp/oversampling_tests.rs crates/block-mod/src/native_ring_modulator.rs crates/block-mod/src/native_ring_modulator_tests.rs
git -C "$W" commit -m "feat(#328): the oversampler reports its measured round trip"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 3: The IR convolver reports its partition latency

**Files:**
- Modify: `crates/ir/src/fft_convolver.rs` (add a method to `impl FftBlockConvolver` after `new`, line 58)
- Modify: `crates/ir/src/ir_processors.rs:20-30`, `:46-70`
- Modify: `crates/ir/src/from_package.rs:149-156` (`DualMonoIr`)
- Test: `crates/ir/tests/issue_328_ir_reports_latency.rs` (create), `crates/ir/src/from_package_tests.rs` (append)

**Interfaces:**
- Consumes: Task 1 trait methods.
- Produces: `MonoIrProcessor::latency_samples() == ir::PARTITION_SIZE` (64), same for `StereoIrProcessor` and `DualMonoIr`.

`block-ir`'s `DualMonoProcessor` (ir_generic_ir.rs) is not changed: `generic_ir` is a `DualMono` model, so the engine always builds its channels as separate mono processors and never reaches that wrapper.

- [ ] **Step 1: Write the failing tests**

Create `crates/ir/tests/issue_328_ir_reports_latency.rs`:

```rust
//! #328: the partitioned convolver delays every sample by one partition. A
//! chain split lines its paths up from what each block reports, so a cab in
//! one path only must report exactly the delay it adds — measured here with a
//! unit impulse response, whose output is the input, one partition late.

use block_core::{MonoProcessor, StereoProcessor};
use ir::{MonoIrProcessor, StereoIrProcessor, PARTITION_SIZE};

fn first_loud(samples: impl Iterator<Item = f32>) -> usize {
    samples
        .enumerate()
        .find(|(_, s)| s.abs() > 0.5)
        .map(|(i, _)| i)
        .expect("the impulse comes out")
}

#[test]
fn mono_convolver_reports_the_delay_it_adds() {
    let mut ir = MonoIrProcessor::new(vec![1.0]).expect("unit impulse response");
    let mut buffer = vec![0.0_f32; 4 * PARTITION_SIZE];
    buffer[0] = 1.0;
    ir.process_block(&mut buffer);
    let measured = first_loud(buffer.iter().copied());
    assert_eq!(
        ir.latency_samples(),
        measured,
        "the convolver must report the {measured}-sample delay it adds"
    );
}

#[test]
fn stereo_convolver_reports_the_delay_it_adds() {
    let mut ir = StereoIrProcessor::new(vec![1.0], vec![1.0]).expect("unit impulse responses");
    let mut buffer = vec![[0.0_f32; 2]; 4 * PARTITION_SIZE];
    buffer[0] = [1.0, 1.0];
    ir.process_block(&mut buffer);
    let measured = first_loud(buffer.iter().map(|frame| frame[0]));
    assert_eq!(
        ir.latency_samples(),
        measured,
        "the stereo convolver must report the {measured}-sample delay it adds"
    );
}
```

Append to `crates/ir/src/from_package_tests.rs`:

```rust
/// #328: a mono IR in a stereo layout runs as a dual-mono pair; the pair must
/// report its channels' convolution latency.
#[test]
fn dual_mono_ir_reports_its_channels_latency() {
    use block_core::StereoProcessor;
    let pair = super::DualMonoIr {
        left: Box::new(crate::MonoIrProcessor::new(vec![1.0]).expect("unit IR")),
        right: Box::new(crate::MonoIrProcessor::new(vec![1.0]).expect("unit IR")),
    };
    assert_eq!(pair.latency_samples(), crate::PARTITION_SIZE);
}
```

- [ ] **Step 2: Run — expected FAIL**

Run: `nice -n 19 cargo test -p ir -j 2 reports`
Expected: FAIL — `the convolver must report the 64-sample delay it adds`, `left: 0`, `right: 64` (both integration tests) and `dual_mono_ir_reports_its_channels_latency` `left: 0`, `right: 64`.

- [ ] **Step 3: Implement**

`fft_convolver.rs`, inside `impl FftBlockConvolver`, after `new`:

```rust

    /// Samples between an input and its convolved output: a sample waits
    /// for its partition to fill before the FFT runs, so every output is
    /// exactly one partition late (#617, ~1.3 ms at 48 kHz). Reported so a
    /// chain split can line its paths up (#328).
    pub(crate) const fn latency_samples(&self) -> usize {
        PARTITION_SIZE
    }
```

`ir_processors.rs`, `impl MonoProcessor for MonoIrProcessor` (after `process_block`):

```rust

    fn latency_samples(&self) -> usize {
        self.convolver.latency_samples()
    }
```

`impl StereoProcessor for StereoIrProcessor` (after `process_block`):

```rust

    fn latency_samples(&self) -> usize {
        self.left.latency_samples().max(self.right.latency_samples())
    }
```

`from_package.rs`, `impl StereoProcessor for DualMonoIr` (after `process_frame`):

```rust

    fn latency_samples(&self) -> usize {
        self.left.latency_samples().max(self.right.latency_samples())
    }
```

- [ ] **Step 4: Run — expected PASS**

Run: `nice -n 19 cargo test -p ir -j 2`
Expected: PASS.

- [ ] **Step 5: Commit and push**

```bash
git -C "$W" add crates/ir/src/fft_convolver.rs crates/ir/src/ir_processors.rs crates/ir/src/from_package.rs crates/ir/src/from_package_tests.rs crates/ir/tests/issue_328_ir_reports_latency.rs
git -C "$W" commit -m "feat(#328): the IR convolver reports its partition latency"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 3b: The brick wall limiter reports its look-ahead

Spec §4.3: "Built-in blocks with no look-ahead stay at 0" — the one built-in block WITH a look-ahead is the brick wall limiter: `LookaheadBuffer::push` (`crates/block-dyn/src/native_limiter_brickwall/lookahead.rs:47-78`) returns the sample written `len` pushes earlier, and `len` is `lookahead_ms` in samples (`mono.rs:20`, `stereo.rs:22`; 144 at the 3 ms default, 48 kHz). A limiter in one path only would comb-filter against the other path unless it reports that delay. No wrapper hides it: the model is `DualMono` (`params.rs:40`), so the engine builds `BrickWallLimiterMono` per channel (`crates/engine/src/runtime_processor_model.rs:67-101`); `BrickWallLimiterStereo` reports it too, for any caller that asks the model for a stereo processor.

**Files:**
- Modify: `crates/block-dyn/src/native_limiter_brickwall/lookahead.rs:21-31` (add a method after `new`)
- Modify: `crates/block-dyn/src/native_limiter_brickwall/mono.rs:35-42`, `crates/block-dyn/src/native_limiter_brickwall/stereo.rs:38-52`
- Test: `crates/block-dyn/src/native_limiter_brickwall/mono_tests.rs` (append), `crates/block-dyn/src/native_limiter_brickwall/stereo_tests.rs` (append)

**Interfaces:**
- Consumes: Task 1 trait methods.
- Produces: `LookaheadBuffer::delay_samples(&self) -> usize`; `BrickWallLimiterMono` / `BrickWallLimiterStereo` report it as `latency_samples()`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/block-dyn/src/native_limiter_brickwall/mono_tests.rs` (its `use super::*;` brings `BrickWallLimiterMono` and `MonoProcessor`):

```rust
/// #328: the look-ahead delays every sample by `lookahead_ms`. A chain split
/// lines its paths up from what each block reports, so the limiter must report
/// exactly the delay an impulse comes out with (0.5 is below the threshold,
/// so the gain is unity and the impulse leaves intact).
#[test]
fn reports_the_lookahead_delay_it_adds() {
    let mut lim = default_limiter();
    let measured = (0..4096)
        .map(|n| lim.process_sample(if n == 0 { 0.5 } else { 0.0 }))
        .position(|out| out.abs() > 0.25)
        .expect("the impulse comes out");
    assert_eq!(
        lim.latency_samples(),
        measured,
        "the limiter must report the {measured}-sample look-ahead it adds"
    );
}
```

Append to `crates/block-dyn/src/native_limiter_brickwall/stereo_tests.rs` (its `use super::*;` brings `BrickWallLimiterStereo` and `StereoProcessor`):

```rust
/// #328: the stereo limiter delays both channels by the same look-ahead and
/// must report it.
#[test]
fn reports_the_lookahead_delay_it_adds() {
    let mut lim = default_limiter();
    let measured = (0..4096)
        .map(|n| lim.process_frame(if n == 0 { [0.5, 0.5] } else { [0.0, 0.0] })[0])
        .position(|out| out.abs() > 0.25)
        .expect("the impulse comes out");
    assert_eq!(
        lim.latency_samples(),
        measured,
        "the stereo limiter must report the {measured}-sample look-ahead it adds"
    );
}
```

- [ ] **Step 2: Run — expected FAIL**

Run: `nice -n 19 cargo test -p block-dyn -j 2 reports_the_lookahead_delay_it_adds`
Expected: both FAIL — `the limiter must report the 144-sample look-ahead it adds` / `the stereo limiter must report the 144-sample look-ahead it adds`, `left: 0`, `right: 144` (the trait default from Task 1).

- [ ] **Step 3: Implement**

`lookahead.rs`, inside `impl LookaheadBuffer`, right after `new` (before the `#[cfg(test)] pub fn len`):

```rust

    /// Samples between a sample going in and the same sample coming out: the
    /// look-ahead the limiter reports as its processing latency (#328).
    pub fn delay_samples(&self) -> usize {
        self.len
    }
```

`mono.rs`, inside `impl MonoProcessor for BrickWallLimiterMono`, after `process_sample` (before the closing `}` at line 42):

```rust

    /// The look-ahead delays every sample by its length (#328).
    fn latency_samples(&self) -> usize {
        self.lookahead.delay_samples()
    }
```

`stereo.rs`, inside `impl StereoProcessor for BrickWallLimiterStereo`, after `process_frame` (before the closing `}` at line 52):

```rust

    /// Both channels are delayed by the same look-ahead (#328).
    fn latency_samples(&self) -> usize {
        self.lookahead_l.delay_samples()
    }
```

- [ ] **Step 4: Run — expected PASS**

Run: `nice -n 19 cargo test -p block-dyn -j 2 && nice -n 19 cargo build -p block-dyn -j 2 2>&1 | grep -c "^warning"`
Expected: PASS; `0`.

- [ ] **Step 5: Commit and push**

```bash
git -C "$W" add crates/block-dyn/src/native_limiter_brickwall/lookahead.rs crates/block-dyn/src/native_limiter_brickwall/mono.rs crates/block-dyn/src/native_limiter_brickwall/stereo.rs crates/block-dyn/src/native_limiter_brickwall/mono_tests.rs crates/block-dyn/src/native_limiter_brickwall/stereo_tests.rs
git -C "$W" commit -m "feat(#328): the brick wall limiter reports its look-ahead"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 4: VST3 processors report the plugin's declared latency

**Files:**
- Modify: `crates/vst3-host/src/host.rs:193-229` (`Vst3Inner` field), `:254-417` (`impl Vst3Plugin` accessor)
- Modify: `crates/vst3-host/src/host_load.rs:222-229` (read), `:304-318` (struct literal)
- Modify: `crates/vst3-host/src/processor.rs:64-110`, `crates/vst3-host/src/stereo.rs:80-145`
- Test: `crates/vst3-host/tests/issue_328_vst3_reports_latency.rs` (create)

**Interfaces:**
- Consumes: Task 1 trait methods.
- Produces: `pub fn Vst3Plugin::latency_samples(&self) -> usize` (value cached at load); `Vst3Processor` / `StereoVst3Processor` report it.

- [ ] **Step 1: Write the failing test** — create `crates/vst3-host/tests/issue_328_vst3_reports_latency.rs`:

```rust
//! #328: a VST3 block reports the processing latency its plugin declares
//! (`IAudioProcessor::getLatencySamples`), so a chain split can line its two
//! paths up. Scans the installed VST3 plugins for one that declares a
//! non-zero latency; skips loudly when none is installed (CI).

use block_core::{MonoProcessor, StereoProcessor};
use vst3::Steinberg::Vst::{IAudioProcessor, IAudioProcessorTrait};

const SR: f64 = 48_000.0;

fn declared_latency(plugin: &vst3_host::Vst3Plugin) -> usize {
    let processor = plugin
        .component()
        .cast::<IAudioProcessor>()
        .expect("the component is an audio processor");
    unsafe { processor.getLatencySamples() as usize }
}

#[test]
fn vst3_processors_report_the_plugins_declared_latency() {
    vst3_host::init_vst3_catalog(SR, &[]);
    for entry in vst3_host::vst3_catalog() {
        let Ok(uid) = vst3_host::resolve_uid_for_model(entry.model_id) else {
            continue;
        };
        let Ok(plugin) = vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[])
        else {
            continue;
        };
        let declared = declared_latency(&plugin);
        if declared == 0 {
            continue;
        }
        let stereo = vst3_host::StereoVst3Processor::new(plugin, None);
        assert_eq!(
            stereo.latency_samples(),
            declared,
            "{}: the stereo processor must report the plugin's latency",
            entry.model_id
        );
        let plugin = vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[])
            .expect("second instance");
        let mono = vst3_host::Vst3Processor::new(plugin, None);
        assert_eq!(
            mono.latency_samples(),
            declared,
            "{}: the mono processor must report the plugin's latency",
            entry.model_id
        );
        return;
    }
    eprintln!("[#328] no installed VST3 declares a processing latency — skipping");
}
```

- [ ] **Step 2: Run — expected FAIL (on a machine with such a plugin)**

Run: `nice -n 19 cargo test -p vst3-host -j 2 --test issue_328_vst3_reports_latency -- --nocapture`
Expected: FAIL — `<model>: the stereo processor must report the plugin's latency`, `left: 0`, `right: <N>`. On a machine with no latency-declaring VST3 it prints the skip line; then note in the issue comment that the red was not observable here.

- [ ] **Step 3: Implement**

`host.rs`, in `pub struct Vst3Inner` after `block_size: usize,`:

```rust

    /// The processing latency the plugin declared (`getLatencySamples`),
    /// read once at load so nothing asks the plugin again on the audio
    /// thread (#328).
    latency_samples: u32,
```

`host.rs`, in `impl Vst3Plugin` after `get_param`:

```rust

    /// Processing latency the plugin declared at load, in samples (#328).
    pub fn latency_samples(&self) -> usize {
        self.latency_samples as usize
    }
```

`host_load.rs`, right after the step-12 `setProcessing(true)` block (after line 229):

```rust

        // #328: the plugin's processing latency, read now that it processes,
        // so a chain split can line its paths up.
        let latency_samples = unsafe { audio_processor.getLatencySamples() };
```

and in the `Vst3Inner { … }` literal add `latency_samples,` after `block_size,`.

`processor.rs`, in `impl MonoProcessor for Vst3Processor` (after `process_block`):

```rust

    fn latency_samples(&self) -> usize {
        self.plugin.latency_samples()
    }
```

`stereo.rs`, in `impl StereoProcessor for StereoVst3Processor` (after `process_block`):

```rust

    fn latency_samples(&self) -> usize {
        self.plugin.latency_samples()
    }
```

- [ ] **Step 4: Run — expected PASS**

Run: `nice -n 19 cargo test -p vst3-host -j 2`
Expected: PASS.

- [ ] **Step 5: Commit and push**

```bash
git -C "$W" add crates/vst3-host/src/host.rs crates/vst3-host/src/host_load.rs crates/vst3-host/src/processor.rs crates/vst3-host/src/stereo.rs crates/vst3-host/tests/issue_328_vst3_reports_latency.rs
git -C "$W" commit -m "feat(#328): VST3 processors report the plugin's declared latency"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 5: LV2 processors report the plugin's latency port

**Files:**
- Modify: `crates/plugin-loader/src/lv2_ports.rs:22-43` (field), `crates/plugin-loader/src/dispatch_lv2_parse.rs:53-67` (parse)
- Modify (test fixtures gaining the field): `crates/lv2/src/from_package_tests.rs:8-23`, `:99-105`; `crates/project/src/block/disk_audio_mode_tests.rs:5-20`
- Modify: `crates/lv2/src/from_package.rs:34-82` (plan), `:234-246`, `:271-283` (apply)
- Modify: `crates/lv2/src/processor.rs`, `crates/lv2/src/stereo_processor.rs`
- Test: `crates/plugin-loader/src/dispatch_tests.rs` (append), `crates/lv2/src/from_package_tests.rs` (append), `crates/lv2/src/processor_tests.rs` (create), `crates/lv2/tests/issue_328_lv2_reports_latency.rs` (create)

**Interfaces:**
- Consumes: Task 1 trait methods.
- Produces: `Lv2Port.reports_latency: bool`; `PortPlan.latency: Option<usize>`; `Lv2Processor::with_latency_port(self, port_idx: usize) -> Self`, `StereoLv2Processor::with_latency_port(self, port_idx: usize) -> Self`; `pub(crate) fn latency_from_port(value: f32) -> usize` in `lv2/src/processor.rs`.

`DualMonoLv2` and `StereoAsMono` are not changed: `lv2_audio_mode` makes the engine ask for `Mono` on 1-out plugins and `Stereo` on 2-out plugins, so it never reaches those wrappers.

- [ ] **Step 1: Write the failing tests**

Append to `crates/plugin-loader/src/dispatch_tests.rs`:

```rust
// ── #328: the port that carries the plugin's processing latency ─────────

#[test]
fn parses_the_latency_port() {
    use std::fs;
    let tmp = std::env::temp_dir().join(format!("openrig-lv2-latency-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    fs::write(
        tmp.join("plug.ttl"),
        "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
<urn:test:plug>\n\
    a lv2:Plugin ;\n\
    lv2:port [\n\
        a lv2:OutputPort, lv2:ControlPort ;\n\
        lv2:index 0 ;\n\
        lv2:symbol \"latency\" ;\n\
        lv2:portProperty lv2:reportsLatency, lv2:integer ;\n\
    ] , [\n\
        a lv2:OutputPort, lv2:ControlPort ;\n\
        lv2:index 1 ;\n\
        lv2:symbol \"delay_report\" ;\n\
        lv2:designation lv2:latency ;\n\
    ] , [\n\
        a lv2:OutputPort, lv2:ControlPort ;\n\
        lv2:index 2 ;\n\
        lv2:symbol \"meter\" ;\n\
    ] .\n",
    )
    .unwrap();
    let ports = scan_lv2_ports(&tmp, "urn:test:plug").expect("scan ok");
    let reports = |symbol: &str| {
        ports
            .iter()
            .find(|p| p.symbol == symbol)
            .expect("port present")
            .reports_latency
    };
    assert!(reports("latency"), "lv2:reportsLatency marks the latency port");
    assert!(reports("delay_report"), "lv2:designation lv2:latency marks it too");
    assert!(!reports("meter"), "a plain meter is not a latency port");
    let _ = fs::remove_dir_all(&tmp);
}
```

Append to `crates/lv2/src/from_package_tests.rs`:

```rust
/// #328: the latency port is planned for the processor to read, and stays in
/// `extra_out` so it is connected before `run()` (#457).
#[test]
fn the_latency_port_is_planned_and_stays_connected() {
    let mut latency = port(7, "latency", Lv2PortRole::ControlOut);
    latency.reports_latency = true;
    let ports = vec![
        port(0, "in", Lv2PortRole::AudioIn),
        port(1, "out", Lv2PortRole::AudioOut),
        port(6, "meter", Lv2PortRole::ControlOut),
        latency,
    ];
    let plan = plan_ports(&ports, &ParameterSet::default());
    assert_eq!(
        plan.latency,
        Some(7),
        "the processor reads the plugin's latency from port 7"
    );
    assert!(
        plan.extra_out.contains(&7),
        "the latency port stays connected until the processor re-points it"
    );
}
```

Create `crates/lv2/src/processor_tests.rs`:

```rust
//! #328: what an LV2 latency port value becomes as a sample count.

use super::latency_from_port;

#[test]
fn a_reported_latency_becomes_whole_samples() {
    assert_eq!(latency_from_port(64.0), 64);
    assert_eq!(latency_from_port(63.6), 64);
    assert_eq!(latency_from_port(-3.0), 0, "a negative report means none");
    assert_eq!(latency_from_port(f32::NAN), 0, "a broken report means none");
    assert_eq!(latency_from_port(f32::INFINITY), 0);
}
```

Create `crates/lv2/tests/issue_328_lv2_reports_latency.rs`:

```rust
//! #328: an LV2 plugin that declares a latency port (`lv2:reportsLatency`)
//! reports that latency through its processor, so a chain split can line its
//! paths up. Needs the owner's plugin tree (`OPENRIG_OWNER_PLUGINS` or a
//! sibling `OpenRig-plugins` checkout); skips loudly without it, or when no
//! buildable LV2 plugin in it declares a latency port.

use std::path::PathBuf;

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use plugin_loader::dispatch::scan_lv2_ports;
use plugin_loader::manifest::Backend;

fn owner_plugins_root() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("OPENRIG_OWNER_PLUGINS") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let cand = dir.join("OpenRig-plugins/plugins/source");
        if cand.is_dir() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn reported(processor: BlockProcessor) -> usize {
    match processor {
        BlockProcessor::Mono(p) => p.latency_samples(),
        BlockProcessor::Stereo(p) => p.latency_samples(),
    }
}

#[test]
fn an_lv2_plugin_with_a_latency_port_reports_it() {
    let Some(root) = owner_plugins_root() else {
        eprintln!("[#328] owner plugin tree absent (set OPENRIG_OWNER_PLUGINS) — skipping");
        return;
    };
    let packages = plugin_loader::discover(&root).expect("plugin tree readable");
    let mut reports = Vec::new();
    for package in packages.into_iter().flatten() {
        let Backend::Lv2 { plugin_uri, .. } = &package.manifest.backend else {
            continue;
        };
        let Ok(ports) = scan_lv2_ports(&package.root.join("data"), plugin_uri) else {
            continue;
        };
        if !ports.iter().any(|p| p.reports_latency) {
            continue;
        }
        for layout in [AudioChannelLayout::Stereo, AudioChannelLayout::Mono] {
            if let Ok(processor) =
                lv2::build_from_package(&package, &ParameterSet::default(), 48_000.0, layout)
            {
                reports.push((package.manifest.id.clone(), reported(processor)));
                break;
            }
        }
    }
    if reports.is_empty() {
        eprintln!("[#328] no buildable LV2 plugin with a latency port here — skipping");
        return;
    }
    assert!(
        reports.iter().any(|(_, samples)| *samples > 0),
        "BUG #328: every LV2 plugin that declares a latency port reported 0: {reports:?}"
    );
}
```

- [ ] **Step 2: Run — expected compile failure**

Run: `nice -n 19 cargo test -p plugin-loader -j 2 parses_the_latency_port`
Expected: FAIL to compile — `no field 'reports_latency' on type '&Lv2Port'`.

- [ ] **Step 3: Expose the new fields and function with inert values**

`lv2_ports.rs`, in `pub struct Lv2Port` after `range_steps`:

```rust
    /// `lv2:portProperty lv2:reportsLatency` (or `lv2:designation
    /// lv2:latency`): this output control port carries the plugin's
    /// processing latency in samples (#328).
    pub reports_latency: bool,
```

`dispatch_lv2_parse.rs`, in the `Lv2Port { … }` literal after `range_steps: …,` add `reports_latency: false,`.

`from_package_tests.rs` `port()` literal and `disk_audio_mode_tests.rs` `port()` literal: add `reports_latency: false,` after `range_steps: None,`; the `buggy_plan` literal in `from_package_tests.rs`: add `latency: None,` after `extra_out: vec![],`.

`from_package.rs`, in `struct PortPlan` after `extra_out`:

```rust
    /// The output control port that reports the plugin's latency (#328).
    /// It stays in `extra_out` so it is connected; the processor re-points
    /// it to its own slot.
    latency: Option<usize>,
```

and in `plan_ports`' `PortPlan { … }` add `latency: None,`.

`processor.rs`, above `impl MonoProcessor for Lv2Processor`:

```rust
/// Samples a plugin's latency-port value stands for (#328): rounded, and 0
/// for a negative or broken report.
pub(crate) fn latency_from_port(value: f32) -> usize {
    let _ = value;
    0
}
```

and at the end of `processor.rs`:

```rust

#[cfg(test)]
#[path = "processor_tests.rs"]
mod tests;
```

- [ ] **Step 4: Run — expected behavioural failure**

Run: `nice -n 19 cargo test -p plugin-loader -j 2 parses_the_latency_port; nice -n 19 cargo test -p lv2 -j 2 latency`
Expected: FAIL — `lv2:reportsLatency marks the latency port`; `the processor reads the plugin's latency from port 7` (`left: None`, `right: Some(7)`); `a_reported_latency_becomes_whole_samples` (`left: 0`, `right: 64`).

- [ ] **Step 5: Implement**

`dispatch_lv2_parse.rs`, replace `reports_latency: false,` with:

```rust
        reports_latency: properties.contains("lv2:reportsLatency")
            || capture_after(block, "lv2:designation").as_deref() == Some("lv2:latency"),
```

`from_package.rs` `plan_ports`, replace `latency: None,` with:

```rust
        latency: ports
            .iter()
            .find(|p| p.role == Lv2PortRole::ControlOut && p.reports_latency)
            .map(|p| p.index),
```

`processor.rs`: replace the stub body of `latency_from_port` with

```rust
    if value.is_finite() && value > 0.0 {
        value.round() as usize
    } else {
        0
    }
```

add two fields to `pub struct Lv2Processor` after `control_values`:

```rust
    /// Slot the plugin's latency port writes into (#328), boxed so its
    /// address stays put while the port is connected to it.
    latency_out: Box<f32>,
    /// Latency the plugin published on that port, in samples, read once at
    /// build (#328). 0 when the plugin declares no latency port.
    latency: usize,
```

initialise them in `with_extra_ports`' `Self { … }` with `latency_out: Box::new(0.0), latency: 0,`, add after `set_control`:

```rust

    /// Point the plugin's latency port at this processor's own slot, let the
    /// plugin publish its latency on one silent sample, and keep the value
    /// (#328). Build time only — never on the audio thread.
    pub fn with_latency_port(mut self, port_idx: usize) -> Self {
        unsafe {
            self.plugin.connect_port(
                port_idx as u32,
                &mut *self.latency_out as *mut f32 as *mut c_void,
            );
        }
        self.in_buf[0] = 0.0;
        self.plugin.run(1);
        self.latency = latency_from_port(*self.latency_out);
        self
    }
```

and in `impl MonoProcessor for Lv2Processor` after `process_block`:

```rust

    fn latency_samples(&self) -> usize {
        self.latency
    }
```

`stereo_processor.rs`: the same two fields in `pub struct StereoLv2Processor` after `control_values`, the same initialisers in its `with_extra_ports`, then after `set_control`:

```rust

    /// Point the plugin's latency port at this processor's own slot, let the
    /// plugin publish its latency on one silent frame, and keep the value
    /// (#328). Build time only — never on the audio thread.
    pub fn with_latency_port(mut self, port_idx: usize) -> Self {
        unsafe {
            self.plugin.connect_port(
                port_idx as u32,
                &mut *self.latency_out as *mut f32 as *mut c_void,
            );
        }
        self.load_input(0, [0.0, 0.0]);
        self.plugin.run(1);
        self.latency = crate::processor::latency_from_port(*self.latency_out);
        self
    }
```

and in `impl StereoProcessor for StereoLv2Processor` after `process_block`:

```rust

    fn latency_samples(&self) -> usize {
        self.latency
    }
```

`from_package.rs`, `build_mono_input`'s `make` closure becomes:

```rust
    let make = || -> Result<crate::Lv2Processor> {
        let processor = build_lv2_processor_full(
            lib_path,
            uri,
            sample_rate,
            bundle_path,
            &plan.audio_in,
            &plan.audio_out,
            &plan.control,
            &plan.atom,
            &plan.extra_out,
        )?;
        Ok(match plan.latency {
            Some(port) => processor.with_latency_port(port),
            None => processor,
        })
    };
```

and `build_stereo_input`'s `make`:

```rust
    let make = || -> Result<crate::StereoLv2Processor> {
        let processor = build_stereo_lv2_processor_full(
            lib_path,
            uri,
            sample_rate,
            bundle_path,
            &plan.audio_in,
            &plan.audio_out,
            &plan.control,
            &plan.atom,
            &plan.extra_out,
        )?;
        Ok(match plan.latency {
            Some(port) => processor.with_latency_port(port),
            None => processor,
        })
    };
```

- [ ] **Step 6: Run — expected PASS**

Run: `nice -n 19 cargo test -p plugin-loader -j 2 && nice -n 19 cargo test -p lv2 -j 2 && nice -n 19 cargo test -p project -j 2 disk_audio_mode`
Expected: PASS (the owner-tree test either asserts or prints its skip line — paste which in the issue comment).

- [ ] **Step 7: Commit and push**

```bash
git -C "$W" add crates/plugin-loader/src/lv2_ports.rs crates/plugin-loader/src/dispatch_lv2_parse.rs crates/plugin-loader/src/dispatch_tests.rs crates/project/src/block/disk_audio_mode_tests.rs crates/lv2/src/from_package.rs crates/lv2/src/from_package_tests.rs crates/lv2/src/processor.rs crates/lv2/src/processor_tests.rs crates/lv2/src/stereo_processor.rs crates/lv2/tests/issue_328_lv2_reports_latency.rs
git -C "$W" commit -m "feat(#328): LV2 processors report the plugin's latency port"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 6: Split and mixer sample math (`runtime_split_mix.rs`)

**Files:**
- Create: `crates/engine/src/runtime_split.rs` (router), `crates/engine/src/runtime_split_mix.rs`
- Modify: `crates/engine/src/lib.rs:78` (add `mod runtime_split;` after `pub mod runtime_segments;`)
- Test: `crates/engine/src/runtime_split_mix_tests.rs` (create)

**Interfaces:**
- Consumes: nothing.
- Produces (no engine types, contract names):
  - `pub struct SplitKnobValues { pub dual_mono: bool, pub level_to_a: f32, pub level_to_b: f32, pub balance_a: f32, pub balance_b: f32, pub mix_level_a: f32, pub mix_level_b: f32, pub mix_pan_a: f32, pub mix_pan_b: f32, pub mix_b_invert: bool, pub mix_master: f32, pub mix_master_sum: bool }` (levels/master linear, balance/pan −50…+50), `#[derive(Clone, Copy, Debug, PartialEq)]`
  - `pub fn SplitKnobValues::with_neutral_mixer(self) -> Self`
  - `pub fn pan_gains(pan: f32) -> (f32, f32)`
  - `pub fn split_inputs(frame: [f32; 2], knobs: &SplitKnobValues) -> ([f32; 2], [f32; 2])`
  - `pub fn mix_frame(a: [f32; 2], b: [f32; 2], knobs: &SplitKnobValues) -> [f32; 2]`
  - module path `crate::runtime_split::mix`

- [ ] **Step 1: Write the failing test** — create `crates/engine/src/runtime_split_mix_tests.rs`:

```rust
//! #328 spec §1.2 / §7 "Engine math": the split's input stage and the
//! mixer's per-sample law, on plain numbers.

use super::{mix_frame, pan_gains, split_inputs, SplitKnobValues};

fn ampero_defaults() -> SplitKnobValues {
    SplitKnobValues {
        dual_mono: false,
        level_to_a: 1.0,
        level_to_b: 1.0,
        balance_a: 0.0,
        balance_b: 0.0,
        mix_level_a: 1.0,
        mix_level_b: 1.0,
        mix_pan_a: 0.0,
        mix_pan_b: 0.0,
        mix_b_invert: false,
        mix_master: 0.5,
        mix_master_sum: false,
    }
}

fn assert_close(got: [f32; 2], want: [f32; 2]) {
    assert!(
        (got[0] - want[0]).abs() < 1e-6 && (got[1] - want[1]).abs() < 1e-6,
        "got {got:?}, want {want:?}"
    );
}

#[test]
fn centre_pan_is_unity_on_both_sides() {
    assert_eq!(pan_gains(0.0), (1.0, 1.0));
}

#[test]
fn hard_pans_silence_the_opposite_side() {
    assert_eq!(pan_gains(-50.0), (1.0, 0.0), "hard left keeps L only");
    assert_eq!(pan_gains(50.0), (0.0, 1.0), "hard right keeps R only");
}

#[test]
fn the_opposite_side_falls_linearly() {
    let (l, r) = pan_gains(25.0);
    assert!((l - 0.5).abs() < 1e-6 && r == 1.0, "pan +25 → ({l}, {r})");
    let (l, r) = pan_gains(-10.0);
    assert!(l == 1.0 && (r - 0.8).abs() < 1e-6, "pan -10 → ({l}, {r})");
}

#[test]
fn identical_paths_at_the_defaults_come_out_at_unity() {
    let knobs = ampero_defaults();
    let input = [0.3, -0.2];
    let (a, b) = split_inputs(input, &knobs);
    assert_eq!((a, b), (input, input), "Mode I at level 100 feeds both paths the bus");
    assert_eq!(mix_frame(a, b, &knobs), input, "master 50 halves the doubled sum");
}

#[test]
fn hard_pans_keep_each_path_on_its_side() {
    let knobs = SplitKnobValues {
        mix_pan_a: -50.0,
        mix_pan_b: 50.0,
        mix_master: 1.0,
        ..ampero_defaults()
    };
    assert_eq!(mix_frame([0.4, 0.4], [0.0, 0.0], &knobs), [0.4, 0.0], "path A only on L");
    assert_eq!(mix_frame([0.0, 0.0], [0.7, 0.7], &knobs), [0.0, 0.7], "path B only on R");
}

#[test]
fn inverted_b_cancels_an_identical_path() {
    let knobs = SplitKnobValues {
        mix_b_invert: true,
        ..ampero_defaults()
    };
    assert_eq!(mix_frame([0.3, -0.2], [0.3, -0.2], &knobs), [0.0, 0.0]);
}

#[test]
fn master_sum_folds_the_output_to_dual_mono() {
    let knobs = SplitKnobValues {
        mix_master: 1.0,
        mix_master_sum: true,
        ..ampero_defaults()
    };
    assert_eq!(mix_frame([0.8, 0.0], [0.0, 0.0], &knobs), [0.4, 0.4]);
}

#[test]
fn mode_ii_feeds_each_path_the_channel_its_balance_picks() {
    let knobs = SplitKnobValues {
        dual_mono: true,
        balance_a: -50.0,
        balance_b: 50.0,
        ..ampero_defaults()
    };
    assert_eq!(split_inputs([0.9, 0.1], &knobs), ([0.9, 0.9], [0.1, 0.1]));
    let centre = SplitKnobValues {
        dual_mono: true,
        ..ampero_defaults()
    };
    assert_close(split_inputs([0.9, 0.1], &centre).0, [0.5, 0.5]);
}

#[test]
fn levels_scale_what_enters_and_leaves_each_path() {
    let knobs = SplitKnobValues {
        level_to_b: 0.5,
        mix_level_a: 0.0,
        mix_master: 1.0,
        ..ampero_defaults()
    };
    let (a, b) = split_inputs([0.8, 0.4], &knobs);
    assert_eq!(a, [0.8, 0.4]);
    assert_eq!(b, [0.4, 0.2]);
    assert_eq!(mix_frame(a, b, &knobs), [0.4, 0.2], "mix level 0 mutes path A");
}

#[test]
fn y_meets_its_paths_at_unity() {
    let knobs = SplitKnobValues {
        mix_pan_a: -50.0,
        mix_b_invert: true,
        mix_master: 0.1,
        mix_master_sum: true,
        ..ampero_defaults()
    }
    .with_neutral_mixer();
    assert_close(mix_frame([0.3, 0.1], [0.2, 0.4], &knobs), [0.5, 0.5]);
}
```

- [ ] **Step 2: Run — expected compile failure**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::mix`
Expected: FAIL to compile — `unresolved import` / `could not find runtime_split_mix`.

- [ ] **Step 3: Expose the module with inert bodies**

`lib.rs`, after `pub mod runtime_segments;` (line 78):

```rust
mod runtime_split;
```

Create `crates/engine/src/runtime_split.rs`:

```rust
//! Responsibility: routes the modules that run a chain split.
//!
//! A Split → Mix is DSP inside ONE segment (#328, spec §4.1): path B runs in
//! the split's own buffer, never in another segment or runtime, so the
//! stream-isolation law holds by construction.

// Task 12 wires the math into the live processor and removes this allow.
#[allow(dead_code)]
#[path = "runtime_split_mix.rs"]
pub(crate) mod mix;
```

Create `crates/engine/src/runtime_split_mix.rs`:

```rust
//! Responsibility: computes the per-sample gains a split applies to its paths.
//!
//! Pure numbers, no engine types (#328, spec §1.2). Levels and master are
//! linear gains (`x/100` of the knob); balance and pan run −50…+50.

/// Where a balance or pan knob reaches its end stop.
pub const PAN_EDGE: f32 = 50.0;

/// The split and mixer knobs as the audio thread uses them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitKnobValues {
    pub dual_mono: bool,
    pub level_to_a: f32,
    pub level_to_b: f32,
    pub balance_a: f32,
    pub balance_b: f32,
    pub mix_level_a: f32,
    pub mix_level_b: f32,
    pub mix_pan_a: f32,
    pub mix_pan_b: f32,
    pub mix_b_invert: bool,
    pub mix_master: f32,
    pub mix_master_sum: bool,
}

impl SplitKnobValues {
    /// Y → A/B: the paths meet at unity; the mixer knobs do not apply.
    pub fn with_neutral_mixer(self) -> Self {
        self
    }
}

pub fn pan_gains(pan: f32) -> (f32, f32) {
    let _ = pan;
    (0.0, 0.0)
}

pub fn split_inputs(frame: [f32; 2], knobs: &SplitKnobValues) -> ([f32; 2], [f32; 2]) {
    let _ = (frame, knobs);
    ([0.0; 2], [0.0; 2])
}

pub fn mix_frame(a: [f32; 2], b: [f32; 2], knobs: &SplitKnobValues) -> [f32; 2] {
    let _ = (b, knobs);
    a
}

#[cfg(test)]
#[path = "runtime_split_mix_tests.rs"]
mod tests;
```

- [ ] **Step 4: Run — expected behavioural failure**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::mix`
Expected: FAIL — e.g. `centre_pan_is_unity_on_both_sides` (`left: (0.0, 0.0)`, `right: (1.0, 1.0)`), `inverted_b_cancels_an_identical_path` (`left: [0.3, -0.2]`, `right: [0.0, 0.0]`).

- [ ] **Step 5: Implement** — replace the four stub bodies in `runtime_split_mix.rs`:

```rust
impl SplitKnobValues {
    /// Y → A/B: the paths meet at unity; the mixer knobs do not apply.
    pub fn with_neutral_mixer(self) -> Self {
        Self {
            mix_level_a: 1.0,
            mix_level_b: 1.0,
            mix_pan_a: 0.0,
            mix_pan_b: 0.0,
            mix_b_invert: false,
            mix_master: 1.0,
            mix_master_sum: false,
            ..self
        }
    }
}

/// Balance law (spec §1.2): centre is unity on both sides; toward one side
/// the opposite side falls linearly to 0 at ±50.
#[inline]
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let pan = pan.clamp(-PAN_EDGE, PAN_EDGE);
    if pan >= 0.0 {
        (1.0 - pan / PAN_EDGE, 1.0)
    } else {
        (1.0, 1.0 + pan / PAN_EDGE)
    }
}

/// Mode II source for one path: −50 = L, 0 = (L+R)/2, +50 = R, linear
/// in between.
#[inline]
fn balance_pick(frame: [f32; 2], balance: f32) -> f32 {
    let right = (balance.clamp(-PAN_EDGE, PAN_EDGE) + PAN_EDGE) / (2.0 * PAN_EDGE);
    frame[0] * (1.0 - right) + frame[1] * right
}

#[inline]
fn path_input(frame: [f32; 2], dual_mono: bool, balance: f32, level: f32) -> [f32; 2] {
    if dual_mono {
        let sample = balance_pick(frame, balance) * level;
        [sample, sample]
    } else {
        [frame[0] * level, frame[1] * level]
    }
}

/// What each path receives from one bus frame: Mode I = the bus × level,
/// Mode II = the dual-mono channel its balance picks × level.
#[inline]
pub fn split_inputs(frame: [f32; 2], knobs: &SplitKnobValues) -> ([f32; 2], [f32; 2]) {
    (
        path_input(frame, knobs.dual_mono, knobs.balance_a, knobs.level_to_a),
        path_input(frame, knobs.dual_mono, knobs.balance_b, knobs.level_to_b),
    )
}

/// The mixer: per-path balance and level, B polarity, master, optional
/// master sum to dual mono.
#[inline]
pub fn mix_frame(a: [f32; 2], b: [f32; 2], knobs: &SplitKnobValues) -> [f32; 2] {
    let (a_left, a_right) = pan_gains(knobs.mix_pan_a);
    let (b_left, b_right) = pan_gains(knobs.mix_pan_b);
    let b_gain = if knobs.mix_b_invert {
        -knobs.mix_level_b
    } else {
        knobs.mix_level_b
    };
    let left = (a[0] * a_left * knobs.mix_level_a + b[0] * b_left * b_gain) * knobs.mix_master;
    let right = (a[1] * a_right * knobs.mix_level_a + b[1] * b_right * b_gain) * knobs.mix_master;
    if knobs.mix_master_sum {
        let mid = (left + right) * 0.5;
        [mid, mid]
    } else {
        [left, right]
    }
}
```

- [ ] **Step 6: Run — expected PASS**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::mix && nice -n 19 cargo build -p engine -j 2`
Expected: PASS; build with zero warnings.

- [ ] **Step 7: Commit and push**

```bash
git -C "$W" add crates/engine/src/lib.rs crates/engine/src/runtime_split.rs crates/engine/src/runtime_split_mix.rs crates/engine/src/runtime_split_mix_tests.rs
git -C "$W" commit -m "feat(#328): split and mixer sample math"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 7: Alignment delay line (`runtime_split_align.rs`)

**Files:**
- Create: `crates/engine/src/runtime_split_align.rs`
- Modify: `crates/engine/src/runtime_split.rs` (declare the module)
- Test: `crates/engine/src/runtime_split_align_tests.rs` (create)

**Interfaces:**
- Consumes: `crate::runtime_audio_frame::AudioFrame`.
- Produces: `pub(crate) const MAX_ALIGN_SAMPLES: usize = 16_384`; `pub(crate) struct AlignPlan { pub(crate) delay_a: usize, pub(crate) delay_b: usize, pub(crate) clamped: bool }` (`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`); `pub(crate) fn plan_alignment(latency_a: usize, latency_b: usize) -> AlignPlan`; `pub(crate) struct AlignDelay` with `with_capacity(usize) -> Self`, `capacity(&self) -> usize`, `set_delay(&mut self, usize)`, `process(&mut self, &mut [AudioFrame])`, `adopt_history(&mut self, AlignDelay)`, and test-only `delay(&self) -> usize`.

- [ ] **Step 1: Write the failing test** — create `crates/engine/src/runtime_split_align_tests.rs`:

```rust
//! #328 spec §4.3: the shorter path is delayed by the difference, in a ring
//! preallocated at build, capped at 16384 samples.

use super::{plan_alignment, AlignDelay, AlignPlan, MAX_ALIGN_SAMPLES};
use crate::runtime_audio_frame::AudioFrame;

fn ramp(start: usize, n: usize) -> Vec<AudioFrame> {
    (start..start + n)
        .map(|i| AudioFrame::Stereo([i as f32, -(i as f32)]))
        .collect()
}

fn left(frames: &[AudioFrame]) -> Vec<f32> {
    frames
        .iter()
        .map(|f| match f {
            AudioFrame::Stereo([l, _]) => *l,
            AudioFrame::Mono(s) => *s,
        })
        .collect()
}

#[test]
fn the_path_with_less_latency_is_delayed_by_the_difference() {
    assert_eq!(
        plan_alignment(0, 64),
        AlignPlan { delay_a: 64, delay_b: 0, clamped: false }
    );
    assert_eq!(
        plan_alignment(71, 7),
        AlignPlan { delay_a: 0, delay_b: 64, clamped: false }
    );
    assert_eq!(
        plan_alignment(15, 15),
        AlignPlan { delay_a: 0, delay_b: 0, clamped: false }
    );
}

#[test]
fn a_difference_above_the_cap_is_clamped() {
    assert_eq!(
        plan_alignment(MAX_ALIGN_SAMPLES + 10, 0),
        AlignPlan { delay_a: 0, delay_b: MAX_ALIGN_SAMPLES, clamped: true }
    );
}

#[test]
fn delays_across_callbacks() {
    let mut delay = AlignDelay::with_capacity(64);
    delay.set_delay(5);
    let mut first = ramp(1, 4);
    delay.process(&mut first);
    let mut second = ramp(5, 4);
    delay.process(&mut second);
    assert_eq!(left(&first), vec![0.0; 4]);
    assert_eq!(left(&second), vec![0.0, 1.0, 2.0, 3.0]);
}

#[test]
fn zero_delay_leaves_the_frames_untouched() {
    let mut delay = AlignDelay::with_capacity(64);
    let mut frames = vec![AudioFrame::Mono(0.25); 3];
    delay.process(&mut frames);
    assert!(frames
        .iter()
        .all(|f| matches!(f, AudioFrame::Mono(s) if *s == 0.25)));
}

#[test]
fn a_delay_beyond_the_capacity_is_clamped() {
    let mut delay = AlignDelay::with_capacity(8);
    delay.set_delay(100);
    assert_eq!(delay.delay(), 8);
}

#[test]
fn the_capacity_never_exceeds_the_cap() {
    assert_eq!(
        AlignDelay::with_capacity(MAX_ALIGN_SAMPLES * 2).capacity(),
        MAX_ALIGN_SAMPLES
    );
}

#[test]
fn changing_the_delay_reads_history_already_recorded() {
    let mut delay = AlignDelay::with_capacity(16);
    let mut warm = ramp(1, 10);
    delay.process(&mut warm);
    delay.set_delay(3);
    let mut next = ramp(11, 2);
    delay.process(&mut next);
    assert_eq!(left(&next), vec![8.0, 9.0]);
}

#[test]
fn adopting_a_ring_of_the_same_size_keeps_its_history() {
    let mut old = AlignDelay::with_capacity(16);
    old.set_delay(3);
    let mut warm = ramp(1, 10);
    old.process(&mut warm);
    let mut fresh = AlignDelay::with_capacity(16);
    fresh.set_delay(3);
    fresh.adopt_history(old);
    let mut next = ramp(11, 2);
    fresh.process(&mut next);
    assert_eq!(left(&next), vec![8.0, 9.0]);
    assert_eq!(fresh.delay(), 3);
}

#[test]
fn a_ring_of_another_size_is_not_adopted() {
    let mut old = AlignDelay::with_capacity(8);
    let mut warm = ramp(1, 10);
    old.process(&mut warm);
    let mut fresh = AlignDelay::with_capacity(16);
    fresh.set_delay(3);
    fresh.adopt_history(old);
    let mut next = ramp(11, 2);
    fresh.process(&mut next);
    assert_eq!(left(&next), vec![0.0, 0.0]);
}
```

- [ ] **Step 2: Run — expected compile failure**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::align`
Expected: FAIL to compile — `could not find align in runtime_split`.

- [ ] **Step 3: Expose with inert bodies**

`runtime_split.rs`, add before the `mix` declaration:

```rust
// Task 16 wires the history adoption into the builder and removes this allow.
#[allow(dead_code)]
#[path = "runtime_split_align.rs"]
pub(crate) mod align;
```

Create `crates/engine/src/runtime_split_align.rs`:

```rust
//! Responsibility: delays one split path so both paths meet in time.
//!
//! Spec §4.3: summing two paths with different latency comb-filters. The
//! shorter path is delayed by the difference in a ring preallocated at
//! build; the longer path is never delayed, so the chain keeps its latency.

use crate::runtime_audio_frame::AudioFrame;

/// Longest alignment a split preallocates: 16384 samples ≈ 341 ms at 48 kHz.
pub(crate) const MAX_ALIGN_SAMPLES: usize = 16_384;

/// How far each path is delayed, and whether the cap cut the difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AlignPlan {
    pub(crate) delay_a: usize,
    pub(crate) delay_b: usize,
    pub(crate) clamped: bool,
}

pub(crate) fn plan_alignment(latency_a: usize, latency_b: usize) -> AlignPlan {
    let _ = (latency_a, latency_b);
    AlignPlan {
        delay_a: 0,
        delay_b: 0,
        clamped: false,
    }
}

/// One path's delay line: a ring of stereo frames, sized at build.
pub(crate) struct AlignDelay {
    ring: Vec<[f32; 2]>,
    write: usize,
    delay: usize,
}

impl AlignDelay {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            ring: vec![[0.0; 2]; capacity.min(MAX_ALIGN_SAMPLES) + 1],
            write: 0,
            delay: 0,
        }
    }

    pub(crate) fn capacity(&self) -> usize {
        self.ring.len() - 1
    }

    #[cfg(test)]
    pub(crate) fn delay(&self) -> usize {
        self.delay
    }

    pub(crate) fn set_delay(&mut self, delay: usize) {
        self.delay = delay;
    }

    pub(crate) fn adopt_history(&mut self, previous: AlignDelay) {
        let _ = previous;
    }

    #[inline]
    pub(crate) fn process(&mut self, frames: &mut [AudioFrame]) {
        let _ = frames;
    }
}

#[cfg(test)]
#[path = "runtime_split_align_tests.rs"]
mod tests;
```

- [ ] **Step 4: Run — expected behavioural failure**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::align`
Expected: FAIL — `the_path_with_less_latency_is_delayed_by_the_difference` (`delay_a: 0` vs `delay_a: 64`), `delays_across_callbacks` (`left: [1.0, 2.0, 3.0, 4.0]`, `right: [0.0, 0.0, 0.0, 0.0]`), `a_delay_beyond_the_capacity_is_clamped` (`left: 100`, `right: 8`).

- [ ] **Step 5: Implement** — replace the inert bodies:

```rust
pub(crate) fn plan_alignment(latency_a: usize, latency_b: usize) -> AlignPlan {
    let difference = latency_a.abs_diff(latency_b);
    let applied = difference.min(MAX_ALIGN_SAMPLES);
    let (delay_a, delay_b) = if latency_b > latency_a {
        (applied, 0)
    } else {
        (0, applied)
    };
    AlignPlan {
        delay_a,
        delay_b,
        clamped: difference > MAX_ALIGN_SAMPLES,
    }
}
```

```rust
    /// Delay the path by `delay` samples, never past the ring's capacity.
    /// No allocation: safe on the audio thread after a toggle.
    pub(crate) fn set_delay(&mut self, delay: usize) {
        self.delay = delay.min(self.capacity());
    }

    /// Continue from a previous build's ring (#328): a rebuild keeps what
    /// the delayed path already recorded instead of restarting it from
    /// silence. A ring of another size is not adopted.
    pub(crate) fn adopt_history(&mut self, previous: AlignDelay) {
        if previous.capacity() == self.capacity() {
            let delay = self.delay;
            *self = previous;
            self.delay = delay;
        }
    }

    /// Delay `frames` in place. The ring records every frame even at delay
    /// 0, so a later delay change reads real history.
    #[inline]
    pub(crate) fn process(&mut self, frames: &mut [AudioFrame]) {
        let len = self.ring.len();
        if len == 1 {
            return;
        }
        for frame in frames.iter_mut() {
            let input = match *frame {
                AudioFrame::Mono(sample) => [sample, sample],
                AudioFrame::Stereo(pair) => pair,
            };
            self.ring[self.write] = input;
            if self.delay > 0 {
                let read = (self.write + len - self.delay) % len;
                *frame = AudioFrame::Stereo(self.ring[read]);
            }
            self.write = (self.write + 1) % len;
        }
    }
```

- [ ] **Step 6: Run — expected PASS**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::align && nice -n 19 cargo build -p engine -j 2`
Expected: PASS; zero warnings.

- [ ] **Step 7: Commit and push**

```bash
git -C "$W" add crates/engine/src/runtime_split.rs crates/engine/src/runtime_split_align.rs crates/engine/src/runtime_split_align_tests.rs
git -C "$W" commit -m "feat(#328): preallocated alignment delay line"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 8: Split knobs as atomics (`runtime_split_knobs.rs`)

**Files:**
- Create: `crates/engine/src/runtime_split_knobs.rs`
- Modify: `crates/engine/src/runtime_split.rs` (declare the module)
- Test: `crates/engine/src/runtime_split_knobs_tests.rs` (create)

**Interfaces:**
- Consumes: `crate::runtime_split::mix::SplitKnobValues` (Task 6); `project::block::split_params::*` (Part 1), including its value constants `SPLIT_MODE_DUAL_MONO` (`"dual_mono"`) and `POLARITY_INVERT` (`"invert"`) — the single source of truth for those strings.
- Produces: `#[derive(Default)] pub(crate) struct SplitKnobs` with `from_params(&ParameterSet) -> Self`, `store(&self, &ParameterSet)`, `load(&self, mixes: bool) -> SplitKnobValues` (Relaxed atomics, same pattern as `volume_pct_bits`).

- [ ] **Step 1: Write the failing test** — create `crates/engine/src/runtime_split_knobs_tests.rs`:

```rust
//! #328 spec §1.2: the split's ParameterSet read as the values the audio
//! thread uses — percent knobs become linear gains, enum knobs become flags.

use block_core::param::ParameterSet;
use domain::value_objects::ParameterValue;
use project::block::split_params::{
    self, default_split_params, POLARITY_INVERT, SPLIT_MODE_DUAL_MONO,
};

use super::SplitKnobs;
use crate::runtime_split::mix::SplitKnobValues;

fn ampero_defaults() -> SplitKnobValues {
    SplitKnobValues {
        dual_mono: false,
        level_to_a: 1.0,
        level_to_b: 1.0,
        balance_a: 0.0,
        balance_b: 0.0,
        mix_level_a: 1.0,
        mix_level_b: 1.0,
        mix_pan_a: 0.0,
        mix_pan_b: 0.0,
        mix_b_invert: false,
        mix_master: 0.5,
        mix_master_sum: false,
    }
}

fn with(overrides: &[(&str, ParameterValue)]) -> ParameterSet {
    let mut params = default_split_params();
    for (key, value) in overrides {
        params.insert(*key, value.clone());
    }
    params
}

#[test]
fn defaults_read_as_the_ampero_defaults() {
    let values = SplitKnobs::from_params(&default_split_params()).load(true);
    assert_eq!(values, ampero_defaults());
}

#[test]
fn a_missing_knob_falls_back_to_its_default() {
    let values = SplitKnobs::from_params(&ParameterSet::default()).load(true);
    assert_eq!(values, ampero_defaults());
}

#[test]
fn percent_knobs_become_linear_gains() {
    let params = with(&[
        (split_params::LEVEL_TO_B, ParameterValue::Float(50.0)),
        (split_params::MIX_LEVEL_A, ParameterValue::Float(25.0)),
        (split_params::MIX_MASTER, ParameterValue::Float(100.0)),
    ]);
    let values = SplitKnobs::from_params(&params).load(true);
    assert_eq!(
        (values.level_to_b, values.mix_level_a, values.mix_master),
        (0.5, 0.25, 1.0)
    );
}

#[test]
fn mode_polarity_balance_pan_and_sum_follow_their_values() {
    let params = with(&[
        (split_params::SPLIT_MODE, ParameterValue::String(SPLIT_MODE_DUAL_MONO.into())),
        (split_params::MIX_B_POLARITY, ParameterValue::String(POLARITY_INVERT.into())),
        (split_params::MIX_MASTER_SUM, ParameterValue::Bool(true)),
        (split_params::BALANCE_A, ParameterValue::Float(-50.0)),
        (split_params::MIX_PAN_B, ParameterValue::Float(50.0)),
    ]);
    let values = SplitKnobs::from_params(&params).load(true);
    assert!(values.dual_mono, "split_mode dual_mono is Mode II");
    assert!(values.mix_b_invert, "mix_b_polarity invert flips path B");
    assert!(values.mix_master_sum);
    assert_eq!((values.balance_a, values.mix_pan_b), (-50.0, 50.0));
}

#[test]
fn a_y_split_reads_a_neutral_mixer_but_keeps_its_split_knobs() {
    let params = with(&[
        (split_params::LEVEL_TO_B, ParameterValue::Float(50.0)),
        (split_params::MIX_PAN_A, ParameterValue::Float(-50.0)),
        (split_params::MIX_B_POLARITY, ParameterValue::String(POLARITY_INVERT.into())),
        (split_params::MIX_MASTER, ParameterValue::Float(10.0)),
    ]);
    let values = SplitKnobs::from_params(&params).load(false);
    assert_eq!(values.level_to_b, 0.5);
    assert_eq!(
        (values.mix_pan_a, values.mix_b_invert, values.mix_master),
        (0.0, false, 1.0)
    );
}
```

- [ ] **Step 2: Run — expected compile failure**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::knobs`
Expected: FAIL to compile — `could not find knobs in runtime_split`.

- [ ] **Step 3: Expose — the struct and `load`, without `store`**

`runtime_split.rs`, add:

```rust
// Task 14 builds split nodes from these knobs and removes this allow.
#[allow(dead_code)]
#[path = "runtime_split_knobs.rs"]
pub(crate) mod knobs;
```

Create `crates/engine/src/runtime_split_knobs.rs`:

```rust
//! Responsibility: holds a split's knob values where the audio thread reads them.
//!
//! Same pattern as `ChainRuntimeState::volume_pct_bits`: every knob is an
//! atomic (f32 bits), stored off the audio thread and loaded with Relaxed
//! ordering once per callback, so a knob value is replaced in place without
//! rebuilding any path (#328, spec §4.1).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use block_core::param::ParameterSet;

use crate::runtime_split::mix::SplitKnobValues;

#[derive(Default)]
pub(crate) struct SplitKnobs {
    dual_mono: AtomicBool,
    level_to_a: AtomicU32,
    level_to_b: AtomicU32,
    balance_a: AtomicU32,
    balance_b: AtomicU32,
    mix_level_a: AtomicU32,
    mix_level_b: AtomicU32,
    mix_pan_a: AtomicU32,
    mix_pan_b: AtomicU32,
    mix_b_invert: AtomicBool,
    mix_master: AtomicU32,
    mix_master_sum: AtomicBool,
}

impl SplitKnobs {
    pub(crate) fn from_params(params: &ParameterSet) -> Self {
        let _ = params;
        Self::default()
    }

    /// The values for this callback. A Y split (`mixes == false`) meets its
    /// paths at unity, so its mixer knobs read neutral.
    #[inline]
    pub(crate) fn load(&self, mixes: bool) -> SplitKnobValues {
        let get = |slot: &AtomicU32| f32::from_bits(slot.load(Ordering::Relaxed));
        let values = SplitKnobValues {
            dual_mono: self.dual_mono.load(Ordering::Relaxed),
            level_to_a: get(&self.level_to_a),
            level_to_b: get(&self.level_to_b),
            balance_a: get(&self.balance_a),
            balance_b: get(&self.balance_b),
            mix_level_a: get(&self.mix_level_a),
            mix_level_b: get(&self.mix_level_b),
            mix_pan_a: get(&self.mix_pan_a),
            mix_pan_b: get(&self.mix_pan_b),
            mix_b_invert: self.mix_b_invert.load(Ordering::Relaxed),
            mix_master: get(&self.mix_master),
            mix_master_sum: self.mix_master_sum.load(Ordering::Relaxed),
        };
        if mixes {
            values
        } else {
            values.with_neutral_mixer()
        }
    }
}

#[cfg(test)]
#[path = "runtime_split_knobs_tests.rs"]
mod tests;
```

- [ ] **Step 4: Run — expected behavioural failure**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::knobs`
Expected: FAIL — `defaults_read_as_the_ampero_defaults` (`level_to_a: 0.0` vs `1.0`), `percent_knobs_become_linear_gains` (`left: (0.0, 0.0, 0.0)`, `right: (0.5, 0.25, 1.0)`), `mode_polarity_balance_pan_and_sum_follow_their_values` (`split_mode dual_mono is Mode II`).

- [ ] **Step 5: Implement `store` and use it** — in `runtime_split_knobs.rs` add the imports

```rust
use project::block::split_params::{
    default_split_params, BALANCE_A, BALANCE_B, LEVEL_TO_A, LEVEL_TO_B, MIX_B_POLARITY,
    MIX_LEVEL_A, MIX_LEVEL_B, MIX_MASTER, MIX_MASTER_SUM, MIX_PAN_A, MIX_PAN_B, POLARITY_INVERT,
    SPLIT_MODE, SPLIT_MODE_DUAL_MONO,
};
```

the constant and helpers above `impl SplitKnobs`:

```rust
/// Percent knobs (`level_to_*`, `mix_level_*`, `mix_master`) are `x/100`.
const PERCENT: f32 = 100.0;

fn number(params: &ParameterSet, defaults: &ParameterSet, key: &str) -> f32 {
    params
        .get_f32(key)
        .or_else(|| defaults.get_f32(key))
        .unwrap_or(0.0)
}

fn text<'a>(params: &'a ParameterSet, defaults: &'a ParameterSet, key: &str) -> &'a str {
    params
        .get_string(key)
        .or_else(|| defaults.get_string(key))
        .unwrap_or_default()
}

fn flag(params: &ParameterSet, defaults: &ParameterSet, key: &str) -> bool {
    params
        .get_bool(key)
        .or_else(|| defaults.get_bool(key))
        .unwrap_or(false)
}
```

and replace `from_params` with:

```rust
    pub(crate) fn from_params(params: &ParameterSet) -> Self {
        let knobs = Self::default();
        knobs.store(params);
        knobs
    }

    /// Replace every knob with the value in `params`, falling back to the
    /// Ampero default for a missing key. Off the audio thread.
    pub(crate) fn store(&self, params: &ParameterSet) {
        let defaults = default_split_params();
        let put = |slot: &AtomicU32, value: f32| slot.store(value.to_bits(), Ordering::Relaxed);
        self.dual_mono.store(
            text(params, &defaults, SPLIT_MODE) == SPLIT_MODE_DUAL_MONO,
            Ordering::Relaxed,
        );
        put(&self.level_to_a, number(params, &defaults, LEVEL_TO_A) / PERCENT);
        put(&self.level_to_b, number(params, &defaults, LEVEL_TO_B) / PERCENT);
        put(&self.balance_a, number(params, &defaults, BALANCE_A));
        put(&self.balance_b, number(params, &defaults, BALANCE_B));
        put(&self.mix_level_a, number(params, &defaults, MIX_LEVEL_A) / PERCENT);
        put(&self.mix_level_b, number(params, &defaults, MIX_LEVEL_B) / PERCENT);
        put(&self.mix_pan_a, number(params, &defaults, MIX_PAN_A));
        put(&self.mix_pan_b, number(params, &defaults, MIX_PAN_B));
        self.mix_b_invert.store(
            text(params, &defaults, MIX_B_POLARITY) == POLARITY_INVERT,
            Ordering::Relaxed,
        );
        put(&self.mix_master, number(params, &defaults, MIX_MASTER) / PERCENT);
        self.mix_master_sum
            .store(flag(params, &defaults, MIX_MASTER_SUM), Ordering::Relaxed);
    }
```

- [ ] **Step 6: Run — expected PASS**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::knobs && nice -n 19 cargo build -p engine -j 2`
Expected: PASS; zero warnings.

- [ ] **Step 7: Commit and push**

```bash
git -C "$W" add crates/engine/src/runtime_split.rs crates/engine/src/runtime_split_knobs.rs crates/engine/src/runtime_split_knobs_tests.rs
git -C "$W" commit -m "feat(#328): split knobs as atomics read once per callback"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 9: Path latency (`runtime_split_latency.rs`)

**Files:**
- Create: `crates/engine/src/runtime_split_latency.rs`, `crates/engine/src/runtime_split_test_support.rs` (test fixtures)
- Modify: `crates/engine/src/runtime_split.rs` (declare both)
- Test: `crates/engine/src/runtime_split_latency_tests.rs` (create)

**Interfaces:**
- Consumes: `MonoProcessor::latency_samples` / `StereoProcessor::latency_samples` (Task 1); `AudioProcessor`, `BlockRuntimeNode`, `RuntimeProcessor`, `SelectRuntimeState`.
- Produces: `pub(crate) fn processor_latency(&AudioProcessor) -> usize`, `node_latency(&BlockRuntimeNode) -> usize`, `node_latency_ceiling(&BlockRuntimeNode) -> usize`, `path_latency(&[BlockRuntimeNode]) -> usize`, `path_latency_ceiling(&[BlockRuntimeNode]) -> usize`. Test support (`crate::runtime_split::test_support`): `FixedDelay::new(samples)`, `Scale(f32)`, `delay_node(id, samples)`, `gain_node(id, gain)`, `mono_node(id, Box<dyn MonoProcessor>)`.

- [ ] **Step 1: Write the fixtures and the failing test**

Create `crates/engine/src/runtime_split_test_support.rs`:

```rust
//! Test fixtures for the split runtime (#328): hand-built runtime nodes whose
//! processors delay or scale by a known amount.

use block_core::{AudioChannelLayout, MonoProcessor};
use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::param::ParameterSet;

use crate::runtime_audio_frame::AudioProcessor;
use crate::runtime_block_builders::audio_block_runtime_node;
use crate::runtime_state::{BlockRuntimeNode, FadeState, ProcessorBuildOutcome};

/// Delays its input by exactly `samples` samples — and says so.
pub(crate) struct FixedDelay {
    ring: Vec<f32>,
    pos: usize,
}

impl FixedDelay {
    pub(crate) fn new(samples: usize) -> Self {
        Self {
            ring: vec![0.0; samples],
            pos: 0,
        }
    }
}

impl MonoProcessor for FixedDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        if self.ring.is_empty() {
            return input;
        }
        let out = self.ring[self.pos];
        self.ring[self.pos] = input;
        self.pos = (self.pos + 1) % self.ring.len();
        out
    }

    fn latency_samples(&self) -> usize {
        self.ring.len()
    }
}

/// Multiplies by a fixed gain, with no delay.
pub(crate) struct Scale(pub(crate) f32);

impl MonoProcessor for Scale {
    fn process_sample(&mut self, input: f32) -> f32 {
        input * self.0
    }
}

fn snapshot(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    }
}

/// An enabled, fully faded-in node running `processor`.
pub(crate) fn mono_node(id: &str, processor: Box<dyn MonoProcessor>) -> BlockRuntimeNode {
    let mut node = audio_block_runtime_node(
        &snapshot(id),
        AudioChannelLayout::Stereo,
        true,
        ProcessorBuildOutcome {
            processor: AudioProcessor::Mono(processor),
            output_layout: AudioChannelLayout::Stereo,
            stream_handle: None,
        },
    );
    node.fade_state = FadeState::Active;
    node
}

pub(crate) fn delay_node(id: &str, samples: usize) -> BlockRuntimeNode {
    mono_node(id, Box::new(FixedDelay::new(samples)))
}

pub(crate) fn gain_node(id: &str, gain: f32) -> BlockRuntimeNode {
    mono_node(id, Box::new(Scale(gain)))
}
```

Create `crates/engine/src/runtime_split_latency_tests.rs`:

```rust
//! #328 spec §4.3: each path's latency is the sum of what its enabled
//! blocks report; the ceiling keeps room for blocks switched off now.

use domain::ids::BlockId;

use super::{node_latency, node_latency_ceiling, path_latency, path_latency_ceiling, processor_latency};
use crate::runtime_audio_frame::AudioProcessor;
use crate::runtime_split::test_support::{delay_node, gain_node, FixedDelay};
use crate::runtime_state::{RuntimeProcessor, SelectRuntimeState};

#[test]
fn an_enabled_block_adds_what_its_processor_reports() {
    assert_eq!(node_latency(&delay_node("ir", 64)), 64);
}

#[test]
fn a_switched_off_block_adds_nothing_but_keeps_its_room() {
    let mut node = delay_node("ir", 64);
    node.block_snapshot.enabled = false;
    assert_eq!(node_latency(&node), 0, "a bypassed block does not process");
    assert_eq!(node_latency_ceiling(&node), 64, "it may be switched back on");
}

#[test]
fn a_faulted_block_adds_nothing() {
    let mut node = delay_node("ir", 64);
    node.faulted = true;
    assert_eq!((node_latency(&node), node_latency_ceiling(&node)), (0, 0));
}

#[test]
fn a_dual_mono_pair_reports_its_channels() {
    let pair = AudioProcessor::DualMono {
        left: Box::new(FixedDelay::new(7)),
        right: Box::new(FixedDelay::new(7)),
    };
    assert_eq!(processor_latency(&pair), 7);
}

#[test]
fn a_select_adds_what_its_selected_option_adds() {
    let mut node = gain_node("select", 1.0);
    node.processor = RuntimeProcessor::Select(SelectRuntimeState {
        selected_block_id: BlockId("slow".into()),
        options: vec![delay_node("fast", 5), delay_node("slow", 9)],
    });
    assert_eq!(node_latency(&node), 9);
}

#[test]
fn a_path_sums_its_blocks() {
    let mut off = delay_node("off", 100);
    off.block_snapshot.enabled = false;
    let path = vec![delay_node("ir", 64), gain_node("amp", 0.5), delay_node("os", 7), off];
    assert_eq!(path_latency(&path), 71);
    assert_eq!(path_latency_ceiling(&path), 171);
}
```

- [ ] **Step 2: Run — expected compile failure**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::latency`
Expected: FAIL to compile — `could not find latency in runtime_split`.

- [ ] **Step 3: Expose with inert bodies**

`runtime_split.rs`, add:

```rust
// Task 14 aligns built split nodes from these sums and removes this allow.
#[allow(dead_code)]
#[path = "runtime_split_latency.rs"]
pub(crate) mod latency;

#[cfg(test)]
#[path = "runtime_split_test_support.rs"]
pub(crate) mod test_support;
```

Create `crates/engine/src/runtime_split_latency.rs`:

```rust
//! Responsibility: measures how many samples of delay a path of runtime nodes adds.

use crate::runtime_audio_frame::AudioProcessor;
use crate::runtime_state::BlockRuntimeNode;

pub(crate) fn processor_latency(processor: &AudioProcessor) -> usize {
    let _ = processor;
    0
}

pub(crate) fn node_latency(node: &BlockRuntimeNode) -> usize {
    let _ = node;
    0
}

pub(crate) fn node_latency_ceiling(node: &BlockRuntimeNode) -> usize {
    let _ = node;
    0
}

pub(crate) fn path_latency(nodes: &[BlockRuntimeNode]) -> usize {
    nodes.iter().map(node_latency).sum()
}

pub(crate) fn path_latency_ceiling(nodes: &[BlockRuntimeNode]) -> usize {
    nodes.iter().map(node_latency_ceiling).sum()
}

#[cfg(test)]
#[path = "runtime_split_latency_tests.rs"]
mod tests;
```

- [ ] **Step 4: Run — expected behavioural failure**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::latency`
Expected: FAIL — `an_enabled_block_adds_what_its_processor_reports` (`left: 0`, `right: 64`), `a_path_sums_its_blocks` (`left: 0`, `right: 71`).

- [ ] **Step 5: Implement** — replace the three inert functions:

```rust
use crate::runtime_state::{BlockRuntimeNode, RuntimeProcessor};

/// Processing latency of one built processor. A dual-mono pair runs one
/// model per channel; the larger report is taken.
pub(crate) fn processor_latency(processor: &AudioProcessor) -> usize {
    match processor {
        AudioProcessor::Mono(processor) => processor.latency_samples(),
        AudioProcessor::DualMono { left, right } => {
            left.latency_samples().max(right.latency_samples())
        }
        AudioProcessor::Stereo(processor) | AudioProcessor::StereoFromMono(processor) => {
            processor.latency_samples()
        }
    }
}

/// Delay the node adds while it processes: nothing when it is switched off.
pub(crate) fn node_latency(node: &BlockRuntimeNode) -> usize {
    if node.block_snapshot.enabled {
        node_latency_ceiling(node)
    } else {
        0
    }
}

/// Delay the node adds once enabled — the room a path keeps for a toggle
/// that switches it back on. A faulted node never processes.
pub(crate) fn node_latency_ceiling(node: &BlockRuntimeNode) -> usize {
    if node.faulted {
        return 0;
    }
    match &node.processor {
        RuntimeProcessor::Audio(processor) => processor_latency(processor),
        RuntimeProcessor::Select(select) => select
            .options
            .iter()
            .find(|option| option.block_id == select.selected_block_id)
            .map(node_latency)
            .unwrap_or(0),
        RuntimeProcessor::Bypass => 0,
    }
}
```

(the file's `use crate::runtime_state::BlockRuntimeNode;` line becomes the combined import above).

- [ ] **Step 6: Run — expected PASS**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::latency && nice -n 19 cargo build -p engine -j 2`
Expected: PASS; zero warnings.

- [ ] **Step 7: Commit and push**

```bash
git -C "$W" add crates/engine/src/runtime_split.rs crates/engine/src/runtime_split_latency.rs crates/engine/src/runtime_split_latency_tests.rs crates/engine/src/runtime_split_test_support.rs
git -C "$W" commit -m "feat(#328): measure the latency of a path of runtime nodes"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 10: The split's runtime state (`runtime_split_state.rs`)

**Files:**
- Create: `crates/engine/src/runtime_split_state.rs`
- Modify: `crates/engine/src/runtime_state.rs:263` (add `SEGMENT_FRAME_CAPACITY` after `FADE_IN_FRAMES`)
- Modify: `crates/engine/src/runtime_graph_assemble.rs:322` (use the constant — same value, behaviour-preserving)
- Modify: `crates/engine/src/runtime_split.rs` (declare the module)
- Test: `crates/engine/src/runtime_split_state_tests.rs` (create)

**Interfaces:**
- Consumes: Tasks 7, 8, 9.
- Produces: `pub(crate) const SEGMENT_FRAME_CAPACITY: usize = 1024` in `crate::runtime_state`; `pub(crate) struct SplitRuntimeState { pub(crate) mixes: bool, pub(crate) a: Vec<BlockRuntimeNode>, pub(crate) b: Vec<BlockRuntimeNode>, pub(crate) b_buf: Vec<AudioFrame>, pub(crate) align_a: AlignDelay, pub(crate) align_b: AlignDelay, pub(crate) knobs: SplitKnobs }` with `new(mixes: bool, a: Vec<BlockRuntimeNode>, b: Vec<BlockRuntimeNode>, knobs: SplitKnobs, block_id: &BlockId) -> Self`, `refresh_alignment(&mut self) -> AlignPlan`, `adopt_history(&mut self, previous: SplitRuntimeState)`.

The largest callback the engine preallocates for is the segment frame buffer, `Vec::with_capacity(1024)` in `runtime_graph_assemble.rs:322`; path B's buffer uses the same constant. 1024 is also the largest buffer the app can ask a device for: `SUPPORTED_BUFFER_SIZES = &[32, 64, 128, 256, 512, 1024]` (`crates/adapter-gui/src/defaults.rs:9`) and the JACK supervisor clamps at `MAX_BUFFER_CLAMP: u32 = 1024` (`crates/infra-cpal/src/jack_supervisor/supervisor_spawn.rs:32`). A device that still delivers a larger callback grows the buffer once, exactly like the segment frame buffer does today (pinned by `a_callback_larger_than_the_preallocated_buffer_still_mixes_every_frame`, Task 11).

- [ ] **Step 1: Write the failing test** — create `crates/engine/src/runtime_split_state_tests.rs`:

```rust
//! #328 spec §4.1 / §4.3: what a split preallocates at build, and how it
//! lines its paths up.

use domain::ids::BlockId;
use project::block::split_params::default_split_params;

use super::SplitRuntimeState;
use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_split::align::MAX_ALIGN_SAMPLES;
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_split::test_support::delay_node;
use crate::runtime_state::{BlockRuntimeNode, SEGMENT_FRAME_CAPACITY};

fn state(a: Vec<BlockRuntimeNode>, b: Vec<BlockRuntimeNode>) -> SplitRuntimeState {
    SplitRuntimeState::new(
        true,
        a,
        b,
        SplitKnobs::from_params(&default_split_params()),
        &BlockId("split".into()),
    )
}

#[test]
fn the_shorter_path_is_delayed_by_the_difference() {
    let s = state(vec![delay_node("ir", 64)], vec![]);
    assert_eq!((s.align_a.delay(), s.align_b.delay()), (0, 64));
    let s = state(vec![], vec![delay_node("os", 7)]);
    assert_eq!((s.align_a.delay(), s.align_b.delay()), (7, 0));
}

#[test]
fn equal_paths_need_no_delay() {
    let s = state(vec![delay_node("ir_a", 64)], vec![delay_node("ir_b", 64)]);
    assert_eq!((s.align_a.delay(), s.align_b.delay()), (0, 0));
}

#[test]
fn a_switched_off_block_counts_again_after_realignment() {
    let mut ir = delay_node("ir", 64);
    ir.block_snapshot.enabled = false;
    let mut s = state(vec![ir], vec![]);
    assert_eq!(s.align_b.delay(), 0, "a bypassed block adds no delay");
    assert_eq!(s.align_b.capacity(), 64, "room is kept for when it comes back");
    s.a[0].block_snapshot.enabled = true;
    s.refresh_alignment();
    assert_eq!(s.align_b.delay(), 64);
}

#[test]
fn path_b_buffer_is_preallocated_for_the_largest_callback() {
    let s = state(vec![], vec![]);
    assert!(s.b_buf.capacity() >= SEGMENT_FRAME_CAPACITY);
}

#[test]
fn alignment_above_the_cap_is_clamped() {
    let s = state(vec![delay_node("lookahead", MAX_ALIGN_SAMPLES + 100)], vec![]);
    assert_eq!(s.align_b.delay(), MAX_ALIGN_SAMPLES);
}

#[test]
fn a_rebuilt_state_keeps_the_previous_delay_history() {
    let mut previous = state(vec![delay_node("ir", 4)], vec![]);
    let mut warm: Vec<AudioFrame> = (1..=10)
        .map(|i| AudioFrame::Stereo([i as f32, i as f32]))
        .collect();
    previous.align_b.process(&mut warm);
    let mut rebuilt = state(vec![delay_node("ir", 4)], vec![]);
    rebuilt.adopt_history(previous);
    let mut next = vec![AudioFrame::Stereo([11.0, 11.0]), AudioFrame::Stereo([12.0, 12.0])];
    rebuilt.align_b.process(&mut next);
    assert!(
        matches!(next[0], AudioFrame::Stereo([l, _]) if l == 7.0),
        "the delayed path continues from its history, got {:?}",
        next[0]
    );
}
```

- [ ] **Step 2: Run — expected compile failure**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::state`
Expected: FAIL to compile — `could not find state in runtime_split` / `SEGMENT_FRAME_CAPACITY not found`.

- [ ] **Step 3: Expose with inert alignment**

`runtime_state.rs`, after line 263 (`FADE_IN_FRAMES`):

```rust

/// Frames a segment preallocates for one callback — its frame buffer and a
/// split's path-B buffer (#328). A larger callback grows them once.
pub(crate) const SEGMENT_FRAME_CAPACITY: usize = 1024;
```

`runtime_graph_assemble.rs:322`: `frame_buffer: Vec::with_capacity(1024),` becomes

```rust
        frame_buffer: Vec::with_capacity(crate::runtime_state::SEGMENT_FRAME_CAPACITY),
```

`runtime_split.rs`, add:

```rust
// Task 16 wires the history adoption into the builder and removes this allow.
#[allow(dead_code)]
#[path = "runtime_split_state.rs"]
pub(crate) mod state;
```

Create `crates/engine/src/runtime_split_state.rs`:

```rust
//! Responsibility: describes the state a split keeps between callbacks.

use domain::ids::BlockId;

use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_split::align::{AlignDelay, AlignPlan};
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_state::BlockRuntimeNode;

/// One split's runtime (#328, spec §4.1): both paths, the buffer path B runs
/// in, the two alignment delay lines and the knobs. Everything is allocated
/// here, at build; the callback only reuses it.
pub(crate) struct SplitRuntimeState {
    /// `true` for Split → Mix; `false` for Y → A/B, whose paths meet at unity.
    pub(crate) mixes: bool,
    pub(crate) a: Vec<BlockRuntimeNode>,
    pub(crate) b: Vec<BlockRuntimeNode>,
    /// Path B's frames for the current callback.
    pub(crate) b_buf: Vec<AudioFrame>,
    pub(crate) align_a: AlignDelay,
    pub(crate) align_b: AlignDelay,
    pub(crate) knobs: SplitKnobs,
}

impl SplitRuntimeState {
    pub(crate) fn new(
        mixes: bool,
        a: Vec<BlockRuntimeNode>,
        b: Vec<BlockRuntimeNode>,
        knobs: SplitKnobs,
        block_id: &BlockId,
    ) -> Self {
        let _ = block_id;
        Self {
            mixes,
            a,
            b,
            b_buf: Vec::new(),
            align_a: AlignDelay::with_capacity(0),
            align_b: AlignDelay::with_capacity(0),
            knobs,
        }
    }

    pub(crate) fn refresh_alignment(&mut self) -> AlignPlan {
        AlignPlan {
            delay_a: 0,
            delay_b: 0,
            clamped: false,
        }
    }

    pub(crate) fn adopt_history(&mut self, previous: SplitRuntimeState) {
        let _ = previous;
    }
}

#[cfg(test)]
#[path = "runtime_split_state_tests.rs"]
mod tests;
```

- [ ] **Step 4: Run — expected behavioural failure**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::state`
Expected: FAIL — `the_shorter_path_is_delayed_by_the_difference` (`left: (0, 0)`, `right: (0, 64)`), `path_b_buffer_is_preallocated_for_the_largest_callback`, `a_rebuilt_state_keeps_the_previous_delay_history`.

- [ ] **Step 5: Implement** — replace the `impl` block and imports:

```rust
use domain::ids::BlockId;

use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_split::align::{plan_alignment, AlignDelay, AlignPlan, MAX_ALIGN_SAMPLES};
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_split::latency::{path_latency, path_latency_ceiling};
use crate::runtime_state::{BlockRuntimeNode, SEGMENT_FRAME_CAPACITY};
```

```rust
impl SplitRuntimeState {
    pub(crate) fn new(
        mixes: bool,
        a: Vec<BlockRuntimeNode>,
        b: Vec<BlockRuntimeNode>,
        knobs: SplitKnobs,
        block_id: &BlockId,
    ) -> Self {
        // Room for any toggle inside a path: the longest a path can get.
        let room = path_latency_ceiling(&a).max(path_latency_ceiling(&b));
        let mut state = Self {
            mixes,
            a,
            b,
            b_buf: Vec::with_capacity(SEGMENT_FRAME_CAPACITY),
            align_a: AlignDelay::with_capacity(room),
            align_b: AlignDelay::with_capacity(room),
            knobs,
        };
        if state.refresh_alignment().clamped {
            log::warn!(
                "split '{}': its paths differ by more than {} samples — aligned up to the cap",
                block_id.0,
                MAX_ALIGN_SAMPLES
            );
        }
        state
    }

    /// Line the paths up again from the blocks enabled now. Runs at build
    /// and, after a toggle inside a path, on the audio thread: no
    /// allocation, no log.
    pub(crate) fn refresh_alignment(&mut self) -> AlignPlan {
        let plan = plan_alignment(path_latency(&self.a), path_latency(&self.b));
        self.align_a.set_delay(plan.delay_a);
        self.align_b.set_delay(plan.delay_b);
        plan
    }

    /// Carry the previous build's delay history and path-B buffer into this
    /// one, so a rebuild (a knob or a path edit) does not restart the delayed
    /// path from silence. The previous path nodes were already handed to the
    /// reuse pool.
    pub(crate) fn adopt_history(&mut self, previous: SplitRuntimeState) {
        let SplitRuntimeState {
            align_a,
            align_b,
            b_buf,
            ..
        } = previous;
        self.align_a.adopt_history(align_a);
        self.align_b.adopt_history(align_b);
        if b_buf.capacity() >= self.b_buf.capacity() {
            self.b_buf = b_buf;
        }
    }
}
```

- [ ] **Step 6: Run — expected PASS**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split && nice -n 19 cargo test -p engine -j 2 volume && nice -n 19 cargo build -p engine -j 2`
Expected: PASS (the `volume*` suites prove the constant swap changed nothing); zero warnings.

- [ ] **Step 7: Commit and push**

```bash
git -C "$W" add crates/engine/src/runtime_state.rs crates/engine/src/runtime_graph_assemble.rs crates/engine/src/runtime_split.rs crates/engine/src/runtime_split_state.rs crates/engine/src/runtime_split_state_tests.rs
git -C "$W" commit -m "feat(#328): split runtime state preallocated at build"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 11: One callback of a split (`runtime_split_process.rs`)

**Files:**
- Create: `crates/engine/src/runtime_split_process.rs`
- Modify: `crates/engine/src/runtime_split.rs` (declare the module)
- Test: `crates/engine/src/runtime_split_process_tests.rs` (create)

**Interfaces:**
- Consumes: Tasks 6, 7, 8, 10; `crate::runtime_process_segment::process_audio_block` (tests only).
- Produces: `pub(crate) fn process_split<F>(split: &mut SplitRuntimeState, frames: &mut [AudioFrame], run: F) where F: FnMut(&mut BlockRuntimeNode, &mut [AudioFrame])` — `run` processes one path node (the live callback passes `process_audio_block`, the offline render its own runner).

- [ ] **Step 1: Write the failing test** — create `crates/engine/src/runtime_split_process_tests.rs`:

```rust
//! #328 spec §4.1 steps 1–4: fill path B's buffer, run both paths, delay
//! the shorter one, mix back into the bus.

use crossbeam_queue::ArrayQueue;
use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::split_params::{self, default_split_params};

use super::process_split;
use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_process_segment::process_audio_block;
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_split::state::SplitRuntimeState;
use crate::runtime_split::test_support::{delay_node, gain_node};
use crate::runtime_state::{BlockError, BlockRuntimeNode, SEGMENT_FRAME_CAPACITY};

fn knobs(overrides: &[(&str, ParameterValue)]) -> SplitKnobs {
    let mut params = default_split_params();
    for (key, value) in overrides {
        params.insert(*key, value.clone());
    }
    SplitKnobs::from_params(&params)
}

fn split(
    mixes: bool,
    a: Vec<BlockRuntimeNode>,
    b: Vec<BlockRuntimeNode>,
    overrides: &[(&str, ParameterValue)],
) -> SplitRuntimeState {
    SplitRuntimeState::new(mixes, a, b, knobs(overrides), &BlockId("split".into()))
}

fn run(state: &mut SplitRuntimeState, frames: &mut [AudioFrame]) {
    let queue = ArrayQueue::<BlockError>::new(8);
    process_split(state, frames, |node, path| process_audio_block(node, path, &queue));
}

fn sine(n: usize, start: usize) -> Vec<AudioFrame> {
    (start..start + n)
        .map(|i| {
            let s = 0.5 * (i as f32 * 0.05).sin();
            AudioFrame::Stereo([s, s])
        })
        .collect()
}

fn pair(frame: AudioFrame) -> [f32; 2] {
    match frame {
        AudioFrame::Stereo(pair) => pair,
        AudioFrame::Mono(s) => [s, s],
    }
}

fn assert_half_of(out: &[AudioFrame], input: &[AudioFrame]) {
    for (i, (o, x)) in out.iter().zip(input).enumerate() {
        let (o, x) = (pair(*o), pair(*x));
        assert!(
            (o[0] - 0.5 * x[0]).abs() < 1e-6 && (o[1] - 0.5 * x[1]).abs() < 1e-6,
            "frame {i}: {o:?} is not half of {x:?}"
        );
    }
}

#[test]
fn identical_paths_at_default_knobs_come_out_at_the_path_level() {
    let mut s = split(true, vec![gain_node("a", 0.5)], vec![gain_node("b", 0.5)], &[]);
    let input = sine(256, 0);
    let mut frames = input.clone();
    run(&mut s, &mut frames);
    assert_half_of(&frames, &input);
}

#[test]
fn inverted_b_cancels_an_identical_but_later_path_a() {
    let mut s = split(
        true,
        vec![delay_node("ir", 64)],
        vec![],
        &[(
            split_params::MIX_B_POLARITY,
            ParameterValue::String(split_params::POLARITY_INVERT.into()),
        )],
    );
    for callback in 0..3 {
        let mut frames = sine(128, callback * 128);
        run(&mut s, &mut frames);
        let peak = frames.iter().map(|f| pair(*f)[0].abs()).fold(0.0_f32, f32::max);
        assert!(peak < 1e-6, "callback {callback}: aligned paths must cancel, peak {peak}");
    }
}

#[test]
fn y_end_sums_both_paths_at_unity_ignoring_the_mixer() {
    let mut s = split(
        false,
        vec![],
        vec![],
        &[
            (split_params::MIX_PAN_A, ParameterValue::Float(-50.0)),
            (split_params::MIX_MASTER, ParameterValue::Float(10.0)),
        ],
    );
    let mut frames = vec![AudioFrame::Stereo([0.2, 0.1]); 4];
    run(&mut s, &mut frames);
    for frame in frames {
        let out = pair(frame);
        assert!((out[0] - 0.4).abs() < 1e-6 && (out[1] - 0.2).abs() < 1e-6, "{out:?}");
    }
}

#[test]
fn a_mono_bus_comes_out_stereo() {
    let mut s = split(true, vec![], vec![], &[]);
    let mut frames = vec![AudioFrame::Mono(0.3); 4];
    run(&mut s, &mut frames);
    assert!(frames
        .iter()
        .all(|f| matches!(f, AudioFrame::Stereo([l, r]) if (*l - 0.3).abs() < 1e-6 && (*r - 0.3).abs() < 1e-6)));
}

#[test]
fn a_callback_larger_than_the_preallocated_buffer_still_mixes_every_frame() {
    let mut s = split(true, vec![gain_node("a", 0.5)], vec![gain_node("b", 0.5)], &[]);
    let input = sine(2 * SEGMENT_FRAME_CAPACITY, 0);
    let mut frames = input.clone();
    run(&mut s, &mut frames);
    assert_half_of(&frames, &input);
}
```

- [ ] **Step 2: Run — expected compile failure**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::process`
Expected: FAIL to compile — `could not find process in runtime_split`.

- [ ] **Step 3: Expose with an inert body**

`runtime_split.rs`, add:

```rust
// Task 12 runs splits in the live callback and removes this allow.
#[allow(dead_code)]
#[path = "runtime_split_process.rs"]
pub(crate) mod process;
```

Create `crates/engine/src/runtime_split_process.rs`:

```rust
//! Responsibility: runs one callback of a split over the segment's bus.

use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_split::state::SplitRuntimeState;
use crate::runtime_state::BlockRuntimeNode;

pub(crate) fn process_split<F>(split: &mut SplitRuntimeState, frames: &mut [AudioFrame], run: F)
where
    F: FnMut(&mut BlockRuntimeNode, &mut [AudioFrame]),
{
    let _ = (split, frames, run);
}

#[cfg(test)]
#[path = "runtime_split_process_tests.rs"]
mod tests;
```

- [ ] **Step 4: Run — expected behavioural failure**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::process`
Expected: FAIL — `frame 1: [0.0249…, 0.0249…] is not half of […]`, `callback 0: aligned paths must cancel, peak 0.49…`, `y_end_sums…` (`[0.2, 0.1]`).

- [ ] **Step 5: Implement** — replace the file body (keep the header and test attachment):

```rust
//! Responsibility: runs one callback of a split over the segment's bus.
//!
//! Audio-thread hot path (#328, spec §4.1): path B runs in the split's own
//! buffer, never in another segment or runtime. No allocation while the
//! callback fits the buffer preallocated at build, no lock, no log.

use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_split::mix::{mix_frame, split_inputs};
use crate::runtime_split::state::SplitRuntimeState;
use crate::runtime_state::BlockRuntimeNode;

/// 1. fill path B's buffer from the bus and feed path A's input to the bus
///    in place; 2. run both paths through `run`; 3. delay the shorter path;
/// 4. mix back into the bus.
pub(crate) fn process_split<F>(split: &mut SplitRuntimeState, frames: &mut [AudioFrame], mut run: F)
where
    F: FnMut(&mut BlockRuntimeNode, &mut [AudioFrame]),
{
    let SplitRuntimeState {
        mixes,
        a,
        b,
        b_buf,
        align_a,
        align_b,
        knobs,
    } = split;
    let values = knobs.load(*mixes);
    b_buf.clear();
    b_buf.reserve(frames.len());
    for frame in frames.iter_mut() {
        let (to_a, to_b) = split_inputs(stereo(*frame), &values);
        *frame = AudioFrame::Stereo(to_a);
        b_buf.push(AudioFrame::Stereo(to_b));
    }
    for node in a.iter_mut() {
        run(node, frames);
    }
    for node in b.iter_mut() {
        run(node, b_buf.as_mut_slice());
    }
    align_a.process(frames);
    align_b.process(b_buf.as_mut_slice());
    for (frame, path_b) in frames.iter_mut().zip(b_buf.iter()) {
        *frame = AudioFrame::Stereo(mix_frame(stereo(*frame), stereo(*path_b), &values));
    }
}

#[inline(always)]
fn stereo(frame: AudioFrame) -> [f32; 2] {
    match frame {
        AudioFrame::Mono(sample) => [sample, sample],
        AudioFrame::Stereo(pair) => pair,
    }
}

#[cfg(test)]
#[path = "runtime_split_process_tests.rs"]
mod tests;
```

- [ ] **Step 6: Run — expected PASS**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split && nice -n 19 cargo build -p engine -j 2`
Expected: PASS; zero warnings.

- [ ] **Step 7: Commit and push**

```bash
git -C "$W" add crates/engine/src/runtime_split.rs crates/engine/src/runtime_split_process.rs crates/engine/src/runtime_split_process_tests.rs
git -C "$W" commit -m "feat(#328): run one callback of a split"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 12: `RuntimeProcessor::Split` in the live and offline processing

**Files:**
- Modify: `crates/engine/src/runtime_state.rs:167-185` (variant + `kind_label`)
- Modify: `crates/engine/src/runtime_process_segment.rs:449-455` (`apply_block_processor`)
- Modify: `crates/engine/src/offline.rs:243-258` (`apply_block_offline`)
- Modify: `crates/engine/src/runtime_block_builders.rs:36-43` (`node_emits_mono_content`)
- Modify: `crates/engine/src/runtime_split_latency.rs` (`node_latency_ceiling`)
- Modify: `crates/engine/src/runtime_split.rs` (remove the `mix`/`process` allows, attach dispatch tests)
- Test: `crates/engine/src/runtime_split_dispatch_tests.rs` (create)

**Interfaces:**
- Consumes: Task 11 `process_split`; Task 9 `path_latency`.
- Produces: `RuntimeProcessor::Split(SplitRuntimeState)` (contract name), `kind_label() == "split"`; a Split node reports `max(path_latency(a), path_latency(b))`; a split node's output is treated as decorrelated stereo by the #588 mono-collapse tracking.

- [ ] **Step 1: Write the failing test** — create `crates/engine/src/runtime_split_dispatch_tests.rs`:

```rust
//! #328: a split node runs wherever a block node runs — the live callback
//! and the offline render — and reports its longer path's latency.

use crossbeam_queue::ArrayQueue;
use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::split_params::{self, default_split_params};

use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_process_segment::apply_block_processor;
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_split::latency::node_latency;
use crate::runtime_split::state::SplitRuntimeState;
use crate::runtime_split::test_support::{delay_node, gain_node};
use crate::runtime_state::{BlockError, BlockRuntimeNode, RuntimeProcessor};

fn split_node(a: Vec<BlockRuntimeNode>, b: Vec<BlockRuntimeNode>, invert: bool) -> BlockRuntimeNode {
    let mut params = default_split_params();
    if invert {
        params.insert(
            split_params::MIX_B_POLARITY,
            ParameterValue::String(split_params::POLARITY_INVERT.into()),
        );
    }
    let mut node = gain_node("split", 1.0);
    node.processor = RuntimeProcessor::Split(SplitRuntimeState::new(
        true,
        a,
        b,
        SplitKnobs::from_params(&params),
        &BlockId("split".into()),
    ));
    node
}

fn peak(frames: &[AudioFrame]) -> f32 {
    frames
        .iter()
        .map(|f| match f {
            AudioFrame::Stereo([l, r]) => l.abs().max(r.abs()),
            AudioFrame::Mono(s) => s.abs(),
        })
        .fold(0.0_f32, f32::max)
}

#[test]
fn the_live_callback_runs_a_split_node() {
    let mut node = split_node(vec![delay_node("ir", 64)], vec![], true);
    let queue = ArrayQueue::<BlockError>::new(8);
    let mut frames: Vec<AudioFrame> = (0..256)
        .map(|i| {
            let s = 0.5 * (i as f32 * 0.05).sin();
            AudioFrame::Stereo([s, s])
        })
        .collect();
    apply_block_processor(&mut node, &mut frames, &queue);
    assert!(peak(&frames) < 1e-6, "aligned, inverted paths cancel; peak {}", peak(&frames));
}

#[test]
fn the_offline_render_runs_a_split_node() {
    let mut nodes = vec![split_node(vec![delay_node("ir", 64)], vec![], true)];
    let input: Vec<[f32; 2]> = (0..512)
        .map(|i| {
            let s = 0.5 * (i as f32 * 0.05).sin();
            [s, s]
        })
        .collect();
    let out = crate::offline::render_nodes_masked(&mut nodes, &input, 128, 0, &[true]);
    let loudest = out.iter().map(|[l, r]| l.abs().max(r.abs())).fold(0.0_f32, f32::max);
    assert!(loudest < 1e-6, "offline split must cancel too; peak {loudest}");
}

#[test]
fn a_split_node_reports_its_longer_path() {
    let node = split_node(vec![delay_node("ir", 64)], vec![delay_node("os", 7)], false);
    assert_eq!(node_latency(&node), 64);
}
```

and attach it in `runtime_split.rs`:

```rust
#[cfg(test)]
#[path = "runtime_split_dispatch_tests.rs"]
mod dispatch_tests;
```

- [ ] **Step 2: Run — expected compile failure**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::dispatch_tests`
Expected: FAIL to compile — `no variant named 'Split' found for enum 'RuntimeProcessor'`.

- [ ] **Step 3: Expose the variant with inert arms**

`runtime_state.rs` enum (lines 167-171):

```rust
pub(crate) enum RuntimeProcessor {
    Audio(AudioProcessor),
    Select(SelectRuntimeState),
    /// #328: a chain split — both paths and their mixer run inside this node.
    Split(crate::runtime_split::state::SplitRuntimeState),
    Bypass,
}
```

and in `kind_label` add `RuntimeProcessor::Split(_) => "split",`.

`runtime_process_segment.rs`, `apply_block_processor`, before `RuntimeProcessor::Bypass => {}`: `RuntimeProcessor::Split(_) => {}`.
`offline.rs`, `apply_block_offline`, before `RuntimeProcessor::Bypass => {}`: `RuntimeProcessor::Split(_) => {}`.
`runtime_block_builders.rs`, `node_emits_mono_content`, after the `Select` arm: `RuntimeProcessor::Split(_) => false,` — a split mixes two paths and may pan them apart, so what leaves it is treated as stereo content.
`runtime_split_latency.rs`, `node_latency_ceiling`, before `RuntimeProcessor::Bypass => 0,`: `RuntimeProcessor::Split(_) => 0,`.

- [ ] **Step 4: Run — expected behavioural failure**

Run: `nice -n 19 cargo test -p engine -j 2 runtime_split::dispatch_tests`
Expected: FAIL — `aligned, inverted paths cancel; peak 0.49…`, `offline split must cancel too; peak 0.49…`, `a_split_node_reports_its_longer_path` (`left: 0`, `right: 64`).

- [ ] **Step 5: Implement the arms**

`runtime_process_segment.rs` (replaces the inert arm):

```rust
        RuntimeProcessor::Split(split) => {
            crate::runtime_split::process::process_split(split, frames, |node, path| {
                process_audio_block(node, path, error_queue)
            });
        }
```

`offline.rs` (replaces the inert arm):

```rust
        RuntimeProcessor::Split(split) => {
            crate::runtime_split::process::process_split(split, frames, |node, path| {
                if node.block_snapshot.enabled {
                    apply_block_offline(node, path);
                }
            });
        }
```

`runtime_split_latency.rs` (replaces the inert arm):

```rust
        RuntimeProcessor::Split(split) => path_latency(&split.a).max(path_latency(&split.b)),
```

`runtime_split.rs`: delete the two `// Task 12 …` comments and their `#[allow(dead_code)]` lines above `mix` and `process`.

- [ ] **Step 6: Run — expected PASS**

Run: `nice -n 19 cargo test -p engine -j 2 && nice -n 19 cargo build -p engine -j 2 2>&1 | grep -c "^warning" `
Expected: all engine tests PASS; warning count `0`.

- [ ] **Step 7: Commit and push**

```bash
git -C "$W" add crates/engine/src/runtime_state.rs crates/engine/src/runtime_process_segment.rs crates/engine/src/offline.rs crates/engine/src/runtime_block_builders.rs crates/engine/src/runtime_split_latency.rs crates/engine/src/runtime_split.rs crates/engine/src/runtime_split_dispatch_tests.rs
git -C "$W" commit -m "feat(#328): split nodes run in the live callback and offline"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 13: Refactor — block reuse rules move out; the node loop takes any block list (behaviour-preserving)

**Files:**
- Create: `crates/engine/src/runtime_block_reuse.rs`
- Modify: `crates/engine/src/runtime_block_builders.rs:45-176` (loop → `build_nodes_for`), move `:178-287`
- Modify: `crates/engine/src/lib.rs:57` (add `mod runtime_block_reuse;` after `pub mod runtime_block_core;`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `pub(crate) fn reuse_pool(existing: Option<Vec<BlockRuntimeNode>>) -> HashMap<BlockId, BlockRuntimeNode>`, `pub(crate) fn try_reuse_block_node(reusable_nodes: &mut HashMap<BlockId, BlockRuntimeNode>, block: &AudioBlock, current_layout: AudioChannelLayout, content_mono: bool, sample_rate: f32) -> Option<BlockRuntimeNode>` (both in `crate::runtime_block_reuse`); `pub(crate) fn build_nodes_for(chain: &Chain, block_iter: &[&AudioBlock], input_layout: AudioChannelLayout, source_is_mono: bool, sample_rate: f32, reusable_nodes: &mut HashMap<BlockId, BlockRuntimeNode>) -> Result<(Vec<BlockRuntimeNode>, AudioChannelLayout, bool)>` in `crate::runtime_block_builders`.

This is a move: no behaviour changes, no test is added or rewritten; the whole engine suite is the guard. It also brings `runtime_block_builders.rs` from 520 to ≈ 430 lines before Task 14 adds the split dispatch.

- [ ] **Step 1: Create `crates/engine/src/runtime_block_reuse.rs`**

```rust
//! Responsibility: decides whether a rebuilt block keeps its existing runtime node.
//!
//! Moved out of `runtime_block_builders.rs` unchanged (#328) so the chain
//! builder and the split builder draw old nodes through one rule.

use std::collections::HashMap;

use block_core::AudioChannelLayout;
use domain::ids::BlockId;

use crate::runtime::FADE_IN_FRAMES;
use crate::runtime_audio_frame::AudioProcessor;
use crate::runtime_state::{BlockRuntimeNode, FadeState, RuntimeProcessor};

/// The old nodes a rebuild may keep, keyed by block id.
pub(crate) fn reuse_pool(
    existing: Option<Vec<BlockRuntimeNode>>,
) -> HashMap<BlockId, BlockRuntimeNode> {
    existing
        .unwrap_or_default()
        .into_iter()
        .map(|node| (node.block_id.clone(), node))
        .collect::<HashMap<_, _>>()
}
```

then cut `runtime_block_builders.rs` lines 178–287 (`fn try_reuse_block_node` and `fn try_in_place_param_update` with their doc comments) and paste them unchanged below `reuse_pool`, changing only `fn try_reuse_block_node(` to `pub(crate) fn try_reuse_block_node(`.

- [ ] **Step 2: Register the module** — `lib.rs` after `pub mod runtime_block_core;`:

```rust
mod runtime_block_reuse;
```

- [ ] **Step 3: Split the builder loop** — in `runtime_block_builders.rs` replace lines 45–176 (`build_runtime_block_nodes`) with:

```rust
pub(crate) fn build_runtime_block_nodes(
    chain: &Chain,
    input_layout: AudioChannelLayout,
    source_is_mono: bool,
    sample_rate: f32,
    existing: Option<Vec<BlockRuntimeNode>>,
    block_indices: Option<&[usize]>,
) -> Result<(Vec<BlockRuntimeNode>, AudioChannelLayout)> {
    let mut reusable_nodes = reuse_pool(existing);
    // If block_indices is provided, iterate only those blocks; otherwise iterate all
    let block_iter: Vec<&project::block::AudioBlock> = match block_indices {
        Some(indices) => indices
            .iter()
            .filter_map(|&i| chain.blocks.get(i))
            .collect(),
        None => chain.blocks.iter().collect(),
    };
    let (blocks, layout, _) = build_nodes_for(
        chain,
        &block_iter,
        input_layout,
        source_is_mono,
        sample_rate,
        &mut reusable_nodes,
    )?;
    Ok((blocks, layout))
}

/// Build the runtime nodes of `block_iter` in order, drawing old nodes from
/// `reusable_nodes`. Returns the nodes, the layout leaving the last one and
/// whether that signal is still effectively mono (#588). The chain builder
/// calls it for the chain's blocks; the split builder calls it per path (#328).
pub(crate) fn build_nodes_for(
    chain: &Chain,
    block_iter: &[&project::block::AudioBlock],
    input_layout: AudioChannelLayout,
    source_is_mono: bool,
    sample_rate: f32,
    reusable_nodes: &mut HashMap<BlockId, BlockRuntimeNode>,
) -> Result<(Vec<BlockRuntimeNode>, AudioChannelLayout, bool)> {
    let mut blocks = Vec::new();
    let mut current_layout = input_layout;
    // Issue #588: track whether the signal reaching the current position is
    // still effectively mono (a mono source broadcast to identical stereo
    // channels). Starts from the source layout and is cleared the moment a
    // block produces genuine stereo.
    let mut content_mono = source_is_mono;

    for &block in block_iter {
```

keep the old loop body (old lines 76–172) unchanged except the one call `try_reuse_block_node(&mut reusable_nodes, …)` which becomes `try_reuse_block_node(reusable_nodes, …)`, and end the function with:

```rust
    }

    Ok((blocks, current_layout, content_mono))
}
```

Update the imports at the top: add `use crate::runtime_block_reuse::{reuse_pool, try_reuse_block_node};`. `build_select_runtime_node` keeps calling `try_reuse_block_node(&mut reusable_option_nodes, …)` through that import.

- [ ] **Step 4: Run the whole engine suite — expected PASS unchanged**

Run: `nice -n 19 cargo test -p engine -j 2 && nice -n 19 cargo build -p engine -j 2 2>&1 | grep -c "^warning"`
Expected: same pass/ignored counts as Task 0 Step 4 plus the tests added since; warning count `0`; `wc -l crates/engine/src/runtime_block_builders.rs` ≈ 430.

- [ ] **Step 5: Commit and push**

```bash
git -C "$W" add crates/engine/src/lib.rs crates/engine/src/runtime_block_reuse.rs crates/engine/src/runtime_block_builders.rs
git -C "$W" commit -m "feat(#328): block reuse rules in their own file, node loop takes any block list (behaviour-preserving move)"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 14: The split plays — builder, dispatch, end-to-end sound, zero allocation, docs

**Files:**
- Create: `crates/engine/src/runtime_split_builder.rs`
- Modify: `crates/engine/src/runtime_split.rs` (declare `builder`; remove the `knobs`/`latency` allows)
- Modify: `crates/engine/src/runtime_block_builders.rs` (`build_nodes_for` split branch; `build_block_runtime_node` split arm; every Part 1 placeholder listed in Task 0 Step 3)
- Modify: `crates/engine/src/runtime.rs:537` (register the e2e test module)
- Test: `crates/engine/src/issue_328_split_mix_tests.rs` (create), `crates/engine/src/audio_alloc_invariant_tests.rs` (append)
- Docs: `docs/blocks-catalog.md` (new `###` subsection at the end of Part 1's `## Chain split (#328)` section, i.e. right before `## Backends de áudio`), `docs/architecture.md` (new section before `## Registry auto-gerado`, line 421)

**Interfaces:**
- Consumes: Tasks 8–13; Part 1 `SplitBlock`, `SplitEnd`, `split_params::SPLIT_MODE`.
- Produces: `pub(crate) fn build_split_runtime_node(chain: &Chain, block: &AudioBlock, split: &SplitBlock, input_layout: AudioChannelLayout, content_mono: bool, sample_rate: f32, reusable_nodes: &mut HashMap<BlockId, BlockRuntimeNode>) -> Result<BlockRuntimeNode>`. E2E test helpers (`pub(super)` in `crate::runtime::issue_328_split_mix`): `volume_block(id, pct)`, `split_block(id, end, knobs, a, b)`, `unit_impulse_ir_block(id)`, `with_split(runtime, read)`, `sine_block(frames, start)`.

- [ ] **Step 1: Write the failing tests** — create `crates/engine/src/issue_328_split_mix_tests.rs`:

```rust
//! #328 end to end: a Split → Mix chain built by the engine and driven
//! through the real input/output callbacks (spec §4.1, §4.3, §6, §7).

#![allow(unused_imports)]
use super::volume_invariants::*;
use super::{update_chain_runtime_state, ChainRuntimeState};

use domain::io_binding::ChannelMode;
use project::block::split_params::{self, default_split_params};
use project::block::{SplitBlock, SplitEnd};

use crate::runtime_split::state::SplitRuntimeState;
use crate::runtime_state::{FadeState, RuntimeProcessor};

pub(super) fn volume_block(id: &str, pct: f32) -> AudioBlock {
    let mut params = neutral_params("gain", "volume");
    params.insert("volume", ParameterValue::Float(pct));
    core_block(id, "gain", "volume", params)
}

pub(super) fn split_block(
    id: &str,
    end: SplitEnd,
    knobs: &[(&str, ParameterValue)],
    a: Vec<AudioBlock>,
    b: Vec<AudioBlock>,
) -> AudioBlock {
    let mut params = default_split_params();
    for (key, value) in knobs {
        params.insert(*key, value.clone());
    }
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock { end, params, a, b }),
    }
}

/// A `generic_ir` block whose response is a unit impulse: a pure delay of
/// one convolution partition (64 samples) with no other change.
pub(super) fn unit_impulse_ir_block(id: &str) -> AudioBlock {
    static PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    let path = PATH.get_or_init(|| {
        let path = std::env::temp_dir().join(format!(
            "openrig_issue_328_unit_impulse_{}.wav",
            std::process::id()
        ));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(&path, spec).expect("create unit-impulse IR");
        for sample in [1.0_f32, 0.0, 0.0, 0.0] {
            writer.write_sample(sample).expect("write IR sample");
        }
        writer.finalize().expect("finalize IR");
        path
    });
    let mut params = ParameterSet::default();
    params.insert("file", ParameterValue::String(path.to_string_lossy().into_owned()));
    core_block(id, "ir", "generic_ir", params)
}

/// Read the first live split node of the runtime's first segment.
pub(super) fn with_split<R>(
    runtime: &ChainRuntimeState,
    read: impl FnOnce(&SplitRuntimeState) -> R,
) -> R {
    let guard = runtime.processing.lock().expect("processing lock");
    let split = guard.input_states[0]
        .blocks
        .iter()
        .find_map(|node| match &node.processor {
            RuntimeProcessor::Split(split) => Some(split),
            _ => None,
        })
        .expect("the chain must run a live split node");
    read(split)
}

pub(super) fn sine_block(frames: usize, start: usize) -> Vec<f32> {
    (start..start + frames)
        .map(|n| 0.5 * (std::f32::consts::TAU * 1_000.0 * n as f32 / SR).sin())
        .collect()
}

fn mono_chain(id: &str, blocks: Vec<AudioBlock>) -> (Chain, Vec<IoBinding>) {
    chain_with_blocks(id, input_mono(vec![0]), blocks, output(ChannelMode::Stereo, vec![0, 1]))
}

fn steady_sine_peak(runtime: &Arc<ChainRuntimeState>, callbacks: usize, skip: usize) -> f32 {
    let mut peak = 0.0_f32;
    for callback in 0..callbacks {
        let out = drive_and_capture(runtime, 1, &sine_block(256, callback * 256), 2);
        if callback >= skip {
            peak = peak.max(peak_abs(&out));
        }
    }
    peak
}

fn invert() -> (&'static str, ParameterValue) {
    (
        split_params::MIX_B_POLARITY,
        ParameterValue::String(split_params::POLARITY_INVERT.into()),
    )
}

#[test]
fn a_new_split_with_empty_paths_passes_the_signal_at_unity() {
    let (chain, registry) =
        mono_chain("empty", vec![split_block("split", SplitEnd::Mix, &[], vec![], vec![])]);
    with_split(&build_runtime(&chain, &registry), |_| ());
    let peaks = measure_steady_per_channel_peak(&chain, &registry, 1, &[0.5], 2, 4);
    assert!(
        (peaks[0] - 0.5).abs() < TOLERANCE && (peaks[1] - 0.5).abs() < TOLERANCE,
        "an empty split must pass the bus at unity, got {peaks:?}"
    );
}

#[test]
fn identical_paths_at_default_knobs_keep_the_level_of_one_path() {
    let (chain, registry) = mono_chain(
        "unity",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[],
            vec![volume_block("a_vol", 50.0)],
            vec![volume_block("b_vol", 50.0)],
        )],
    );
    let (reference, ref_registry) = mono_chain("unity_ref", vec![volume_block("ref_vol", 50.0)]);
    with_split(&build_runtime(&chain, &registry), |_| ());
    let got = measure_steady_per_channel_peak(&chain, &registry, 1, &[0.5], 2, 4);
    let want = measure_steady_per_channel_peak(&reference, &ref_registry, 1, &[0.5], 2, 4);
    assert!(
        (got[0] - want[0]).abs() < TOLERANCE && (got[1] - want[1]).abs() < TOLERANCE,
        "two identical paths at the default mixer must sound like one path: {got:?} vs {want:?}"
    );
}

#[test]
fn dual_amp_hard_left_right_keeps_each_amp_on_its_side() {
    let (chain, registry) = mono_chain(
        "dual_amp",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[
                (split_params::MIX_PAN_A, ParameterValue::Float(-50.0)),
                (split_params::MIX_PAN_B, ParameterValue::Float(50.0)),
                (split_params::MIX_MASTER, ParameterValue::Float(100.0)),
            ],
            vec![volume_block("amp_a", 100.0)],
            vec![volume_block("amp_b", 50.0)],
        )],
    );
    let (reference, ref_registry) = mono_chain("dual_amp_ref", vec![volume_block("ref_b", 50.0)]);
    with_split(&build_runtime(&chain, &registry), |_| ());
    let peaks = measure_steady_per_channel_peak(&chain, &registry, 1, &[0.5], 2, 4);
    let amp_b = measure_steady_per_channel_peak(&reference, &ref_registry, 1, &[0.5], 2, 4)[1];
    assert!((peaks[0] - 0.5).abs() < TOLERANCE, "L carries amp A only, got {}", peaks[0]);
    assert!((peaks[1] - amp_b).abs() < TOLERANCE, "R carries amp B only, got {} want {amp_b}", peaks[1]);
}

#[test]
fn inverted_path_b_cancels_an_identical_path_a() {
    let (chain, registry) = mono_chain(
        "polarity",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[invert()],
            vec![volume_block("a_vol", 100.0)],
            vec![volume_block("b_vol", 100.0)],
        )],
    );
    with_split(&build_runtime(&chain, &registry), |_| ());
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.5], 2, 4);
    assert!(peak < 1e-6, "identical paths with B inverted cancel, peak {peak}");
}

#[test]
fn mode_ii_feeds_each_path_the_channel_its_balance_picks() {
    let (chain, registry) = chain_with_blocks(
        "mode_ii",
        input_stereo(vec![0, 1]),
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[
                (
                    split_params::SPLIT_MODE,
                    ParameterValue::String(split_params::SPLIT_MODE_DUAL_MONO.into()),
                ),
                (split_params::BALANCE_A, ParameterValue::Float(50.0)),
                (split_params::BALANCE_B, ParameterValue::Float(-50.0)),
                (split_params::MIX_PAN_A, ParameterValue::Float(-50.0)),
                (split_params::MIX_PAN_B, ParameterValue::Float(50.0)),
                (split_params::MIX_MASTER, ParameterValue::Float(100.0)),
            ],
            vec![],
            vec![],
        )],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    with_split(&build_runtime(&chain, &registry), |_| ());
    let peaks = measure_steady_per_channel_peak(&chain, &registry, 2, &[0.9, 0.1], 2, 4);
    assert!(
        (peaks[0] - 0.1).abs() < TOLERANCE && (peaks[1] - 0.9).abs() < TOLERANCE,
        "path A takes R onto L, path B takes L onto R: {peaks:?}"
    );
}

#[test]
fn ir_in_path_a_is_aligned_against_a_dry_path_b() {
    let (cancel, registry) = mono_chain(
        "align",
        vec![split_block("split", SplitEnd::Mix, &[invert()], vec![unit_impulse_ir_block("a_ir")], vec![])],
    );
    let runtime = build_runtime(&cancel, &registry);
    with_split(&runtime, |split| {
        assert_eq!(split.align_b.delay(), 64, "the dry path waits for the cab")
    });
    let peak = steady_sine_peak(&runtime, 24, 8);
    assert!(peak < 1e-4, "an aligned dry path cancels the cab path, peak {peak}");

    let (sum, sum_registry) = mono_chain(
        "align_sum",
        vec![split_block("split", SplitEnd::Mix, &[], vec![unit_impulse_ir_block("a_ir")], vec![])],
    );
    let peak = steady_sine_peak(&build_runtime(&sum, &sum_registry), 24, 8);
    assert!(peak > 0.4, "without the inversion the signal flows, peak {peak}");
}

#[test]
fn a_block_after_the_split_is_built_for_stereo_content() {
    let (chain, registry) = mono_chain(
        "post",
        vec![
            split_block("split", SplitEnd::Mix, &[], vec![volume_block("a_vol", 100.0)], vec![]),
            volume_block("post_vol", 100.0),
        ],
    );
    let runtime = build_runtime(&chain, &registry);
    let guard = runtime.processing.lock().expect("processing lock");
    let post = guard.input_states[0]
        .blocks
        .iter()
        .find(|node| node.block_id.0 == "post_vol")
        .expect("post-split block");
    assert!(!post.content_mono, "a split may pan its paths apart; what follows is stereo");
}

#[test]
fn each_segment_builds_its_own_split() {
    let (chain, _) = mono_chain(
        "isolation",
        vec![split_block("split", SplitEnd::Mix, &[], vec![volume_block("a_vol", 100.0)], vec![])],
    );
    let registry = vec![IoBinding {
        id: IO_BINDING_ID.into(),
        name: "IO".into(),
        inputs: vec![
            input_mono(vec![0]),
            IoEndpoint {
                name: "in1".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Mono,
                channels: vec![1],
            },
        ],
        outputs: vec![output(ChannelMode::Stereo, vec![0, 1])],
    }];
    let runtime = build_runtime(&chain, &registry);
    let guard = runtime.processing.lock().expect("processing lock");
    let buffers: Vec<*const crate::runtime_audio_frame::AudioFrame> = guard
        .input_states
        .iter()
        .filter_map(|state| {
            state.blocks.iter().find_map(|node| match &node.processor {
                RuntimeProcessor::Split(split) => Some(split.b_buf.as_ptr()),
                _ => None,
            })
        })
        .collect();
    assert_eq!(buffers.len(), 2, "every segment runs its own split");
    assert_ne!(buffers[0], buffers[1], "no path buffer is shared between segments");
}

#[test]
fn disabling_the_split_fades_it_out_through_its_paths() {
    let (chain, registry) = mono_chain(
        "disable",
        vec![split_block("split", SplitEnd::Mix, &[], vec![volume_block("a_vol", 100.0)], vec![])],
    );
    let runtime = build_runtime(&chain, &registry);
    let mut off = chain.clone();
    off.blocks[0].enabled = false;
    update_chain_runtime_state(&runtime, &off, SR, false, &[DEFAULT_ELASTIC_TARGET], &registry)
        .expect("in-place update");
    let guard = runtime.processing.lock().expect("processing lock");
    let node = &guard.input_states[0].blocks[0];
    assert!(matches!(node.fade_state, FadeState::FadingOut { .. }), "got {:?}", node.fade_state);
    assert!(
        matches!(&node.processor, RuntimeProcessor::Split(split) if split.a.len() == 1),
        "the fade-out runs through the real paths"
    );
}

#[test]
fn the_offline_render_plays_the_split() {
    let (chain, _) = mono_chain(
        "offline",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[invert()],
            vec![volume_block("a_vol", 100.0)],
            vec![volume_block("b_vol", 100.0)],
        )],
    );
    let input: Vec<[f32; 2]> = sine_block(1024, 0).into_iter().map(|s| [s, s]).collect();
    let outcome = crate::offline::render_chain(&chain, SR, &input, 64, 0).expect("offline render");
    let loudest = outcome
        .samples
        .iter()
        .skip(256)
        .map(|[l, r]| l.abs().max(r.abs()))
        .fold(0.0_f32, f32::max);
    assert!(loudest < 1e-6, "the offline render runs both paths and the mixer, peak {loudest}");
}

#[test]
fn the_tone_doctor_hears_the_split() {
    let (chain, _) = mono_chain(
        "doctor",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[invert()],
            vec![volume_block("a_vol", 100.0)],
            vec![volume_block("b_vol", 100.0)],
        )],
    );
    let input: Vec<[f32; 2]> = sine_block(48_000, 0).into_iter().map(|s| [s, s]).collect();
    let diagnosis = crate::tone_doctor::diagnose(&chain, SR, &input, 256).expect("diagnosis");
    assert!(
        diagnosis.full_descriptors.rms_dbfs < -90.0,
        "the tone doctor renders the split (here: cancelled), rms {} dBFS",
        diagnosis.full_descriptors.rms_dbfs
    );
}
```

Register it at the end of `runtime.rs`:

```rust

#[cfg(test)]
#[path = "issue_328_split_mix_tests.rs"]
mod issue_328_split_mix;
```

Append to `crates/engine/src/audio_alloc_invariant_tests.rs`:

```rust
/// #328: a Split → Mix runs path B in a buffer preallocated at build and
/// lines its paths up through a preallocated ring (spec §4.1 / §4.3); none of
/// it may allocate once warm. Not `#[ignore]`d — new ignores are forbidden,
/// and this one needs none: the counter only counts the measuring thread, and
/// every other measurer in this binary is ignored, so nothing resets it
/// concurrently.
#[test]
fn audio_callback_does_not_allocate_with_split_mix() {
    use super::issue_328_split_mix::{split_block, unit_impulse_ir_block, volume_block};
    use crate::runtime_state::RuntimeProcessor;
    use project::block::SplitEnd;

    let mut chain = chain();
    chain.blocks = vec![
        volume_block("pre", 100.0),
        split_block(
            "split",
            SplitEnd::Mix,
            &[],
            vec![unit_impulse_ir_block("a_ir"), volume_block("a_vol", 100.0)],
            vec![volume_block("b_vol", 100.0)],
        ),
        volume_block("post", 100.0),
    ];
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0_f32,
            &[DEFAULT_ELASTIC_TARGET],
            &registry_mono_in_stereo_out(),
        )
        .expect("split runtime should build"),
    );
    {
        let guard = runtime.processing.lock().expect("processing lock");
        assert!(
            guard.input_states[0]
                .blocks
                .iter()
                .any(|node| matches!(node.processor, RuntimeProcessor::Split(_))),
            "the chain must run a live split node"
        );
    }
    let input_buf = vec![0.3_f32; 64];
    let mut output_buf = vec![0.0_f32; 64 * 2];
    for _ in 0..256 {
        process_input_f32(&runtime, 0, &input_buf, 1);
        process_output_f32(&runtime, 0, &mut output_buf, 2);
    }
    let allocs = measure_allocs(|| {
        for _ in 0..1_000 {
            process_input_f32(&runtime, 0, &input_buf, 1);
            process_output_f32(&runtime, 0, &mut output_buf, 2);
        }
    });
    eprintln!("[#328 alloc] split mix @64: {allocs} allocations / 1000 callbacks");
    assert_eq!(
        allocs, 0,
        "CLAUDE.md invariant #8 broken by the split: {allocs} heap allocations in \
         1000 steady-state callbacks — path B's buffer and the alignment rings \
         must be preallocated at build."
    );
}
```

- [ ] **Step 2: Run — expected FAIL**

Run: `nice -n 19 cargo test -p engine -j 2 -- issue_328_split_mix audio_callback_does_not_allocate_with_split_mix`
Expected: FAIL — the tests that call `with_split` and the allocation test with `the chain must run a live split node` (Part 1's placeholder builds no `RuntimeProcessor::Split`); `a_block_after_the_split_is_built_for_stereo_content` with `a split may pan its paths apart; what follows is stereo`; `each_segment_builds_its_own_split` with `every segment runs its own split` (`left: 0`, `right: 2`); `disabling_the_split_fades_it_out_through_its_paths` on its fade/processor assertion; `the_offline_render_plays_the_split` / `the_tone_doctor_hears_the_split` with peak ≈ 0.5 / rms ≈ −9 dBFS.

- [ ] **Step 3: Implement the builder** — create `crates/engine/src/runtime_split_builder.rs`:

```rust
//! Responsibility: builds the runtime node a split block turns into.

use std::collections::HashMap;

use anyhow::Result;
use block_core::AudioChannelLayout;
use domain::ids::BlockId;
use project::block::split_params::{SPLIT_MODE, SPLIT_MODE_DUAL_MONO};
use project::block::{AudioBlock, SplitBlock, SplitEnd};
use project::chain::Chain;

use crate::runtime::FADE_IN_FRAMES;
use crate::runtime_audio_frame::ProcessorScratch;
use crate::runtime_block_builders::{build_nodes_for, bypass_runtime_node, next_block_instance_serial};
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_split::state::SplitRuntimeState;
use crate::runtime_state::{BlockRuntimeNode, FadeState, RuntimeProcessor};

/// Build (or rebuild) the node of `block`, a split (#328). Both paths are
/// built for a stereo bus — the split always hands them stereo frames — and
/// for mono content when the bus reaching the split is mono or Mode II feeds
/// each path one channel, so the #588 mono collapse still applies inside a
/// path. `reusable_nodes` may hold this split's previous node.
pub(crate) fn build_split_runtime_node(
    chain: &Chain,
    block: &AudioBlock,
    split: &SplitBlock,
    input_layout: AudioChannelLayout,
    content_mono: bool,
    sample_rate: f32,
    reusable_nodes: &mut HashMap<BlockId, BlockRuntimeNode>,
) -> Result<BlockRuntimeNode> {
    let previous = reusable_nodes.remove(&block.id).filter(|node| {
        node.input_layout == input_layout && matches!(node.processor, RuntimeProcessor::Split(_))
    });
    if previous.is_none() && !block.enabled {
        return Ok(bypass_runtime_node(block, input_layout, content_mono));
    }
    let path_content_mono =
        content_mono || split.params.get_string(SPLIT_MODE) == Some(SPLIT_MODE_DUAL_MONO);
    let a_blocks: Vec<&AudioBlock> = split.a.iter().collect();
    let b_blocks: Vec<&AudioBlock> = split.b.iter().collect();
    let mut path_pool: HashMap<BlockId, BlockRuntimeNode> = HashMap::new();
    let (a, _, _) = build_nodes_for(
        chain,
        &a_blocks,
        AudioChannelLayout::Stereo,
        path_content_mono,
        sample_rate,
        &mut path_pool,
    )?;
    let (b, _, _) = build_nodes_for(
        chain,
        &b_blocks,
        AudioChannelLayout::Stereo,
        path_content_mono,
        sample_rate,
        &mut path_pool,
    )?;
    let state = SplitRuntimeState::new(
        matches!(split.end, SplitEnd::Mix),
        a,
        b,
        SplitKnobs::from_params(&split.params),
        &block.id,
    );
    let (instance_serial, fade_state) = match previous {
        Some(node) => (
            node.instance_serial,
            split_fade(node.block_snapshot.enabled, block.enabled, node.fade_state),
        ),
        None => (
            next_block_instance_serial(),
            FadeState::FadingIn {
                frames_remaining: FADE_IN_FRAMES,
            },
        ),
    };
    Ok(BlockRuntimeNode {
        instance_serial,
        block_id: block.id.clone(),
        block_snapshot: block.clone(),
        input_layout,
        content_mono,
        output_layout: AudioChannelLayout::Stereo,
        scratch: ProcessorScratch::None,
        processor: RuntimeProcessor::Split(state),
        stream_handle: None,
        fade_state,
        fade_dry_buffer: Vec::new(),
        faulted: false,
        fault_reason: None,
    })
}

/// A split that stays on or off keeps its fade; one switched on fades in and
/// one switched off fades out through its real paths.
fn split_fade(was_enabled: bool, enabled: bool, previous: FadeState) -> FadeState {
    match (was_enabled, enabled) {
        (false, true) => FadeState::FadingIn {
            frames_remaining: FADE_IN_FRAMES,
        },
        (true, false) => FadeState::FadingOut {
            frames_remaining: FADE_IN_FRAMES,
        },
        _ => previous,
    }
}
```

The builder has no test module of its own: every branch of it is driven end to end by `issue_328_split_mix_tests.rs` (build, fade on disable, reuse in Task 15, history in Task 16).

- [ ] **Step 4: Dispatch splits to the builder** — `runtime_block_builders.rs`, at the top of the loop body in `build_nodes_for` (right after `for &block in block_iter {`, before the `// Disabled blocks:` comment):

```rust
        // #328: a split builds its own node, on or off — a split switched off
        // must fade out through its real paths, not through an empty shell.
        if let AudioBlockKind::Split(split) = &block.kind {
            let node = crate::runtime_split::builder::build_split_runtime_node(
                chain,
                block,
                split,
                current_layout,
                content_mono,
                sample_rate,
                reusable_nodes,
            )?;
            current_layout = node.output_layout;
            content_mono = node_emits_mono_content(&node, content_mono);
            blocks.push(node);
            continue;
        }
```

In `build_block_runtime_node`'s `match &block.kind`, replace Part 1's Split placeholder arm (Task 0 Step 3) with:

```rust
        AudioBlockKind::Split(split) => crate::runtime_split::builder::build_split_runtime_node(
            chain,
            block,
            split,
            input_layout,
            content_mono,
            sample_rate,
            &mut HashMap::new(),
        )?,
```

Remove every other Split placeholder Part 1 added in `crates/engine/src` (Task 0 Step 3 list).

`runtime_split.rs`: add

```rust
#[path = "runtime_split_builder.rs"]
pub(crate) mod builder;
```

and delete the `// Task 14 …` comments with their `#[allow(dead_code)]` above `knobs` and `latency`.

- [ ] **Step 5: Run — expected PASS**

Run: `nice -n 19 cargo test -p engine -j 2 && nice -n 19 cargo build -p engine -j 2 2>&1 | grep -c "^warning"`
Expected: every engine test PASS (including all `volume*` suites, untouched); warning count `0`.

- [ ] **Step 6: Docs** — `docs/blocks-catalog.md`: Part 1 (Task 1) added the `## Chain split (#328)` section right before `## Backends de áudio`. Append this `###` subsection at the end of that section, directly above the `## Backends de áudio` line (find it with `grep -n "^## Chain split (#328)\|^## Backends de áudio" docs/blocks-catalog.md`):

```markdown
### Split engine behaviour (#328)

A Split → Mix runs inside the chain's own segment: shared blocks → split →
path A and path B → mixer → shared blocks. Nothing is summed across segments
or runtimes (stream isolation); path B runs in the split's own buffer,
preallocated at build for a 1024-frame callback.

**Into the paths.** Mode I (`split_mode: same`): each path gets the bus ×
`level_to_a` / `level_to_b` (`x/100`). Mode II (`dual_mono`): each path gets
the one channel its balance picks — −50 = L, 0 = (L+R)/2, +50 = R, linear in
between — as dual mono, × its level. Mode II only means something when the bus
is still stereo at the split (a stereo or dual-mono source, before any mono
block); with a mono source both paths get the same signal.

**Mixer.** Per path: balance law (centre = unity on both sides; toward one side
the other side falls linearly to 0 at ±50) × `mix_level` (`x/100`); path B ×
−1 with `mix_b_polarity: invert`; the sum × `mix_master` (`x/100`); with
`mix_master_sum` on, both sides become `(L+R)/2`. At the defaults two identical
paths come out at unity (`mix_master` 50 halves the doubled sum). The split's
output is always stereo; a 1-channel output averages L/R, so pan does nothing
there. A Y → A/B split meets its paths at unity: its mixer knobs do not apply.

**Alignment.** Every block reports the processing latency it adds. At build the
split sums it per path and delays the shorter path by the difference, in a ring
preallocated up to 16384 samples; above that it clamps and logs. The longer
path is never delayed, so the chain's latency does not change.

| Source | Reported latency |
|---|---|
| IR convolution (cab, body, `generic_ir`) | 64 samples (one partition) |
| 2× oversampler round trip (ring modulator) | 15 samples |
| Brick wall limiter | its look-ahead: `lookahead_ms` in samples (144 at the 3 ms default, 48 kHz) |
| VST3 | `IAudioProcessor::getLatencySamples()`, read at load |
| LV2 | the `lv2:reportsLatency` output port, read once at build |
| everything else | 0 |

Not aligned by design: delays and the pitch shifter (their delay is the
effect), the IR reverb's dry/wet blend and the ring modulator below 100 % mix
(their dry part is not delayed inside the block).
```

(If Task 2 was skipped, the oversampler row reads `ring modulator | 0 — not reported (owner decision, #328 Task 2)`.)

`docs/architecture.md`, insert before `## Registry auto-gerado` (line 421):

```markdown
## Chain split runs inside one segment (#328)

`RuntimeProcessor::Split` is a node of the segment like any block. It holds
both paths of a Split → Mix, path B's buffer (preallocated to
`SEGMENT_FRAME_CAPACITY`), one alignment delay line per path and the knobs as
atomics loaded once per callback. The mix is DSP inside that node — never a sum
of two segments or two runtimes — so each segment of a chain runs its own split
and stream isolation holds by construction.

Files: `runtime_split.rs` routes `runtime_split_builder.rs` (model → node),
`runtime_split_state.rs` (what a split keeps), `runtime_split_process.rs` (one
callback), `runtime_split_mix.rs` (pure per-sample math),
`runtime_split_align.rs` (delay line), `runtime_split_knobs.rs` (atomics) and
`runtime_split_latency.rs` (path latency, from each processor's
`latency_samples()`).
```

- [ ] **Step 7: Commit and push**

```bash
git -C "$W" add crates/engine/src/runtime_split.rs crates/engine/src/runtime_split_builder.rs crates/engine/src/runtime_block_builders.rs crates/engine/src/runtime.rs crates/engine/src/issue_328_split_mix_tests.rs crates/engine/src/audio_alloc_invariant_tests.rs docs/blocks-catalog.md docs/architecture.md
git -C "$W" commit -m "feat(#328): the engine plays a Split -> Mix"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 15: Rebuilds keep the path processors (knob moves, lane drags)

**Files:**
- Modify: `crates/engine/src/runtime_block_reuse.rs` (`reuse_pool` hands split path nodes to the pool)
- Modify: `crates/engine/src/runtime_split_builder.rs` (paths draw from the caller's pool)
- Test: `crates/engine/src/issue_328_split_mix_tests.rs` (append)

**Interfaces:**
- Consumes: Task 13 `reuse_pool`, Task 14 builder.
- Produces: path nodes reused by id across `update_chain_runtime_state`, including moves between A, B and the shared blocks. Test helpers `serial_of(runtime, id) -> u64`, `with_split_knob(chain, key, value) -> Chain`.

- [ ] **Step 1: Write the failing tests** — append to `issue_328_split_mix_tests.rs`:

```rust
pub(super) fn serial_of(runtime: &ChainRuntimeState, id: &str) -> u64 {
    let guard = runtime.processing.lock().expect("processing lock");
    for node in &guard.input_states[0].blocks {
        if node.block_id.0 == id {
            return node.instance_serial;
        }
        if let RuntimeProcessor::Split(split) = &node.processor {
            for path_node in split.a.iter().chain(split.b.iter()) {
                if path_node.block_id.0 == id {
                    return path_node.instance_serial;
                }
            }
        }
    }
    panic!("block {id} is not in the runtime")
}

pub(super) fn with_split_knob(chain: &Chain, key: &str, value: ParameterValue) -> Chain {
    let mut edited = chain.clone();
    for block in edited.blocks.iter_mut() {
        if let AudioBlockKind::Split(split) = &mut block.kind {
            split.params.insert(key, value.clone());
        }
    }
    edited
}

fn update(runtime: &Arc<ChainRuntimeState>, chain: &Chain, registry: &[IoBinding]) {
    update_chain_runtime_state(runtime, chain, SR, false, &[DEFAULT_ELASTIC_TARGET], registry)
        .expect("in-place update");
}

#[test]
fn a_mixer_knob_edit_keeps_the_path_processors() {
    let (chain, registry) = mono_chain(
        "knob_edit",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[],
            vec![volume_block("amp_a", 100.0)],
            vec![volume_block("amp_b", 100.0)],
        )],
    );
    let runtime = build_runtime(&chain, &registry);
    let before = (serial_of(&runtime, "amp_a"), serial_of(&runtime, "amp_b"));
    update(
        &runtime,
        &with_split_knob(&chain, split_params::MIX_PAN_A, ParameterValue::Float(-50.0)),
        &registry,
    );
    assert_eq!(
        (serial_of(&runtime, "amp_a"), serial_of(&runtime, "amp_b")),
        before,
        "a knob move must not rebuild the amps in the paths"
    );
    with_split(&runtime, |split| {
        assert_eq!(split.knobs.load(true).mix_pan_a, -50.0, "the new knob value applies")
    });
}

#[test]
fn moving_a_block_from_path_a_to_path_b_keeps_its_processor() {
    let (chain, registry) = mono_chain(
        "lane_drag",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[],
            vec![volume_block("a1", 100.0), volume_block("a2", 80.0)],
            vec![volume_block("b1", 100.0)],
        )],
    );
    let runtime = build_runtime(&chain, &registry);
    let before = serial_of(&runtime, "a2");
    let mut moved = chain.clone();
    if let AudioBlockKind::Split(split) = &mut moved.blocks[0].kind {
        let dragged = split.a.remove(1);
        split.b.push(dragged);
    }
    update(&runtime, &moved, &registry);
    assert_eq!(serial_of(&runtime, "a2"), before, "dragging across lanes keeps the processor");
}

#[test]
fn moving_a_block_out_of_a_path_keeps_its_processor() {
    let (chain, registry) = mono_chain(
        "to_shared",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[],
            vec![volume_block("a1", 100.0)],
            vec![volume_block("b1", 100.0)],
        )],
    );
    let runtime = build_runtime(&chain, &registry);
    let before = serial_of(&runtime, "a1");
    let mut moved = chain.clone();
    let dragged = match &mut moved.blocks[0].kind {
        AudioBlockKind::Split(split) => split.a.remove(0),
        _ => unreachable!("block 0 is the split"),
    };
    moved.blocks.insert(0, dragged);
    update(&runtime, &moved, &registry);
    assert_eq!(serial_of(&runtime, "a1"), before, "dragging to the shared blocks keeps the processor");
}
```

- [ ] **Step 2: Run — expected FAIL**

Run: `nice -n 19 cargo test -p engine -j 2 issue_328_split_mix`
Expected: FAIL — `a knob move must not rebuild the amps in the paths` (serials differ), `dragging across lanes keeps the processor`, `dragging to the shared blocks keeps the processor`.

- [ ] **Step 3: Implement**

`runtime_block_reuse.rs`, replace `reuse_pool`:

```rust
/// The old nodes a rebuild may keep, keyed by block id. A split's path nodes
/// join the pool beside it (#328), so a path block is found wherever it moved
/// to — the other lane, or the shared blocks before or after the split.
pub(crate) fn reuse_pool(
    existing: Option<Vec<BlockRuntimeNode>>,
) -> HashMap<BlockId, BlockRuntimeNode> {
    let mut pool = HashMap::new();
    for mut node in existing.unwrap_or_default() {
        if let RuntimeProcessor::Split(split) = &mut node.processor {
            for path_node in split.a.drain(..).chain(split.b.drain(..)) {
                pool.insert(path_node.block_id.clone(), path_node);
            }
        }
        pool.insert(node.block_id.clone(), node);
    }
    pool
}
```

`runtime_split_builder.rs`: delete `let mut path_pool: HashMap<BlockId, BlockRuntimeNode> = HashMap::new();`, pass `reusable_nodes` instead of `&mut path_pool` to both `build_nodes_for` calls, and put this comment above the first call:

```rust
    // #328: paths draw from the caller's pool, so a knob move keeps every
    // amp and a block dragged between lanes keeps its processor.
```

- [ ] **Step 4: Run — expected PASS**

Run: `nice -n 19 cargo test -p engine -j 2 && nice -n 19 cargo build -p engine -j 2 2>&1 | grep -c "^warning"`
Expected: PASS; `0`.

- [ ] **Step 5: Commit and push**

```bash
git -C "$W" add crates/engine/src/runtime_block_reuse.rs crates/engine/src/runtime_split_builder.rs crates/engine/src/issue_328_split_mix_tests.rs
git -C "$W" commit -m "feat(#328): rebuilds keep split path processors"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 16: Rebuilds keep the alignment history

**Files:**
- Modify: `crates/engine/src/runtime_split_builder.rs` (adopt the previous state's history)
- Modify: `crates/engine/src/runtime_split.rs` (remove the `align`/`state` allows)
- Test: `crates/engine/src/issue_328_split_mix_tests.rs` (append)
- Docs: `docs/blocks-catalog.md` (the Split engine subsection)

**Interfaces:**
- Consumes: `SplitRuntimeState::adopt_history` (Task 10), `AlignDelay::adopt_history` (Task 7).
- Produces: a rebuilt split continues its delay lines where the previous build left them.

- [ ] **Step 1: Write the failing test** — append to `issue_328_split_mix_tests.rs`:

```rust
#[test]
fn a_knob_edit_keeps_the_alignment_history() {
    let (chain, registry) = mono_chain(
        "history",
        vec![split_block("split", SplitEnd::Mix, &[invert()], vec![unit_impulse_ir_block("a_ir")], vec![])],
    );
    let runtime = build_runtime(&chain, &registry);
    let mut callback = 0;
    let mut before = 0.0_f32;
    for _ in 0..12 {
        let out = drive_and_capture(&runtime, 1, &sine_block(256, callback * 256), 2);
        if callback >= 8 {
            before = before.max(peak_abs(&out));
        }
        callback += 1;
    }
    assert!(before < 1e-4, "precondition: the aligned paths cancel, peak {before}");
    update(
        &runtime,
        &with_split_knob(&chain, split_params::MIX_MASTER, ParameterValue::Float(40.0)),
        &registry,
    );
    let mut after = 0.0_f32;
    for _ in 0..8 {
        let out = drive_and_capture(&runtime, 1, &sine_block(256, callback * 256), 2);
        after = after.max(peak_abs(&out));
        callback += 1;
    }
    assert!(after < 1e-4, "a knob edit must not restart the delayed path from silence, peak {after}");
}
```

- [ ] **Step 2: Run — expected FAIL**

Run: `nice -n 19 cargo test -p engine -j 2 a_knob_edit_keeps_the_alignment_history`
Expected: FAIL — `a knob edit must not restart the delayed path from silence, peak 0.2…` (the IR in path A keeps its history since Task 15, the fresh delay ring in path B starts from zeros).

- [ ] **Step 3: Implement** — in `runtime_split_builder.rs`:

`let previous = …` becomes `let mut previous = …`; right after the `if previous.is_none() && !block.enabled { … }` block add:

```rust
    // #328: take the previous build's split state out of its node; its
    // delay lines continue in the new state below.
    let previous_state = previous.as_mut().and_then(|node| {
        match std::mem::replace(&mut node.processor, RuntimeProcessor::Bypass) {
            RuntimeProcessor::Split(state) => Some(state),
            _ => None,
        }
    });
```

and `let state = SplitRuntimeState::new(…);` becomes:

```rust
    let mut state = SplitRuntimeState::new(
        matches!(split.end, SplitEnd::Mix),
        a,
        b,
        SplitKnobs::from_params(&split.params),
        &block.id,
    );
    if let Some(previous_state) = previous_state {
        state.adopt_history(previous_state);
    }
```

`runtime_split.rs`: delete the `// Task 16 …` comments with their `#[allow(dead_code)]` above `align` and `state`.

- [ ] **Step 4: Run — expected PASS**

Run: `nice -n 19 cargo test -p engine -j 2 && nice -n 19 cargo build -p engine -j 2 2>&1 | grep -c "^warning"`
Expected: PASS; `0`.

- [ ] **Step 5: Docs** — in the `### Split engine behaviour (#328)` subsection of `docs/blocks-catalog.md`, append to the **Alignment** paragraph:

```markdown
A knob move or a path edit reuses every path processor by block id (also when
a block is dragged between lanes or to the shared blocks) and continues the
delay lines where they were, so it is not heard as a gap.
```

- [ ] **Step 6: Commit and push**

```bash
git -C "$W" add crates/engine/src/runtime_split_builder.rs crates/engine/src/runtime_split.rs crates/engine/src/issue_328_split_mix_tests.rs docs/blocks-catalog.md
git -C "$W" commit -m "feat(#328): rebuilds keep the split's alignment history"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 17: Walkers see inside split paths (bypass mirror, offline faulted list, probe summary)

**Files:**
- Create: `crates/engine/src/runtime_split_walk.rs`
- Modify: `crates/engine/src/runtime_split.rs` (declare `walk`)
- Modify: `crates/engine/src/runtime_graph_assemble.rs:235-245` (`collect_bypass_block_ids`)
- Modify: `crates/engine/src/offline.rs:224-241` (`collect_faulted_blocks`)
- Modify: `crates/engine/src/probe.rs:93-138` (extract `runtime_summary`, then walk)
- Test: `crates/engine/src/issue_328_split_mix_tests.rs` (append), `crates/engine/src/runtime_split_walk_tests.rs` (create)
- Docs: `docs/architecture.md` (file list of the split section)

**Interfaces:**
- Consumes: `RuntimeProcessor::Split`.
- Produces: `pub(crate) fn for_each_node<'a, F>(nodes: &'a [BlockRuntimeNode], visit: &mut F) where F: FnMut(&'a BlockRuntimeNode)`; `pub(crate) fn runtime_summary(input_states: &[InputProcessingState]) -> String` in `crate::probe`.

Stream handles are not walked: no block type produces a `StreamHandle` today (`block-util` has no model), so a path cannot hold one.

- [ ] **Step 1: Extract the probe summary (behaviour-preserving)** — in `probe.rs` replace lines 93–138 with:

```rust
    let summary: String = {
        let guard = match runtime.processing.try_lock() {
            Ok(g) => g,
            Err(_) => {
                // Should not happen on a freshly built runtime — the lock
                // is uncontended because we just created the Arc and no
                // one else holds it. Be defensive anyway.
                eprintln!("[probe] lock contention on fresh runtime — skipping summary");
                return 0.0;
            }
        };
        runtime_summary(&guard.input_states)
    };
```

rename `runtime_summary` to `summary` in the final `eprintln!`, and add after `measure_chain_dsp_latency_ms`:

```rust

/// Per segment: how many runtime nodes run real DSP vs. sit as a silent
/// `Bypass`, naming each bypassed one (`!` = faulted). A probe that reads
/// fast because a model failed to load shows it here.
pub(crate) fn runtime_summary(
    input_states: &[crate::runtime_state::InputProcessingState],
) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(input_states.len());
    for (i, seg) in input_states.iter().enumerate() {
        let total = seg.blocks.len();
        let mut audio = 0;
        let mut bypass = 0;
        let mut select = 0;
        let mut faulted_or_bypass: Vec<String> = Vec::new();
        for node in &seg.blocks {
            match node.processor.kind_label() {
                "audio" => audio += 1,
                "select" => select += 1,
                "bypass" => {
                    bypass += 1;
                    let model = node
                        .block_snapshot
                        .model_ref()
                        .map(|r| format!("{}:{}", r.effect_type, r.model))
                        .unwrap_or_else(|| node.block_snapshot.kind.label().to_string());
                    let suffix = if node.faulted { "!" } else { "" };
                    faulted_or_bypass.push(format!("{}({}){}", node.block_id.0, model, suffix));
                }
                _ => {}
            }
        }
        parts.push(format!(
            "seg{i}={total}/A{audio}/B{bypass}/S{select}{}",
            if faulted_or_bypass.is_empty() {
                String::new()
            } else {
                format!(" bypassed={faulted_or_bypass:?}")
            }
        ));
    }
    parts.join(" | ")
}
```

Run: `nice -n 19 cargo test -p engine -j 2 probe` — Expected: PASS (no behaviour change).

- [ ] **Step 2: Write the failing tests**

Create `crates/engine/src/runtime_split_walk_tests.rs`:

```rust
//! #328: the walk reaches the nodes inside both paths of a split.

use domain::ids::BlockId;
use project::block::split_params::default_split_params;

use super::for_each_node;
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_split::state::SplitRuntimeState;
use crate::runtime_split::test_support::gain_node;
use crate::runtime_state::RuntimeProcessor;

#[test]
fn the_walk_visits_both_paths_of_a_split() {
    let mut split = gain_node("split", 1.0);
    split.processor = RuntimeProcessor::Split(SplitRuntimeState::new(
        true,
        vec![gain_node("amp_a", 1.0)],
        vec![gain_node("amp_b", 1.0)],
        SplitKnobs::from_params(&default_split_params()),
        &BlockId("split".into()),
    ));
    let nodes = vec![gain_node("pre", 1.0), split, gain_node("post", 1.0)];
    let mut seen = Vec::new();
    for_each_node(&nodes, &mut |node| seen.push(node.block_id.0.clone()));
    assert_eq!(seen, vec!["pre", "split", "amp_a", "amp_b", "post"]);
}
```

Append to `issue_328_split_mix_tests.rs`:

```rust
fn broken_block(id: &str) -> AudioBlock {
    core_block(id, "gain", "does_not_exist", ParameterSet::default())
}

fn chain_with_a_broken_path_block() -> (Chain, Vec<IoBinding>) {
    mono_chain(
        "broken",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[],
            vec![volume_block("a_vol", 100.0)],
            vec![broken_block("b_broken")],
        )],
    )
}

#[test]
fn a_born_disabled_block_inside_a_path_declines_the_fast_toggle() {
    let mut off = volume_block("a_off", 100.0);
    off.enabled = false;
    let (chain, registry) = mono_chain(
        "born_off",
        vec![split_block("split", SplitEnd::Mix, &[], vec![off], vec![volume_block("b_vol", 100.0)])],
    );
    let runtime = build_runtime(&chain, &registry);
    assert!(
        runtime.bypass_block_ids.load().contains(&BlockId("a_off".into())),
        "re-enabling a path block with no processor must take the rebuild path"
    );
}

#[test]
fn a_path_block_that_fails_to_build_is_reported_offline() {
    let (chain, _) = chain_with_a_broken_path_block();
    let input = vec![[0.1_f32, 0.1]; 256];
    let outcome = crate::offline::render_chain(&chain, SR, &input, 64, 0).expect("offline render");
    assert!(
        outcome.faulted_blocks.iter().any(|f| f.block_id == "b_broken"),
        "a render that silently bypassed a path block must say so: {:?}",
        outcome.faulted_blocks
    );
}

#[test]
fn the_latency_probe_summary_names_a_faulted_path_block() {
    let (chain, registry) = chain_with_a_broken_path_block();
    let runtime = build_runtime(&chain, &registry);
    let guard = runtime.processing.lock().expect("processing lock");
    let summary = crate::probe::runtime_summary(&guard.input_states);
    assert!(
        summary.contains("b_broken(gain:does_not_exist)!"),
        "the probe must see inside the paths: {summary}"
    );
}
```

- [ ] **Step 3: Expose the walk with an inert body (top level only) and route the three walkers through it** — behaviour-preserving: a top-level-only walk visits exactly the nodes the old loops visited, so everything compiles and the reds of Step 4 are behavioural.

Create `crates/engine/src/runtime_split_walk.rs`:

```rust
//! Responsibility: visits every runtime node including those inside split paths.

use crate::runtime_state::BlockRuntimeNode;

/// Call `visit` on every node of `nodes`.
pub(crate) fn for_each_node<'a, F>(nodes: &'a [BlockRuntimeNode], visit: &mut F)
where
    F: FnMut(&'a BlockRuntimeNode),
{
    for node in nodes {
        visit(node);
    }
}

#[cfg(test)]
#[path = "runtime_split_walk_tests.rs"]
mod tests;
```

`runtime_split.rs`, add:

```rust
#[path = "runtime_split_walk.rs"]
pub(crate) mod walk;
```

`runtime_graph_assemble.rs`, `collect_bypass_block_ids` body:

```rust
    let mut ids = HashSet::new();
    for input_state in input_states {
        crate::runtime_split::walk::for_each_node(&input_state.blocks, &mut |node| {
            if matches!(node.processor, RuntimeProcessor::Bypass) {
                ids.insert(node.block_id.clone());
            }
        });
    }
    ids
```

`offline.rs`, `collect_faulted_blocks`:

```rust
fn collect_faulted_blocks(nodes: &[BlockRuntimeNode]) -> Vec<FaultedBlock> {
    let mut faulted = Vec::new();
    crate::runtime_split::walk::for_each_node(nodes, &mut |node| {
        let Some(reason) = node.fault_reason.as_ref() else {
            return;
        };
        let (effect_type, model) = match node.block_snapshot.model_ref() {
            Some(m) => (m.effect_type.to_string(), m.model.to_string()),
            None => (node.block_snapshot.kind.label().to_string(), String::new()),
        };
        faulted.push(FaultedBlock {
            block_id: node.block_id.0.clone(),
            effect_type,
            model,
            error: reason.clone(),
        });
    });
    faulted
}
```

`probe.rs`, `runtime_summary` (extracted in Step 1): replace `let total = seg.blocks.len();` with `let mut total = 0;`, the `for node in &seg.blocks {` loop header with `crate::runtime_split::walk::for_each_node(&seg.blocks, &mut |node| {`, its closing `}` with `});`, and add `total += 1;` as the first line inside the closure. The format string stays `"seg{i}={total}/A{audio}/B{bypass}/S{select}{}"` (a split node counts in `total` only, like any label the `match` does not list).

- [ ] **Step 4: Run — expected behavioural FAIL**

Run: `nice -n 19 cargo test -p engine -j 2 -- issue_328_split_mix runtime_split::walk probe`
Expected: the `probe` tests PASS (the routing is behaviour-preserving); FAIL — `the_walk_visits_both_paths_of_a_split` (`left: ["pre", "split", "post"]`, `right: ["pre", "split", "amp_a", "amp_b", "post"]`), `re-enabling a path block with no processor must take the rebuild path`, `a render that silently bypassed a path block must say so: []`, `the probe must see inside the paths: seg0=1/A0/B0/S0`.

- [ ] **Step 5: Implement** — replace `crates/engine/src/runtime_split_walk.rs` above the test attachment with:

```rust
//! Responsibility: visits every runtime node including those inside split paths.

use crate::runtime_state::{BlockRuntimeNode, RuntimeProcessor};

/// Call `visit` on every node of `nodes`, then on the nodes of both paths of
/// every split among them (#328). Select options are not visited — the
/// walkers that use this treated them that way before splits existed.
pub(crate) fn for_each_node<'a, F>(nodes: &'a [BlockRuntimeNode], visit: &mut F)
where
    F: FnMut(&'a BlockRuntimeNode),
{
    for node in nodes {
        visit(node);
        if let RuntimeProcessor::Split(split) = &node.processor {
            for_each_node(&split.a, visit);
            for_each_node(&split.b, visit);
        }
    }
}
```

- [ ] **Step 6: Run — expected PASS**

Run: `nice -n 19 cargo test -p engine -j 2 && nice -n 19 cargo build -p engine -j 2 2>&1 | grep -c "^warning"`
Expected: PASS; `0`.

- [ ] **Step 7: Docs** — `docs/architecture.md`, split section: after "`runtime_split_latency.rs` (path latency, from each processor's `latency_samples()`)" add "and `runtime_split_walk.rs`, which the read-only node walkers (bypass mirror, offline faulted list, probe summary) use to see inside both paths; the block toggle (Task 18) descends into the paths itself because it mutates them".

- [ ] **Step 8: Commit and push**

```bash
git -C "$W" add crates/engine/src/runtime_split.rs crates/engine/src/runtime_split_walk.rs crates/engine/src/runtime_split_walk_tests.rs crates/engine/src/runtime_graph_assemble.rs crates/engine/src/offline.rs crates/engine/src/probe.rs crates/engine/src/issue_328_split_mix_tests.rs docs/architecture.md
git -C "$W" commit -m "feat(#328): node walkers see inside split paths"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 18: A block toggled inside a path fades and re-aligns

**Files:**
- Modify: `crates/engine/src/runtime_block_toggle.rs:120-167`
- Test: `crates/engine/src/issue_328_split_mix_tests.rs` (append)
- Docs: `docs/blocks-catalog.md` (Split engine subsection)

**Interfaces:**
- Consumes: `SplitRuntimeState::refresh_alignment` (Task 10), `crate::runtime::set_block_enabled`.
- Produces: queued toggles reach path nodes; the owning split re-aligns on the same callback, on the audio thread, without allocation.

- [ ] **Step 1: Write the failing tests** — append to `issue_328_split_mix_tests.rs`:

```rust
#[test]
fn toggling_a_block_inside_a_path_fades_it_out() {
    let (chain, registry) = mono_chain(
        "toggle",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[],
            vec![volume_block("amp_a", 100.0)],
            vec![volume_block("amp_b", 100.0)],
        )],
    );
    let runtime = build_runtime(&chain, &registry);
    drive_and_capture(&runtime, 1, &sine_block(256, 0), 2);
    super::set_block_enabled(&runtime, &BlockId("amp_b".into()), false).expect("queued");
    drive_and_capture(&runtime, 1, &sine_block(256, 256), 2);
    drive_and_capture(&runtime, 1, &sine_block(256, 512), 2);
    with_split(&runtime, |split| {
        assert!(!split.b[0].block_snapshot.enabled, "amp B is off");
        assert_eq!(split.b[0].fade_state, FadeState::Bypassed, "after its fade-out");
    });
    let errors = runtime.poll_errors();
    assert!(errors.is_empty(), "the toggle must find the block inside the path: {errors:?}");
}

#[test]
fn switching_the_ir_in_path_a_off_realigns_the_paths() {
    let (chain, registry) = mono_chain(
        "toggle_align",
        vec![split_block("split", SplitEnd::Mix, &[invert()], vec![unit_impulse_ir_block("a_ir")], vec![])],
    );
    let runtime = build_runtime(&chain, &registry);
    let mut callback = 0;
    for _ in 0..8 {
        drive_and_capture(&runtime, 1, &sine_block(256, callback * 256), 2);
        callback += 1;
    }
    super::set_block_enabled(&runtime, &BlockId("a_ir".into()), false).expect("queued");
    let mut peak = 0.0_f32;
    for step in 0..12 {
        let out = drive_and_capture(&runtime, 1, &sine_block(256, callback * 256), 2);
        callback += 1;
        if step >= 4 {
            peak = peak.max(peak_abs(&out));
        }
    }
    with_split(&runtime, |split| {
        assert_eq!(split.align_b.delay(), 0, "with the cab off the dry path waits for nothing")
    });
    assert!(peak < 1e-4, "the realigned paths cancel again, peak {peak}");
}
```

- [ ] **Step 2: Run — expected FAIL**

Run: `nice -n 19 cargo test -p engine -j 2 -- issue_328_split_mix::toggl issue_328_split_mix::switching`
Expected: FAIL — `amp B is off` (the toggle never reached the path; `poll_errors` holds `block 'amp_b' not found in any input runtime of the chain`), `with the cab off the dry path waits for nothing` (`left: 64`, `right: 0`).

- [ ] **Step 3: Implement** — replace `apply_block_toggle` (lines 120–167) with:

```rust
/// In-place mutation that flips `fade_state` for every node matching
/// `block_id` across every per-input runtime of the chain — inside the paths
/// of a split too, which then lines its paths up again (#328). Never takes
/// the `processing` lock itself (the audio-thread caller already holds it
/// via `process_input_f32`'s try_lock guard); no allocation on success.
fn apply_block_toggle(
    processing: &mut ChainProcessingState,
    block_id: &BlockId,
    enabled: bool,
    runtime: &ChainRuntimeState,
) {
    let mut touched = 0usize;
    for input_state in processing.input_states.iter_mut() {
        for node in input_state.blocks.iter_mut() {
            touched += toggle_node(node, block_id, enabled, runtime);
            if let RuntimeProcessor::Split(split) = &mut node.processor {
                let mut in_paths = 0usize;
                for path_node in split.a.iter_mut().chain(split.b.iter_mut()) {
                    in_paths += toggle_node(path_node, block_id, enabled, runtime);
                }
                if in_paths > 0 {
                    split.refresh_alignment();
                }
                touched += in_paths;
            }
        }
    }
    if touched == 0 {
        let _ = runtime.error_queue.push(BlockError {
            block_id: block_id.clone(),
            message: format!(
                "block '{}' not found in any input runtime of the chain",
                block_id.0
            ),
        });
    }
}

/// Flip one node when it is `block_id`; returns 1 when it was toggled.
fn toggle_node(
    node: &mut BlockRuntimeNode,
    block_id: &BlockId,
    enabled: bool,
    runtime: &ChainRuntimeState,
) -> usize {
    if &node.block_snapshot.id != block_id {
        return 0;
    }
    if enabled && matches!(node.processor, RuntimeProcessor::Bypass) {
        let _ = runtime.error_queue.push(BlockError {
            block_id: block_id.clone(),
            message: format!(
                "block '{}' has no live processor — needs full rebuild to re-enable",
                block_id.0
            ),
        });
        return 0;
    }
    let was_enabled = node.block_snapshot.enabled;
    if was_enabled != enabled {
        node.fade_state = if enabled {
            FadeState::FadingIn {
                frames_remaining: FADE_IN_FRAMES,
            }
        } else {
            FadeState::FadingOut {
                frames_remaining: FADE_IN_FRAMES,
            }
        };
    }
    node.block_snapshot.enabled = enabled;
    1
}
```

and add `BlockRuntimeNode` to the `use crate::runtime_state::{…}` import at the top of the file.

- [ ] **Step 4: Run — expected PASS**

Run: `nice -n 19 cargo test -p engine -j 2 && nice -n 19 cargo build -p engine -j 2 2>&1 | grep -c "^warning"`
Expected: PASS (including `block_enabled_fast_path_tests`, `audio_under_block_toggle` suites unchanged); `0`.

- [ ] **Step 5: Docs** — in `docs/blocks-catalog.md`, Split engine subsection, **Alignment** paragraph, append: "A block switched on or off inside a path (a footswitch) fades like any block and the split lines its paths up again on the same callback."

- [ ] **Step 6: Commit and push**

```bash
git -C "$W" add crates/engine/src/runtime_block_toggle.rs crates/engine/src/issue_328_split_mix_tests.rs docs/blocks-catalog.md
git -C "$W" commit -m "feat(#328): block toggles reach split paths and re-align them"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 19: A convolver inside a path gives the route its cushion

**Files:**
- Modify: `crates/engine/src/route_convolution.rs:16-27`
- Test: `crates/engine/src/route_convolution_tests.rs` (append)

**Interfaces:**
- Consumes: Part 1 `SplitBlock`.
- Produces: `block_is_convolution` is true for an enabled split with an enabled convolver in either path (#592 route cushion).

- [ ] **Step 1: Write the failing test** — append to `route_convolution_tests.rs`:

```rust
// ── #328: a cab inside a split path convolves into the split's routes ──

#[test]
fn a_cab_inside_a_split_path_counts_as_convolution() {
    use project::block::split_params::default_split_params;
    use project::block::{SplitBlock, SplitEnd};

    let split = AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Mix,
            params: default_split_params(),
            a: vec![core("gain", "fuzz_ge")],
            b: vec![core(block_core::EFFECT_TYPE_CAB, "ir_marshall_4x12_v30")],
        }),
    };
    assert!(block_is_convolution(&split), "path B's cab convolves into the split's output");
}
```

- [ ] **Step 2: Run — expected FAIL**

Run: `nice -n 19 cargo test -p engine -j 2 a_cab_inside_a_split_path`
Expected: FAIL — `path B's cab convolves into the split's output`.

- [ ] **Step 3: Implement** — in `block_is_convolution`, add before `_ => false,`:

```rust
            // #328: a split convolves when either of its paths does.
            AudioBlockKind::Split(split) => {
                split.a.iter().chain(split.b.iter()).any(block_is_convolution)
            }
```

- [ ] **Step 4: Run — expected PASS**

Run: `nice -n 19 cargo test -p engine -j 2 route_convolution`
Expected: PASS.

- [ ] **Step 5: Commit and push**

```bash
git -C "$W" add crates/engine/src/route_convolution.rs crates/engine/src/route_convolution_tests.rs
git -C "$W" commit -m "feat(#328): a convolver inside a split path gives its route the IR cushion"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 19b: A VST3 inside a path takes the #779 in-place live rebuild

Part 1's hand-off names this walker: `block_contains_vst3` (`crates/infra-cpal/src/controller_offthread_live_rebuild.rs:245-253`) recurses into `Select` options only. On a live edit (`request_offthread_rebuild_if_live`, `:102`), a chain it answers `false` for takes the off-thread FRESH rebuild, which calls `createInstance` on the control worker while the audio thread is inside the old instance's `process()` — the JUCE SIGSEGV #779 fixed. A VST3 amp in path B of a dual-amp split must take the in-place `update_chain_runtime_state` path like any other VST3.

**Files:**
- Modify: `crates/infra-cpal/src/controller_offthread_live_rebuild.rs:237-253` (doc comment + one match arm; test attachment at the end of the file)
- Test: `crates/infra-cpal/src/controller_offthread_live_rebuild_tests.rs` (create)

**Interfaces:**
- Consumes: Part 1 `AudioBlockKind::Split(SplitBlock)`, `split_params::default_split_params`; the existing test fixture `crate::controller_live_edit_replicates_user_report_tests::gain_chain` (`pub(super)` at the crate root, `#[cfg(test)]`, `src/lib.rs:208-209`).
- Produces: `chain_contains_vst3` is true for a chain whose split holds a VST3 in either path.

- [ ] **Step 1: Write the failing test** — create `crates/infra-cpal/src/controller_offthread_live_rebuild_tests.rs`:

```rust
//! #328: a VST3 inside a split path must take the #779 in-place live rebuild.
//! A fresh off-thread build would call `createInstance` while the audio thread
//! is inside the old instance's `process()` — the JUCE SIGSEGV #779 fixed.

use domain::ids::BlockId;
use project::block::split_params::default_split_params;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, SplitBlock, SplitEnd};
use project::param::ParameterSet;

use super::chain_contains_vst3;
use crate::controller_live_edit_replicates_user_report_tests::gain_chain;

fn vst3_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: block_core::EFFECT_TYPE_VST3.into(),
            model: "vst3:Missing:Missing".into(),
            params: ParameterSet::default(),
        }),
    }
}

#[test]
fn a_vst3_inside_a_split_path_takes_the_in_place_rebuild() {
    let mut chain = gain_chain(100.0);
    chain.blocks.push(AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Mix,
            params: default_split_params(),
            a: vec![],
            b: vec![vst3_block("amp_b")],
        }),
    });
    assert!(
        chain_contains_vst3(&chain),
        "#779: a VST3 in path B must be updated in place, never re-instantiated off-thread"
    );
}
```

Attach it at the end of `controller_offthread_live_rebuild.rs` (after line 253), with the same platform gate as the function it tests:

```rust

#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]
#[path = "controller_offthread_live_rebuild_tests.rs"]
mod tests;
```

- [ ] **Step 2: Run — expected behavioural FAIL**

Run: `nice -n 19 cargo test -p infra-cpal --lib -j 2 a_vst3_inside_a_split_path`
Expected: FAIL — `#779: a VST3 in path B must be updated in place, never re-instantiated off-thread` (the `_ => false` arm answers for the split).

- [ ] **Step 3: Implement** — in `block_contains_vst3`, after the `Select` arm (line 250):

```rust
        // #328: a VST3 in either path of a split is a VST3 of the chain.
        AudioBlockKind::Split(split) => {
            split.a.iter().chain(split.b.iter()).any(block_contains_vst3)
        }
```

and change the doc comment of `chain_contains_vst3` (lines 237-239) to end with: "Recurses into `Select` options and both paths of a split (#328), so a VST3 nested inside one is covered too."

- [ ] **Step 4: Run — expected PASS**

Run: `nice -n 19 cargo test -p infra-cpal --lib -j 2 && nice -n 19 cargo build -p infra-cpal -j 2 2>&1 | grep -c "^warning"`
Expected: PASS; `0`.

- [ ] **Step 5: Commit and push**

```bash
git -C "$W" add crates/infra-cpal/src/controller_offthread_live_rebuild.rs crates/infra-cpal/src/controller_offthread_live_rebuild_tests.rs
git -C "$W" commit -m "feat(#328): a VST3 inside a split path takes the in-place live rebuild"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 20: Hardware battery — dual amp L/R, zero xruns

**Files:**
- Create: `crates/infra-cpal/tests/issue_328_dual_amp_split.rs`
- Docs: `docs/testing.md:189-211` (battery list and command)

**Interfaces:**
- Consumes: the whole engine split (Tasks 12–18); `hw_harness::{device_guard, hw_tests_enabled, init_registry, load_di_pcm, rig_project}`.
- Produces: spec §7 "Hardware battery: dual amp L/R on the real interface, zero xruns", with the single-amp vs dual-amp peak load printed (spec §6 CPU).

- [ ] **Step 1: Write the test** — create `crates/infra-cpal/tests/issue_328_dual_amp_split.rs`:

```rust
//! Issue #328 — the dual-amp rig on the REAL interface: a Split → Mix with a
//! native amp on each path, amp A hard left and amp B hard right, playing the
//! Green Day DI for 60 s through the real CoreAudio streams. The engine's own
//! xrun / underrun counters must stay at ZERO. The chain's peak load is
//! printed next to the single-amp chain's (spec §6: the split costs path A +
//! path B + one mix pass, measured here before and after).
//!
//! macOS + release only; gated by OPENRIG_HW_TESTS=1 (docs/testing.md).
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use std::time::Duration;

use domain::ids::{BlockId, ChainId};
use domain::io_binding::IoBinding;
use domain::value_objects::ParameterValue;
use hw_harness::{device_guard, hw_tests_enabled, init_registry, load_di_pcm, rig_project, BUFFER};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::split_params::{self, default_split_params};
use project::block::{
    schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, SplitBlock, SplitEnd,
};
use project::param::ParameterSet;
use project::project::Project;

fn native_amp(id: &str, model: &str) -> AudioBlock {
    let schema = schema_for_block_model("amp", model).expect("native amp schema");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("amp defaults normalize");
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "amp".into(),
            model: model.into(),
            params,
        }),
    }
}

fn dual_amp_split() -> AudioBlock {
    let mut params = default_split_params();
    params.insert(split_params::MIX_PAN_A, ParameterValue::Float(-50.0));
    params.insert(split_params::MIX_PAN_B, ParameterValue::Float(50.0));
    params.insert(split_params::MIX_MASTER, ParameterValue::Float(100.0));
    AudioBlock {
        id: BlockId("dual_amp".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Mix,
            params,
            a: vec![native_amp("amp_a", "tweed_breakup")],
            b: vec![native_amp("amp_b", "blackface_clean")],
        }),
    }
}

/// Play the Green Day DI through `project` for `seconds`; returns
/// (xruns, underruns, peak load) measured after a 2 s settle.
fn play(
    project: &Project,
    chain_id: &ChainId,
    registry: Vec<IoBinding>,
    seconds: u64,
) -> (u64, u64, f32) {
    let mut controller = ProjectRuntimeController::start(project).expect("start real streams");
    controller.set_io_bindings(registry);
    controller.sync_project(project).expect("resync with bindings");
    controller.set_chain_di_loop(chain_id, Some(load_di_pcm("phil-STRATO-green_day.wav")));
    std::thread::sleep(Duration::from_secs(2));
    let x0 = controller.chain_xrun_count(chain_id);
    let u0 = controller.chain_underrun_count(chain_id);
    std::thread::sleep(Duration::from_secs(seconds));
    (
        controller.chain_xrun_count(chain_id) - x0,
        controller.chain_underrun_count(chain_id) - u0,
        controller.chain_peak_load(chain_id),
    )
}

#[test]
fn dual_amp_split_left_right_no_xruns() {
    if !hw_tests_enabled("dual_amp_split_left_right_no_xruns") {
        return;
    }
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let (Some(input), Some(output)) = (inputs.first(), outputs.first()) else {
        panic!("no audio devices available — this test needs real devices");
    };
    let (mut project, chain_id, registry) = rig_project("clean.yaml", input, output);

    project.chains[0].blocks = vec![native_amp("amp_a", "tweed_breakup")];
    let (_, _, single_amp_load) = play(&project, &chain_id, registry.clone(), 20);

    project.chains[0].blocks = vec![dual_amp_split()];
    let (xruns, underruns, split_load) = play(&project, &chain_id, registry, 60);

    eprintln!(
        "[#328 REAL] buffer={BUFFER}: single amp peak load {single_amp_load:.3}, \
         dual-amp split peak load {split_load:.3}; 60 s split: xruns={xruns} underruns={underruns}"
    );
    assert_eq!(
        (xruns, underruns),
        (0, 0),
        "BUG #328: the dual-amp split recorded {xruns} xruns / {underruns} underruns in \
         60 s on the real interface at buffer {BUFFER}"
    );
}
```

- [ ] **Step 2: Run on the real interface (idle machine)**

Run: `OPENRIG_HW_TESTS=1 nice -n 19 cargo test -p infra-cpal --release -j 2 --test issue_328_dual_amp_split -- --nocapture`
Expected: PASS with the `[#328 REAL]` line; paste it (both loads) in the issue comment. This test is the battery's pin for the finished feature; it has no red of its own because Tasks 12–18 already made the split play — its role is the xrun/CPU measurement spec §6/§7 require. If it FAILS, stop: follow `docs/audio-incidents/` (search the symptom first, record the numbers) before any fix.

- [ ] **Step 3: Docs** — `docs/testing.md`, battery section: add `` `crates/infra-cpal/tests/issue_328_dual_amp_split.rs` `` to the file list (after `issue_698_owner_64_dual_chain.rs`, adjusting "and") with the sentence "The #328 test plays a Split → Mix dual-amp chain (amp A hard left, amp B hard right) and prints its peak load next to a single amp's." and extend the command:

```sh
OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
    --test issue_670_cab_swap --test issue_670_real_streams_no_xruns \
    --test issue_698_pitch_shifter_live --test issue_698_owner_64_dual_chain \
    --test issue_328_dual_amp_split
```

- [ ] **Step 4: Commit and push**

```bash
git -C "$W" add crates/infra-cpal/tests/issue_328_dual_amp_split.rs docs/testing.md
git -C "$W" commit -m "feat(#328): hardware battery plays a dual-amp split with zero xruns"W=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$W" && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates   # push gate (CLAUDE.md "Antes de TODO push"); push only if it exits 0
git -C "$W" fetch origin && git -C "$W" status -sb   # behind: git -C "$W" pull --rebase origin feature/issue-328, re-run the gate
git -C "$W" push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 pushed $(git -C "$W" rev-parse --short HEAD) — $(git -C "$W" log -1 --format=%s) — files: $(git -C "$W" show --name-only --format= HEAD | tr '\n' ' ') — push gate green"
```

---

### Task 21: Final gate for Part 3

**Files:** none new (fixes only if a gate fails, each with its own red test).

- [ ] **Step 1: No temporary allow is left**

Run: `grep -n "allow(dead_code)" crates/engine/src/runtime_split*.rs`
Expected: no output.

- [ ] **Step 2: The volume pin is untouched by this part**

Run: `git -C "$W" diff --stat P3_BASE..HEAD -- crates/engine/src/volume_invariants_tests.rs` (with `P3_BASE` = the hash posted in Task 0 Step 1)
Expected: no output — no commit of this part touches the pinned file.

- [ ] **Step 3: Format, build, test, static checks**

```bash
nice -n 19 cargo fmt --all -- --check
nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)" ; echo "exit=$?"
nice -n 19 cargo test --workspace -j 2
VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
```

Expected: fmt clean; no `warning`/`error` lines (grep exit 1); every test passes and the `ignored` counts equal Task 0's baseline (this part adds no ignore); `validate.sh` passes (every new file declares one responsibility, none over its cap).

- [ ] **Step 4: Patch coverage (recommended before the PR gate)**

Run: `./scripts/patch-coverage.sh P3_BASE --files` (with `P3_BASE` from Task 0 Step 1)
Expected: the new `runtime_split_*.rs` files are covered (each is unit-tested on its own); anything listed as uncovered gets a test before the push.

- [ ] **Step 5: Push and report**

```bash
git -C "$W" push
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 3 (engine Split -> Mix + alignment) done: <hash>. cargo fmt/build/test --workspace green, validate.sh crates green, hardware battery: <[#328 REAL] line>."
```
