# Pipeline-per-worker (#980) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every (input × output) pipeline of a chain runs in its own `ChainRuntimeState`, and therefore on its own dsp-worker, on macOS/Windows cpal AND on Linux/JACK, while a bound insert's footswitch keeps the same runtime slots (no new streams) and no route changes level or latency.

**Architecture:** The engine groups a chain's segments by `PipelineKey { route, entry_group }` into dense slots in segment order and stamps each runtime with the pipeline it runs; in-place updates find segments by that stamp and only run while the live stamps equal the chain's pipelines. infra-cpal is hardened for several runtimes per guitar (teardown, slot install, rebuild landing, off-thread bail-out, DI per route). The JACK-direct client gets one ring + one dsp-worker per runtime and plays every route, then the engine's JACK grouping is deleted. A chain with a bound insert whose pipelines read one input stream keeps a fixed slot set in both insert states; its in-place switch rebuilds every slot together from one shared node pool.

**Tech Stack:** Rust 2021 workspace (`engine`, `infra-cpal`, `application`, `adapter-gui`, `adapter-mcp`), cpal (CoreAudio/WASAPI), `jack` crate (Linux JACK-direct), `arc-swap`, `crossbeam-queue`, `anyhow`; Linux verification in the `docker/Dockerfile.linux-builder` image.

**Spec:** `docs/superpowers/specs/2026-09-27-issue-980-pipeline-per-worker-design.md` (written next to this plan in the scratchpad; Task 1 commits both into the repo).

## Global Constraints

- TDD red-first: no production change without a test that failed on an ASSERTION first; paste the FAILED line in the issue comment.
- One responsibility per production file: header `//! Responsibility: <one sentence>`; the sentence must not contain ` and `, ` e `, ` plus `, ` also `, `,`, `;`, `+`, `&` or `/` (`scripts/validate.sh` check 1).
- `.rs` production files ≤ 600 lines; test files (`*_tests.rs`, `tests/`) are uncapped; `mod.rs`/`lib.rs` stay thin routers.
- Zero allocation, lock, syscall or I/O on the audio thread (invariant #8).
- Stream isolation LAW: runtimes are selected or grouped by stream/device/route identity, never by rate or "all that match".
- Volume per stream is immutable: never edit `crates/engine/src/volume_invariants_tests.rs`, never relax a sound, latency, volume or golden assertion — a failure there is a STOP and report.
- (TESTS) Old tests whose assertion encodes the old one-runtime-per-input topology may be adjusted; list each one in the commit body.
- Repo content (code, comments, docs, commits, issue comments) in English.
- Docs updated in the same commit as the behaviour they describe.
- Zero warnings (`cargo build --workspace` and the test build).
- Work only in `.solvers/issue-980`; git only as `git -C $S`; stage explicit paths (never `git add -A`); never `git worktree`; never touch the main folder.
- Pre-push gate: `cargo fmt --all -- --check` + `cargo build --workspace` + `cargo test --workspace` + `VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates`, all from `$S`.
- Commit ⇒ push at once (fetch + `pull --rebase` first); no force push; no PR, no tag.
- After every push: `gh issue comment 980` with hash, files, what ran green.
- JACK code (`cfg(all(target_os = "linux", feature = "jack"))`) is verified in the Linux builder image (`$LINUX`), never on the owner's machine, never against a running jackd.
- Real-hardware battery (`OPENRIG_HW_TESTS=1`) runs on an idle machine in Task 19; its assertions are never edited.
- No hand-off, PR or validation request before Tasks 17 and 18 are pushed (R1: the #967 regression window closes only at 17; R9: a re-pairing over the same I/O keeps stale streams until 18) and Task 14 is green in `$LINUX cargo test --workspace` (R2).
- Linux stays green on every push (R2): a new test that needs per-pipeline grouping carries `#[cfg(not(all(target_os = "linux", feature = "jack")))]` (an integration test file: the inner `#![cfg(…)]`) until Task 14 deletes the engine's JACK key and removes those gates.

## Review Focus

1. **The owner's footswitch** (one E/S, Main + Out 2, a bound loop on the same interface): switching the loop must open no stream and both outputs must keep their level — pinned by Task 16 (`a_loop_on_a_two_output_es_keeps_the_chains_structure`, `switching_the_loop_keeps_the_same_slots`, `every_tail_sounds_exactly_as_the_whole_chain_in_both_loop_states`, the BlackHole HW twin) and Task 15 (`a_held_runtime_that_writes_nothing_here_leaves_the_writer_byte_identical`).
2. **A loop switch on a VST3/NAM chain** (in-place path): every processor that existed before the switch keeps running where its block goes (no VST3 re-instantiated, no delay tail cut); the instances a switch needs in addition — loop off, a block the cut ran once now runs once per pipeline — are built BEFORE any runtime gives its processors up, so no output plays its guitar unprocessed while a VST3 or NAM model loads; and every slot goes live under all the chain's `processing` locks, so no output has two writers or none mid-switch — pinned by Task 17 (`a_loop_switch_moves_each_block_with_its_processor`, `switching_the_loop_off_builds_the_missing_processors_before_taking_any`, `switching_the_loop_on_never_gives_out_2_two_writers`, `switching_the_loop_off_never_leaves_out_2_without_a_writer`).
3. **An E/S re-bind or preset that keeps the runtime COUNT but changes the pipelines** (1 in × 2 out → 2 in × 1 out, or two E/S re-paired over the same I/O): an output must never play the wrong guitar, fail the edit or keep playing a runtime the graph dropped — pinned by Task 2 (`an_in_place_update_refills_a_runtime_with_its_own_pipeline`), Task 3 (`a_rebind_that_keeps_the_runtime_count_rebuilds_the_pipelines`), Task 6 (`a_rebuild_never_lands_in_another_pipelines_slot`), Task 7 (`a_live_edit_whose_pipelines_moved_takes_the_synchronous_path`), Task 18 (`a_repairing_that_keeps_every_stream_signature_rebuilds_the_streams`).
4. **One guitar into two outputs, or a split-mono guitar into two outputs**: each output's level and cushion must equal the one-runtime build, and split-mono siblings must stay together per output — pinned by Task 1 (`splitting_the_pipelines_changes_no_route_level_or_latency`, `split_mono_siblings_share_each_output_pipeline`).
5. **Orange Pi / Linux JACK**: every output of an E/S and every guitar on one interface must play, each pipeline on its own worker, and two outputs on the same channels must not clip — pinned by Task 11 (`every_output_of_the_chain_plays_on_the_jack_client`, `two_e_s_on_the_same_channels_never_clip_the_device`), Task 12 (`a_route_plays_every_pipeline_that_writes_it`, `every_pipeline_is_processed_by_its_own_worker`), Task 14 (`each_output_plays_from_the_pipeline_that_writes_it`, `issue_980_one_worker_per_pipeline` green under `$LINUX`).

## File Structure

Created — production:

| File | Responsibility line | Task |
|---|---|---|
| `crates/engine/src/pipeline_grouping.rs` | partitions a chain's segments into its isolated pipelines. | 1 |
| `crates/infra-cpal/src/chain_slot_install.rs` | installs the live slots a chain's new streams read. | 5 |
| `crates/infra-cpal/src/jack_ring.rs` | carries interleaved JACK periods from the process callback to one worker. | 9 |
| `crates/infra-cpal/src/jack_pipeline.rs` | holds one JACK pipeline's private hand-off to its worker. | 10 |
| `crates/infra-cpal/src/jack_period.rs` | moves one JACK period through the chain's pipelines. | 10 |
| `crates/infra-cpal/src/jack_dsp_worker.rs` | runs one JACK pipeline's DSP off the process callback. | 10 |
| `crates/infra-cpal/src/jack_route_owners.rs` | decides which pipelines feed each output route of a JACK client. | 11 |
| `crates/engine/src/insert_fixed_slots.rs` | decides the runtime slots a chain keeps across an insert switch. | 16 |
| `crates/engine/src/runtime_node_pool.rs` | lends a chain's old block processors to the runtimes being rebuilt. | 17 |
| `crates/engine/src/runtime_node_plan.rs` | readies every processor a chain's rebuilt runtimes need before any runtime gives its own up. | 17 |
| `crates/engine/src/runtime_graph_update_together.rs` | switches every runtime slot of a chain to its new state at one instant. | 17 |

Created — tests: `crates/engine/src/issue_980_pipeline_split_tests.rs` (1–3), `crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs` (1, 4–7), `crates/infra-cpal/src/jack_ring_tests.rs` (9), `crates/infra-cpal/src/jack_period_tests.rs` (11, 12, 14), `crates/infra-cpal/src/jack_dsp_worker_tests.rs` (12), `crates/infra-cpal/src/controller_sync_jack_tests.rs` (13), `crates/engine/tests/issue_980_idle_slot_output.rs` (15), `crates/engine/tests/issue_980_insert_fixed_slots.rs` (16), `crates/infra-cpal/src/issue_980_fixed_slot_binding_tests.rs` (16), `crates/infra-cpal/src/issue_980_insert_fixed_slots_hw_tests.rs` (16), `crates/engine/src/issue_980_fixed_slot_switch_tests.rs` (17), `crates/infra-cpal/src/issue_980_pipeline_pairing_tests.rs` (18), `crates/infra-cpal/src/controller_jack_slots_tests.rs` (18).

Modified — main production sites: `crates/engine/src/runtime_graph.rs` (1, 2, 3, 16), `runtime_graph_update.rs` (1, 2, 17), `runtime_graph_impl.rs` (3, 17), `runtime_chain_state.rs` (1, 2, 8), `runtime_segments.rs` (2), `runtime_graph_assemble.rs` (2, 17), `runtime_block_builders.rs` (17), `runtime_output_process.rs` (15), `runtime_state.rs` (17), `engine/Cargo.toml` (14); `crates/infra-cpal/src/controller.rs` (4), `controller_upsert.rs` (5, 18), `controller_rebuild_queue.rs` (5, 6), `controller_offthread_live_rebuild.rs` (7, 17), `controller_taps.rs` (8), `jack_handlers.rs` (9, 10), `jack_direct.rs` (9, 10), `stream_builder.rs` (10, 12), `active_runtime.rs` (10), `controller_sync.rs` (13), `io_topology.rs` (18), `infra-cpal/Cargo.toml` (14); docs `docs/audio-config.md`, `docs/mcp.md`, `docs/testing.md`.

## Conventions (apply to every task)

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-980
SCRATCH=/private/tmp/claude-501/-Users-joao-faria-Projetos-github-com-jpfaria-OpenRig/57c5c85b-4e46-4843-a1bf-b5589db16ca9/scratchpad
```

- Every command runs as `(cd $S && …)`; git always `git -C $S …`.
- **Gate** (run once before every push; `cargo build` must print zero `warning:` lines):

```bash
(cd $S && cargo fmt --all && cargo fmt --all -- --check && cargo build --workspace && cargo test --workspace && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates)
```

- **Push** (right after every commit):

```bash
git -C $S fetch && git -C $S pull --rebase   # if the remote moved, run the Gate again before pushing
git -C $S push
gh issue comment 980 --body "<short hash> — <subject> — files: <list> — green: Gate<, plus the task's \$LINUX lines>"
```

- **Linux runner** (JACK tasks 9–14, 18). Build the image once; the cargo registry cache stays under the git-ignored `target/`:

```bash
docker build --platform linux/arm64 --build-arg "RUSTFLAGS_CPU=-C target-cpu=cortex-a53" \
  -t openrig-linux-builder:aarch64 -f "$S/docker/Dockerfile.linux-builder" "$S/docker"
mkdir -p "$S/target/linux-cargo-registry"
LINUX="docker run --rm --platform linux/arm64 -v $S:/workspace \
  -v $S/target/linux-cargo-registry:/root/.cargo/registry \
  -e CARGO_TARGET_DIR=/workspace/target/linux-docker openrig-linux-builder:aarch64"
```

- A test that fails for any other reason than the one a step predicts: classify it before touching it — (a) its assertion encodes one runtime per input → adjust it (TESTS decision) and list it in the commit body; (b) sound, latency, volume or golden → STOP and report on the issue, never relax.
- Commit trailer on every commit: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

---

### Task 1: Group segments by pipeline, dense slots in segment order

Makes the existing red contract test `crates/engine/tests/issue_980_one_worker_per_pipeline.rs` green on macOS.

**Files:**
- Create: `crates/engine/src/pipeline_grouping.rs`
- Create: `crates/engine/src/issue_980_pipeline_split_tests.rs`
- Create: `crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs`
- Create (copy): `docs/superpowers/specs/2026-09-27-issue-980-pipeline-per-worker-design.md`, `docs/superpowers/plans/2026-09-27-issue-980-pipeline-per-worker.md`
- Commit (already on disk, untracked): `crates/engine/tests/issue_980_one_worker_per_pipeline.rs` — its only edit is the Linux+JACK gate (Step 1), removed again in Task 14
- Modify: `crates/engine/src/lib.rs:43` (module line)
- Modify: `crates/engine/src/runtime.rs` (test mount, end of file, Linux+JACK gated until Task 14)
- Modify: `crates/engine/src/runtime_graph.rs:50,58-73,78-92,124-169,204-207,228,232-234,281-287,306-307,446`
- Modify: `crates/engine/src/runtime_graph_update.rs:21,128-136`
- Modify: `crates/engine/src/runtime_chain_state.rs:30-36` (doc only)
- Modify: `crates/infra-cpal/src/lib.rs:87` (test mount)
- Modify (old-topology tests): `crates/engine/src/issue_953_route_latency_drift_tests.rs:76-125`, `crates/engine/src/issue_965_insert_latency_tests.rs:153` (message), `crates/engine/tests/issue_716_bound_chain_builds.rs:70` (message), `crates/infra-cpal/tests/build_chain_runtime.rs:6-7,62` (doc + message), `crates/engine/src/stream_isolation_same_device_tests.rs:26` (comment)
- Modify docs: `docs/audio-config.md` ("Per-entry stream isolation" section, the #967 insert paragraph), `docs/mcp.md` (`openrig://routes` row)

**Interfaces:**
- Consumes: `crate::runtime_graph::chain_has_insert_cut(&Chain, &[IoBinding]) -> bool` (`runtime_graph.rs:93`), `ChainSegment { input, cpal_input_index, block_indices, output_route_indices, mid_output_taps, split_mono_sibling_count, entry_group }` (`segment_types.rs:29-48`).
- Produces: `pub struct PipelineKey { pub route: usize, pub entry_group: usize }` (derive `Clone, Copy, Debug, PartialEq, Eq, Hash`), re-exported as `engine::runtime_graph::PipelineKey`; `pub(crate) fn pipeline_key_of(segment: &ChainSegment) -> PipelineKey`; `pub(crate) fn group_segments_into_pipelines(chain: &Chain, registry: &[IoBinding], segments: Vec<ChainSegment>) -> Vec<(usize, Vec<ChainSegment>)>` (Task 2 changes the return type to `Vec<PipelineGroup>`); private `fn first_seen<K: Copy + Eq + Hash>(segments: Vec<ChainSegment>, key_of: impl Fn(&ChainSegment) -> K) -> Vec<(K, Vec<ChainSegment>)>`.
- Removes: `crate::runtime_graph::group_segments_by_input`.

- [ ] **Step 1: Write the failing tests**

Create `crates/engine/src/issue_980_pipeline_split_tests.rs`:

```rust
//! #980 — one (input × output) pipeline, one runtime, so one dsp-worker.
//!
//! The grouping must give every pipeline its own runtime, keep split-mono
//! siblings together (g02/g03), number the runtimes in the order the chain's
//! streams are labelled, keep an in-place edit pipeline-local, and leave every
//! route's level and cushion where the whole-chain runtime puts them
//! (invariants #1 and #10).

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use super::{process_input_f32, process_output_f32};
use crate::runtime_graph::{
    build_chain_runtime_state, build_per_input_runtimes, build_runtime_graph, RuntimeGraph,
};
use crate::runtime_state::ChainRuntimeState;

const DEVICE: &str = "coreaudio:quantum";
const SR: f32 = 48_000.0;
const TARGET: usize = 256;
const CHANNELS: usize = 12;
const FRAMES: usize = 128;
const MAIN: [usize; 2] = [0, 1];
const OUT2: [usize; 2] = [10, 11];

fn endpoint(name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(DEVICE.into()),
        mode,
        channels: channels.to_vec(),
    }
}

/// One E/S with the given inputs and outputs.
fn binding(inputs: Vec<IoEndpoint>, outputs: Vec<IoEndpoint>) -> Vec<IoBinding> {
    vec![IoBinding {
        id: "guitarra-1".into(),
        name: "Guitarra 1".into(),
        inputs,
        outputs,
    }]
}

fn main_and_out2() -> Vec<IoEndpoint> {
    vec![
        endpoint("main", ChannelMode::Stereo, &MAIN),
        endpoint("out 2", ChannelMode::Stereo, &OUT2),
    ]
}

fn guitar(channel: usize) -> IoEndpoint {
    endpoint(&format!("in {channel}"), ChannelMode::Mono, &[channel])
}

/// One guitar into Main and Out 2 — the owner's E/S.
fn one_guitar() -> Vec<IoBinding> {
    binding(vec![guitar(0)], main_and_out2())
}

fn two_guitars() -> Vec<IoBinding> {
    binding(vec![guitar(0), guitar(1)], main_and_out2())
}

/// One raw entry split by the engine into two mono siblings.
fn split_mono_guitar() -> Vec<IoBinding> {
    binding(
        vec![endpoint("in", ChannelMode::Mono, &[0, 1])],
        main_and_out2(),
    )
}

fn gain(id: &str) -> AudioBlock {
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

fn chain(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitarra-1".into()],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

fn per_pipeline(chain: &Chain, registry: &[IoBinding]) -> Vec<Arc<ChainRuntimeState>> {
    build_per_input_runtimes(chain, SR, &HashMap::new(), &[TARGET, TARGET], registry)
        .expect("the chain must build")
        .into_iter()
        .map(|(_, state)| Arc::new(state))
        .collect()
}

fn graph(chain: &Chain, registry: &[IoBinding]) -> RuntimeGraph {
    let project = Project {
        name: None,
        chains: vec![chain.clone()],
        device_settings: Vec::new(),
        midi: None,
    };
    let rates = HashMap::from([(chain.id.clone(), SR)]);
    let targets = HashMap::from([(chain.id.clone(), vec![TARGET, TARGET])]);
    build_runtime_graph(&project, &rates, &targets, registry).expect("the graph must build")
}

/// The routes, out of Main and Out 2, a runtime writes.
fn written(runtime: &ChainRuntimeState) -> Vec<usize> {
    (0..2).filter(|&route| runtime.writes_output(route)).collect()
}

/// RED — split-mono siblings stay together inside EACH output pipeline.
#[test]
fn split_mono_siblings_share_each_output_pipeline() {
    let runtimes = per_pipeline(&chain(vec![]), &split_mono_guitar());
    assert_eq!(
        runtimes.len(),
        2,
        "one split-mono guitar x 2 outputs = 2 pipelines = 2 runtimes"
    );
    for (slot, runtime) in runtimes.iter().enumerate() {
        assert_eq!(
            runtime.stream_count(),
            2,
            "slot {slot}: both split-mono siblings stay in the pipeline (g02/g03 \
             sum before the limiter)"
        );
        assert_eq!(written(runtime), vec![slot], "slot {slot} writes its own output only");
    }
}

/// RED — meter row k is pipeline k: the slot walk follows segment order, the
/// order `chain_stream_io_labels` names the rows in.
#[test]
fn runtimes_are_numbered_in_the_order_the_streams_are_labelled() {
    let chain = chain(vec![]);
    let graph = graph(&chain, &two_guitars());
    let walk: Vec<(Vec<usize>, Vec<usize>)> = graph
        .runtimes_with_groups_for(&chain.id)
        .iter()
        .flat_map(|(_, runtime)| {
            (0..runtime.stream_count())
                .map(|k| {
                    let (_, _, channels) = runtime
                        .input_routing_for_stream(k)
                        .expect("the stream exists");
                    (channels, written(runtime))
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        walk,
        vec![
            (vec![0], vec![0]),
            (vec![1], vec![0]),
            (vec![0], vec![1]),
            (vec![1], vec![1]),
        ],
        "(Main: guitar 1, guitar 2), then (Out 2: guitar 1, guitar 2) — one runtime each"
    );
}

/// RED — an in-place edit keeps every pipeline in its own runtime.
#[test]
fn an_in_place_edit_keeps_every_pipeline_in_its_own_runtime() {
    let chain = chain(vec![gain("amp")]);
    let registry = one_guitar();
    let mut graph = graph(&chain, &registry);
    let mut edited = chain.clone();
    edited.volume = 80.0;
    graph
        .upsert_chain(&edited, SR, &HashMap::new(), false, &[TARGET, TARGET], &registry)
        .expect("in-place upsert must succeed");
    let runtimes = graph.runtimes_with_groups_for(&chain.id);
    assert_eq!(runtimes.len(), 2, "topology unchanged => still one runtime per pipeline");
    for (slot, runtime) in &runtimes {
        {
            let processing = runtime.processing.lock().expect("processing lock");
            assert_eq!(
                processing.input_states.len(),
                1,
                "slot {slot} runs one pipeline after the edit"
            );
            let dispatched: usize = processing.input_to_segments.iter().map(Vec::len).sum();
            assert_eq!(dispatched, 1, "slot {slot} dispatches only its own segment");
        }
        assert_eq!(written(runtime), vec![*slot], "slot {slot} writes its own output only");
    }
}

/// Peak of each route over the second half of `periods`, and each route's
/// cushion at the end — every route popped from the runtime that writes it.
fn play(runtimes: &[Arc<ChainRuntimeState>], periods: usize) -> ([f32; 2], [usize; 2]) {
    let mut input = vec![0.0_f32; FRAMES * CHANNELS];
    for frame in input.chunks_mut(CHANNELS) {
        frame[0] = 0.25;
    }
    let mut out = vec![0.0_f32; FRAMES * CHANNELS];
    let mut peaks = [0.0_f32; 2];
    for period in 0..periods {
        for runtime in runtimes {
            process_input_f32(runtime, 0, &input, CHANNELS);
        }
        for route in 0..2 {
            let owner = runtimes
                .iter()
                .find(|runtime| runtime.writes_output(route))
                .expect("every route has a writer");
            out.fill(0.0);
            process_output_f32(owner, route, &mut out, CHANNELS);
            if period >= periods / 2 {
                let channel = [MAIN[0], OUT2[0]][route];
                peaks[route] = out
                    .chunks_exact(CHANNELS)
                    .fold(peaks[route], |peak, frame| peak.max(frame[channel].abs()));
            }
        }
    }
    let mut fill = [0_usize; 2];
    for (route, slot) in fill.iter_mut().enumerate() {
        let owner = runtimes
            .iter()
            .find(|runtime| runtime.writes_output(route))
            .expect("every route has a writer");
        *slot = owner
            .take_output_route_stats()
            .into_iter()
            .find(|stats| stats.route == route)
            .map(|stats| stats.fill_frames)
            .expect("the writer owns the route");
    }
    (peaks, fill)
}

/// Pin — invariants #10 (volume) and #1 (latency) on the grouped path.
#[test]
fn splitting_the_pipelines_changes_no_route_level_or_latency() {
    let chain = chain(vec![gain("amp")]);
    let registry = one_guitar();
    let whole = vec![Arc::new(
        build_chain_runtime_state(&chain, SR, &[TARGET, TARGET], &registry)
            .expect("the whole chain builds"),
    )];
    let split = per_pipeline(&chain, &registry);
    let (whole_peaks, whole_fill) = play(&whole, 64);
    let (split_peaks, split_fill) = play(&split, 64);
    for route in 0..2 {
        assert!(whole_peaks[route] > 0.05, "route {route} carries the guitar");
        assert!(
            (split_peaks[route] - whole_peaks[route]).abs() < 1e-6,
            "volume #10: route {route} peak {} split vs {} whole",
            split_peaks[route],
            whole_peaks[route]
        );
        assert_eq!(
            split_fill[route], whole_fill[route],
            "latency #1: route {route} cushion must not move"
        );
    }
}
```

Append to `crates/engine/src/runtime.rs` (after the last mount, `mod issue_965_insert_latency;`). Linux+JACK still groups per device until Task 14, where these tests cannot hold — the gate keeps `$LINUX cargo test --workspace` green on every push (R2) and Task 14 turns it back into `#[cfg(test)]`:

```rust

// #980: Linux+JACK groups per device until the JACK-direct client runs one
// pipeline per runtime; this gate goes with the engine's JACK key.
#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]
#[path = "issue_980_pipeline_split_tests.rs"]
mod issue_980_pipeline_split;
```

Gate the untracked contract test the same way: in `crates/engine/tests/issue_980_one_worker_per_pipeline.rs`, insert right after its `//!` header (line 15, `//! runtimes, each running exactly one pipeline and writing exactly one route.`) and before `use std::collections::HashMap;`:

```rust
//!
//! Linux+JACK groups per device until the JACK-direct client runs one pipeline
//! per runtime (#980); the gate goes with the engine's JACK key.
#![cfg(not(all(target_os = "linux", feature = "jack")))]
```

(`cargo test --workspace` on Linux unifies `engine/jack` through `infra-cpal/jack` — `crates/infra-cpal/Cargo.toml:7`, enabled by `crates/adapter-gui/Cargo.toml:50` — and an integration test sees the package's features.)

Create `crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs` (Tasks 4–7 append to it):

```rust
//! #980 — infra-cpal side of one runtime per (input × output) pipeline: the
//! device stream feeds every pipeline, each output holds only its own, and
//! the controller never mixes up, leaks or half-applies pipeline runtimes.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_per_input_runtime_states, ChainRuntimeState};
use project::chain::Chain;

use crate::slot_processing::{slots_for_input_stream, slots_for_output_stream};
use crate::{build_chain_slots, LiveRuntimeSlot};

const DEVICE: &str = "coreaudio:quantum";
const SR: f32 = 48_000.0;
const TARGET: usize = 256;

fn endpoint(name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(DEVICE.into()),
        mode,
        channels: channels.to_vec(),
    }
}

/// One guitar (mono ch 0) into one stereo output per entry of `outputs`.
fn registry(outputs: &[&[usize]]) -> Vec<IoBinding> {
    vec![IoBinding {
        id: "guitarra-1".into(),
        name: "Guitarra 1".into(),
        inputs: vec![endpoint("in", ChannelMode::Mono, &[0])],
        outputs: outputs
            .iter()
            .enumerate()
            .map(|(i, channels)| endpoint(&format!("out {i}"), ChannelMode::Stereo, channels))
            .collect(),
    }]
}

fn chain() -> Chain {
    Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitarra-1".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

fn pipelines(registry: &[IoBinding]) -> Vec<(usize, Arc<ChainRuntimeState>)> {
    build_per_input_runtime_states(&chain(), SR, &HashMap::new(), &[TARGET, TARGET], registry)
        .expect("the chain must build")
}

/// RED — one guitar's device stream feeds BOTH of its pipelines (one
/// dsp-worker each), and each output holds exactly the pipeline that writes it.
#[test]
fn one_device_stream_feeds_every_pipeline_of_its_guitar() {
    let slots = build_chain_slots(&pipelines(&registry(&[&[0, 1], &[10, 11]])));
    assert_eq!(
        slots_for_input_stream(&slots, 0).len(),
        2,
        "the guitar's device stream feeds both pipelines, each on its own dsp-worker"
    );
    let held: Vec<Vec<LiveRuntimeSlot>> = (0..2)
        .map(|j| slots_for_output_stream(&slots, &[vec![DEVICE.to_string()]], DEVICE, j))
        .collect();
    for (j, slots) in held.iter().enumerate() {
        assert_eq!(slots.len(), 1, "output {j} holds exactly the pipeline that writes it");
    }
    assert!(
        !Arc::ptr_eq(&held[0][0].load(), &held[1][0].load()),
        "Main and Out 2 are fed by two different runtimes"
    );
}
```

Register it in `crates/infra-cpal/src/lib.rs` right after line 87 (`pub use slot_processing::{build_chain_slots, process_input_buffer, process_output_buffer};`):

```rust
#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]
#[path = "issue_980_pipeline_slots_tests.rs"]
mod issue_980_pipeline_slots_tests;
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
(cd $S && cargo test -p engine --test issue_980_one_worker_per_pipeline)
(cd $S && cargo test -p engine --lib issue_980_pipeline_split)
(cd $S && cargo test -p infra-cpal --lib issue_980_pipeline_slots)
```

Expected:
- `each_output_pipeline_of_one_guitar_runs_in_its_own_runtime` FAILS `left: 1, right: 2` ("1 input x 2 outputs = 2 pipelines = 2 runtimes (one dsp-worker each)").
- `split_mono_siblings_share_each_output_pipeline` FAILS `left: 1, right: 2`.
- `runtimes_are_numbered_in_the_order_the_streams_are_labelled` FAILS `left: [([0], [0, 1]), ([0], [0, 1]), ([1], [0, 1]), ([1], [0, 1])]`.
- `an_in_place_edit_keeps_every_pipeline_in_its_own_runtime` FAILS `left: 1, right: 2`.
- `one_device_stream_feeds_every_pipeline_of_its_guitar` FAILS `left: 1, right: 2`.
- `splitting_the_pipelines_changes_no_route_level_or_latency` PASSES (pin).

- [ ] **Step 3: Implement**

Create `crates/engine/src/pipeline_grouping.rs`:

```rust
//! Responsibility: partitions a chain's segments into its isolated pipelines.
//!
//! #980 — the owner's LAW (CLAUDE.md): "if I have 1 input and 2 outputs I have
//! 2 streams"; N streams are N isolated pipelines, and isolation includes CPU
//! time. infra-cpal spawns one dsp-worker per runtime, so this grouping decides
//! how many realtime threads a chain's DSP runs on: one per pipeline, named by
//! the RAW input entry it starts from and the output route it ends at.
//! Split-mono siblings (one raw entry over N channels) share their pipeline,
//! so they still sum before the limiter once (volume invariants g02/g03).
//!
//! Setup-time only: nothing here runs on the audio thread.

use std::collections::HashMap;
use std::hash::Hash;

use domain::io_binding::IoBinding;
use project::chain::Chain;

use crate::runtime_segments::ChainSegment;

/// One isolated pipeline of a chain: the output route it ends at and the RAW
/// input entry it starts from. Both are positions in the chain's resolved I/O,
/// so the same I/O and the same structure always name the same pipelines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PipelineKey {
    pub route: usize,
    pub entry_group: usize,
}

/// The pipeline a segment outside any insert cut runs in: such a segment
/// writes exactly one route (`runtime_segments::segments_without_inserts`).
// Linux+JACK still groups per device (below), so it has no caller there.
#[cfg_attr(all(target_os = "linux", feature = "jack"), allow(dead_code))]
pub(crate) fn pipeline_key_of(segment: &ChainSegment) -> PipelineKey {
    PipelineKey {
        route: segment.output_route_indices.first().copied().unwrap_or(0),
        entry_group: segment.entry_group,
    }
}

/// Partition a chain's segments into `(slot, segments)`, one per runtime.
/// `slot` is a dense ordinal in segment order — the `RuntimeGraph` key and
/// the infra-cpal slot id — so walking the runtimes by slot walks the streams
/// in the order `chain_stream_io_labels` names them.
///
/// An insert's cut is one pipeline across every head (send → gear → return →
/// tail), so the chain stays ONE runtime while an insert cuts it.
///
/// Linux/JACK keeps the per-device grouping (slot = cpal index) behind its
/// cfg: the JACK-direct client still binds one runtime.
pub(crate) fn group_segments_into_pipelines(
    chain: &Chain,
    registry: &[IoBinding],
    segments: Vec<ChainSegment>,
) -> Vec<(usize, Vec<ChainSegment>)> {
    if segments.is_empty() || crate::runtime_graph::chain_has_insert_cut(chain, registry) {
        return vec![(0, segments)];
    }
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        return first_seen(segments, |segment| segment.cpal_input_index);
    }
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        first_seen(segments, pipeline_key_of)
            .into_iter()
            .enumerate()
            .map(|(slot, (_, segments))| (slot, segments))
            .collect()
    }
}

/// Partition `segments` by `key_of`, in first-seen order of the keys, keeping
/// segment order inside each group.
fn first_seen<K: Copy + Eq + Hash>(
    segments: Vec<ChainSegment>,
    key_of: impl Fn(&ChainSegment) -> K,
) -> Vec<(K, Vec<ChainSegment>)> {
    let mut order: Vec<K> = Vec::new();
    let mut groups: HashMap<K, Vec<ChainSegment>> = HashMap::new();
    for segment in segments {
        let key = key_of(&segment);
        if !groups.contains_key(&key) {
            order.push(key);
        }
        groups.entry(key).or_default().push(segment);
    }
    order
        .into_iter()
        .map(|key| {
            let segments = groups.remove(&key).unwrap_or_default();
            (key, segments)
        })
        .collect()
}
```

`crates/engine/src/lib.rs` — insert after line 43 (`pub mod output_meter;`):

```rust
mod pipeline_grouping;
```

`crates/engine/src/runtime_graph.rs`:

- Line 50: `use crate::runtime_segments::{split_chain_into_segments, ChainSegment};` becomes `use crate::runtime_segments::split_chain_into_segments;`.
- Replace the doc comment at lines 58-73 (above `pub struct RuntimeGraph`) with:

```rust
/// Issue #350 / #703 / #980 — stream isolation invariant (CLAUDE.md #4).
///
/// One `ChainRuntimeState` per PIPELINE — one (raw input entry × output route)
/// pair — not one per YAML chain and not one per input. The key is
/// `(ChainId, slot)`, `slot` a dense ordinal in segment order
/// (`pipeline_grouping::group_segments_into_pipelines`). Every runtime has its
/// own `output_routes` (`OutputRoutingState` + `ElasticBuffer`), `input_taps`
/// and `processing` Mutex — and, in infra-cpal, its own dsp-worker, so one
/// pipeline's DSP never waits on another's (#980: CPU time is part of the
/// isolation). Mixing into one physical device happens at the cpal/JACK
/// backend, never by two producers on one of our SPSC rings.
///
/// `.len()` = total pipelines. A chain with one input and one output is one
/// runtime, byte-identical to the whole-chain build; so is a chain an insert
/// cuts (see `chain_has_insert_cut`).
```

- Replace the doc comment at lines 78-92 (above `pub(crate) fn chain_has_insert_cut`) with:

```rust
/// Whether an insert cuts the chain. A cut forms a cross-cpal-index pipeline
/// (input → insert send → insert return → output); splitting it would sever
/// the pipeline, so a chain an insert cuts is a single runtime.
///
/// #967: only a real cut counts (`insert_cut::insert_cuts_chain`): a disabled
/// insert — or one with no E/S — leaves every pipeline in its own isolated
/// runtime (#980). Switching an insert on a chain whose uncut form has more
/// than one pipeline (several inputs or several outputs) regroups its
/// runtimes; `chain_structure_signature` (infra-cpal) says so and the chain
/// gets new streams. A chain with one pipeline is one runtime either way, so
/// its switch is a DSP rebuild into the slot its streams feed.
```

- Delete lines 124-169 (the doc comment and the whole `pub(crate) fn group_segments_by_input`).
- Replace the doc comment of `build_per_input_runtimes` (lines 204-207) with:

```rust
/// Build one `ChainRuntimeState` per pipeline of the chain (#980). Returns
/// `(slot, state)` pairs in slot order. A chain with one input and one output,
/// or one an insert cuts, is exactly one pair whose state equals
/// `build_chain_runtime_state` (byte-identical audio path).
```

- Line 228: `let groups = group_segments_by_input(chain, registry, all_segments);` becomes:

```rust
    let groups =
        crate::pipeline_grouping::group_segments_into_pipelines(chain, registry, all_segments);
```

- Lines 232-234 comment becomes:

```rust
        // All segments of a pipeline share one effective input, hence one
        // cpal stream — record it so infra-cpal can fan a shared device
        // callback out to every pipeline runtime it feeds (#703, #980).
```

- Replace the first paragraph of the `build_per_input_runtime_states` doc (lines 281-286) with:

```rust
/// Issue #703 / #980: public seam for infra layers that build a chain's
/// runtimes off-thread (cold activation, live rebuild). One isolated
/// `ChainRuntimeState` per pipeline — the shape `RuntimeGraph` holds — so the
/// caller publishes each runtime into its `(chain, slot)` slot. A one-input,
/// one-output chain returns exactly one `(0, state)` pair (byte-identical).
```

- `input_group_ids` doc, first sentence (line 306): `/// The per-input group ids a chain would produce, WITHOUT instantiating any` becomes `/// The slot ids a chain's runtimes would get — one per pipeline (#980) —` and the next line starts `/// WITHOUT instantiating any block processor. Issue #588: …` (keep the rest of the paragraph).
- Line 327: `group_segments_by_input(chain, registry, all_segments)` becomes `crate::pipeline_grouping::group_segments_into_pipelines(chain, registry, all_segments)`.
- After line 446 (`use crate::runtime_graph_assemble::assemble_chain_runtime_state;`) add:

```rust
pub use crate::pipeline_grouping::PipelineKey;
```

`crates/engine/src/runtime_graph_update.rs`:

- Line 21: `use crate::runtime_graph::group_segments_by_input;` becomes `use crate::pipeline_grouping::group_segments_into_pipelines;`.
- Replace lines 128-136 (the comment and the `Some((group, _)) =>` arm head) so the block reads:

```rust
    // Issue #703 / #980: a pipeline runtime is refilled with ONLY its own
    // pipeline's segments. Both pipelines of one device dispatch on the same
    // cpal index, so refilling every runtime with ALL segments would make the
    // one device callback process the same guitar in every sibling runtime —
    // summed at the backend mix (audible double volume). A whole-chain runtime
    // (`owned_entry == None`: probe, offline) keeps every segment.
    let segments: Vec<ChainSegment> = match runtime.owned_entry {
        Some((group, _)) => group_segments_into_pipelines(chain, registry, all_segments)
```

(the `.into_iter().find(|(g, _)| *g == group)…` tail and the `None => all_segments` arm stay as they are).

`crates/engine/src/runtime_chain_state.rs` — replace the doc of `owned_entry` (lines 30-36) with:

```rust
    /// Issue #703 / #980: set when this runtime is one isolated pipeline
    /// runtime — `(slot, cpal_input_index)`. `slot` is its `RuntimeGraph`
    /// key; `cpal_input_index` is the device stream that feeds it — SHARED
    /// with every other pipeline reading the same device (the other outputs
    /// of the same guitar, other entries on the interface). `None` for
    /// whole-chain runtimes (probe, offline render). Written once at graph
    /// assembly, before the state is shared.
```

Old-topology test adjustments (TESTS decision):

(a) `crates/engine/src/issue_953_route_latency_drift_tests.rs` — replace `fn runtime()` and `struct Rig` / `Rig::new` / `Rig::period` (lines 76-125) with the code below; `pulse_latency` and the test body are unchanged. Its latency assertions are NOT edited; if they fail after Step 3 that is a real #953 regression: STOP and report.

```rust
fn runtimes() -> Vec<Arc<ChainRuntimeState>> {
    let runtimes = build_per_input_runtimes(
        &chain(),
        44_100.0,
        &HashMap::new(),
        &vec![DEFAULT_ELASTIC_TARGET; 2],
        &registry(),
    )
    .expect("the chain must build");
    // #980: Main and FRFR are two pipelines of one guitar. Whether they run in
    // one runtime or two, each route has exactly one writer.
    for route in 0..2 {
        assert_eq!(
            runtimes
                .iter()
                .filter(|(_, rt)| rt.writes_output(route))
                .count(),
            1,
            "route {route} must have exactly one writer"
        );
    }
    runtimes.into_iter().map(|(_, rt)| Arc::new(rt)).collect()
}

/// Frame (counted from the pulse) at which each route first plays the pulse.
struct Rig {
    runtimes: Vec<Arc<ChainRuntimeState>>,
    input: Vec<f32>,
    out: Vec<f32>,
}

impl Rig {
    fn new() -> Self {
        Self {
            runtimes: runtimes(),
            input: vec![0.0; FRAMES * DEVICE_CHANNELS],
            out: vec![0.0; FRAMES * DEVICE_CHANNELS],
        }
    }

    /// One device period: the input callback into every pipeline, then the
    /// output callback of every route in `served`, each popped from the
    /// runtime that writes it. A route left out is a stalled output stream.
    /// Returns, per route, the first frame of this period carrying signal.
    fn period(&mut self, pulse: bool, served: &[usize]) -> [Option<usize>; 2] {
        self.input.fill(0.0);
        if pulse {
            self.input[GUITAR_IN] = 0.5;
        }
        for runtime in &self.runtimes {
            process_input_f32(runtime, 0, &self.input, DEVICE_CHANNELS);
        }
        let mut hit = [None, None];
        for &route in served {
            let owner = self
                .runtimes
                .iter()
                .find(|rt| rt.writes_output(route))
                .expect("every route has a writer");
            self.out.fill(0.0);
            process_output_f32(owner, route, &mut self.out, DEVICE_CHANNELS);
            let ch = [MAIN_OUT[0], FRFR_OUT[0]][route];
            hit[route] = self
                .out
                .chunks_exact(DEVICE_CHANNELS)
                .position(|frame| frame[ch].abs() > 1e-3);
        }
        hit
    }
```

(the `pulse_latency` method that follows and the closing `}` of `impl Rig` stay.)

(b) Message-only edits (the fixtures have one output; the assertion is untouched):
- `crates/engine/src/issue_965_insert_latency_tests.rs:153` — `"one binding, one input = one runtime"` becomes `"one input x one output = one pipeline runtime"`.
- `crates/engine/tests/issue_716_bound_chain_builds.rs:70` — `"one input endpoint in the binding → one isolated input runtime"` becomes `"one input x one output in the binding → one pipeline runtime"`.
- `crates/infra-cpal/tests/build_chain_runtime.rs:6-7` — `//! A chain with input entries builds one isolated runtime per input port; a` / `//! chain with no I/O blocks builds NONE.` becomes `//! A chain builds one isolated runtime per (input × output) pipeline (#980); a` / `//! chain with no I/O blocks builds NONE.`
- `crates/infra-cpal/tests/build_chain_runtime.rs:62` — `"a single-input chain produces exactly one isolated input runtime"` becomes `"a one-input, one-output chain produces exactly one pipeline runtime"`.

(c) `crates/engine/src/stream_isolation_same_device_tests.rs:26` — `` // `group_segments_by_input` deliberately partitions by cpal input index `` becomes `` // `pipeline_grouping::group_segments_into_pipelines` deliberately partitions by cpal input index ``.

Docs — `docs/audio-config.md`:
- Replace the heading `### Per-entry stream isolation (issues #350 / #703)` and the paragraph under it (up to `is only logical grouping.`) with:

```markdown
### Per-pipeline stream isolation (issues #350 / #703 / #980)

Every **pipeline** of a chain — one raw input entry × one output route — owns
its own isolated `ChainRuntimeState` (CLAUDE.md invariant #4): its own
`processing` Mutex, `output_routes` (+ `ElasticBuffer`), `input_taps`,
scratch, and on macOS its own `dsp_worker` realtime thread. "1 input and 2
outputs = 2 streams": one guitar into Main and Out 2 is two runtimes, so one
output's DSP never waits for the other's (#980: with both pipelines on one
worker, every route of the owner's rig lost exactly the frames it later
dropped). The `RuntimeGraph` is keyed by `(ChainId, slot)`, `slot` a dense
ordinal in segment order — the order the meter rows are labelled in; "chain"
in the YAML is only logical grouping.
```

- Replace the bullet that starts `- **Two entries on ONE device** (#703)` (whole bullet) with:

```markdown
- **One device, several pipelines** (#703, #980): Core Audio cannot open two
  streams on one device (a previous attempt produced total silence), so the
  device keeps ONE cpal stream whose callback fans out to every pipeline
  runtime bound to that cpal index — two entries on one interface, or one
  entry into two outputs. On macOS each pipeline gets its own `dsp_worker`
  realtime thread; Windows / Linux-cpal run the fan-out inline, one pipeline
  after another, in the device callback. State isolation is the contract; the
  hardware deadline of one device callback is inherently shared.
```

- In the bullet `- **Split-mono siblings** (one entry, …)`, `stay in ONE runtime:` becomes `stay in ONE runtime per output pipeline:`.
- Replace the bullet that starts `- **Insert chains** are a single runtime` with:

```markdown
- **Insert chains**: while an enabled, bound insert cuts the chain it is ONE
  runtime (the send/return pipeline spans cpal indices); a disabled insert
  cuts nothing and the chain splits per pipeline like any other.
  **Linux/JACK** keeps the per-device grouping behind the `jack` cfg until the
  JACK-direct client binds one runtime per pipeline.
```

- The line `Contract tests: \`crates/engine/src/stream_isolation_tests.rs\` +` and the two lines after it become:

```markdown
Contract tests: `crates/engine/tests/issue_980_one_worker_per_pipeline.rs`,
`crates/engine/src/issue_980_pipeline_split_tests.rs`,
`crates/engine/src/stream_isolation_tests.rs` +
`stream_isolation_same_device_tests.rs`; cpal binding in
`crates/infra-cpal/src/tests_regression.rs` and
`crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs`.
```

- In the #967 insert paragraph, replace the two sentences from `The one exception is a chain with several input` up to `is already bound when the loop is switched on.` with:

```markdown
The one exception is a chain with several pipelines — several input entries
(several E/S, an E/S with two input endpoints, a mid `Input`) or several
outputs (#980: one runtime per input × output pipeline): its runtimes are one
pipeline while the loop cuts it and one per pipeline while it does not, so the
switch regroups them — `chain_structure_signature` carries the grouping and
such a switch gets new streams. A chain with one pipeline (one input, one
output) is one runtime either way and owns every one of its routes
(`switch_owned_routes`), so a route only the loop's cut writes — its send, a
tail only the return feeds — is already bound when the loop is switched on.
```

`docs/mcp.md` — in the `openrig://routes` bullet, `row per (chain, runtime \`group\`, \`route\`)` becomes ``row per (chain, runtime `group`, `route`) — `group` is the pipeline slot (#980: one runtime per input × output pipeline, so a runtime normally owns one route; a chain an insert cuts is one runtime owning every route)``.

Copy the spec and this plan into the repo:

```bash
mkdir -p $S/docs/superpowers/specs $S/docs/superpowers/plans
cp $SCRATCH/2026-09-27-issue-980-pipeline-per-worker-design.md $S/docs/superpowers/specs/
cp $SCRATCH/2026-09-27-issue-980-pipeline-per-worker.md $S/docs/superpowers/plans/
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
(cd $S && cargo test -p engine --test issue_980_one_worker_per_pipeline)
(cd $S && cargo test -p engine --lib -- issue_980_pipeline_split issue_953 issue_965 stream_isolation issue_947 issue_923 issue_967)
(cd $S && cargo test -p infra-cpal --lib issue_980_pipeline_slots)
```

Expected: all PASS. Then run the Gate, and `$LINUX cargo test -p engine` (the gated contract test and `issue_980_pipeline_split` compile out; everything else stays green — a red here is a real Linux regression). Linux note (R2): under `$LINUX` the engine still groups per device until Task 14, so every new test that needs per-pipeline grouping carries the Linux+JACK gate (the two above, Task 8's DI test); Task 14 removes those gates. Must stay green unchanged: `stream_isolation_same_device_tests.rs` (`split_mono_siblings_stay_in_one_runtime`, `in_place_upsert_keeps_same_device_runtimes_entry_local`), `issue_947_routes_owned_by_their_stream_tests.rs`, `issue_967_insert_streams_tests.rs:167-175` (`[0, 1]`), `infra-cpal/src/tests_regression.rs:472-473`, `engine/src/rig_runtime_tests.rs:312-317`, `issue_947_output_stream_route_owner_tests.rs`, `controller_taps_tests.rs`, `volume_invariants_tests.rs`.

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/engine/src/pipeline_grouping.rs crates/engine/src/lib.rs crates/engine/src/runtime_graph.rs \
  crates/engine/src/runtime_graph_update.rs crates/engine/src/runtime_chain_state.rs crates/engine/src/runtime.rs \
  crates/engine/src/issue_980_pipeline_split_tests.rs crates/engine/tests/issue_980_one_worker_per_pipeline.rs \
  crates/engine/src/issue_953_route_latency_drift_tests.rs crates/engine/src/issue_965_insert_latency_tests.rs \
  crates/engine/tests/issue_716_bound_chain_builds.rs crates/engine/src/stream_isolation_same_device_tests.rs \
  crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs crates/infra-cpal/src/lib.rs \
  crates/infra-cpal/tests/build_chain_runtime.rs docs/audio-config.md docs/mcp.md \
  docs/superpowers/specs/2026-09-27-issue-980-pipeline-per-worker-design.md \
  docs/superpowers/plans/2026-09-27-issue-980-pipeline-per-worker.md
git -C $S commit -m "feat(#980): one runtime per (input x output) pipeline" -m "Group a chain's segments by (output route, raw input entry) with dense slots in segment order, so infra spawns one dsp-worker per pipeline. Split-mono siblings stay together; insert-cut chains and Linux/JACK keep their grouping for now (the new tests are gated off Linux+JACK until the engine's JACK key goes).

Old-topology tests adjusted: issue_953_route_latency_drift_tests (rig drives every pipeline), messages in issue_965_insert_latency_tests, issue_716_bound_chain_builds, build_chain_runtime.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 2: Stamp each runtime with its pipeline; refill it in place by that stamp

**Files:**
- Modify: `crates/engine/src/pipeline_grouping.rs` (whole file below)
- Modify: `crates/engine/src/runtime_segments.rs:29` (import), add `chain_segments` after `split_chain_into_segments` (after line 124)
- Modify: `crates/engine/src/runtime_chain_state.rs:19-22` (import), `:30-37` (field), `:228-235` (accessor); add `OwnedPipeline` and `pipeline_key()`
- Modify: `crates/engine/src/runtime_graph_assemble.rs:177-179`
- Modify: `crates/engine/src/runtime_graph.rs` (build loop of `build_per_input_runtimes`, tail of `input_group_ids`)
- Modify: `crates/engine/src/runtime_graph_update.rs` (the segments lookup edited in Task 1)
- Test: `crates/engine/src/issue_980_pipeline_split_tests.rs` (append)
- Modify docs: `docs/audio-config.md` (one paragraph after the Task 1 intro)

**Interfaces:**
- Consumes (Task 1): `PipelineKey`, `pipeline_key_of`, `first_seen`, `crate::runtime_graph::chain_has_insert_cut`.
- Produces:
  - `pub(crate) struct PipelineGroup { pub(crate) slot: usize, pub(crate) key: PipelineKey, pub(crate) segments: Vec<ChainSegment> }`
  - `pub(crate) fn group_segments_into_pipelines(chain: &Chain, registry: &[IoBinding], segments: Vec<ChainSegment>) -> Vec<PipelineGroup>`
  - `pub(crate) fn uncut_segments(chain: &Chain, registry: &[IoBinding]) -> Vec<ChainSegment>`
  - `pub(crate) fn pipeline_keys(segments: &[ChainSegment]) -> Vec<PipelineKey>`
  - `pub(crate) fn uncut_pipeline_keys(chain: &Chain, registry: &[IoBinding]) -> Vec<PipelineKey>`
  - `pub(crate) fn chain_segments(chain: &Chain, registry: &[IoBinding]) -> Vec<ChainSegment>` (in `runtime_segments`)
  - `pub(crate) struct OwnedPipeline { pub(crate) key: PipelineKey, pub(crate) cpal_input_index: usize }` (derive `Clone, Copy, Debug, PartialEq, Eq`)
  - `ChainRuntimeState::owned_pipeline: Option<OwnedPipeline>` (replaces `owned_entry: Option<(usize, usize)>`; only engine touches it)
  - `impl ChainRuntimeState { pub fn pipeline_key(&self) -> Option<PipelineKey> }`; `input_cpal_index()` keeps its signature and meaning.

- [ ] **Step 1: Write the failing tests**

Add to the `use` block at the top of `crates/engine/src/issue_980_pipeline_split_tests.rs`:

```rust
use crate::runtime_graph::{update_chain_runtime_state, PipelineKey};
```

Append to the same file:

```rust
fn main_only() -> Vec<IoEndpoint> {
    vec![endpoint("main", ChannelMode::Stereo, &MAIN)]
}

fn one_guitar_main_only() -> Vec<IoBinding> {
    binding(vec![guitar(0)], main_only())
}

fn key(route: usize, entry_group: usize) -> Option<PipelineKey> {
    Some(PipelineKey { route, entry_group })
}

/// RED — every pipeline runtime carries the pipeline it runs.
#[test]
fn every_runtime_is_stamped_with_the_pipeline_it_runs() {
    let runtimes = per_pipeline(&chain(vec![]), &one_guitar());
    let stamps: Vec<Option<PipelineKey>> = runtimes.iter().map(|rt| rt.pipeline_key()).collect();
    assert_eq!(
        stamps,
        vec![key(0, 0), key(1, 0)],
        "slot k runs pipeline k: (Main, guitar) then (Out 2, guitar)"
    );
    let whole = build_chain_runtime_state(&chain(vec![]), SR, &[TARGET, TARGET], &one_guitar())
        .expect("the whole chain builds");
    assert_eq!(whole.pipeline_key(), None, "a whole-chain runtime runs no single pipeline");
}

/// RED — #967: the loop switch refills the SAME runtime in place, so the stamp
/// of a runtime an insert cuts must not depend on whether the loop is engaged.
#[test]
fn a_runtime_an_insert_cuts_carries_the_first_pipeline_of_its_uncut_chain() {
    let registry = super::tests::insert_registry();
    let on = super::tests::insert_chain();
    let mut off = on.clone();
    for block in off.blocks.iter_mut() {
        if block.id.0 == "insert:0" {
            block.enabled = false;
        }
    }
    let stamps = |chain: &Chain| -> Vec<Option<PipelineKey>> {
        build_per_input_runtimes(chain, SR, &HashMap::new(), &[TARGET], &registry)
            .expect("the chain must build")
            .iter()
            .map(|(_, rt)| rt.pipeline_key())
            .collect()
    };
    assert_eq!(stamps(&off), vec![key(0, 0)], "loop off: one pipeline, the head into out0");
    assert_eq!(
        stamps(&on),
        stamps(&off),
        "loop on: the cut runtime keeps the stamp the loop-off runtime had"
    );
}

/// RED — an in-place refill finds the runtime's OWN pipeline by its stamp,
/// never by position: after a second guitar joins, (Out 2, guitar 1) moves
/// from slot 1 to slot 2, and slot 1 becomes (Main, guitar 2).
#[test]
fn an_in_place_update_refills_a_runtime_with_its_own_pipeline() {
    let chain = chain(vec![gain("amp")]);
    let out2 = Arc::clone(&per_pipeline(&chain, &one_guitar())[1]);
    update_chain_runtime_state(&out2, &chain, SR, false, &[TARGET, TARGET], &two_guitars())
        .expect("its pipeline still exists");
    assert_eq!(written(&out2), vec![1], "the Out 2 runtime keeps writing Out 2 only");
    assert_eq!(
        out2.input_routing_for_stream(0).map(|(_, _, channels)| channels),
        Some(vec![0]),
        "…fed by guitar 1 (ch 0), never guitar 2"
    );
    assert_eq!(out2.stream_count(), 1);
}

/// Pin — a runtime whose pipeline is gone refuses the in-place refill and
/// keeps playing what it had (the caller must rebuild instead).
#[test]
fn an_in_place_update_refuses_a_pipeline_that_is_gone() {
    let chain = chain(vec![gain("amp")]);
    let out2 = Arc::clone(&per_pipeline(&chain, &one_guitar())[1]);
    assert!(
        update_chain_runtime_state(&out2, &chain, SR, false, &[TARGET], &one_guitar_main_only())
            .is_err(),
        "Out 2 no longer exists: in place is not an option"
    );
    assert_eq!(written(&out2), vec![1], "the runtime is left untouched");
}
```

Step 1b — a seam so the tests compile, returning today's answer (no stamp). In `crates/engine/src/runtime_chain_state.rs`, inside `impl ChainRuntimeState`, right after `input_cpal_index`:

```rust
    /// #980: the pipeline this runtime runs; `None` for a whole-chain runtime.
    pub fn pipeline_key(&self) -> Option<crate::pipeline_grouping::PipelineKey> {
        None
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
(cd $S && cargo test -p engine --lib issue_980_pipeline_split)
```

Expected:
- `every_runtime_is_stamped_with_the_pipeline_it_runs` FAILS `left: [None, None]`, `right: [Some(PipelineKey { route: 0, entry_group: 0 }), Some(PipelineKey { route: 1, entry_group: 0 })]`.
- `a_runtime_an_insert_cuts_carries_the_first_pipeline_of_its_uncut_chain` FAILS `left: [None]`.
- `an_in_place_update_refills_a_runtime_with_its_own_pipeline` FAILS `left: [0], right: [1]` (slot lookup refilled slot 1 with (Main, guitar 2)).
- `an_in_place_update_refuses_a_pipeline_that_is_gone` PASSES (pin).

- [ ] **Step 3: Implement**

`crates/engine/src/runtime_segments.rs` — line 29 becomes:

```rust
use crate::runtime_endpoints::{
    effective_inputs, effective_outputs, resolve_chain_io, resolve_chain_io_by_binding,
    InputEntry, OutputEntry,
};
```

and add right after `split_chain_into_segments` ends (after line 124):

```rust

/// The segments of `chain` resolved against `registry` — the one call every
/// reader that needs a chain's pipelines without building it shares.
pub(crate) fn chain_segments(chain: &Chain, registry: &[IoBinding]) -> Vec<ChainSegment> {
    let (resolved_inputs, resolved_outputs) = resolve_chain_io(chain, registry);
    let (inputs, cpal_indices, split_positions, entry_groups) =
        effective_inputs(chain, &resolved_inputs, registry);
    let outputs = effective_outputs(chain, &resolved_outputs, registry);
    split_chain_into_segments(
        chain,
        &inputs,
        &cpal_indices,
        &split_positions,
        &entry_groups,
        &outputs,
        registry,
    )
}
```

Replace `crates/engine/src/pipeline_grouping.rs` with:

```rust
//! Responsibility: partitions a chain's segments into its isolated pipelines.
//!
//! #980 — the owner's LAW (CLAUDE.md): "if I have 1 input and 2 outputs I have
//! 2 streams"; N streams are N isolated pipelines, and isolation includes CPU
//! time. infra-cpal spawns one dsp-worker per runtime, so this grouping decides
//! how many realtime threads a chain's DSP runs on: one per pipeline, named by
//! the RAW input entry it starts from and the output route it ends at.
//! Split-mono siblings (one raw entry over N channels) share their pipeline,
//! so they still sum before the limiter once (volume invariants g02/g03).
//!
//! Setup-time only: nothing here runs on the audio thread.

use std::collections::HashMap;
use std::hash::Hash;

use domain::io_binding::IoBinding;
use project::block::AudioBlockKind;
use project::chain::Chain;

use crate::runtime_segments::ChainSegment;

/// One isolated pipeline of a chain: the output route it ends at and the RAW
/// input entry it starts from. Both are positions in the chain's resolved I/O,
/// so the same I/O and the same structure always name the same pipelines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PipelineKey {
    pub route: usize,
    pub entry_group: usize,
}

/// No I/O resolved at all: the one fallback pipeline.
const NO_PIPELINE: PipelineKey = PipelineKey {
    route: 0,
    entry_group: 0,
};

/// The segments one runtime runs.
pub(crate) struct PipelineGroup {
    /// Dense ordinal in segment order: the `RuntimeGraph` key and the
    /// infra-cpal slot id.
    pub(crate) slot: usize,
    /// The pipeline this runtime runs — stamped on it so an in-place update
    /// refills it with its own segments, never with whoever sits at its slot.
    pub(crate) key: PipelineKey,
    pub(crate) segments: Vec<ChainSegment>,
}

/// The pipeline a segment outside any insert cut runs in: such a segment
/// writes exactly one route (`runtime_segments::segments_without_inserts`).
pub(crate) fn pipeline_key_of(segment: &ChainSegment) -> PipelineKey {
    PipelineKey {
        route: segment.output_route_indices.first().copied().unwrap_or(0),
        entry_group: segment.entry_group,
    }
}

/// The pipelines `segments` run, in first-seen (slot) order.
pub(crate) fn pipeline_keys(segments: &[ChainSegment]) -> Vec<PipelineKey> {
    let mut keys: Vec<PipelineKey> = Vec::new();
    for segment in segments {
        let key = pipeline_key_of(segment);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

/// The segments `chain` has with every insert switched off — the pipelines it
/// runs whenever no insert cuts it (#967 keeps the route and input numbering
/// the same in both states).
pub(crate) fn uncut_segments(chain: &Chain, registry: &[IoBinding]) -> Vec<ChainSegment> {
    let mut uncut = chain.clone();
    for block in uncut.blocks.iter_mut() {
        if matches!(block.kind, AudioBlockKind::Insert(_)) {
            block.enabled = false;
        }
    }
    crate::runtime_segments::chain_segments(&uncut, registry)
}

/// The pipelines `chain` has when no insert cuts it, in slot order. A cutting
/// insert runs them as ONE runtime, stamped with the first of these, so
/// switching the insert refills that same runtime in place (#967).
pub(crate) fn uncut_pipeline_keys(chain: &Chain, registry: &[IoBinding]) -> Vec<PipelineKey> {
    pipeline_keys(&uncut_segments(chain, registry))
}

/// Partition a chain's segments into one [`PipelineGroup`] per runtime, in
/// slot order. Walking the runtimes by slot walks the streams in the order
/// `chain_stream_io_labels` names them.
///
/// An insert's cut is one pipeline across every head (send → gear → return →
/// tail), so the chain stays ONE runtime while an insert cuts it, stamped with
/// the first pipeline of the uncut chain.
///
/// Linux/JACK keeps the per-device grouping (slot = cpal index) behind its
/// cfg: the JACK-direct client still binds one runtime.
pub(crate) fn group_segments_into_pipelines(
    chain: &Chain,
    registry: &[IoBinding],
    segments: Vec<ChainSegment>,
) -> Vec<PipelineGroup> {
    if segments.is_empty() || crate::runtime_graph::chain_has_insert_cut(chain, registry) {
        let key = uncut_pipeline_keys(chain, registry)
            .first()
            .copied()
            .or_else(|| segments.first().map(pipeline_key_of))
            .unwrap_or(NO_PIPELINE);
        return vec![PipelineGroup {
            slot: 0,
            key,
            segments,
        }];
    }
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        return first_seen(segments, |segment| segment.cpal_input_index)
            .into_iter()
            .map(|(cpal, segments)| PipelineGroup {
                slot: cpal,
                key: segments.first().map(pipeline_key_of).unwrap_or(NO_PIPELINE),
                segments,
            })
            .collect();
    }
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        first_seen(segments, pipeline_key_of)
            .into_iter()
            .enumerate()
            .map(|(slot, (key, segments))| PipelineGroup {
                slot,
                key,
                segments,
            })
            .collect()
    }
}

/// Partition `segments` by `key_of`, in first-seen order of the keys, keeping
/// segment order inside each group.
fn first_seen<K: Copy + Eq + Hash>(
    segments: Vec<ChainSegment>,
    key_of: impl Fn(&ChainSegment) -> K,
) -> Vec<(K, Vec<ChainSegment>)> {
    let mut order: Vec<K> = Vec::new();
    let mut groups: HashMap<K, Vec<ChainSegment>> = HashMap::new();
    for segment in segments {
        let key = key_of(&segment);
        if !groups.contains_key(&key) {
            order.push(key);
        }
        groups.entry(key).or_default().push(segment);
    }
    order
        .into_iter()
        .map(|key| {
            let segments = groups.remove(&key).unwrap_or_default();
            (key, segments)
        })
        .collect()
}
```

`crates/engine/src/runtime_chain_state.rs`:
- Add to the imports (`:19-22`): `use crate::pipeline_grouping::PipelineKey;`
- Add right above `pub struct ChainRuntimeState`:

```rust
/// #980: which pipeline an isolated runtime runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OwnedPipeline {
    /// The pipeline (for a chain an insert cuts: the first pipeline of the
    /// uncut chain, `pipeline_grouping::uncut_pipeline_keys`).
    pub(crate) key: PipelineKey,
    /// The device stream that feeds it — SHARED with every other pipeline
    /// reading the same device.
    pub(crate) cpal_input_index: usize,
}
```

- Replace the `owned_entry` field and its doc (edited in Task 1) with:

```rust
    /// #703 / #980: the pipeline this runtime runs, stamped at graph assembly
    /// before the state is shared. `None` for whole-chain runtimes (probe,
    /// offline render).
    pub(crate) owned_pipeline: Option<OwnedPipeline>,
```

- Replace `input_cpal_index` (with its doc) and the Step 1b stub with:

```rust
    /// Issue #703 / #980: the cpal input-stream index that feeds this
    /// pipeline runtime. Every pipeline reading the same device has the SAME
    /// index — the cpal layer fans that device's single callback out to all of
    /// them. `None` for whole-chain runtimes (probe, offline render).
    pub fn input_cpal_index(&self) -> Option<usize> {
        self.owned_pipeline.map(|owned| owned.cpal_input_index)
    }

    /// #980: the pipeline this runtime runs; `None` for a whole-chain runtime.
    pub fn pipeline_key(&self) -> Option<PipelineKey> {
        self.owned_pipeline.map(|owned| owned.key)
    }
```

`crates/engine/src/runtime_graph_assemble.rs:177-179` becomes:

```rust
        // Whole-chain by default; `build_per_input_runtimes` stamps the
        // pipeline right after assembly (#703, #980).
        owned_pipeline: None,
```

`crates/engine/src/runtime_graph.rs` — in `build_per_input_runtimes`, replace everything from `let groups =` to `Ok(out)` with:

```rust
    let groups =
        crate::pipeline_grouping::group_segments_into_pipelines(chain, registry, all_segments);
    let switch_owned = switch_owned_routes(chain, registry, groups.len(), eff_outputs.len());
    let mut out = Vec::with_capacity(groups.len());
    for group in groups {
        // All segments of a pipeline share one effective input, hence one
        // cpal stream — record it so infra-cpal can fan a shared device
        // callback out to every pipeline runtime it feeds (#703, #980).
        let cpal_input_index = group
            .segments
            .first()
            .map(|s| s.cpal_input_index)
            .unwrap_or(0);
        // #736: this pipeline is one isolated input → one device → its OWN
        // rate. Empty/absent override falls back to the chain scalar
        // (bit-identical single-binding behaviour). Within a binding, input
        // rate == output rate is validated at resolve time, so the input
        // device's rate is the whole stream's rate.
        let group_rate = group
            .segments
            .first()
            .and_then(|s| device_rates.get(&s.input.device_id).copied())
            .unwrap_or(sample_rate);
        let mut state = assemble_chain_runtime_state(
            chain,
            &group.segments,
            &switch_owned,
            &eff_outputs,
            group_rate,
            device_rates,
            elastic_targets,
            None,
        )?;
        state.owned_pipeline = Some(crate::runtime_chain_state::OwnedPipeline {
            key: group.key,
            cpal_input_index,
        });
        out.push((group.slot, state));
    }
    Ok(out)
```

and in `input_group_ids` replace the final expression with:

```rust
    crate::pipeline_grouping::group_segments_into_pipelines(chain, registry, all_segments)
        .into_iter()
        .map(|group| group.slot)
        .collect()
```

`crates/engine/src/runtime_graph_update.rs` — replace the whole `let segments: Vec<ChainSegment> = match runtime.owned_entry { … };` statement and its comment (edited in Task 1) with:

```rust
    // #703 / #980: a pipeline runtime is refilled with ONLY its own pipeline's
    // segments, found by the pipeline it is stamped with — never by its slot:
    // slots are positions, and the pipeline at a position moves when a guitar
    // or an output joins. Refilling by position would feed this runtime's
    // output from another guitar, or run the same guitar twice on a shared
    // device callback (audible double volume). A whole-chain runtime
    // (`owned_pipeline == None`: probe, offline) keeps every segment.
    let segments: Vec<ChainSegment> = match runtime.owned_pipeline {
        Some(owned) => group_segments_into_pipelines(chain, registry, all_segments)
            .into_iter()
            .find(|group| group.key == owned.key)
            .map(|group| group.segments)
            .ok_or_else(|| {
                anyhow!(
                    "chain '{}' in-place update: pipeline {:?} no longer exists \
                     (a topology change must take the full-rebuild path)",
                    chain.id.0,
                    owned.key
                )
            })?,
        None => all_segments,
    };
```

`docs/audio-config.md` — add after the "Per-pipeline stream isolation" intro paragraph (before the first bullet):

```markdown
Each runtime is stamped with the pipeline it runs
(`ChainRuntimeState::pipeline_key`). An in-place edit refills a runtime by
that stamp, never by its slot: slots are positions, and adding a guitar or an
output moves the pipeline at a position. A runtime an insert cuts is stamped
with the first pipeline of the same chain with the insert off, so switching
the insert refills the same runtime (#967).
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
(cd $S && cargo test -p engine --lib -- issue_980_pipeline_split issue_967 issue_947 stream_isolation issue_923 insert)
```

Expected: all PASS (`issue_947_routes_owned_by_their_stream_tests` and `stream_isolation_same_device_tests::in_place_upsert_keeps_same_device_runtimes_entry_local` pin the refill). Then run the Gate.

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/engine/src/pipeline_grouping.rs crates/engine/src/runtime_segments.rs \
  crates/engine/src/runtime_chain_state.rs crates/engine/src/runtime_graph_assemble.rs \
  crates/engine/src/runtime_graph.rs crates/engine/src/runtime_graph_update.rs \
  crates/engine/src/issue_980_pipeline_split_tests.rs docs/audio-config.md
git -C $S commit -m "fix(#980): a pipeline runtime is refilled in place by the pipeline it runs" -m "Stamp every runtime with its PipelineKey (an insert-cut runtime with the first pipeline of its uncut chain) and look its segments up by that stamp instead of by slot position.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 3: Update in place only while the live runtimes run the chain's pipelines

**Files:**
- Modify: `crates/engine/src/runtime_graph.rs` (`input_group_ids` → add `pipeline_slots`)
- Modify: `crates/engine/src/runtime_graph_impl.rs:18` (import), `:24-51` (docs), `:95-170` (`upsert_chain_impl` in-place branch), `:196-199` (doc); add `runs_pipelines_of`
- Test: `crates/engine/src/issue_980_pipeline_split_tests.rs` (append)

**Interfaces:**
- Consumes (Task 2): `chain_segments`, `group_segments_into_pipelines -> Vec<PipelineGroup>`, `ChainRuntimeState::pipeline_key`.
- Produces: `pub fn pipeline_slots(chain: &Chain, registry: &[IoBinding]) -> Vec<(usize, PipelineKey)>` (in `engine::runtime_graph`); `impl RuntimeGraph { pub fn runs_pipelines_of(&self, chain: &Chain, registry: &[IoBinding]) -> bool }`. `input_group_ids` keeps its signature.

- [ ] **Step 1: Write the failing tests**

Append to `crates/engine/src/issue_980_pipeline_split_tests.rs`:

```rust
fn two_guitars_main_only() -> Vec<IoBinding> {
    binding(vec![guitar(0), guitar(1)], main_only())
}

/// RED — an E/S re-bind that keeps the runtime COUNT (1 in × 2 out → 2 in × 1
/// out) changes every pipeline: the upsert must rebuild them, not refill
/// slots whose pipeline is gone (which errors, or plays the wrong one).
#[test]
fn a_rebind_that_keeps_the_runtime_count_rebuilds_the_pipelines() {
    let chain = chain(vec![]);
    let mut graph = graph(&chain, &one_guitar());
    graph
        .upsert_chain(&chain, SR, &HashMap::new(), true, &[TARGET], &two_guitars_main_only())
        .expect("a re-bound E/S must rebuild its pipelines, not fail");
    let runtimes = graph.runtimes_with_groups_for(&chain.id);
    let stamps: Vec<(usize, Option<PipelineKey>)> = runtimes
        .iter()
        .map(|(slot, runtime)| (*slot, runtime.pipeline_key()))
        .collect();
    assert_eq!(
        stamps,
        vec![(0, key(0, 0)), (1, key(0, 1))],
        "two guitars into Main: two pipelines"
    );
    for (slot, runtime) in &runtimes {
        assert_eq!(written(runtime), vec![0], "slot {slot} writes Main only");
    }
}

/// Pin — a runtime with no pipeline stamp (whole-chain: probe, tests) still
/// takes the in-place path, so the callbacks' Arc sees the edit.
#[test]
fn an_edit_keeps_a_whole_chain_runtime_in_place() {
    let chain = chain(vec![gain("amp")]);
    let registry = one_guitar_main_only();
    let whole = Arc::new(
        build_chain_runtime_state(&chain, SR, &[TARGET], &registry).expect("the chain builds"),
    );
    let mut graph = RuntimeGraph {
        chains: HashMap::from([((chain.id.clone(), 0), Arc::clone(&whole))]),
    };
    let mut edited = chain.clone();
    edited.volume = 50.0;
    let live = graph
        .upsert_chain(&edited, SR, &HashMap::new(), false, &[TARGET], &registry)
        .expect("in-place edit");
    assert!(Arc::ptr_eq(&live, &whole), "the Arc the callbacks hold is kept");
    assert_eq!(whole.volume_pct(), 50.0, "…and it carries the edit");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
(cd $S && cargo test -p engine --lib issue_980_pipeline_split)
```

Expected:
- `a_rebind_that_keeps_the_runtime_count_rebuilds_the_pipelines` panics at `expect`: "a re-bound E/S must rebuild its pipelines, not fail: chain 'rig:input-7' in-place update: pipeline PipelineKey { route: 1, entry_group: 0 } no longer exists …" (the slot sets `{0,1}` match, so the in-place path ran).
- `an_edit_keeps_a_whole_chain_runtime_in_place` PASSES (pin).

- [ ] **Step 3: Implement**

`crates/engine/src/runtime_graph.rs` — replace the whole `input_group_ids` function (doc included) with:

```rust
/// #980: the `(slot, pipeline)` pairs a chain's runtimes will have, WITHOUT
/// instantiating any block processor — the same grouping the build uses.
/// Issue #588: an edit compares these against the live runtimes; building
/// runtimes for that reloaded every NAM/IR model from disk on each edit.
pub fn pipeline_slots(chain: &Chain, registry: &[IoBinding]) -> Vec<(usize, PipelineKey)> {
    crate::pipeline_grouping::group_segments_into_pipelines(
        chain,
        registry,
        crate::runtime_segments::chain_segments(chain, registry),
    )
    .into_iter()
    .map(|group| (group.slot, group.key))
    .collect()
}

/// The slot ids a chain's runtimes would get — one per pipeline (#980) — in
/// slot order. `chain_structure_signature` (infra-cpal) carries them: a change
/// of runtime count needs new streams.
pub fn input_group_ids(chain: &Chain, registry: &[IoBinding]) -> Vec<usize> {
    pipeline_slots(chain, registry)
        .into_iter()
        .map(|(slot, _)| slot)
        .collect()
}
```

`crates/engine/src/runtime_graph_impl.rs`:
- Line 18 becomes `use crate::runtime_graph::{build_per_input_runtimes, pipeline_slots, PipelineKey, RuntimeGraph};`
- Doc of `runtimes_for` (lines 24-27): `/// All per-input runtimes for a chain, ordered by group id (the cpal` / `/// input index). For single-input / Insert chains this is a one-element` becomes `/// All pipeline runtimes for a chain, in slot order (#980). For a` / `/// one-input, one-output chain or an insert-cut chain this is a one-element` (rest unchanged).
- Doc of `runtimes_with_groups_for` (lines 35-38): `keeps the group id (the cpal input index` / `/// the runtime owns) alongside each runtime, ordered by group.` becomes `keeps the slot id alongside each runtime, in slot order.` (keep the #350 sentence after it).
- Add inside `impl RuntimeGraph`, right after `runtimes_with_groups_for`:

```rust
    /// #980: whether the chain's live runtimes run exactly the pipelines
    /// `chain` has now — the same slots, each stamped with the pipeline that
    /// slot gets. A runtime with no stamp (whole-chain: probe, tests) matches
    /// whatever runs at its slot. Only then may an edit refill the runtimes in
    /// place: the streams hold them by slot, and each one refills itself by
    /// its stamp.
    pub fn runs_pipelines_of(&self, chain: &Chain, registry: &[IoBinding]) -> bool {
        let live: Vec<(usize, Option<PipelineKey>)> = self
            .runtimes_with_groups_for(&chain.id)
            .into_iter()
            .map(|(slot, runtime)| (slot, runtime.pipeline_key()))
            .collect();
        let mut next = pipeline_slots(chain, registry);
        next.sort_by_key(|(slot, _)| *slot);
        live.len() == next.len()
            && live
                .iter()
                .zip(&next)
                .all(|((slot, stamp), (next_slot, next_key))| {
                    slot == next_slot
                        && match stamp {
                            Some(key) => key == next_key,
                            None => true,
                        }
                })
    }
```

- In `upsert_chain_impl`, replace everything from `let existing_groups: Vec<usize> = self` down to the end of the `if !existing_groups.is_empty() { … }` block (lines 106-170) with:

```rust
        let mut existing_groups: Vec<usize> = self
            .chains
            .keys()
            .filter(|(cid, _)| cid == &chain.id)
            .map(|(_, g)| *g)
            .collect();
        existing_groups.sort_unstable();

        // Fast in-place rebuild path: the chain still has exactly the
        // pipelines its live runtimes run (#980: same slots, each stamped with
        // the same pipeline). Update every existing runtime in place so the
        // `Arc<ChainRuntimeState>` each live cpal callback captured stays
        // valid and observes the edit (volume, knob, block toggle).
        //
        // Issue #350 regression: the previous version only took this path
        // for single-input chains (`existing_groups.len() == 1`). For a
        // multi-input chain (e.g. 2 guitars on 2 devices) it fell through
        // to the full rebuild below, which drops the old Arcs and inserts
        // brand-new ones — but a volume/param edit does NOT rebuild the
        // cpal streams, so the callbacks kept the OLD Arcs and the edit
        // never reached the audio thread (slider did nothing).
        //
        // Issue #588: the comparison derives the pipelines WITHOUT building
        // runtimes (`pipeline_slots`) — building here reloaded every NAM/IR
        // model on each edit.
        if !existing_groups.is_empty() && self.runs_pipelines_of(chain, registry) {
            for group in &existing_groups {
                if let Some(runtime) = self.chains.get(&(chain.id.clone(), *group)) {
                    if spillover {
                        update_chain_runtime_state_spillover(
                            runtime,
                            chain,
                            sample_rate,
                            reset_output_queue,
                            elastic_targets,
                            registry,
                        )?;
                    } else {
                        // #967: new routes at their own device's rate.
                        update_chain_runtime_state_at_device_rates(
                            runtime,
                            chain,
                            device_rates,
                            reset_output_queue,
                            elastic_targets,
                            registry,
                        )?;
                    }
                }
            }
            if let Some(rt) = self.chains.get(&(chain.id.clone(), existing_groups[0])) {
                return Ok(rt.clone());
            }
        }
        // Pipelines changed (an input or output added/removed, an E/S
        // re-bound, a device swapped): full rebuild. The stream signature or
        // structure changed too, so the cpal streams WILL be rebuilt and will
        // capture the fresh Arcs.
```

(the full-rebuild block that follows, `// Full rebuild: drop every stale …` through `first.ok_or_else(…)`, is unchanged.)

- Doc of `runtime_for_chain` (lines 196-199): `/// First (lowest-group) per-input runtime for a chain.` becomes `/// The slot-0 pipeline runtime — the chain's first pipeline (first route × first input), where the latency probe is armed.` (keep the rest of the paragraph).

- [ ] **Step 4: Run the tests to verify they pass**

```bash
(cd $S && cargo test -p engine --lib -- issue_980_pipeline_split runtime_graph stream_isolation issue_736)
(cd $S && cargo test -p engine --test issue_588_no_model_reload_on_chain_edit)
```

Expected: all PASS (`issue_588_no_model_reload_on_chain_edit` pins no model reload on edit). Then run the Gate.

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/engine/src/runtime_graph.rs crates/engine/src/runtime_graph_impl.rs \
  crates/engine/src/issue_980_pipeline_split_tests.rs
git -C $S commit -m "fix(#980): update in place only while the live runtimes run the chain's pipelines" -m "RuntimeGraph::runs_pipelines_of compares (slot, pipeline) instead of slot ids, so a re-bind that keeps the runtime count rebuilds instead of failing mid-update.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 4: A stream teardown silences every pipeline of the chain

**Files:**
- Modify: `crates/infra-cpal/src/controller.rs:386-417` (`teardown_active_chain_for_rebuild`) and add `runtimes_silenced_by_teardown`
- Test: `crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs` (imports + `controller_with` + one test)

**Interfaces:**
- Consumes: `RuntimeGraph::runtimes_for(&self, &ChainId) -> Vec<Arc<ChainRuntimeState>>` (`runtime_graph_impl.rs:28`), `ChainRuntimeState::{set_draining, clear_draining}`, `ProjectRuntimeController::for_testing(RuntimeGraph) -> Self` (`controller.rs:149`).
- Produces: `impl ProjectRuntimeController { pub(crate) fn runtimes_silenced_by_teardown(&self, chain_id: &ChainId) -> Vec<Arc<ChainRuntimeState>> }`; test helper `fn controller_with(runtimes: &[(usize, Arc<ChainRuntimeState>)]) -> ProjectRuntimeController` (used by Tasks 5–7).

- [ ] **Step 1: Write the failing test**

In `crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs` change the two imports to:

```rust
use engine::runtime::{build_per_input_runtime_states, ChainRuntimeState, RuntimeGraph};
```

```rust
use crate::{build_chain_slots, LiveRuntimeSlot, ProjectRuntimeController};
```

and append:

```rust
/// A controller whose graph holds `runtimes` for `chain()`, one slot each.
fn controller_with(runtimes: &[(usize, Arc<ChainRuntimeState>)]) -> ProjectRuntimeController {
    let mut graph = RuntimeGraph {
        chains: HashMap::new(),
    };
    for (slot, runtime) in runtimes {
        graph
            .chains
            .insert((chain().id, *slot), Arc::clone(runtime));
    }
    ProjectRuntimeController::for_testing(graph)
}

/// RED — both pipelines of a guitar are drained before their streams go;
/// draining only the first let Out 2's DSP run mid-teardown.
#[test]
fn a_stream_teardown_silences_every_pipeline_of_the_chain() {
    let runtimes = pipelines(&registry(&[&[0, 1], &[10, 11]]));
    let controller = controller_with(&runtimes);
    let silenced = controller.runtimes_silenced_by_teardown(&chain().id);
    assert_eq!(silenced.len(), 2, "both pipelines are silenced");
    for ((_, live), silenced) in runtimes.iter().zip(&silenced) {
        assert!(Arc::ptr_eq(live, silenced));
    }
}
```

Step 1b — behaviour-preserving seam. In `crates/infra-cpal/src/controller.rs`, inside `impl ProjectRuntimeController`, right above `teardown_active_chain_for_rebuild`:

```rust
    /// #980: the runtimes a stream teardown silences before the streams go.
    pub(crate) fn runtimes_silenced_by_teardown(
        &self,
        chain_id: &ChainId,
    ) -> Vec<Arc<ChainRuntimeState>> {
        self.runtime_graph
            .runtime_for_chain(chain_id)
            .into_iter()
            .collect()
    }
```

and replace the body of `teardown_active_chain_for_rebuild` (keep its doc) with:

```rust
    pub(crate) fn teardown_active_chain_for_rebuild(&mut self, chain_id: &ChainId) {
        if !self.active_chains.contains_key(chain_id) {
            return;
        }
        let runtimes = self.runtimes_silenced_by_teardown(chain_id);
        for runtime in &runtimes {
            runtime.set_draining();
        }
        if !runtimes.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        self.active_chains.remove(chain_id);
        // The Arc<ChainRuntimeState> stays alive in `runtime_graph` and is
        // reused by the rebuild that follows. The new CPAL/JACK callbacks
        // call `process_input_f32`, which short-circuits on `is_draining()`
        // — so without this reset every rebuild after a signature change
        // (e.g. toggling an input channel) silences audio for every segment
        // on the chain, including sibling InputEntries that were not
        // touched, until the chain is fully removed and re-added.
        for runtime in &runtimes {
            runtime.clear_draining();
        }
    }
```

(Behaviour equals today's: one runtime drained.)

- [ ] **Step 2: Run the test to verify it fails**

```bash
(cd $S && cargo test -p infra-cpal --lib issue_980_pipeline_slots)
```

Expected: `a_stream_teardown_silences_every_pipeline_of_the_chain` FAILS `left: 1, right: 2` ("both pipelines are silenced").

- [ ] **Step 3: Implement**

Replace `runtimes_silenced_by_teardown` in `controller.rs` with:

```rust
    /// #980: the runtimes a stream teardown silences before the streams go —
    /// every pipeline of the chain. One guitar into two outputs is two
    /// runtimes; draining only the first let the second's DSP run while its
    /// streams were being torn down.
    pub(crate) fn runtimes_silenced_by_teardown(
        &self,
        chain_id: &ChainId,
    ) -> Vec<Arc<ChainRuntimeState>> {
        self.runtime_graph.runtimes_for(chain_id)
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
(cd $S && cargo test -p infra-cpal --lib -- issue_980_pipeline_slots controller)
```

Expected: PASS. Then run the Gate.

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/infra-cpal/src/controller.rs crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs
git -C $S commit -m "fix(#980): a stream teardown silences every pipeline of the chain" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 5: Installing a chain's slots retires the ones its old topology left

**Files:**
- Create: `crates/infra-cpal/src/chain_slot_install.rs`
- Modify: `crates/infra-cpal/src/lib.rs:89` (add `mod chain_slot_install;` after `mod chain_stream_registry;`)
- Modify: `crates/infra-cpal/src/controller_upsert.rs:177-181`
- Modify: `crates/infra-cpal/src/controller_rebuild_queue.rs:139-158`
- Test: `crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs` (append)

**Interfaces:**
- Consumes: `ControlWorker::submit<T, F>(&self, F) -> Receiver<T>` (`control_worker.rs:59`), `LiveRuntimeSlot::handle(&self) -> Self` (`live_runtime.rs:45`), `controller_with` (Task 4).
- Produces: `impl ProjectRuntimeController { pub(crate) fn install_chain_slots(&mut self, chain_id: &ChainId, slots: &[(usize, LiveRuntimeSlot)]) }`.

- [ ] **Step 1: Write the failing test**

Append to `crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs`:

```rust
/// RED — a topology with fewer pipelines must not leave a slot behind: it
/// keeps an old runtime alive for anything still reading it.
#[test]
fn installing_a_chains_slots_retires_the_slots_its_old_topology_left() {
    let mut controller = controller_with(&pipelines(&registry(&[&[0, 1], &[10, 11], &[12, 13]])));
    let two = pipelines(&registry(&[&[0, 1], &[10, 11]]));
    controller.install_chain_slots(&chain().id, &build_chain_slots(&two));
    let mut left: Vec<usize> = controller
        .chain_slots
        .keys()
        .filter(|(id, _)| id == &chain().id)
        .map(|(_, slot)| *slot)
        .collect();
    left.sort_unstable();
    assert_eq!(
        left,
        vec![0, 1],
        "slot 2 belonged to the three-output topology — it must be retired"
    );
}

/// RED — a slot replaced under the SAME key can hold the last reference to
/// the runtime a full rebuild superseded; it dies on the control worker,
/// never on the frontend thread (#934).
#[test]
fn a_slot_replaced_under_its_key_dies_on_the_control_worker() {
    let registry = registry(&[&[0, 1]]);
    let mut controller = controller_with(&[]);
    let old = pipelines(&registry);
    controller.install_chain_slots(&chain().id, &build_chain_slots(&old));
    let superseded = Arc::downgrade(&old[0].1);
    drop(old);
    // Park the (single, FIFO) worker: whatever is handed to it waits there.
    let (release, parked) = std::sync::mpsc::channel::<()>();
    let held = controller.worker.submit(move || parked.recv().ok());
    controller.install_chain_slots(&chain().id, &build_chain_slots(&pipelines(&registry)));
    assert!(
        superseded.upgrade().is_some(),
        "the replaced slot's runtime must not die on the frontend thread"
    );
    release.send(()).expect("the worker is parked");
    let _ = held.recv();
    let _ = controller.worker.submit(|| ()).recv();
    assert!(
        superseded.upgrade().is_none(),
        "…it dies on the control worker"
    );
}
```

Step 1b — behaviour-preserving seam. Create `crates/infra-cpal/src/chain_slot_install.rs` with today's behaviour (insert only):

```rust
//! Responsibility: installs the live slots a chain's new streams read.

use domain::ids::ChainId;

use crate::{LiveRuntimeSlot, ProjectRuntimeController};

impl ProjectRuntimeController {
    /// Install `slots` as the chain's live slot set (#672).
    pub(crate) fn install_chain_slots(
        &mut self,
        chain_id: &ChainId,
        slots: &[(usize, LiveRuntimeSlot)],
    ) {
        for (slot, handle) in slots {
            self.chain_slots
                .insert((chain_id.clone(), *slot), handle.handle());
        }
    }
}
```

Add to `crates/infra-cpal/src/lib.rs` right after line 89 (`mod chain_stream_registry;`):

```rust
mod chain_slot_install;
```

In `crates/infra-cpal/src/controller_upsert.rs` replace lines 178-181:

```rust
            for (group, slot) in &slots {
                self.chain_slots
                    .insert((chain.id.clone(), *group), slot.handle());
            }
```

with:

```rust
            self.install_chain_slots(&chain.id, &slots);
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
(cd $S && cargo test -p infra-cpal --lib issue_980_pipeline_slots)
```

Expected:
- `installing_a_chains_slots_retires_the_slots_its_old_topology_left` FAILS `left: [0, 1, 2], right: [0, 1]`.
- `a_slot_replaced_under_its_key_dies_on_the_control_worker` FAILS "the replaced slot's runtime must not die on the frontend thread" (`HashMap::insert` dropped the replaced handle — the last one — right there).

- [ ] **Step 3: Implement**

Replace `crates/infra-cpal/src/chain_slot_install.rs` with:

```rust
//! Responsibility: installs the live slots a chain's new streams read.

use domain::ids::ChainId;

use crate::{LiveRuntimeSlot, ProjectRuntimeController};

impl ProjectRuntimeController {
    /// Install `slots` as the chain's live slot set (#672: the streams built
    /// next read them; the control worker publishes rebuilt runtimes into
    /// them). #980: a topology with fewer pipelines leaves slots behind — each
    /// keeps an old runtime alive for anything still reading it (garbage that
    /// plays) — and a slot replaced under the same key can hold the last
    /// reference to the runtime a full rebuild superseded. Both are retired on
    /// the control worker (#934: a runtime never dies on the frontend thread).
    pub(crate) fn install_chain_slots(
        &mut self,
        chain_id: &ChainId,
        slots: &[(usize, LiveRuntimeSlot)],
    ) {
        let live: Vec<usize> = slots.iter().map(|(slot, _)| *slot).collect();
        let stale_keys: Vec<(ChainId, usize)> = self
            .chain_slots
            .keys()
            .filter(|(id, slot)| id == chain_id && !live.contains(slot))
            .cloned()
            .collect();
        let mut stale: Vec<LiveRuntimeSlot> = stale_keys
            .iter()
            .filter_map(|key| self.chain_slots.remove(key))
            .collect();
        for (slot, handle) in slots {
            if let Some(replaced) = self
                .chain_slots
                .insert((chain_id.clone(), *slot), handle.handle())
            {
                stale.push(replaced);
            }
        }
        if !stale.is_empty() {
            let _ = self.worker.submit(move || drop(stale));
        }
    }
}
```

In `crates/infra-cpal/src/controller_rebuild_queue.rs` (cold activation), replace lines 139-158 — from the `// Issue #703: install every per-entry runtime —` comment through the `for (group, slot) in &slots { … }` loop — with (same resulting slot set as today, but stale slots now die on the worker):

```rust
                    // #703 / #980: install every pipeline runtime of the chain,
                    // all fed by their device streams.
                    for (group, runtime) in &runtimes {
                        self.runtime_graph
                            .chains
                            .insert((chain_id.clone(), *group), Arc::clone(runtime));
                    }
                    let slots = crate::build_chain_slots(&runtimes);
                    // #881: this build OWNS the chain's slots now. A previous
                    // topology may have had more pipelines; the graph drops
                    // their runtimes and `install_chain_slots` retires their
                    // slots (on the worker, #934).
                    let live_groups: Vec<usize> = slots.iter().map(|(g, _)| *g).collect();
                    self.runtime_graph
                        .chains
                        .retain(|(id, g), _| id != &chain_id || live_groups.contains(g));
                    self.install_chain_slots(&chain_id, &slots);
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
(cd $S && cargo test -p infra-cpal --lib -- issue_980_pipeline_slots controller issue_881)
```

Expected: PASS. Then run the Gate.

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/infra-cpal/src/chain_slot_install.rs crates/infra-cpal/src/lib.rs \
  crates/infra-cpal/src/controller_upsert.rs crates/infra-cpal/src/controller_rebuild_queue.rs \
  crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs
git -C $S commit -m "fix(#980): installing a chain's slots retires the ones its old topology left" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 6: A finished rebuild lands only if every slot runs its own pipeline, all slots together

**Files:**
- Modify: `crates/infra-cpal/src/controller_rebuild_queue.rs:7-13` (import), `:77-104` (the publish loop of the `Ok(Ok(runtimes))` arm at `:75`); add `fn lands_in_its_slot` at the end of the file
- Test: `crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs` (append)

**Interfaces:**
- Consumes: `ChainRuntimeState::pipeline_key` (Task 2); `pending_rebuilds: Vec<(ChainId, Receiver<Result<Vec<(usize, Arc<ChainRuntimeState>)>>>)>` (`controller.rs:58-62`); `poll_pending_rebuilds(&mut self) -> usize`; `LiveRuntimeSlot::{load, publish, handle}`.
- Produces: private `fn lands_in_its_slot(live: &ChainRuntimeState, rebuilt: &ChainRuntimeState) -> bool` (an unstamped live runtime — whole-chain fixtures such as `crates/infra-cpal/tests/controller_schedule_rebuild.rs` — accepts any rebuild of its chain). A build lands whole or not at all: every slot is checked first, then every slot is published back to back (lock-free `ArcSwap` stores with nothing between them — #980 P3: a loop switch moves a tail between slots), then the graph and the drops follow.

- [ ] **Step 1: Write the failing test**

Append to `crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs`:

```rust
/// RED — a slot is bound to ONE pipeline (its output streams hold it by the
/// route that pipeline writes). A build whose runtime at that slot runs
/// another pipeline must never land there.
#[test]
fn a_rebuild_never_lands_in_another_pipelines_slot() {
    let registry = registry(&[&[0, 1], &[10, 11]]);
    let live = pipelines(&registry);
    let mut controller = controller_with(&live);
    let rebuilt = pipelines(&registry);
    let swapped = vec![
        (0, Arc::clone(&rebuilt[1].1)),
        (1, Arc::clone(&rebuilt[0].1)),
    ];
    let (tx, rx) = std::sync::mpsc::channel();
    tx.send(Ok(swapped)).expect("the channel is open");
    controller.pending_rebuilds.push((chain().id, rx));
    controller.poll_pending_rebuilds();
    for (slot, runtime) in &live {
        let held = controller.chain_slots[&(chain().id, *slot)].load();
        assert_eq!(
            held.pipeline_key(),
            runtime.pipeline_key(),
            "slot {slot} must keep running its own pipeline"
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
(cd $S && cargo test -p infra-cpal --lib issue_980_pipeline_slots)
```

Expected: `a_rebuild_never_lands_in_another_pipelines_slot` FAILS on slot 0: `left: Some(PipelineKey { route: 1, entry_group: 0 })`, `right: Some(PipelineKey { route: 0, entry_group: 0 })`.

- [ ] **Step 3: Implement**

`crates/infra-cpal/src/controller_rebuild_queue.rs` — add to the imports:

```rust
use engine::runtime::ChainRuntimeState;
```

Replace the publish loop (lines 77-104, from the `// Issue #703: publish each per-entry runtime into ITS` comment through the closing `}` of `for (group, runtime) in runtimes { … }`; the `self.streams.rebuild_settled(&chain_id);` line above it stays) with:

```rust
                    // #703 / #980: every pipeline runtime lands in ITS OWN
                    // (chain, slot) — and only if EVERY slot of the build runs
                    // the pipeline its live slot runs. A slot's output streams
                    // hold it by the route its pipeline writes, so a build made
                    // for another shape (the chain changed while it was
                    // building) would make an output play the wrong pipeline,
                    // and landing only the slots that fit would half-apply the
                    // edit. Such a build is dropped whole: the edit that moved
                    // the pipelines took the synchronous path, which rebuilds
                    // the streams (`request_offthread_rebuild_if_live`).
                    let key = |group: usize| (chain_id.clone(), group);
                    let slots: Vec<LiveRuntimeSlot> = runtimes
                        .iter()
                        .filter_map(|(group, runtime)| {
                            self.chain_slots
                                .get(&key(*group))
                                .filter(|slot| lands_in_its_slot(&slot.load(), runtime))
                                .map(LiveRuntimeSlot::handle)
                        })
                        .collect();
                    if slots.len() != runtimes.len() {
                        log::error!(
                            "chain '{}' rebuild does not fit the chain's live slots — dropped",
                            chain_id.0
                        );
                        // #934: never on the frontend thread.
                        let _ = self.worker.submit(move || drop(runtimes));
                        continue;
                    }
                    // #740: carry the live meter/spectrum/tuner taps over to
                    // every rebuilt runtime BEFORE any goes live, or the graph
                    // freezes after a preset switch / live edit (the UI's tap
                    // rings were subscribed on the old runtimes).
                    for (slot, (_, runtime)) in slots.iter().zip(&runtimes) {
                        runtime.adopt_taps_from(&slot.load());
                    }
                    // #980 P3: publish every slot back to back, nothing in
                    // between — a loop switch moves a tail between slots, so
                    // the chain's slots change together.
                    let superseded: Vec<Arc<ChainRuntimeState>> = slots
                        .iter()
                        .zip(&runtimes)
                        .map(|(slot, (_, runtime))| slot.publish(Arc::clone(runtime)))
                        .collect();
                    for (group, runtime) in runtimes {
                        self.runtime_graph.chains.insert(key(group), runtime);
                        applied += 1;
                    }
                    // Drop the old runtimes off the audio/frontend thread.
                    let _ = self.worker.submit(move || drop(superseded));
```

(`continue` moves to the next finished build: this arm sits inside `for (chain_id, rx) in std::mem::take(&mut self.pending_rebuilds)`, `controller_rebuild_queue.rs:73`. Add `use engine::runtime::ChainRuntimeState;` as above; `LiveRuntimeSlot` is already imported at `:13`.)

Append at the end of `controller_rebuild_queue.rs`:

```rust

/// #980: whether a rebuilt runtime may be published into a live slot — the
/// slot runs the same pipeline, or holds a whole-chain runtime (no stamp:
/// tests, legacy fixtures) that any rebuild of its chain replaces.
fn lands_in_its_slot(live: &ChainRuntimeState, rebuilt: &ChainRuntimeState) -> bool {
    match live.pipeline_key() {
        Some(key) => rebuilt.pipeline_key() == Some(key),
        None => true,
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
(cd $S && cargo test -p infra-cpal --lib -- issue_980_pipeline_slots controller issue_881 issue_957)
(cd $S && cargo test -p infra-cpal --test controller_schedule_rebuild)
```

Expected: PASS (`controller_schedule_rebuild` pins the unstamped-live rule; the #672/#740 rebuild tests — `controller_live_edit_replicates_user_report_tests`, `controller_per_stream_input_tap_tests::an_offthread_rebuild_keeps_the_graph_tap_alive`, the #881/#957 lifecycle tests — stay green: their rebuilds fit every live slot). A test that waits for a rebuild whose runtimes do NOT all fit the live slots (it used to land partially) is a real behaviour change: STOP and report it on the issue, do not loosen the all-or-nothing rule. Then run the Gate.

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/infra-cpal/src/controller_rebuild_queue.rs crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs
git -C $S commit -m "fix(#980): a finished rebuild lands whole, only where every slot runs its own pipeline" -m "Every slot is checked before any is published, then all are published back to back; a build that does not fit is dropped on the worker instead of half-applied.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 7: A live edit whose pipelines moved takes the synchronous path

**Files:**
- Modify: `crates/infra-cpal/src/controller_offthread_live_rebuild.rs:67-69` (insert after the `chain_io_changed` check)
- Test: `crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs` (imports + helper + test)

**Interfaces:**
- Consumes: `RuntimeGraph::runs_pipelines_of` (Task 3); `crate::io_topology::bound_io_signature(&Chain, &[IoBinding]) -> (Vec<(DeviceId, Vec<usize>)>, Vec<(DeviceId, Vec<usize>)>)` (`io_topology.rs:44`); `crate::resolved::{ChainStreamSignature, InputStreamSignature, OutputStreamSignature}`; `crate::active_runtime::ActiveChainRuntime` (cpal fields: `stream_signature`, `structure`, `generation`, `resolved`, `_input_streams`, `_output_streams`).
- Produces: nothing new (`request_offthread_rebuild_if_live` returns `Ok(false)` in one more case).

- [ ] **Step 1: Write the failing test**

Add to the imports of `crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs`:

```rust
use crate::active_runtime::ActiveChainRuntime;
use crate::resolved::{ChainStreamSignature, InputStreamSignature, OutputStreamSignature};
```

Append:

```rust
/// The signature streams opened for `registry` would have — so the live
/// edit sees NO I/O change.
fn live_signature(registry: &[IoBinding]) -> ChainStreamSignature {
    let (inputs, outputs) = crate::io_topology::bound_io_signature(&chain(), registry);
    ChainStreamSignature {
        inputs: inputs
            .into_iter()
            .map(|(device, channels)| InputStreamSignature {
                device_id: device.0,
                channels,
                stream_channels: 12,
                sample_rate: 48_000,
                buffer_size_frames: 64,
            })
            .collect(),
        outputs: outputs
            .into_iter()
            .map(|(device, channels)| OutputStreamSignature {
                device_id: device.0,
                channels,
                stream_channels: 12,
                sample_rate: 48_000,
                buffer_size_frames: 64,
            })
            .collect(),
    }
}

/// RED — the off-thread rebuild lands slot by slot; if the live runtimes no
/// longer run the chain's pipelines it would half-apply. It must decline, so
/// the caller takes the synchronous path.
#[test]
fn a_live_edit_whose_pipelines_moved_takes_the_synchronous_path() {
    let mut controller = controller_with(&pipelines(&registry(&[&[0, 1]])));
    controller.io_bindings = registry(&[&[0, 1], &[10, 11]]);
    controller.active_chains.insert(
        chain().id,
        ActiveChainRuntime {
            stream_signature: live_signature(&controller.io_bindings),
            structure: Vec::new(),
            generation: 1,
            resolved: None,
            _input_streams: Vec::new(),
            _output_streams: Vec::new(),
        },
    );
    let project = project::project::Project {
        name: None,
        chains: vec![chain()],
        device_settings: Vec::new(),
        midi: None,
    };
    let scheduled = controller
        .request_offthread_rebuild_if_live(&project, &chain())
        .expect("the check itself cannot fail");
    assert!(
        !scheduled,
        "one live pipeline, two pipelines now: no slot-by-slot swap"
    );
    assert!(controller.pending_rebuilds.is_empty());
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
(cd $S && cargo test -p infra-cpal --lib issue_980_pipeline_slots)
```

Expected: `a_live_edit_whose_pipelines_moved_takes_the_synchronous_path` FAILS "assertion failed: !scheduled — one live pipeline, two pipelines now: no slot-by-slot swap".

- [ ] **Step 3: Implement**

In `crates/infra-cpal/src/controller_offthread_live_rebuild.rs`, right after

```rust
        if self.chain_io_changed(project, chain)? {
            return Ok(false);
        }
```

insert:

```rust
        // #980: a rebuild lands slot by slot, and each slot's streams hold it by
        // the pipeline it runs. If the chain's pipelines no longer match the
        // live runtimes, a slot-by-slot swap (or the VST3 in-place update below)
        // would half-apply — take the synchronous path.
        if !self.runtime_graph.runs_pipelines_of(chain, &self.io_bindings) {
            return Ok(false);
        }
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
(cd $S && cargo test -p infra-cpal --lib -- issue_980_pipeline_slots controller_live_edit issue_85_live_rebuild)
```

Expected: PASS. Then run the Gate.

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/infra-cpal/src/controller_offthread_live_rebuild.rs crates/infra-cpal/src/issue_980_pipeline_slots_tests.rs
git -C $S commit -m "fix(#980): a live edit whose pipelines moved takes the synchronous path" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 8: Arm the DI once per output route, never per rate

**Files:**
- Modify: `crates/engine/src/runtime_chain_state.rs` (add `written_routes` right after `writes_output`)
- Modify: `crates/infra-cpal/src/controller_taps.rs:347-352` (doc of `set_chain_di_loop`), `:388-426` (`arm_di_loop_per_output_stream` + doc)
- Modify: `crates/infra-cpal/src/controller_taps_tests.rs:4-111` (imports, new RED test, fixture of `di_loop_reaches_every_output_rate_on_a_multirate_chain`)

**Interfaces:**
- Produces: `impl ChainRuntimeState { pub fn written_routes(&self) -> Vec<usize> }` (allocates — never on the audio thread).
- Signature unchanged: `pub(crate) fn arm_di_loop_per_output_stream(runtimes: &[Arc<ChainRuntimeState>], di: Option<Arc<DiPcm>>)`. (`set_chain_di_loop` has no production caller: test and HW-harness path only; production DI goes through `arm_di_stream`, untouched.)

- [ ] **Step 1: Write the failing test**

In `crates/infra-cpal/src/controller_taps_tests.rs`, module `di_loop_multirate_output_tests`, add to its imports:

```rust
    use engine::runtime::build_per_input_runtime_states;
    use std::collections::HashMap;
```

and add inside the module:

```rust
    /// RED #980 — one guitar into two outputs at ONE rate is two pipelines:
    /// two runtimes, each the only writer of its route. Arming per rate left
    /// the second output silent while the DI icon was lit.
    // Linux+JACK groups per device until the JACK-direct client runs one
    // pipeline per runtime; this gate goes with the engine's JACK key.
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn di_loop_reaches_both_outputs_of_one_guitar() {
        let chain = Chain {
            id: ChainId("one-guitar".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["io".into()],
            blocks: vec![],
            di_output: None,
            loopers: vec![],
        };
        let out = |name: &str, channels: Vec<usize>| IoEndpoint {
            name: name.into(),
            device_id: DeviceId("d".into()),
            mode: ChannelMode::Stereo,
            channels,
        };
        let registry = vec![IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId("d".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![out("main", vec![0, 1]), out("out 2", vec![2, 3])],
        }];
        let runtimes: Vec<_> = build_per_input_runtime_states(
            &chain,
            48_000.0,
            &HashMap::new(),
            &[256, 256],
            &registry,
        )
        .expect("the chain builds")
        .into_iter()
        .map(|(_, runtime)| runtime)
        .collect();
        assert_eq!(runtimes.len(), 2, "precondition #980: one runtime per pipeline");

        arm_di_loop_per_output_stream(
            &runtimes,
            Some(Arc::new(DiPcm::new(vec![0.5; 256], 48_000, 1))),
        );

        assert!(runtimes[0].has_di_loop(), "Main plays the DI");
        assert!(
            runtimes[1].has_di_loop(),
            "Out 2 plays the DI too — a route no armed runtime writes is silent"
        );
    }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
(cd $S && cargo test -p infra-cpal --lib di_loop)
```

Expected: `di_loop_reaches_both_outputs_of_one_guitar` FAILS "Out 2 plays the DI too — a route no armed runtime writes is silent" (only the first runtime of the rate was armed).

- [ ] **Step 3: Implement**

`crates/engine/src/runtime_chain_state.rs`, right after `writes_output`:

```rust
    /// #980: every chain output this runtime writes — one for a pipeline
    /// runtime, every route for a whole-chain one. Off the audio thread only
    /// (allocates).
    pub fn written_routes(&self) -> Vec<usize> {
        self.output_routes
            .load()
            .iter()
            .enumerate()
            .filter(|(_, route)| route.is_some())
            .map(|(index, _)| index)
            .collect()
    }
```

`crates/infra-cpal/src/controller_taps.rs` — replace the doc and body of `arm_di_loop_per_output_stream` (lines 388-426) with:

```rust
/// Arm a chain's DI loop exactly once per output route, resampled to the rate
/// of the runtime that plays it.
///
/// #980: a runtime runs ONE pipeline and writes ONE route (a chain an insert
/// cuts is one runtime writing all of them). The loop replaces the input, so
/// it plays in the first runtime writing each route and in no other: two
/// runtimes writing the same route (two guitars into one output) sum at that
/// output (#715 doubling), and a route no armed runtime writes is silent
/// (#749 "dead always" — and, since #980, the second output of one guitar).
/// The selector is the route the runtime owns, never its rate (LAW).
///
/// #749: the buffer is resampled HERE, to each armed runtime's rate, instead
/// of once to a single `engine_sr` in the loader. A 48 kHz-built loop played
/// on a 44.1 kHz output stretches (one loop frame per output frame) — the
/// owner's "está lento". The resample runs off the audio thread (this is the
/// enable/disable path, not a callback).
pub(crate) fn arm_di_loop_per_output_stream(
    runtimes: &[Arc<ChainRuntimeState>],
    di: Option<Arc<DiPcm>>,
) {
    let mut covered: Vec<usize> = Vec::new();
    for runtime in runtimes {
        let routes = runtime.written_routes();
        let reaches_a_new_route = routes.iter().any(|route| !covered.contains(route));
        match di.as_ref() {
            Some(pcm) if reaches_a_new_route => {
                covered.extend(routes);
                let rate = runtime.sample_rate();
                runtime.set_di_loop(Some(Arc::new(pcm.to_loop_at(rate as u32))));
            }
            _ => runtime.set_di_loop(None),
        }
    }
}
```

and in the doc of `set_chain_di_loop` (lines 347-352), `` /// `Some(pcm)` arms the chain's output streams with a loop resampled to `` / `/// EACH stream's own rate (#749) so the audio thread picks it up on the` becomes `` /// `Some(pcm)` arms the first runtime writing each output route with a loop `` / `/// resampled to that runtime's rate (#749, #980) so the audio thread picks it up on the` (rest unchanged).

Old-topology fixture fix in `controller_taps_tests.rs` (TESTS decision — the fixture built two whole-chain runtimes that both write route 0, which is not a multi-rate chain; the assertions at `:99-108` stay unchanged): in `di_loop_reaches_every_output_rate_on_a_multirate_chain` replace

```rust
        let e0 = runtime_at(44_100.0); // Scarlett input entry
        let e1 = runtime_at(48_000.0); // TEYUN input entry
```

with

```rust
        let (e0, e1) = scarlett_and_teyun();
```

and add to the module:

```rust
    /// #736 shape: Scarlett E/S @ 44.1 kHz and TEYUN E/S @ 48 kHz, each input
    /// into its own E/S's output — one pipeline runtime per E/S.
    fn scarlett_and_teyun() -> (
        Arc<engine::runtime::ChainRuntimeState>,
        Arc<engine::runtime::ChainRuntimeState>,
    ) {
        let endpoint = |device: &str, mode: ChannelMode, channels: Vec<usize>| IoEndpoint {
            name: device.into(),
            device_id: DeviceId(device.into()),
            mode,
            channels,
        };
        let binding = |id: &str| IoBinding {
            id: id.into(),
            name: id.into(),
            inputs: vec![endpoint(id, ChannelMode::Mono, vec![0])],
            outputs: vec![endpoint(id, ChannelMode::Stereo, vec![0, 1])],
        };
        let registry = vec![binding("scarlett"), binding("teyun")];
        let chain = Chain {
            id: ChainId("mr".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["scarlett".into(), "teyun".into()],
            blocks: vec![],
            di_output: None,
            loopers: vec![],
        };
        let rates = HashMap::from([
            (DeviceId("scarlett".into()), 44_100.0_f32),
            (DeviceId("teyun".into()), 48_000.0_f32),
        ]);
        let mut built =
            build_per_input_runtime_states(&chain, 44_100.0, &rates, &[256, 256], &registry)
                .expect("the two-E/S chain builds");
        assert_eq!(built.len(), 2, "one runtime per E/S");
        let (_, e1) = built.pop().expect("TEYUN runtime");
        let (_, e0) = built.pop().expect("Scarlett runtime");
        (e0, e1)
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
(cd $S && cargo test -p infra-cpal --lib di_loop)
(cd $S && cargo test -p infra-cpal --test '*' di)
```

Expected: PASS — `di_loop_does_not_double_same_rate_entries`, the doubling module and `clearing_disarms_every_runtime` stay green. Then run the Gate and `$LINUX cargo test -p infra-cpal --features jack --lib di_loop` (the gated test compiles out; `di_loop_reaches_every_output_rate_on_a_multirate_chain` builds two devices, so two runtimes under either grouping, and stays green).

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/engine/src/runtime_chain_state.rs crates/infra-cpal/src/controller_taps.rs \
  crates/infra-cpal/src/controller_taps_tests.rs
git -C $S commit -m "fix(#980): arm the DI once per output route, never per rate" -m "One guitar into two outputs is two pipeline runtimes at one rate; per-rate arming left the second output silent. Old-topology fixture adjusted: di_loop_reaches_every_output_rate_on_a_multirate_chain now builds the real two-E/S chain (each runtime writes its own route); its assertions are unchanged.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 9: A JACK ring slot carries the period it was written with

With one ring per pipeline the JACK callback writes N rings per period; today each write zero-fills `MAX_JACK_FRAMES × ports` floats and each read copies the whole slot. The slot's own length also replaces the shared `current_n_frames` atomic.

**Files:**
- Create: `crates/infra-cpal/src/jack_ring.rs` (the ring, moved out of `jack_handlers.rs:32-117`)
- Create: `crates/infra-cpal/src/jack_ring_tests.rs`
- Modify: `crates/infra-cpal/src/jack_handlers.rs:1-22` (module doc), `:28` (imports), `:32-117` (removed), `:159-165` (field `current_n_frames` removed), `:204-210` (its store removed)
- Modify: `crates/infra-cpal/src/jack_direct.rs:42` (import), `:209-211`, `:224`, `:236`, `:275-299`
- Modify: `crates/infra-cpal/src/lib.rs:49-50` (add `mod jack_ring;`)
- Modify docs: `docs/testing.md` (new subsection after the `## Workspace` block)

**Interfaces:**
- Produces: `pub(crate) struct SpscRingBuffer` in `crate::jack_ring` with `pub(crate) fn new(num_slots: usize, max_samples_per_slot: usize) -> Self`, `pub(crate) fn try_write(&self, samples: &[f32]) -> bool`, `pub(crate) fn try_read(&self, dst: &mut [f32]) -> Option<usize>` (was `-> bool`), `pub(crate) max_samples_per_slot: usize`.

- [ ] **Step 1: Write the failing test**

Step 0 — pure move, no behaviour change: cut `jack_handlers.rs:32-117` (the doc comment, `pub(crate) struct SpscRingBuffer`, its `unsafe impl`s and `impl`) verbatim into a new `crates/infra-cpal/src/jack_ring.rs` under the header of Step 3; in `jack_handlers.rs` add `use crate::jack_ring::SpscRingBuffer;`; in `jack_direct.rs:42` change `use crate::jack_handlers::{JackProcessHandler, JackShutdownHandler, SpscRingBuffer};` to `use crate::jack_handlers::{JackProcessHandler, JackShutdownHandler};` plus `use crate::jack_ring::SpscRingBuffer;`; in `crates/infra-cpal/src/lib.rs` add after line 50 (`mod jack_handlers;`):

```rust
#[cfg(all(target_os = "linux", feature = "jack"))]
mod jack_ring;
```

Create `crates/infra-cpal/src/jack_ring_tests.rs` and mount it at the end of `jack_ring.rs` with `#[cfg(test)] #[path = "jack_ring_tests.rs"] mod tests;`:

```rust
//! #980 — a JACK pipeline's ring moves one period, not its whole slot.
//!
//! With one ring per pipeline, the process callback writes one ring per
//! pipeline every period. A write zeroed the slot's tail up to
//! `MAX_JACK_FRAMES × channels` and a read copied the whole slot back, so the
//! real-time callback's cost grew with the slot size times the pipeline count.

use super::SpscRingBuffer;

#[test]
fn a_read_copies_only_the_period_it_was_written_with() {
    let ring = SpscRingBuffer::new(8, 16);
    assert!(ring.try_write(&[1.0, 2.0, 3.0, 4.0]));

    let mut dst = [9.0_f32; 16];
    let _ = ring.try_read(&mut dst);

    assert_eq!(&dst[..4], &[1.0, 2.0, 3.0, 4.0], "the period must arrive intact");
    assert!(
        dst[4..].iter().all(|s| *s == 9.0),
        "the read must copy only the period, not the slot's padding: {:?}",
        &dst[4..]
    );
}
```

(`let _ =` compiles against both the old `bool` and the new `Option<usize>`, so the RED is the assertion, not a type error.)

- [ ] **Step 2: Run the test to verify it fails (Linux only)**

```bash
$LINUX cargo test -p infra-cpal --features jack --lib jack_ring
```

Expected: `jack_ring::tests::a_read_copies_only_the_period_it_was_written_with` FAILS "the read must copy only the period, not the slot's padding: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]".

- [ ] **Step 3: Implement**

Replace `crates/infra-cpal/src/jack_ring.rs` with:

```rust
//! Responsibility: carries interleaved JACK periods from the process callback to one worker.
//!
//! Single-producer / single-consumer ring of fixed slots (moved out of
//! `jack_handlers.rs`, #980). The JACK real-time callback writes a period; one
//! DSP worker reads it. Power-of-two slot count for a free modulo.
//!
//! #980: a slot records how many samples the callback wrote. A read copies only
//! that period and reports its length, so the callback never zeroes a slot's
//! tail and the worker never processes padding — with one ring per pipeline the
//! callback writes one ring per pipeline every period.
//!
//! Inlining contract (issue #194 Phase 5): `try_write` runs on the JACK
//! real-time thread and stays `#[inline]`.

#![cfg(all(target_os = "linux", feature = "jack"))]

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) struct SpscRingBuffer {
    /// Flat storage: `num_slots * max_samples_per_slot` f32s.
    data: Vec<UnsafeCell<f32>>,
    /// Samples the producer wrote into each slot.
    lens: Vec<AtomicUsize>,
    /// How many f32 samples each slot holds at most.
    pub(crate) max_samples_per_slot: usize,
    /// Number of slots (power of 2 for fast modulo).
    num_slots: usize,
    /// Monotonically increasing write counter (slot index = write_pos % num_slots).
    write_pos: AtomicUsize,
    /// Monotonically increasing read counter.
    read_pos: AtomicUsize,
}

// One producer and one consumer; a slot's cells are handed over by the
// Release/Acquire pair on `write_pos` / `read_pos`, never touched by both.
unsafe impl Send for SpscRingBuffer {}
unsafe impl Sync for SpscRingBuffer {}

impl SpscRingBuffer {
    pub(crate) fn new(num_slots: usize, max_samples_per_slot: usize) -> Self {
        assert!(num_slots.is_power_of_two());
        let total = num_slots * max_samples_per_slot;
        let mut data = Vec::with_capacity(total);
        for _ in 0..total {
            data.push(UnsafeCell::new(0.0));
        }
        Self {
            data,
            lens: (0..num_slots).map(|_| AtomicUsize::new(0)).collect(),
            max_samples_per_slot,
            num_slots,
            write_pos: AtomicUsize::new(0),
            read_pos: AtomicUsize::new(0),
        }
    }

    /// Try to write `samples` into the next slot. Returns false if full.
    /// SAFETY: Only one thread may call this (producer).
    #[inline]
    pub(crate) fn try_write(&self, samples: &[f32]) -> bool {
        let wp = self.write_pos.load(Ordering::Relaxed);
        let rp = self.read_pos.load(Ordering::Acquire);
        if wp.wrapping_sub(rp) >= self.num_slots {
            return false; // full
        }
        let slot = wp & (self.num_slots - 1);
        let base = slot * self.max_samples_per_slot;
        let n = samples.len().min(self.max_samples_per_slot);
        for (i, sample) in samples.iter().take(n).enumerate() {
            unsafe {
                *self.data[base + i].get() = *sample;
            }
        }
        self.lens[slot].store(n, Ordering::Relaxed);
        self.write_pos.store(wp.wrapping_add(1), Ordering::Release);
        true
    }

    /// Try to read the next slot into `dst`: `Some(len)` with the period's
    /// sample count, `None` when empty. Copies only the period.
    /// SAFETY: Only one thread may call this (consumer).
    #[inline]
    pub(crate) fn try_read(&self, dst: &mut [f32]) -> Option<usize> {
        let rp = self.read_pos.load(Ordering::Relaxed);
        let wp = self.write_pos.load(Ordering::Acquire);
        if rp == wp {
            return None; // empty
        }
        let slot = rp & (self.num_slots - 1);
        let base = slot * self.max_samples_per_slot;
        let n = self.lens[slot].load(Ordering::Relaxed).min(dst.len());
        for (i, out) in dst.iter_mut().take(n).enumerate() {
            *out = unsafe { *self.data[base + i].get() };
        }
        self.read_pos.store(rp.wrapping_add(1), Ordering::Release);
        Some(n)
    }
}

#[cfg(test)]
#[path = "jack_ring_tests.rs"]
mod tests;
```

Tighten the test's read line (the RED already failed on the padding assertion):

```rust
    assert_eq!(ring.try_read(&mut dst), Some(4), "the read must report the period's length");
```

replacing `let _ = ring.try_read(&mut dst);`.

`crates/infra-cpal/src/jack_direct.rs` — the worker slices by the slot's length; the shared atomic goes:
- Delete lines 209-211 (`// Seed with current buffer size …` and `let current_n_frames = Arc::new(…);`).
- Delete line 224 (`current_n_frames: Arc::clone(&current_n_frames),` in the handler literal).
- Delete line 236 (`let worker_current_frames = Arc::clone(&current_n_frames);`).
- Replace lines 275-299 (from `// Process all available buffers.` through `processed_any = true;` and its closing `}`) with:

```rust
                // #980: a slot carries the length the callback wrote — the
                // period jackd delivered, even across a live buffer resize —
                // so the engine never processes ring padding.
                let mut processed_any = false;
                while let Some(len) = worker_ring.try_read(&mut read_buf) {
                    let n_frames = len / worker_channels.max(1);
                    let real = &read_buf[..len];
                    let callback_start = std::time::Instant::now();
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_f32(&worker_runtime, 0, real, worker_channels);
                    }));
                    crate::callback_load_timing::record_callback_deadline(
                        &worker_runtime,
                        callback_start.elapsed(),
                        n_frames,
                        worker_sample_rate,
                    );
                    processed_any = true;
                }
```

`crates/infra-cpal/src/jack_handlers.rs`:
- Delete the `current_n_frames` field and its doc (lines 159-165) and its store with its comment (lines 204-210: from `// Publish the current callback size …` through `.store(n_frames, std::sync::atomic::Ordering::Relaxed);`).
- Replace the module doc (lines 1-22) with:

```rust
//! Responsibility: serves the JACK process callback.
//! Audio-thread JACK handlers.
//!
//! Everything here is on the JACK real-time path:
//!
//! - `JackShutdownHandler` — replaces libjack's default `()` notification
//!   handler so a USB unplug / jackd crash sets an atomic flag instead
//!   of calling `std::process::exit(0)`.
//! - `JackProcessHandler` — the RT process callback itself. Pins itself
//!   to big cores on first invocation, copies port data in/out, hands
//!   the input block to the worker through its ring (`jack_ring.rs`).
//!
//! Inlining contract (issue #194 Phase 5): every helper that
//! `JackProcessHandler::process` reaches across this module boundary —
//! `SpscRingBuffer::try_write`, `pin_thread_to_cpus`, `detect_big_cores`
//! — is `#[inline]`. The audio thread cannot pay an extra call/jump
//! because of refactor.
```

`docs/testing.md` — add right after the `## Workspace` block (after its `(~1100+ testes)` line):

````markdown
### Linux + JACK tests from macOS (#980)

Code under `#[cfg(all(target_os = "linux", feature = "jack"))]` — the
JACK-direct client (`crates/infra-cpal/src/jack_*.rs`) and its tests — does not
compile on macOS. CI compiles it (`cargo test --workspace` on ubuntu turns
`infra-cpal/jack` on through adapter-gui), but CI runs only on pull requests
into `develop` / `release/**` / `main`. Before pushing JACK work, run it in the
Linux builder image; no jackd is needed and none of these tests opens a JACK
client:

```bash
docker build --platform linux/arm64 --build-arg "RUSTFLAGS_CPU=-C target-cpu=cortex-a53" \
  -t openrig-linux-builder:aarch64 -f docker/Dockerfile.linux-builder docker   # once
mkdir -p target/linux-cargo-registry
docker run --rm --platform linux/arm64 -v "$PWD:/workspace" \
  -v "$PWD/target/linux-cargo-registry:/root/.cargo/registry" \
  -e CARGO_TARGET_DIR=/workspace/target/linux-docker \
  openrig-linux-builder:aarch64 cargo test -p infra-cpal --features jack --lib jack_
```

Both the registry cache and `CARGO_TARGET_DIR` stay under the git-ignored
`target/`, out of the macOS build cache.
````

- [ ] **Step 4: Run the tests to verify they pass**

```bash
$LINUX cargo test -p infra-cpal --features jack --lib jack_ring   # 1 passed
$LINUX cargo build -p infra-cpal --features jack                  # zero warnings
```

Then run the Gate on macOS (unchanged there).

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/infra-cpal/src/jack_ring.rs crates/infra-cpal/src/jack_ring_tests.rs \
  crates/infra-cpal/src/jack_handlers.rs crates/infra-cpal/src/jack_direct.rs crates/infra-cpal/src/lib.rs \
  docs/testing.md
git -C $S commit -m "fix(#980): a JACK ring slot carries the period it was written with" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push (issue comment includes the `$LINUX` lines).

---

### Task 10: Split the JACK client into period, pipeline and worker units (no behaviour change)

Sanctioned split (CLAUDE.md "UM ARQUIVO": move code, change no behaviour, rewrite no test). It is the seam that makes Tasks 11–12 testable without jackd. Faithful to today: **first slot only, route 0 only, one worker.** The only non-move edits, both no-ops in production: (a) the handler and the worker read the runtime through its `LiveRuntimeSlot` (nothing publishes on JACK today: the only `publish` is `controller_rebuild_queue.rs:91`, reached only from the cpal path); (b) the dead inline fallback (`input_ring: None`, `jack_handlers.rs:237-242`) goes — `jack_direct.rs:222` always sets `Some`.

**Files:**
- Create: `crates/infra-cpal/src/jack_pipeline.rs`, `crates/infra-cpal/src/jack_period.rs`, `crates/infra-cpal/src/jack_dsp_worker.rs`
- Modify: `crates/infra-cpal/src/jack_handlers.rs` (handler struct + `impl jack::ProcessHandler`, imports, module doc)
- Modify: `crates/infra-cpal/src/jack_direct.rs:1-26` (doc), `:30-45` (imports), `:47-56` (signature), `:199-366` (body from the ring setup to the end)
- Modify: `crates/infra-cpal/src/stream_builder.rs:158-182` (JACK branch hands every slot)
- Modify: `crates/infra-cpal/src/active_runtime.rs:68-70` (`_dsp_worker: Option<Vec<DspWorkerHandle>>`; `Option` kept so the five fixtures writing `_dsp_worker: None` stay untouched)
- Modify: `crates/infra-cpal/src/lib.rs` (three `mod` lines, JACK cfg)

**Interfaces:**
- Consumes: `LiveRuntimeSlot::{handle, load}`, `crate::process_output_buffer(slots: &[LiveRuntimeSlot], loaded: &mut Vec<Arc<ChainRuntimeState>>, output_index: usize, out: &mut [f32], output_total_channels: usize, scratch: &mut [f32])` (`slot_processing.rs:113`), `crate::di_playback::{mix_di_playback, DiPlaybackCell}`, `crate::resolved::MAX_JACK_FRAMES`, `SpscRingBuffer` (Task 9), `engine::runtime::{process_input_f32, ChainRuntimeState}`, `crate::callback_load_timing::record_callback_deadline(&ChainRuntimeState, Duration, usize, u32)`, `crate::cpu_affinity::{detect_big_cores, pin_thread_to_cpus}`, `crate::active_runtime::DspWorkerHandle { stop_flag, wake, thread }`.
- Produces:
  - `pub(crate) struct JackPipeline { pub(crate) slot: LiveRuntimeSlot, pub(crate) ring: Arc<SpscRingBuffer>, pub(crate) wake: Arc<(Mutex<bool>, Condvar)> }`, `JackPipeline::new(slot: LiveRuntimeSlot, samples_per_buffer: usize) -> Self`, `JackPipeline::hand_off(&self, interleaved: &[f32])`
  - `pub(crate) struct JackPeriod`, `JackPeriod::new(slots: Vec<LiveRuntimeSlot>, _route_channels: Vec<Vec<usize>>, in_channels: usize, out_channels: usize, di_cells: Vec<DiPlaybackCell>) -> Self` (`route_channels[r]` = the device channels chain output route `r` writes; unused until Task 11), `pipelines(&self) -> &[JackPipeline]`, `input_frames(&mut self, n_frames: usize, channels: usize) -> &mut [f32]`, `hand_off_input(&self, n_frames: usize, channels: usize)`, `render_output(&mut self, n_frames: usize, channels: usize) -> &[f32]`
  - `pub(crate) fn spawn_pipeline_workers(chain_label: &str, pipelines: &[JackPipeline], channels: usize, sample_rate: u32) -> anyhow::Result<Vec<DspWorkerHandle>>`
  - `pub(crate) fn build_jack_direct_chain(chain_id: &ChainId, chain: &Chain, slots: Vec<LiveRuntimeSlot>, registry: &[IoBinding], di_cells: Vec<DiPlaybackCell>) -> Result<(jack::AsyncClient<JackShutdownHandler, JackProcessHandler>, Vec<DspWorkerHandle>)>`

- [ ] **Step 1: No new test** — a pure split. Record the green baseline instead:

```bash
$LINUX cargo test -p infra-cpal --features jack --lib 2>&1 | tail -3   # note the "test result" line
```

- [ ] **Step 2: Confirm the baseline builds warning-free**

```bash
$LINUX cargo build -p infra-cpal --features jack   # zero warnings
```

- [ ] **Step 3: Implement**

Create `crates/infra-cpal/src/jack_pipeline.rs`:

```rust
//! Responsibility: holds one JACK pipeline's private hand-off to its worker.
//!
//! #980: a chain runs one runtime per (input × output) pipeline. On Linux/JACK
//! each pipeline gets its own ring, its own wake pair and its own dsp-worker,
//! so a heavy pipeline never makes another one late. The process callback is
//! the only thing the pipelines of one device share — the device's period.

#![cfg(all(target_os = "linux", feature = "jack"))]

use std::sync::{Arc, Condvar, Mutex};

use crate::jack_ring::SpscRingBuffer;
use crate::LiveRuntimeSlot;

/// Ring slots per pipeline: headroom for the callback to write while the
/// worker processes (the JACK path's value since #294).
const RING_SLOTS: usize = 8;

pub(crate) struct JackPipeline {
    /// The runtime this pipeline processes, read live every buffer (#672).
    pub(crate) slot: LiveRuntimeSlot,
    /// Interleaved periods from the process callback to this pipeline's worker.
    pub(crate) ring: Arc<SpscRingBuffer>,
    /// Wakes this pipeline's worker when a period lands.
    pub(crate) wake: Arc<(Mutex<bool>, Condvar)>,
}

impl JackPipeline {
    /// `samples_per_buffer` is the largest period the ring holds
    /// (`MAX_JACK_FRAMES × input ports`), so a live buffer resize never
    /// reallocates on the audio thread.
    pub(crate) fn new(slot: LiveRuntimeSlot, samples_per_buffer: usize) -> Self {
        Self {
            slot,
            ring: Arc::new(SpscRingBuffer::new(RING_SLOTS, samples_per_buffer)),
            wake: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }

    /// Audio thread: hand one interleaved period to this pipeline's worker —
    /// a bounded copy, a `try_lock` of the wake flag (never waits: a worker
    /// holding it is about to read the ring anyway) and one `notify_one` (a
    /// futex wake — one syscall per pipeline per period, spec R4).
    #[inline]
    pub(crate) fn hand_off(&self, interleaved: &[f32]) {
        let _ = self.ring.try_write(interleaved);
        if let Ok(mut flag) = self.wake.0.try_lock() {
            *flag = true;
        }
        self.wake.1.notify_one();
    }
}
```

Create `crates/infra-cpal/src/jack_period.rs` (faithful shape — first slot, route 0):

```rust
//! Responsibility: moves one JACK period through the chain's pipelines.
//!
//! The JACK process callback (`jack_handlers.rs`) only copies port data; what
//! happens to a period between the ports lives here, over plain buffers, so it
//! is tested without a JACK server. The `#[inline]` methods run on the JACK
//! real-time thread: every buffer is sized for `MAX_JACK_FRAMES` up front, no
//! allocation, no blocking lock, no I/O. Handing a period to a pipeline takes
//! its wake flag with `try_lock` (never waits) and wakes its worker with one
//! `notify_one` — a futex syscall per pipeline per period, the hand-off the
//! single JACK worker already had.

#![cfg(all(target_os = "linux", feature = "jack"))]

use std::sync::Arc;

use engine::runtime::ChainRuntimeState;

use crate::di_playback::{mix_di_playback, DiPlaybackCell};
use crate::jack_pipeline::JackPipeline;
use crate::resolved::MAX_JACK_FRAMES;
use crate::{process_output_buffer, LiveRuntimeSlot};

pub(crate) struct JackPeriod {
    pipelines: Vec<JackPipeline>,
    /// Per chain output route, the pipelines that own it.
    route_owners: Vec<Vec<LiveRuntimeSlot>>,
    input_buf: Vec<f32>,
    output_buf: Vec<f32>,
    route_buf: Vec<f32>,
    mix_scratch: Vec<f32>,
    /// Reused by `process_output_buffer`; its capacity is the pipeline count,
    /// so the audio thread never grows it.
    loaded: Vec<Arc<ChainRuntimeState>>,
    /// #771: every DI cell of the chain — JACK-direct runs one output stream.
    di_cells: Vec<DiPlaybackCell>,
}

impl JackPeriod {
    pub(crate) fn new(
        slots: Vec<LiveRuntimeSlot>,
        _route_channels: Vec<Vec<usize>>,
        in_channels: usize,
        out_channels: usize,
        di_cells: Vec<DiPlaybackCell>,
    ) -> Self {
        // The JACK-direct client binds the chain's first runtime and plays its
        // route 0.
        let first: Vec<LiveRuntimeSlot> = slots.into_iter().take(1).collect();
        let route_owners = vec![first.iter().map(LiveRuntimeSlot::handle).collect()];
        let pipelines: Vec<JackPipeline> = first
            .into_iter()
            .map(|slot| JackPipeline::new(slot, MAX_JACK_FRAMES * in_channels))
            .collect();
        let out_len = MAX_JACK_FRAMES * out_channels.max(1);
        Self {
            loaded: Vec::with_capacity(pipelines.len()),
            pipelines,
            route_owners,
            input_buf: vec![0.0; MAX_JACK_FRAMES * in_channels.max(1)],
            output_buf: vec![0.0; out_len],
            route_buf: vec![0.0; out_len],
            mix_scratch: vec![0.0; out_len],
            di_cells,
        }
    }

    pub(crate) fn pipelines(&self) -> &[JackPipeline] {
        &self.pipelines
    }

    /// Audio thread: the interleaved buffer one period of `n_frames` over
    /// `channels` capture ports is copied into.
    #[inline]
    pub(crate) fn input_frames(&mut self, n_frames: usize, channels: usize) -> &mut [f32] {
        let needed = n_frames * channels;
        if self.input_buf.len() < needed {
            self.input_buf.resize(needed, 0.0);
        }
        &mut self.input_buf[..needed]
    }

    /// Audio thread: hand the period `input_frames` holds to every pipeline.
    #[inline]
    pub(crate) fn hand_off_input(&self, n_frames: usize, channels: usize) {
        let period = &self.input_buf[..n_frames * channels];
        for pipeline in &self.pipelines {
            pipeline.hand_off(period);
        }
    }

    /// Audio thread: one period of interleaved output — every served route
    /// popped from the pipelines that own it, then the chain's DI.
    #[inline]
    pub(crate) fn render_output(&mut self, n_frames: usize, channels: usize) -> &[f32] {
        let needed = n_frames * channels;
        for buf in [&mut self.output_buf, &mut self.route_buf, &mut self.mix_scratch] {
            if buf.len() < needed {
                buf.resize(needed, 0.0);
            }
        }
        let Self {
            route_owners,
            output_buf,
            route_buf,
            mix_scratch,
            loaded,
            di_cells,
            ..
        } = self;
        let out = &mut output_buf[..needed];
        out.fill(0.0);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for (route, owners) in route_owners.iter().enumerate() {
                if owners.is_empty() {
                    continue;
                }
                let buf = &mut route_buf[..needed];
                process_output_buffer(
                    owners,
                    loaded,
                    route,
                    buf,
                    channels,
                    &mut mix_scratch[..needed],
                );
                // Each route writes only its own device channels; `out` is
                // their union.
                for (dst, src) in out.iter_mut().zip(buf.iter()) {
                    *dst += *src;
                }
            }
            for cell in di_cells.iter() {
                mix_di_playback(cell, out, channels);
            }
        }));
        &output_buf[..needed]
    }
}
```

Create `crates/infra-cpal/src/jack_dsp_worker.rs` (the loop of `jack_direct.rs:240-329`, reading through the slot):

```rust
//! Responsibility: runs one JACK pipeline's DSP off the process callback.
//!
//! Moved out of `jack_direct.rs` (#980). The process callback only copies the
//! period into the pipeline's ring; this worker drains it and runs the chain
//! DSP, pinned to the big cores (A76 on the RK3588) at nice -10.

#![cfg(all(target_os = "linux", feature = "jack"))]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use engine::runtime::process_input_f32;

use crate::active_runtime::DspWorkerHandle;
use crate::cpu_affinity::{detect_big_cores, pin_thread_to_cpus};
use crate::jack_pipeline::JackPipeline;

/// Spawn the dsp-workers of one JACK client's pipelines.
pub(crate) fn spawn_pipeline_workers(
    chain_label: &str,
    pipelines: &[JackPipeline],
    channels: usize,
    sample_rate: u32,
) -> Result<Vec<DspWorkerHandle>> {
    // The JACK-direct client runs one worker, on its first pipeline.
    pipelines
        .iter()
        .take(1)
        .enumerate()
        .map(|(k, pipeline)| {
            spawn_pipeline_worker(format!("{chain_label}:{k}"), pipeline, channels, sample_rate)
        })
        .collect()
}

fn spawn_pipeline_worker(
    label: String,
    pipeline: &JackPipeline,
    channels: usize,
    sample_rate: u32,
) -> Result<DspWorkerHandle> {
    let slot = pipeline.slot.handle();
    let ring = Arc::clone(&pipeline.ring);
    let wake = Arc::clone(&pipeline.wake);
    let worker_wake = Arc::clone(&pipeline.wake);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop_flag);
    let samples_per_buffer = ring.max_samples_per_slot;
    let thread = std::thread::Builder::new()
        .name(format!("dsp-worker-{label}"))
        .spawn(move || {
            // Pin to big cores (A76 on RK3588)
            let big_cores = detect_big_cores();
            if !big_cores.is_empty() {
                pin_thread_to_cpus(&big_cores);
                log::info!("DSP worker '{}': pinned to cores {:?}", label, big_cores);
            }
            // Set high priority (not RT, but high normal)
            unsafe {
                let param = libc::sched_param { sched_priority: 0 };
                libc::sched_setscheduler(0, libc::SCHED_OTHER, &param);
                // Use nice -10 for higher scheduling priority
                libc::setpriority(libc::PRIO_PROCESS, 0, -10);
            }
            let mut read_buf = vec![0.0f32; samples_per_buffer];
            log::info!("DSP worker '{}': started (channels={})", label, channels);
            loop {
                if worker_stop.load(Ordering::Acquire) {
                    break;
                }
                let mut processed_any = false;
                while let Some(len) = ring.try_read(&mut read_buf) {
                    let runtime = slot.load();
                    let callback_start = std::time::Instant::now();
                    // The period is the capture ports of the ONE JACK server
                    // this client joined — the device of the chain's first
                    // input, cpal index 0 — so it is dispatched at 0, as the
                    // single JACK worker always did. A pipeline of another
                    // device is not fed by this client (it returns before any
                    // lock, `input_is_fed`); multi-device JACK chains are out
                    // of #980's scope.
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_f32(&runtime, 0, &read_buf[..len], channels);
                    }));
                    crate::callback_load_timing::record_callback_deadline(
                        &runtime,
                        callback_start.elapsed(),
                        len / channels.max(1),
                        sample_rate,
                    );
                    processed_any = true;
                }
                if !processed_any {
                    // Wait for wake signal. Must check the flag before
                    // waiting: Condvar::notify_one() with no waiter is LOST
                    // (POSIX semantics), so if jackd wrote to the ring AND
                    // notified between `try_read` returning empty and here,
                    // blocking unconditionally would miss the wake and stall
                    // the worker for the full timeout. Consuming the flag
                    // after the wait also closes the race window going
                    // forward.
                    //
                    // Timeout is kept short (2ms) as a safety net so the stop
                    // flag is still polled quickly on shutdown. At 64 samples
                    // @ 48 kHz the audio period is 1.33 ms; any wait longer
                    // than ~1 period risks swallowing multiple buffers on a
                    // missed notification and producing an audible click.
                    let mut flag = worker_wake.0.lock().unwrap();
                    if !*flag {
                        let (new_flag, _) = worker_wake
                            .1
                            .wait_timeout(flag, std::time::Duration::from_millis(2))
                            .unwrap();
                        flag = new_flag;
                    }
                    *flag = false;
                }
            }
            log::info!("DSP worker '{}': stopped", label);
        })
        .map_err(|e| anyhow!("failed to spawn DSP worker thread: {}", e))?;
    Ok(DspWorkerHandle {
        stop_flag,
        wake,
        thread: Some(thread),
    })
}
```

`crates/infra-cpal/src/jack_handlers.rs`:
- Imports become:

```rust
use std::sync::Arc;

use crate::cpu_affinity::{detect_big_cores, pin_thread_to_cpus};
use crate::jack_period::JackPeriod;
```

- Replace the module doc's `JackProcessHandler` bullet with:

```rust
//! - `JackProcessHandler` — the RT process callback itself. Pins itself
//!   to big cores on first invocation and copies port data; `JackPeriod`
//!   (`jack_period.rs`) hands each period to the pipelines' workers and
//!   assembles the output (#980).
```

  and in the inlining-contract paragraph `` `SpscRingBuffer::try_write`, `pin_thread_to_cpus`, `detect_big_cores` `` becomes `` `JackPeriod::{input_frames, hand_off_input, render_output}`, `pin_thread_to_cpus`, `detect_big_cores` ``.
- Replace `pub(crate) struct JackProcessHandler { … }` and `impl jack::ProcessHandler for JackProcessHandler { … }` (from the `/// Direct JACK process handler` doc to the end of the file) with:

```rust
/// Direct JACK process handler — runs in the JACK real-time thread. Copies
/// port data only; `JackPeriod` hands the period to the pipelines' workers and
/// assembles the output (#980).
pub(crate) struct JackProcessHandler {
    pub(crate) input_ports: Vec<jack::Port<jack::AudioIn>>,
    pub(crate) output_ports: Vec<jack::Port<jack::AudioOut>>,
    pub(crate) period: JackPeriod,
    /// `true` once the RT thread has pinned itself to the big cores.
    /// libjack spawns the thread that ends up calling `process` lazily
    /// inside its own infrastructure, and there is no public hook to
    /// configure its affinity at creation. We therefore pin on the first
    /// call from the thread itself — a one-time write, no hot-path cost.
    pub(crate) affinity_pinned: bool,
}

impl jack::ProcessHandler for JackProcessHandler {
    fn process(&mut self, _client: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        // libjack creates the RT callback thread inside our process when
        // `Client::activate` runs. The thread inherits the process-wide
        // CPU mask (set by systemd's CPUAffinity=0-3 in the service
        // drop-in), which forces the audio-critical callback onto the
        // little A55 cores where it competes with the Slint UI thread
        // and the Mesa llvmpipe workers. On the first invocation from
        // this thread we widen the mask to the big A76 cores so the
        // callback runs alongside the DSP worker on the isolated RT
        // cores instead. `sched_setaffinity` may widen beyond the
        // service-level mask because systemd uses affinity — not a
        // cgroup cpuset — to apply CPUAffinity=. Check-and-set is
        // racy-safe here: the thread only calls itself.
        if !self.affinity_pinned {
            let big_cores = detect_big_cores();
            if !big_cores.is_empty() {
                pin_thread_to_cpus(&big_cores);
                log::info!(
                    "JackProcessHandler: RT callback thread pinned to big cores {:?}",
                    big_cores
                );
            }
            self.affinity_pinned = true;
        }
        let n_frames = ps.n_frames() as usize;

        // --- Input: read from JACK ports, interleave, hand to the pipelines ---
        let total_in_ports = self.input_ports.len();
        if total_in_ports > 0 {
            let buf = self.period.input_frames(n_frames, total_in_ports);
            for (ch, port) in self.input_ports.iter().enumerate() {
                let port_data = port.as_slice(ps);
                for frame in 0..n_frames {
                    buf[frame * total_in_ports + ch] = port_data[frame];
                }
            }
            self.period.hand_off_input(n_frames, total_in_ports);
        }

        // --- Output: pull from the pipelines, deinterleave into JACK ports ---
        // This is lightweight — just pops from ElasticBuffers, no DSP.
        let total_out_ports = self.output_ports.len();
        if total_out_ports > 0 {
            let buf = self.period.render_output(n_frames, total_out_ports);
            for (ch, port) in self.output_ports.iter_mut().enumerate() {
                let port_data = port.as_mut_slice(ps);
                for frame in 0..n_frames {
                    port_data[frame] = buf[frame * total_out_ports + ch];
                }
            }
        }

        jack::Control::Continue
    }
}
```

`crates/infra-cpal/src/jack_direct.rs`:
- Doc lines 2-3 become `//! \`build_jack_direct_chain\` — assemble the live JACK \`AsyncClient\` plus` / `//! its pipelines' dsp-workers for one chain on Linux+JACK.`; items 4-5 of the numbered list (lines 16-20) become:

```rust
//! 4. Build the chain's `JackPeriod` — one ring per pipeline and scratch
//!    buffers sized for MAX_JACK_FRAMES, so a later `jack_set_buffer_size`
//!    cannot trigger a realloc on the audio thread.
//! 5. Spawn one dsp-worker per pipeline (`jack_dsp_worker`).
```

  and lines 24-26 become `//! Setup-time only.`
- Imports (`:30-45`) become:

```rust
use std::sync::Arc;

use anyhow::{anyhow, Result};

use domain::ids::ChainId;
use domain::io_binding::IoBinding;
use engine::runtime_endpoints::{resolve_chain_io, InputEntry, OutputEntry};
use project::chain::Chain;

use crate::active_runtime::DspWorkerHandle;
use crate::jack_dsp_worker::spawn_pipeline_workers;
use crate::jack_handlers::{JackProcessHandler, JackShutdownHandler};
use crate::jack_period::JackPeriod;
use crate::jack_supervisor;
use crate::usb_proc::{detect_all_usb_audio_cards, jack_server_is_running_for};
use crate::LiveRuntimeSlot;
```

- Signature (`:47-56`) becomes:

```rust
pub(crate) fn build_jack_direct_chain(
    chain_id: &ChainId,
    chain: &Chain,
    slots: Vec<LiveRuntimeSlot>,
    registry: &[IoBinding],
    di_cells: Vec<crate::di_playback::DiPlaybackCell>,
) -> Result<(
    jack::AsyncClient<JackShutdownHandler, JackProcessHandler>,
    Vec<DspWorkerHandle>,
)> {
```

- Replace everything from line 199 (`// Set up DSP worker thread with ring buffer. …`) through the `let worker_handle = DspWorkerHandle { … };` statement (line 335) with:

```rust
    // #980: what a period does between the ports lives in `JackPeriod`; each
    // pipeline's DSP runs on its own worker (`jack_dsp_worker`). One entry per
    // chain output route — the device channels it writes — or the one fallback
    // route a chain without outputs plays (`effective_outputs`: mono, ch 0).
    let mut route_channels: Vec<Vec<usize>> = resolved_outputs
        .iter()
        .map(|output| output.channels.clone())
        .collect();
    if route_channels.is_empty() {
        route_channels.push(vec![0]);
    }
    let period = JackPeriod::new(
        slots,
        route_channels,
        input_ports.len(),
        output_ports.len(),
        di_cells,
    );
    let workers = spawn_pipeline_workers(
        &chain_id.0,
        period.pipelines(),
        max_in_ch,
        sample_rate as u32,
    )?;
    let handler = JackProcessHandler {
        input_ports,
        output_ports,
        period,
        affinity_pinned: false,
    };
```

- The last line `Ok((active_client, worker_handle))` becomes `Ok((active_client, workers))`.

`crates/infra-cpal/src/stream_builder.rs` — in the JACK branch replace lines 158-172 (from `// JACK-direct chains are a single runtime by Phase-1 design` through `build_jack_direct_chain(chain_id, chain, runtime, registry, di_cells.to_vec())?;`) with:

```rust
            // The JACK client reads the chain's runtimes through their live
            // slots (#672); `JackPeriod` decides which of them it plays.
            if slots.is_empty() {
                return Err(anyhow::anyhow!(
                    "chain '{}' has no runtime state",
                    chain_id.0
                ));
            }
            let slots = slots.into_iter().map(|(_, slot)| slot).collect();
            // #771: JACK-direct runs a single output stream — hand it ALL
            // the chain's DI cells so the DI is audible whichever output the
            // arm parked on.
            let (jack_client, dsp_workers) =
                build_jack_direct_chain(chain_id, chain, slots, registry, di_cells.to_vec())?;
```

and `_dsp_worker: Some(dsp_worker),` (line 181) becomes `_dsp_worker: Some(dsp_workers),`.

`crates/infra-cpal/src/active_runtime.rs:68-70` becomes:

```rust
    /// The JACK client's dsp-workers (Linux/JACK only). Dropped when the chain
    /// stops — after `_jack_client` (field order), so the callback has stopped.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    pub(crate) _dsp_worker: Option<Vec<DspWorkerHandle>>,
```

`crates/infra-cpal/src/lib.rs`, right after `mod jack_ring;` (Task 9):

```rust
#[cfg(all(target_os = "linux", feature = "jack"))]
mod jack_pipeline;
#[cfg(all(target_os = "linux", feature = "jack"))]
mod jack_period;
#[cfg(all(target_os = "linux", feature = "jack"))]
mod jack_dsp_worker;
```

- [ ] **Step 4: Run the tests to verify nothing changed**

```bash
$LINUX cargo build -p infra-cpal --features jack        # zero warnings
$LINUX cargo test  -p infra-cpal --features jack --lib   # same "test result" line as Step 1, all green
```

Then run the Gate on macOS.

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/infra-cpal/src/jack_pipeline.rs crates/infra-cpal/src/jack_period.rs \
  crates/infra-cpal/src/jack_dsp_worker.rs crates/infra-cpal/src/jack_handlers.rs \
  crates/infra-cpal/src/jack_direct.rs crates/infra-cpal/src/stream_builder.rs \
  crates/infra-cpal/src/active_runtime.rs crates/infra-cpal/src/lib.rs
git -C $S commit -m "refactor(#980): split the JACK client into period, pipeline and worker units" -m "No behaviour change: first runtime, route 0, one worker — as before.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 11: The JACK client plays every output of its chain

Today the JACK callback pops route 0 only (`jack_handlers.rs:256` before Task 10), so the second output of an E/S (e.g. `[10,11]`) and every mid `Output` are silent on Linux/JACK. Serving every route also means summing routes that share device channels — the owner's rig has two E/S, each on Main `[0,1]` — inside the client: that sum is this client's backend mix (the cpal twin is `process_output_f32_mixed`, `runtime_output_process.rs:134-161`) and passes the same saturation guard, `engine::runtime_dsp::output_limiter`, on exactly the shared channels; every other channel is copied as its pipeline rendered it.

**Files:**
- Create: `crates/infra-cpal/src/jack_route_owners.rs`
- Create: `crates/infra-cpal/src/jack_period_tests.rs`
- Modify: `crates/infra-cpal/src/jack_period.rs` (imports, struct field, `new`, `render_output`, `shared_channels`, test mount)
- Modify: `crates/infra-cpal/src/lib.rs` (`mod jack_route_owners;`, JACK cfg)
- Modify docs: `docs/audio-config.md` (new subsection at the end of `## JACK lifecycle (Linux only)`)

**Interfaces:**
- Consumes: `ChainRuntimeState::{owns_output, writes_output}`, `engine::runtime_graph::build_per_input_runtime_states`, `engine::runtime::process_output_f32`, `engine::runtime_dsp::output_limiter(f32) -> f32` (`runtime_dsp.rs:80`, already used by `di_playback.rs:15`), `crate::process_input_buffer(slot: &LiveRuntimeSlot, input_index: usize, data: &[f32], input_total_channels: usize)`.
- Produces: `pub(crate) fn route_owners(slots: &[LiveRuntimeSlot], route_count: usize) -> Vec<Vec<LiveRuntimeSlot>>`; private `fn shared_channels(route_channels: &[Vec<usize>]) -> Vec<usize>` in `jack_period.rs`; `JackPeriod::new(…, route_channels: Vec<Vec<usize>>, …)` now uses the parameter.

- [ ] **Step 1: Write the failing tests**

Append to `crates/infra-cpal/src/jack_period.rs`:

```rust

#[cfg(test)]
#[path = "jack_period_tests.rs"]
mod tests;
```

Create `crates/infra-cpal/src/jack_period_tests.rs`:

```rust
//! #980 — what one JACK period does between the device ports.
//!
//! `JackProcessHandler::process` handed each period to ONE runtime and popped
//! route 0 only: a guitar E/S with two outputs (`[0,1]`, `[10,11]`) was silent
//! on the second one under Linux/JACK, and a chain's other pipelines had no
//! worker at all. The owner's law: 1 input × 2 outputs = 2 streams, each its
//! own pipeline, its own worker, its own output.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{process_output_f32, ChainRuntimeState};
use project::chain::Chain;

use super::JackPeriod;
use crate::{process_input_buffer, LiveRuntimeSlot};

const SR: f32 = 48_000.0;
const FRAMES: usize = 64;
const IN_CHANNELS: usize = 2;
const OUT_CHANNELS: usize = 12;

fn endpoint(device: &str, name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode,
        channels: channels.to_vec(),
    }
}

/// The owner's E/S: one guitar, two stereo outputs on one interface.
fn one_guitar_two_outputs() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "guitarra-1".into(),
        name: "Guitarra 1".into(),
        inputs: vec![endpoint("jack:quantum", "in", ChannelMode::Mono, &[0])],
        outputs: vec![
            endpoint("jack:quantum", "main", ChannelMode::Stereo, &[0, 1]),
            endpoint("jack:quantum", "out 2", ChannelMode::Stereo, &[10, 11]),
        ],
    }]
}

fn chain(registry: &[IoBinding]) -> Chain {
    Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: registry.iter().map(|b| b.id.clone()).collect(),
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

fn runtimes(registry: &[IoBinding]) -> Vec<Arc<ChainRuntimeState>> {
    engine::runtime_graph::build_per_input_runtime_states(
        &chain(registry),
        SR,
        &HashMap::new(),
        &[FRAMES, FRAMES],
        registry,
    )
    .expect("the chain must build")
    .into_iter()
    .map(|(_, runtime)| runtime)
    .collect()
}

fn slots(runtimes: &[Arc<ChainRuntimeState>]) -> Vec<LiveRuntimeSlot> {
    runtimes
        .iter()
        .map(|runtime| LiveRuntimeSlot::new(Arc::clone(runtime)))
        .collect()
}

fn handles(slots: &[LiveRuntimeSlot]) -> Vec<LiveRuntimeSlot> {
    slots.iter().map(LiveRuntimeSlot::handle).collect()
}

/// What each pipeline's dsp-worker does with one period, run inline so the
/// test is deterministic: the period is the one JACK server's capture ports,
/// dispatched at cpal index 0 exactly as `jack_dsp_worker` does.
fn run_workers(slots: &[LiveRuntimeSlot], input: &[f32]) {
    for slot in slots {
        process_input_buffer(slot, 0, input, IN_CHANNELS);
    }
}

fn peak(out: &[f32], channel: usize) -> f32 {
    out.chunks(OUT_CHANNELS)
        .map(|frame| frame[channel].abs())
        .fold(0.0, f32::max)
}

/// Eight periods through `period` (the routes start primed with silence);
/// returns the last one the client played.
fn play(period: &mut JackPeriod, slots: &[LiveRuntimeSlot]) -> Vec<f32> {
    let input = vec![0.25_f32; FRAMES * IN_CHANNELS];
    let mut last = Vec::new();
    for _ in 0..8 {
        run_workers(slots, &input);
        last = period.render_output(FRAMES, OUT_CHANNELS).to_vec();
    }
    last
}

/// The device channels of Main and Out 2.
fn main_and_out2() -> Vec<Vec<usize>> {
    vec![vec![0, 1], vec![10, 11]]
}

#[test]
fn every_output_of_the_chain_plays_on_the_jack_client() {
    let registry = one_guitar_two_outputs();
    let slots = slots(&runtimes(&registry));
    let mut period = JackPeriod::new(
        handles(&slots),
        main_and_out2(),
        IN_CHANNELS,
        OUT_CHANNELS,
        Vec::new(),
    );

    let out = play(&mut period, &slots);

    for channel in [0, 1] {
        assert!(
            peak(&out, channel) > 0.1,
            "output 1 (channel {channel}) must carry the guitar"
        );
    }
    for channel in [10, 11] {
        assert!(
            peak(&out, channel) > 0.1,
            "output 2 (channel {channel}) must carry the guitar — the JACK client served route 0 only"
        );
    }
    for channel in 2..10 {
        assert_eq!(peak(&out, channel), 0.0, "channel {channel} belongs to no output");
    }
}

/// Volume (invariant #10): the client plays each route exactly as its
/// pipeline rendered it — no gain, no extra limiter, bit for bit.
#[test]
fn the_jack_client_plays_each_output_at_the_level_its_pipeline_rendered() {
    let registry = one_guitar_two_outputs();
    let jack = runtimes(&registry);
    let reference = runtimes(&registry);
    let jack_slots = slots(&jack);
    let reference_slots = slots(&reference);
    let mut period = JackPeriod::new(
        handles(&jack_slots),
        main_and_out2(),
        IN_CHANNELS,
        OUT_CHANNELS,
        Vec::new(),
    );
    let input = vec![0.25_f32; FRAMES * IN_CHANNELS];

    for _ in 0..8 {
        run_workers(&jack_slots, &input);
        run_workers(&reference_slots, &input);
        let played = period.render_output(FRAMES, OUT_CHANNELS).to_vec();
        let mut expected = vec![0.0_f32; FRAMES * OUT_CHANNELS];
        for route in 0..2 {
            let writer = reference
                .iter()
                .find(|runtime| runtime.writes_output(route))
                .expect("every output has a pipeline");
            let mut buf = vec![0.0_f32; FRAMES * OUT_CHANNELS];
            process_output_f32(writer, route, &mut buf, OUT_CHANNELS);
            for (e, s) in expected.iter_mut().zip(&buf) {
                *e += *s;
            }
        }
        assert_eq!(
            played, expected,
            "the JACK client must play every route bit for bit as its pipeline rendered it"
        );
    }
}

/// The owner's rig: two E/S on one interface, each guitar into Main `[0,1]`.
fn two_e_s_on_main() -> Vec<IoBinding> {
    let e_s = |id: &str, channel: usize| IoBinding {
        id: id.into(),
        name: id.into(),
        inputs: vec![endpoint("jack:quantum", "in", ChannelMode::Mono, &[channel])],
        outputs: vec![endpoint("jack:quantum", "main", ChannelMode::Stereo, &[0, 1])],
    };
    vec![e_s("guitarra-1", 0), e_s("guitarra-2", 1)]
}

/// Two outputs on the SAME device channels are summed inside the client — the
/// JACK client is this device's backend mix — so the sum must pass the same
/// saturation guard as `process_output_f32_mixed` and never reach the device
/// above full scale.
#[test]
fn two_e_s_on_the_same_channels_never_clip_the_device() {
    let registry = two_e_s_on_main();
    let slots = slots(&runtimes(&registry));
    let mut period = JackPeriod::new(
        handles(&slots),
        vec![vec![0, 1], vec![0, 1]],
        IN_CHANNELS,
        OUT_CHANNELS,
        Vec::new(),
    );
    let input = vec![0.9_f32; FRAMES * IN_CHANNELS];
    let mut out = Vec::new();
    for _ in 0..8 {
        run_workers(&slots, &input);
        out = period.render_output(FRAMES, OUT_CHANNELS).to_vec();
    }
    for channel in [0, 1] {
        let level = peak(&out, channel);
        assert!(
            level > 0.95,
            "channel {channel}: both E/S must play on Main (one alone stays below the \
             limiter knee), got {level}"
        );
        assert!(
            level <= 1.0,
            "channel {channel}: the two outputs' sum must pass the saturation guard, got {level}"
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail (Linux)**

```bash
$LINUX cargo test -p infra-cpal --features jack --lib jack_period
```

Expected (the engine still groups by device on JACK, so one runtime writes every route and only route 0 is served):
- `every_output_of_the_chain_plays_on_the_jack_client` FAILS "output 2 (channel 10) must carry the guitar — the JACK client served route 0 only".
- `the_jack_client_plays_each_output_at_the_level_its_pipeline_rendered` FAILS "assertion `left == right` failed: the JACK client must play every route bit for bit as its pipeline rendered it".
- `two_e_s_on_the_same_channels_never_clip_the_device` FAILS "channel 0: both E/S must play on Main …" (route 0 only). Serving every route without the guard would then fail it on "… must pass the saturation guard" (0.9 + 0.9 on one channel).

- [ ] **Step 3: Implement**

Create `crates/infra-cpal/src/jack_route_owners.rs`:

```rust
//! Responsibility: decides which pipelines feed each output route of a JACK client.
//!
//! #980: a JACK client registers the device's ports once and plays every output
//! route of its chain. A route is served ONLY by the pipelines whose runtime
//! owns it — writes it, or can write it after an insert switch (#967) — the
//! same `owns_output` rule the cpal output streams apply
//! (`slot_processing::slots_for_output_stream`, #947). A JACK client drives one
//! device, so there is no device filter here.

#![cfg(all(target_os = "linux", feature = "jack"))]

use crate::LiveRuntimeSlot;

/// For each route `0..route_count`, the pipelines that own it. Setup-time only.
#[must_use]
pub(crate) fn route_owners(
    slots: &[LiveRuntimeSlot],
    route_count: usize,
) -> Vec<Vec<LiveRuntimeSlot>> {
    (0..route_count)
        .map(|route| {
            slots
                .iter()
                .filter(|slot| slot.load().owns_output(route))
                .map(LiveRuntimeSlot::handle)
                .collect()
        })
        .collect()
}
```

In `crates/infra-cpal/src/jack_period.rs`:

- Add to the imports: `use engine::runtime_dsp::output_limiter;`
- Add a field to `pub(crate) struct JackPeriod`, right after `route_owners`:

```rust
    /// Device channels two or more routes write (two E/S on Main): their sum
    /// is this client's backend mix and passes the saturation guard.
    shared_channels: Vec<usize>,
```

- In `new`, rename the parameter `_route_channels` to `route_channels` and replace

```rust
        // The JACK-direct client binds the chain's first runtime and plays its
        // route 0.
        let first: Vec<LiveRuntimeSlot> = slots.into_iter().take(1).collect();
        let route_owners = vec![first.iter().map(LiveRuntimeSlot::handle).collect()];
```

with

```rust
        // The JACK-direct client binds the chain's first runtime; it plays every
        // route that runtime owns (#980 — it used to play route 0 only).
        let first: Vec<LiveRuntimeSlot> = slots.into_iter().take(1).collect();
        let route_owners = crate::jack_route_owners::route_owners(&first, route_channels.len());
        let shared_channels = shared_channels(&route_channels);
```

  and add `shared_channels,` to the `Self { … }` literal after `route_owners,`.

- In `render_output`, add `shared_channels,` to the `let Self { … } = self;` destructuring, and replace

```rust
                // Each route writes only its own device channels; `out` is
                // their union.
                for (dst, src) in out.iter_mut().zip(buf.iter()) {
                    *dst += *src;
                }
            }
```

with

```rust
                // A route writes only its own device channels. Two routes on
                // the same channels (two E/S on Main) are summed here: this
                // client is the device's backend mix, as
                // `process_output_f32_mixed` is for a cpal output stream.
                for (dst, src) in out.iter_mut().zip(buf.iter()) {
                    *dst += *src;
                }
            }
            // The mix's saturation guard, on the shared channels only: a
            // channel one route writes is played exactly as rendered (#10).
            if !shared_channels.is_empty() {
                for frame in out.chunks_mut(channels) {
                    for &channel in shared_channels.iter() {
                        if let Some(sample) = frame.get_mut(channel) {
                            *sample = output_limiter(*sample);
                        }
                    }
                }
            }
```

- Append after `impl JackPeriod { … }` (before the test mount):

```rust
/// Device channels written by two or more of the chain's routes — setup-time
/// only.
fn shared_channels(route_channels: &[Vec<usize>]) -> Vec<usize> {
    let mut seen: Vec<usize> = Vec::new();
    let mut shared: Vec<usize> = Vec::new();
    for channels in route_channels {
        let mut route = channels.clone();
        route.sort_unstable();
        route.dedup();
        for channel in route {
            if !seen.contains(&channel) {
                seen.push(channel);
            } else if !shared.contains(&channel) {
                shared.push(channel);
            }
        }
    }
    shared
}
```

`crates/infra-cpal/src/lib.rs`, after `mod jack_dsp_worker;`:

```rust
#[cfg(all(target_os = "linux", feature = "jack"))]
mod jack_route_owners;
```

`docs/audio-config.md` — append at the end of the `## JACK lifecycle (Linux only)` section:

```markdown
### JACK-direct pipelines (Linux, #980)

A chain's JACK-direct client registers every port of the device once
(`jack_direct.rs`). Its process callback (`jack_handlers.rs`) only copies port
data; what happens to a period lives in `jack_period.rs`, over plain buffers:

- **Output:** every output route of the chain (its E/S outputs, or the single
  fallback route of a chain without outputs) is popped from the pipelines that
  own it (`jack_route_owners.rs` — the same `owns_output` rule the cpal output
  streams use) and written to its device channels. Two routes can share
  channels (two E/S on Main): the client sums them — it is this device's
  backend mix, as `process_output_f32_mixed` is for a cpal output stream — and
  the shared channels pass the same saturation guard (`output_limiter`); a
  channel one route writes is played exactly as rendered. Before #980 the
  callback popped route 0 only, so the second output of an E/S (e.g.
  `[10,11]`) was silent on Linux/JACK. Insert sends are not served on JACK.
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
$LINUX cargo test -p infra-cpal --features jack --lib jack_period   # 3 passed
$LINUX cargo build -p infra-cpal --features jack                    # zero warnings
```

Then run the Gate on macOS.

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/infra-cpal/src/jack_route_owners.rs crates/infra-cpal/src/jack_period.rs \
  crates/infra-cpal/src/jack_period_tests.rs crates/infra-cpal/src/lib.rs docs/audio-config.md
git -C $S commit -m "fix(#980): the JACK client plays every output of its chain, not route 0 only" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 12: Every JACK pipeline gets its own ring and its own dsp-worker

**Files:**
- Modify: `crates/infra-cpal/src/jack_period.rs` (`new`: every slot)
- Modify: `crates/infra-cpal/src/jack_dsp_worker.rs` (`spawn_pipeline_workers`: no `take(1)`; test mount)
- Modify: `crates/infra-cpal/src/jack_period_tests.rs` (fixture + two tests; both build their pipelines one by one on ONE device, so they hold before Task 14 — the owner's one-E/S-two-outputs case on JACK is pinned in Task 14, once the engine groups by pipeline there)
- Create: `crates/infra-cpal/src/jack_dsp_worker_tests.rs`
- Modify: `crates/infra-cpal/src/stream_builder.rs:128-133` (doc of `build_active_chain_runtime`)
- Modify docs: `docs/audio-config.md` (bullet added to "JACK-direct pipelines")

**Interfaces:**
- Consumes: Task 10/11 units; `ChainRuntimeState::take_output_route_stats() -> Vec<OutputRouteStats>` (`fill_frames`).
- Produces: no new signatures — `JackPeriod::new` builds one `JackPipeline` per slot and owners over all slots; `spawn_pipeline_workers` returns one handle per pipeline.

- [ ] **Step 1: Write the failing tests**

Append to `crates/infra-cpal/src/jack_period_tests.rs`:

```rust
/// Guitar `channel` into Main on the one interface, built on its own: one
/// pipeline runtime whatever the grouping, fed at cpal index 0 like every
/// pipeline of this client. Two of them are what the engine gives two guitars
/// into Main once it groups by pipeline on JACK too.
fn guitar_into_main(channel: usize) -> Arc<ChainRuntimeState> {
    let registry = vec![IoBinding {
        id: "guitarra-1".into(),
        name: "Guitarra 1".into(),
        inputs: vec![endpoint("jack:quantum", "in", ChannelMode::Mono, &[channel])],
        outputs: vec![endpoint("jack:quantum", "main", ChannelMode::Stereo, &[0, 1])],
    }];
    let mut built = runtimes(&registry);
    assert_eq!(built.len(), 1, "one guitar into one output is one pipeline");
    built.remove(0)
}

#[test]
fn a_period_reaches_every_pipeline_of_the_chain() {
    let slots = slots(&[guitar_into_main(0), guitar_into_main(1)]);
    let mut period = JackPeriod::new(slots, vec![vec![0, 1]], IN_CHANNELS, OUT_CHANNELS, Vec::new());

    period.input_frames(FRAMES, IN_CHANNELS).fill(0.5);
    period.hand_off_input(FRAMES, IN_CHANNELS);

    let received = period
        .pipelines()
        .iter()
        .filter(|pipeline| {
            let mut dst = vec![0.0_f32; FRAMES * IN_CHANNELS];
            pipeline.ring.try_read(&mut dst) == Some(FRAMES * IN_CHANNELS)
                && dst.iter().all(|s| *s == 0.5)
        })
        .count();
    assert_eq!(received, 2, "every pipeline's worker must be handed the period");
}

/// Two guitars into Main are two pipelines writing one route: the client must
/// play every one of them, not its first only.
#[test]
fn a_route_plays_every_pipeline_that_writes_it() {
    let slots = slots(&[guitar_into_main(0), guitar_into_main(1)]);
    let mut period = JackPeriod::new(
        handles(&slots),
        vec![vec![0, 1]],
        IN_CHANNELS,
        OUT_CHANNELS,
        Vec::new(),
    );
    // Only guitar 2 (capture port 2) plays.
    let input: Vec<f32> = (0..FRAMES).flat_map(|_| [0.0_f32, 0.25]).collect();
    let mut out = Vec::new();
    for _ in 0..8 {
        run_workers(&slots, &input);
        out = period.render_output(FRAMES, OUT_CHANNELS).to_vec();
    }
    assert!(
        peak(&out, 0) > 0.1,
        "Main must carry guitar 2 — the JACK client played its first pipeline only"
    );
}
```

Append to `crates/infra-cpal/src/jack_dsp_worker.rs`:

```rust

#[cfg(test)]
#[path = "jack_dsp_worker_tests.rs"]
mod tests;
```

Create `crates/infra-cpal/src/jack_dsp_worker_tests.rs`:

```rust
//! #980 — every JACK pipeline runs on its own dsp-worker.
//!
//! The JACK-direct client spawned ONE worker for the whole chain: two
//! pipelines sharing it meant the second waited for the first on every
//! buffer — the late delivery measured on the owner's rig (underruns ==
//! dropped_frames on every route).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::ChainRuntimeState;
use project::chain::Chain;

use super::spawn_pipeline_workers;
use crate::jack_period::JackPeriod;
use crate::LiveRuntimeSlot;

const SR: f32 = 48_000.0;
const FRAMES: usize = 64;
const IN_CHANNELS: usize = 2;
const OUT_CHANNELS: usize = 2;

fn runtime() -> Arc<ChainRuntimeState> {
    let endpoint = |name: &str, mode: ChannelMode, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("jack:quantum".into()),
        mode,
        channels,
    };
    let registry = vec![IoBinding {
        id: "g".into(),
        name: "G".into(),
        inputs: vec![endpoint("in", ChannelMode::Mono, vec![0])],
        outputs: vec![endpoint("main", ChannelMode::Stereo, vec![0, 1])],
    }];
    let chain = Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["g".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    };
    engine::runtime_graph::build_per_input_runtime_states(
        &chain,
        SR,
        &HashMap::new(),
        &[FRAMES],
        &registry,
    )
    .expect("the chain must build")
    .into_iter()
    .next()
    .expect("one runtime")
    .1
}

/// Frames queued on the runtime's only route — grows once a worker processed a period.
fn queued(runtime: &ChainRuntimeState) -> usize {
    runtime
        .take_output_route_stats()
        .first()
        .map_or(0, |route| route.fill_frames)
}

#[test]
fn every_pipeline_is_processed_by_its_own_worker() {
    let (a, b) = (runtime(), runtime());
    let primed = (queued(&a), queued(&b));
    let mut period = JackPeriod::new(
        vec![
            LiveRuntimeSlot::new(Arc::clone(&a)),
            LiveRuntimeSlot::new(Arc::clone(&b)),
        ],
        vec![vec![0, 1]],
        IN_CHANNELS,
        OUT_CHANNELS,
        Vec::new(),
    );
    let workers = spawn_pipeline_workers("issue-980", period.pipelines(), IN_CHANNELS, SR as u32)
        .expect("the workers must spawn");

    period.input_frames(FRAMES, IN_CHANNELS).fill(0.25);
    period.hand_off_input(FRAMES, IN_CHANNELS);

    let deadline = Instant::now() + Duration::from_secs(2);
    while (queued(&a) == primed.0 || queued(&b) == primed.1) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(queued(&a) > primed.0, "pipeline 0's worker must process the period");
    assert!(
        queued(&b) > primed.1,
        "pipeline 1 must be processed by a worker of its own"
    );
    assert_eq!(workers.len(), 2, "one dsp-worker per pipeline");
}
```

(The workers pin to the big cores and ask nice -10 exactly as production; in Docker `detect_big_cores` falls back and `setpriority` is refused — both logged and ignored, `cpu_affinity.rs:22-65`. They affect only the test process's own threads.)

- [ ] **Step 2: Run the tests to verify they fail (Linux)**

```bash
$LINUX cargo test -p infra-cpal --features jack --lib -- jack_period jack_dsp_worker
```

Expected:
- `a_period_reaches_every_pipeline_of_the_chain` FAILS `left: 1, right: 2` ("every pipeline's worker must be handed the period").
- `a_route_plays_every_pipeline_that_writes_it` FAILS "Main must carry guitar 2 — the JACK client played its first pipeline only".
- `every_pipeline_is_processed_by_its_own_worker` FAILS after ~2 s "pipeline 1 must be processed by a worker of its own".

- [ ] **Step 3: Implement**

`crates/infra-cpal/src/jack_period.rs::new` — replace

```rust
        // The JACK-direct client binds the chain's first runtime; it plays every
        // route that runtime owns (#980 — it used to play route 0 only).
        let first: Vec<LiveRuntimeSlot> = slots.into_iter().take(1).collect();
        let route_owners = crate::jack_route_owners::route_owners(&first, route_channels.len());
        let shared_channels = shared_channels(&route_channels);
        let pipelines: Vec<JackPipeline> = first
```

with

```rust
        // #980: one pipeline per runtime — its own ring, wake and worker — and
        // every route played from the pipelines that own it.
        let route_owners = crate::jack_route_owners::route_owners(&slots, route_channels.len());
        let shared_channels = shared_channels(&route_channels);
        let pipelines: Vec<JackPipeline> = slots
```

(the `.into_iter().map(|slot| JackPipeline::new(slot, MAX_JACK_FRAMES * in_channels)).collect();` tail stays).

`crates/infra-cpal/src/jack_dsp_worker.rs::spawn_pipeline_workers` — replace

```rust
    // The JACK-direct client runs one worker, on its first pipeline.
    pipelines
        .iter()
        .take(1)
        .enumerate()
```

with

```rust
    // #980: one worker per pipeline — a late pipeline never holds another back.
    pipelines
        .iter()
        .enumerate()
```

`crates/infra-cpal/src/stream_builder.rs:128-133` — the doc of `build_active_chain_runtime` becomes:

```rust
/// Build (and start) the streams for one chain. `slots` is the chain's full
/// ordered `(slot, runtime)` list, one per pipeline (#980): the cpal path binds
/// each device stream to the pipelines it feeds and each output to the
/// runtimes that write it; the Linux/JACK path hands every runtime to the
/// chain's one JACK client, which runs one pipeline — ring and dsp-worker —
/// per runtime and plays each output from the pipelines that own it.
```

`docs/audio-config.md` — add to the "JACK-direct pipelines" subsection:

```markdown
- **Input:** the callback interleaves the capture ports once and hands the
  period to every pipeline. Each pipeline (one runtime) has its own ring, its
  own wake and its own `dsp-worker` (`jack_dsp_worker.rs`, big cores, nice
  -10). A late pipeline fills only its own ring; the others are not held back.
  The callback itself is shared — it is the device's period.
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
$LINUX cargo test -p infra-cpal --features jack --lib -- jack_period jack_dsp_worker   # 6 passed
$LINUX cargo build -p infra-cpal --features jack                                      # zero warnings
```

Then run the Gate on macOS.

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/infra-cpal/src/jack_period.rs crates/infra-cpal/src/jack_period_tests.rs \
  crates/infra-cpal/src/jack_dsp_worker.rs crates/infra-cpal/src/jack_dsp_worker_tests.rs \
  crates/infra-cpal/src/stream_builder.rs docs/audio-config.md
git -C $S commit -m "fix(#980): every JACK pipeline runs on its own ring and dsp-worker" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 13: Removing a chain on JACK silences every pipeline first, and retires all of it

**Files:**
- Modify: `crates/infra-cpal/src/controller_sync.rs:158-167` (+ test mount at the end)
- Create: `crates/infra-cpal/src/controller_sync_jack_tests.rs`

**Interfaces:**
- Consumes: `ProjectRuntimeController::kill_chain_streams(&mut self, &ChainId)` (`controller.rs:273-341`: drains every runtime of the chain, drops its streams/JACK client — whose drop joins the dsp-workers (`active_runtime.rs:6-11`) — waits 50 ms, retires the chain's graph entries and every `(chain, slot)` slot, cancels its in-flight builds, and hands all of it to the control worker, #934), `ChainRuntimeState::{set_draining, is_draining}`, `ProjectRuntimeController::for_testing` (seeds one live slot per graph runtime, `controller.rs:156-161`), the private `sync_project_jack_direct(&mut self, &Project) -> Result<()>` (`controller_sync.rs:149`, reachable from a child test module).
- Produces: nothing new. The JACK removal path used to drain only the first runtime, drop the rest on the frontend thread (`runtime_graph.remove_chain`) and never retire the chain's slots — N per chain now, each keeping a NAM/IR runtime alive until stop.

- [ ] **Step 1: Write the failing test**

Append to `crates/infra-cpal/src/controller_sync.rs`:

```rust

#[cfg(all(test, target_os = "linux", feature = "jack"))]
#[path = "controller_sync_jack_tests.rs"]
mod jack_tests;
```

Create `crates/infra-cpal/src/controller_sync_jack_tests.rs`:

```rust
//! #980 — a chain removed on Linux/JACK silences every pipeline first.
//!
//! A chain runs one runtime per (input × output) pipeline. Removing it drained
//! only the first runtime before its JACK client closed, so the other
//! pipelines kept processing blocks through the client's teardown — the window
//! the drain exists for (NAM C++ destructors).

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{ChainRuntimeState, RuntimeGraph};
use project::chain::Chain;
use project::project::Project;

use crate::active_runtime::ActiveChainRuntime;
use crate::controller::ProjectRuntimeController;
use crate::resolved::ChainStreamSignature;

fn runtime() -> Arc<ChainRuntimeState> {
    let endpoint = |name: &str, mode: ChannelMode, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("jack:quantum".into()),
        mode,
        channels,
    };
    let registry = vec![IoBinding {
        id: "g".into(),
        name: "G".into(),
        inputs: vec![endpoint("in", ChannelMode::Mono, vec![0])],
        outputs: vec![endpoint("main", ChannelMode::Stereo, vec![0, 1])],
    }];
    let chain = Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["g".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    };
    engine::runtime_graph::build_per_input_runtime_states(
        &chain,
        48_000.0,
        &HashMap::new(),
        &[64],
        &registry,
    )
    .expect("the chain must build")
    .into_iter()
    .next()
    .expect("one runtime")
    .1
}

#[test]
fn removing_a_chain_silences_every_pipeline_before_its_client_closes() {
    let chain_id = ChainId("rig:input-7".into());
    let (a, b) = (runtime(), runtime());
    let mut chains = HashMap::new();
    chains.insert((chain_id.clone(), 0), Arc::clone(&a));
    chains.insert((chain_id.clone(), 1), Arc::clone(&b));
    let mut controller = ProjectRuntimeController::for_testing(RuntimeGraph { chains });
    controller.active_chains.insert(
        chain_id.clone(),
        ActiveChainRuntime {
            stream_signature: ChainStreamSignature {
                inputs: vec![],
                outputs: vec![],
            },
            structure: Vec::new(),
            generation: 1,
            resolved: None,
            _input_streams: vec![],
            _output_streams: vec![],
            _jack_client: None,
            _dsp_worker: None,
        },
    );
    let without_the_chain = Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };

    controller
        .sync_project_jack_direct(&without_the_chain)
        .expect("removing a chain needs no JACK server");

    assert!(a.is_draining(), "pipeline 0 must be silenced before its client closes");
    assert!(b.is_draining(), "pipeline 1 must be silenced before its client closes");
    assert!(
        !controller.chain_slots.keys().any(|(id, _)| id == &chain_id),
        "every slot of the removed chain is retired — a left slot keeps its runtime alive"
    );
    assert!(
        controller.runtime_graph.runtimes_for(&chain_id).is_empty(),
        "the removed chain leaves the graph"
    );
}
```

- [ ] **Step 2: Run the test to verify it fails (Linux)**

```bash
$LINUX cargo test -p infra-cpal --features jack --lib controller_sync::jack_tests
```

Expected: FAILS "pipeline 1 must be silenced before its client closes" (then, once that holds, "every slot of the removed chain is retired …").

- [ ] **Step 3: Implement**

In `crates/infra-cpal/src/controller_sync.rs` replace the removal body (from the comment `// Signal the audio callback to stop processing blocks BEFORE` through `self.runtime_graph.remove_chain(&chain_id);`, lines 158-167) with

```rust
                // #980: a chain runs one runtime per pipeline. The cpal
                // removal path silences EVERY one before the client closes
                // (NAM C++ destructors must not race a running callback),
                // retires every (chain, slot) slot and hands the runtimes to
                // the control worker (#934) — one path for both backends.
                self.kill_chain_streams(&chain_id);
```

(`log::info!("removing chain '{}' from runtime", …)` above it stays. `kill_chain_streams` drains before it drops the `ActiveChainRuntime` — the JACK client stops, then its dsp-workers are joined — and sleeps 50 ms after, so no worker is inside a runtime when the control worker drops it.)

- [ ] **Step 4: Run the tests to verify they pass**

```bash
$LINUX cargo test -p infra-cpal --features jack --lib controller_sync::jack_tests   # 1 passed
$LINUX cargo build -p infra-cpal --features jack   # zero warnings
```

Then run the Gate on macOS.

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/infra-cpal/src/controller_sync.rs crates/infra-cpal/src/controller_sync_jack_tests.rs
git -C $S commit -m "fix(#980): removing a chain on JACK silences every pipeline, not the first" -m "The JACK removal path now goes through kill_chain_streams, like cpal: every runtime drained, every slot retired, everything dropped on the control worker.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 14: The engine groups by pipeline on Linux/JACK too

Owner decision (JACK). Preconditions: Tasks 1, 4, 5 (key, teardown drains every runtime, stale slots retired) and 10–13 (the JACK client runs one pipeline per runtime) are pushed. This task makes `issue_980_one_worker_per_pipeline.rs` and the new engine tests run and pass on Linux: it removes the Linux+JACK gates Tasks 1 and 8 put on them.

**Files:**
- Modify: `crates/engine/src/pipeline_grouping.rs` (delete the JACK branch)
- Modify: `crates/engine/Cargo.toml:6-10` (delete the `[features]` block)
- Modify: `crates/infra-cpal/Cargo.toml:7`
- Modify: `crates/engine/src/runtime_graph.rs:454` (issue_736 test gate)
- Modify: `crates/engine/src/stream_isolation_same_device_tests.rs:22-31,42,93,140,160,192,244` (gates removed — coverage widened, no assertion touched)
- Modify: `crates/engine/src/issue_967_insert_streams_tests.rs:160-165` (gate + "cpal only" paragraph removed)
- Modify: `crates/infra-cpal/src/controller_taps_tests.rs:130-136` (doubling module gate removed) and the Task 8 gate on `di_loop_reaches_both_outputs_of_one_guitar`
- Modify: `crates/engine/src/runtime.rs` (the Task 1 gate on the `issue_980_pipeline_split` mount), `crates/engine/tests/issue_980_one_worker_per_pipeline.rs` (the Task 1 inner gate and its three `//!` lines)
- Modify: `crates/infra-cpal/src/jack_period_tests.rs` (one test: the owner's E/S on JACK)
- Modify docs: `docs/audio-config.md` (the "Insert chains" bullet from Task 1)

**Interfaces:**
- Produces: `group_segments_into_pipelines` has one key on every platform; the engine crate has no `jack` feature; infra-cpal's `jack = ["cpal/jack", "dep:jack"]`.

- [ ] **Step 1: Widen the tests (the failing-test step: existing contract tests start covering Linux+JACK)**

- `crates/engine/src/stream_isolation_same_device_tests.rs`: delete the comment block at lines 22-30 (`// The per-entry same-device contract below holds only where …` through `// provides on this platform (and so \`Arc\` doesn't go unused under jack).`) and the seven lines `#[cfg(not(all(target_os = "linux", feature = "jack")))]` (at `:31, :42, :93, :140, :160, :192, :244`).
- `crates/engine/src/issue_967_insert_streams_tests.rs`: delete lines 160-165:

```rust
///
/// cpal only: linux+JACK groups runtimes per DEVICE, not per input entry
/// (#703's cfg law — the JACK-direct client binds one runtime per device), so
/// two E/S on one interface are one runtime there whether or not a loop cuts
/// the chain.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
```

- `crates/engine/src/runtime_graph.rs:454`: `#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]` becomes `#[cfg(test)]`.
- `crates/infra-cpal/src/controller_taps_tests.rs`: delete the comment at lines 130-135 (`// The doubling premise — …` through `// (mirrors the engine same-device isolation tests).`) and change `#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]` above `mod di_loop_doubling_tests` to `#[cfg(test)]`; above `fn di_loop_reaches_both_outputs_of_one_guitar` delete the two `// Linux+JACK groups per device …` comment lines and `#[cfg(not(all(target_os = "linux", feature = "jack")))]` (Task 8).
- `crates/engine/src/runtime.rs`: above `mod issue_980_pipeline_split;` delete the two `// #980: Linux+JACK groups per device …` comment lines and change `#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]` to `#[cfg(test)]` (Task 1).
- `crates/engine/tests/issue_980_one_worker_per_pipeline.rs`: delete the four lines Task 1 added (`//!`, the two `//! Linux+JACK groups per device …` lines, `#![cfg(not(all(target_os = "linux", feature = "jack")))]`) — the file is back to its original content.
- Append to `crates/infra-cpal/src/jack_period_tests.rs` (JACK-only module; the owner's E/S becomes two pipelines on JACK only now):

```rust
/// The owner's E/S on Linux/JACK: one guitar into Main and Out 2 is two
/// pipelines, and each output plays from the runtime that writes it alone.
#[test]
fn each_output_plays_from_the_pipeline_that_writes_it() {
    let registry = one_guitar_two_outputs();
    let runtimes = runtimes(&registry);
    assert_eq!(runtimes.len(), 2, "1 input x 2 outputs = 2 pipelines on JACK too");
    for (route, runtime) in runtimes.iter().enumerate() {
        assert_eq!(
            runtime.written_routes(),
            vec![route],
            "pipeline {route} writes its own output only"
        );
    }
    let slots = slots(&runtimes);
    let mut period = JackPeriod::new(
        handles(&slots),
        main_and_out2(),
        IN_CHANNELS,
        OUT_CHANNELS,
        Vec::new(),
    );

    let out = play(&mut period, &slots);

    for channel in [0, 1, 10, 11] {
        assert!(peak(&out, channel) > 0.1, "channel {channel} must carry the guitar");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail (Linux; the engine still has the JACK key)**

```bash
$LINUX cargo test -p engine --features jack --test issue_980_one_worker_per_pipeline
$LINUX cargo test -p engine --features jack --lib -- stream_isolation_same_device issue_967_insert_streams issue_980_pipeline_split
$LINUX cargo test -p infra-cpal --features jack --lib -- jack_period di_loop_reaches_both_outputs_of_one_guitar
```

Expected:
- `each_output_pipeline_of_one_guitar_runs_in_its_own_runtime` FAILS `left: 1, right: 2`.
- `stream_isolation_same_device_tests::two_entries_on_same_device_produce_two_independent_runtimes` FAILS ("two raw input entries (even on ONE device) => two isolated per-entry runtimes, got 1").
- `issue_967_insert_streams_tests::a_disabled_insert_keeps_every_head_in_its_own_runtime` FAILS `left: [0], right: [0, 1]`.
- `issue_980_pipeline_split::split_mono_siblings_share_each_output_pipeline` FAILS `left: 1, right: 2`.
- `jack_period::tests::each_output_plays_from_the_pipeline_that_writes_it` FAILS `left: 1, right: 2` ("1 input x 2 outputs = 2 pipelines on JACK too").
- `di_loop_reaches_both_outputs_of_one_guitar` FAILS `left: 1, right: 2` ("precondition #980: one runtime per pipeline").

- [ ] **Step 3: Implement**

`crates/engine/src/pipeline_grouping.rs` — replace the last paragraph of the `group_segments_into_pipelines` doc (`/// Linux/JACK keeps the per-device grouping (slot = cpal index) behind its` / `/// cfg: the JACK-direct client still binds one runtime.`) with

```rust
/// Every platform groups the same way: the JACK-direct client runs one
/// pipeline (ring + dsp-worker) per runtime too (#980).
```

and replace the two cfg blocks at the end of the function (from `#[cfg(all(target_os = "linux", feature = "jack"))]` to the end of the `#[cfg(not(…))] { … }` block) with:

```rust
    first_seen(segments, pipeline_key_of)
        .into_iter()
        .enumerate()
        .map(|(slot, (key, segments))| PipelineGroup {
            slot,
            key,
            segments,
        })
        .collect()
```

`crates/engine/Cargo.toml` — delete lines 6-10:

```toml
[features]
# Issue #703: Linux/JACK keeps the per-device runtime grouping (the
# JACK-direct client binds one runtime per chain); enabled transitively
# by infra-cpal's `jack` feature.
jack = []
```

(and the blank line after them).

`crates/infra-cpal/Cargo.toml:7` becomes:

```toml
jack = ["cpal/jack", "dep:jack"]
```

`docs/audio-config.md` — replace the "Insert chains" bullet (Task 1 version) with:

```markdown
- **Insert chains**: while an enabled, bound insert cuts the chain it is ONE
  runtime (the send/return pipeline spans cpal indices); a disabled insert
  cuts nothing and the chain splits per pipeline like any other.
  **Linux/JACK** groups exactly the same way — its client runs one pipeline
  per runtime (see "JACK-direct pipelines").
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
$LINUX cargo test -p engine --test issue_980_one_worker_per_pipeline
$LINUX cargo test -p engine --lib -- stream_isolation_same_device issue_967_insert_streams issue_980_pipeline_split volume_invariants issue_736
$LINUX cargo test -p infra-cpal --features jack --lib -- jack_ controller_sync::jack_tests di_loop
$LINUX cargo build --workspace          # zero warnings (no unexpected_cfgs left)
$LINUX cargo test --workspace           # what CI will run
git -C $S grep -n 'feature = "jack"' -- crates/engine   # must print nothing (the runtime.rs mount and the contract test included)
git -C $S grep -n 'Linux+JACK groups per device' -- crates   # must print nothing: every Task 1/8 gate is gone
```

Expected: all green. A Linux failure in a newly ungated test that is NOT about the grouping (e.g. a cpal-only helper) → restore that one test's gate, note it in the commit body and on the issue. Then run the Gate on macOS.

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/engine/src/pipeline_grouping.rs crates/engine/Cargo.toml crates/infra-cpal/Cargo.toml \
  crates/engine/src/runtime_graph.rs crates/engine/src/stream_isolation_same_device_tests.rs \
  crates/engine/src/issue_967_insert_streams_tests.rs crates/infra-cpal/src/controller_taps_tests.rs \
  crates/engine/src/runtime.rs crates/engine/tests/issue_980_one_worker_per_pipeline.rs \
  crates/infra-cpal/src/jack_period_tests.rs docs/audio-config.md
git -C $S commit -m "fix(#980): Linux/JACK runs one runtime per pipeline, like every platform" -m "Deletes the engine's per-device JACK grouping and its jack feature; the isolation contract tests now run on Linux too.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push (issue comment includes `$LINUX cargo test --workspace` green).

---

### Task 15: An output stream holding a runtime that writes nothing there plays its writer untouched

With the fixed slot set (Task 16) the stream of a chain's second output holds two slots — the one that runs the loop's cut while the insert is on, and its own pipeline while it is off — and only ONE writes it at a time. `process_output_f32_mixed` picks its path by slice length (`runtime_output_process.rs:142-161`): two held runtimes take the "many" path and the writer's already-limited samples go through `output_limiter` a SECOND time, so anything above the 0.95 knee comes out quieter (volume invariant #10). Fix: pick the path by the number of runtimes that WRITE the output.

**Files:**
- Create: `crates/engine/tests/issue_980_idle_slot_output.rs`
- Modify: `crates/engine/src/runtime_output_process.rs:112-163` (`process_output_f32_mixed` + its doc)
- Modify docs: `docs/audio-config.md` (`## One output route per stream (#947)` section)

**Interfaces:**
- Consumes: `ChainRuntimeState::writes_output(&self, output_index: usize) -> bool` (`runtime_chain_state.rs:240`; one wait-free `ArcSwap` load), `process_output_f32` (`runtime_output_process.rs:22`).
- Produces: same signature, `pub fn process_output_f32_mixed(runtimes: &[Arc<ChainRuntimeState>], output_index: usize, out: &mut [f32], output_total_channels: usize, scratch: &mut [f32])`; new contract: the path follows the number of held runtimes that WRITE `output_index` (0 → silence, 1 → the single path, ≥ 2 → the backend mix).

- [ ] **Step 1: Write the failing test**

Create `crates/engine/tests/issue_980_idle_slot_output.rs`:

```rust
//! #980 P3 — an output stream that holds a runtime which writes nothing there.
//!
//! With the fixed slot set (owner decision P3) the stream of a chain's second
//! output holds two slots — the one that runs the loop's cut while the insert
//! is on, and its own pipeline while it is off — and only ONE of them writes
//! that output at a time. The other must leave the writer's samples
//! byte-identical: holding two runtimes used to send the writer through the
//! backend-mix limiter a second time, so a hot guitar (above the 0.95 knee)
//! came out quieter than the same runtime heard alone (volume invariant #10).
//!
//! Two E/S on two interfaces, so the chain builds two runtimes and B's writes
//! nothing to A's output.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{process_input_f32, process_output_f32_mixed, ChainRuntimeState};
use engine::runtime_graph::build_per_input_runtime_states;
use project::chain::Chain;

const SR: f32 = 48_000.0;
const FRAMES: usize = 64;
const CHANNELS: usize = 2;
const HOT: f32 = 0.99;
const CALLBACKS: usize = 64;

fn endpoint(device: &str, name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode,
        channels: channels.to_vec(),
    }
}

/// A (in 0 → out 0/1 on `dev-a`) and B (in 0 → out 0/1 on `dev-b`): B's
/// runtime writes nothing to A's output — the "held but not writing" slot.
fn registry() -> Vec<IoBinding> {
    ["a", "b"]
        .into_iter()
        .map(|id| {
            let device = format!("dev-{id}");
            IoBinding {
                id: id.into(),
                name: id.to_uppercase(),
                inputs: vec![endpoint(&device, "in", ChannelMode::Mono, &[0])],
                outputs: vec![endpoint(&device, "out", ChannelMode::Stereo, &[0, 1])],
            }
        })
        .collect()
}

fn chain() -> Chain {
    Chain {
        id: ChainId("rig:input-980".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["a".into(), "b".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

/// The chain's runtimes, A's first.
fn runtimes() -> Vec<Arc<ChainRuntimeState>> {
    let built = build_per_input_runtime_states(
        &chain(),
        SR,
        &HashMap::new(),
        &[FRAMES, FRAMES],
        &registry(),
    )
    .expect("the chain must build");
    let runtimes: Vec<Arc<ChainRuntimeState>> = built.into_iter().map(|(_, rt)| rt).collect();
    assert_eq!(runtimes.len(), 2, "two E/S on two interfaces = two runtimes");
    assert!(
        runtimes[0].writes_output(0) && !runtimes[1].writes_output(0),
        "A writes output 0, B does not"
    );
    runtimes
}

#[test]
fn a_held_runtime_that_writes_nothing_here_leaves_the_writer_byte_identical() {
    // Two identical builds: one heard alone, one heard with B held too.
    let alone = runtimes();
    let held = runtimes();
    let mut guitar = vec![0.0_f32; FRAMES * CHANNELS];
    for frame in guitar.chunks_exact_mut(CHANNELS) {
        frame[0] = HOT;
    }
    let silence = vec![0.0_f32; FRAMES * CHANNELS];
    let mut out_alone = vec![0.0_f32; FRAMES * CHANNELS];
    let mut out_held = vec![0.0_f32; FRAMES * CHANNELS];
    let mut scratch_alone = vec![0.0_f32; FRAMES * CHANNELS];
    let mut scratch_held = vec![0.0_f32; FRAMES * CHANNELS];
    for callback in 0..CALLBACKS {
        for set in [&alone, &held] {
            process_input_f32(&set[0], 0, &guitar, CHANNELS);
            process_input_f32(&set[1], 1, &silence, CHANNELS);
        }
        process_output_f32_mixed(&alone[..1], 0, &mut out_alone, CHANNELS, &mut scratch_alone);
        process_output_f32_mixed(&held, 0, &mut out_held, CHANNELS, &mut scratch_held);
        assert_eq!(
            out_held, out_alone,
            "callback {callback}: holding B on A's output changed A's samples — the \
             writer went through the backend-mix limiter twice"
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
(cd $S && cargo test -p engine --test issue_980_idle_slot_output)
```

Expected: FAILS "assertion `left == right` failed: callback N: holding B on A's output changed A's samples — the writer went through the backend-mix limiter twice" (N = the first callback past the route's primed cushion and fade-in; the samples differ only above 0.95).

- [ ] **Step 3: Implement**

`crates/engine/src/runtime_output_process.rs` — replace `process_output_f32_mixed` and its doc (lines 112-163) with:

```rust
/// Drive one physical output device from the pipeline runtimes its stream
/// holds (issue #350, phase 3; #980). Each runtime has its own SPSC output
/// ring; the shared output device sums them at the backend — the ONLY place
/// CLAUDE.md invariant #4 permits mixing across streams. Each ring still has
/// exactly one producer and is consumed once here, so SPSC is preserved.
///
/// The path follows how many held runtimes WRITE this output (#980 P3): a
/// stream may hold a runtime that writes nothing here right now — the loop's
/// cut slot while the insert is off, an idle slot while it is on — and that
/// runtime contributes silence. One writer takes the single path:
/// `process_output_f32` writes straight into `out`, byte-identical to hearing
/// that runtime alone (volume invariant #10 — the backend-mix limiter below
/// would otherwise touch its samples a second time above the knee).
///
/// Several writers: each runtime's output (already per-runtime limited +
/// volume-scaled inside `process_output_f32`) is rendered into the
/// caller-owned `scratch` and summed into `out`; the summed buffer then
/// passes through `output_limiter` (the same tanh the chain already trusts to
/// hold a multi-stream sum transparent below 0 dBFS — see the route-mix note
/// in `mix_segment_into_routes`) so the device never receives a clipped
/// buffer. `scratch` MUST be pre-allocated by the caller at stream-build time
/// and be at least `out.len()` long — this function performs ZERO allocation
/// and ZERO locking on the audio thread (counting writers is one wait-free
/// `ArcSwap` load per held runtime).
pub fn process_output_f32_mixed(
    runtimes: &[Arc<ChainRuntimeState>],
    output_index: usize,
    out: &mut [f32],
    output_total_channels: usize,
    scratch: &mut [f32],
) {
    let mut writers = runtimes
        .iter()
        .filter(|runtime| runtime.writes_output(output_index));
    match (writers.next(), writers.next()) {
        (None, _) => out.fill(0.0),
        // One writer → byte-identical to that runtime alone.
        (Some(only), None) => process_output_f32(only, output_index, out, output_total_channels),
        _ => {
            out.fill(0.0);
            let n = out.len();
            for runtime in runtimes {
                let buf = &mut scratch[..n];
                process_output_f32(runtime, output_index, buf, output_total_channels);
                for (dst, src) in out.iter_mut().zip(buf.iter()) {
                    *dst += *src;
                }
            }
            // Backend mix saturation guard: N per-runtime-limited streams
            // can sum past 1.0; tanh holds it transparent below 0 dBFS.
            for s in out.iter_mut() {
                *s = output_limiter(*s);
            }
        }
    }
}
```

`docs/audio-config.md` — append to the `## One output route per stream (#947)` section:

```markdown
An output stream may hold a runtime that writes nothing there right now (an
idle fixed slot, or the loop's cut slot while the loop is off — #980 P3).
`process_output_f32_mixed` counts the held runtimes that WRITE the output: one
writer takes the single path, byte-identical to hearing it alone; only
several writers (two guitars into one output) are summed and limited at the
backend.
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
(cd $S && cargo test -p engine --test issue_980_idle_slot_output)   # 1 passed
(cd $S && cargo test -p engine --test mono_output_audible)           # multi-writer path unchanged
(cd $S && cargo test -p engine --lib volume_invariants)
```

Then run the Gate.

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/engine/src/runtime_output_process.rs crates/engine/tests/issue_980_idle_slot_output.rs docs/audio-config.md
git -C $S commit -m "fix(#980): an output stream holding a runtime that writes nothing there plays its writer untouched" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 16: A chain with a bound insert keeps the same runtime slots whether its loop is on or off

Owner decision P3. Scope: chains that own insert streams (`insert_cut::insert_owns_streams`) AND whose loop-off pipelines all read ONE input stream. Loop off: slot k runs loop-off pipeline k. Loop on: slot 0 runs the whole cut, slots 1..N idle. Every slot keeps one identity in both states (stamp = loop-off pipeline k, cpal index = the one input stream, clock = its device rate, owned routes = slot 0 every route / slot k its own), so each output stream holds the same slots whichever state built it and `input_group_ids` is `[0..N]` both ways.

**Files:**
- Create: `crates/engine/src/insert_fixed_slots.rs`
- Modify: `crates/engine/src/lib.rs:27` (add `mod insert_fixed_slots;` after `mod insert_endpoints;`)
- Modify: `crates/engine/src/pipeline_grouping.rs` (`group_segments_into_pipelines` consults the fixed slots; add `group_segments_with`)
- Modify: `crates/engine/src/runtime_graph.rs` (docs of `RuntimeGraph`, `chain_has_insert_cut`, `switch_owned_routes`; the build loop of `build_per_input_runtimes`)
- Create: `crates/engine/tests/issue_980_insert_fixed_slots.rs`
- Modify: `crates/infra-cpal/src/io_topology_tests.rs` (append)
- Create: `crates/infra-cpal/src/issue_980_fixed_slot_binding_tests.rs`; modify `crates/infra-cpal/src/slot_processing.rs` (mount after the #947 mount, line 130)
- Create: `crates/infra-cpal/src/issue_980_insert_fixed_slots_hw_tests.rs`; modify `crates/infra-cpal/src/lib.rs:69-71` (mount after the #967 HW mount)
- Modify (old-topology tests, TESTS decision — no sound/latency/volume assertion touched): `crates/engine/src/issue_967_insert_streams_tests.rs` (`an_enabled_insert_makes_the_heads_one_runtime` and its module-doc sentence), `crates/engine/src/issue_923_insert_tail_routes_tests.rs:131`
- Modify docs: `docs/audio-config.md` ("Insert chains" bullet, the #967 insert paragraph), `docs/mcp.md` (`openrig://routes` bullet)

**Interfaces:**
- Consumes: `insert_cut::insert_owns_streams(&AudioBlock, &[IoBinding]) -> bool`, `chain_has_insert_cut`, `pipeline_grouping::{uncut_segments, pipeline_keys, pipeline_key_of, PipelineGroup, PipelineKey}` (Task 2), `assemble_chain_runtime_state(chain, segments, switch_owned_routes, eff_outputs, sample_rate, device_rates, elastic_targets, existing_blocks)`, `switch_owned_routes`.
- Produces:
  - `pub(crate) struct FixedSlots { pub(crate) keys: Vec<PipelineKey>, pub(crate) cpal_input_index: usize, pub(crate) input_device: DeviceId }` with `pub(crate) fn owned_routes(&self, slot: usize, route_count: usize) -> Vec<usize>`
  - `pub(crate) fn fixed_slots(chain: &Chain, registry: &[IoBinding]) -> Option<FixedSlots>`
  - `pub(crate) fn fixed_slot_groups(slots: &FixedSlots, insert_cuts: bool, segments: Vec<ChainSegment>) -> Vec<PipelineGroup>`
  - `pub(crate) fn group_segments_with(fixed: Option<&FixedSlots>, chain: &Chain, registry: &[IoBinding], segments: Vec<ChainSegment>) -> Vec<PipelineGroup>` (in `pipeline_grouping`)
  - unchanged public: `build_per_input_runtime_states`, `input_group_ids`, `pipeline_slots` (now the same in both loop states for fixed-slot chains).

- [ ] **Step 1: Write the failing tests**

(a) Create `crates/engine/tests/issue_980_insert_fixed_slots.rs`:

```rust
//! #980 P3 — a chain with a bound insert keeps the SAME runtime slots whether
//! its loop is on or off (owner decision), so the switch stays a DSP rebuild on
//! the streams the chain already has (#967), while the loop-off pipelines
//! still run one per runtime, one dsp-worker each (#980).
//!
//! The owner's layout: one E/S (guitar in 0, outputs Main `[0,1]` and Out 2
//! `[10,11]`) plus a loop on the same interface (send 12, return 13). Loop
//! off: slot 0 is the Main pipeline, slot 1 the Out 2 pipeline. Loop on: slot
//! 0 runs the whole cut (guitar → send, return → both tails), slot 1 idles.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{process_input_f32, process_output_f32_mixed, ChainRuntimeState};
use engine::runtime_graph::{
    build_chain_runtime_state, build_per_input_runtime_states, input_group_ids,
};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;

const DEVICE: &str = "coreaudio:quantum";
const SR: f32 = 44_100.0;
const TARGET: usize = 64;
const FRAMES: usize = 64;
const CALLBACKS: usize = 128;
const GUITAR: usize = 0;
const SEND: usize = 12;
const RETURN: usize = 13;
const DEVICE_CHANNELS: usize = 14;
/// Route numbering (#967): the E/S's two tails, then the loop's send.
const ROUTE_MAIN: usize = 0;
const ROUTE_OUT2: usize = 1;
const ROUTE_SEND: usize = 2;
const ROUTES: usize = 3;

fn endpoint(name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(DEVICE.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "guitarra-1".into(),
            name: "Guitarra 1".into(),
            inputs: vec![endpoint("in", ChannelMode::Mono, &[GUITAR])],
            outputs: vec![
                endpoint("main", ChannelMode::Stereo, &[0, 1]),
                endpoint("out 2", ChannelMode::Stereo, &[10, 11]),
            ],
        },
        IoBinding {
            id: "loop".into(),
            name: "Loop".into(),
            inputs: vec![endpoint("ret", ChannelMode::Mono, &[RETURN])],
            outputs: vec![endpoint("snd", ChannelMode::Mono, &[SEND])],
        },
    ]
}

fn chain(loop_on: bool) -> Chain {
    Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitarra-1".into()],
        blocks: vec![AudioBlock {
            id: BlockId("insert:loop".into()),
            enabled: loop_on,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "external_loop".into(),
                io: "loop".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    }
}

fn build_at(
    loop_on: bool,
    device_rates: &HashMap<DeviceId, f32>,
) -> Vec<(usize, Arc<ChainRuntimeState>)> {
    build_per_input_runtime_states(
        &chain(loop_on),
        SR,
        device_rates,
        &[TARGET; ROUTES],
        &registry(),
    )
    .expect("the chain must build")
}

fn build(loop_on: bool) -> Vec<(usize, Arc<ChainRuntimeState>)> {
    build_at(loop_on, &HashMap::new())
}

fn state(loop_on: bool) -> &'static str {
    if loop_on {
        "on"
    } else {
        "off"
    }
}

fn written(runtime: &ChainRuntimeState) -> Vec<usize> {
    (0..ROUTES).filter(|&r| runtime.writes_output(r)).collect()
}

/// Per route, the slots an output stream on it holds.
fn holders(runtimes: &[(usize, Arc<ChainRuntimeState>)]) -> Vec<Vec<usize>> {
    (0..ROUTES)
        .map(|route| {
            runtimes
                .iter()
                .filter(|(_, rt)| rt.owns_output(route))
                .map(|(slot, _)| *slot)
                .collect()
        })
        .collect()
}

/// Per route, the slots that write it right now.
fn writers(runtimes: &[(usize, Arc<ChainRuntimeState>)]) -> Vec<Vec<usize>> {
    (0..ROUTES)
        .map(|route| {
            runtimes
                .iter()
                .filter(|(_, rt)| rt.writes_output(route))
                .map(|(slot, _)| *slot)
                .collect()
        })
        .collect()
}

#[test]
fn switching_the_loop_keeps_the_same_slots() {
    let slots = |loop_on| build(loop_on).iter().map(|(slot, _)| *slot).collect::<Vec<_>>();
    assert_eq!(slots(false), vec![0, 1], "loop off: one slot per output pipeline (#980)");
    assert_eq!(
        slots(true),
        vec![0, 1],
        "loop on: the SAME slots — the switch must not regroup the chain (#967)"
    );
}

#[test]
fn with_the_loop_off_each_slot_runs_one_pipeline() {
    for (slot, runtime) in build(false) {
        assert_eq!(runtime.stream_count(), 1, "slot {slot}: one pipeline");
        assert_eq!(written(&runtime), vec![slot], "slot {slot} writes its own tail only");
    }
}

#[test]
fn with_the_loop_on_slot_zero_runs_the_whole_cut_and_the_other_idles() {
    let runtimes = build(true);
    assert_eq!(runtimes.len(), 2, "loop on keeps both slots");
    assert_eq!(
        runtimes[0].1.stream_count(),
        2,
        "slot 0: guitar → send, return → both tails"
    );
    assert_eq!(
        written(&runtimes[0].1),
        vec![ROUTE_MAIN, ROUTE_OUT2, ROUTE_SEND],
        "slot 0 writes every route of the cut"
    );
    assert_eq!(runtimes[1].1.stream_count(), 0, "slot 1 idles while the loop cuts");
    assert!(
        written(&runtimes[1].1).is_empty(),
        "slot 1 writes nothing while the loop cuts"
    );
}

#[test]
fn every_output_stream_holds_the_same_slots_in_both_states() {
    let expected = vec![vec![0], vec![0, 1], vec![0]];
    for loop_on in [false, true] {
        assert_eq!(
            holders(&build(loop_on)),
            expected,
            "loop {}: Main and the send are slot 0's; Out 2 is held by slot 0 (the cut) \
             and slot 1 (its own pipeline)",
            state(loop_on)
        );
    }
}

#[test]
fn each_tail_has_exactly_one_writer_in_each_state() {
    assert_eq!(
        writers(&build(false)),
        vec![vec![0], vec![1], vec![]],
        "loop off: each tail its own pipeline, the send silent"
    );
    assert_eq!(
        writers(&build(true)),
        vec![vec![0], vec![0], vec![0]],
        "loop on: the cut in slot 0 writes every route"
    );
}

#[test]
fn an_idle_slot_runs_at_its_input_devices_rate() {
    let rates: HashMap<DeviceId, f32> =
        [(DeviceId(DEVICE.into()), 48_000.0)].into_iter().collect();
    let runtimes = build_at(true, &rates);
    assert_eq!(runtimes.len(), 2, "loop on keeps both slots");
    for (slot, runtime) in &runtimes {
        assert_eq!(
            runtime.sample_rate(),
            48_000.0,
            "slot {slot} must run on the interface's clock — an idle slot becomes a \
             pipeline again when the loop goes off"
        );
    }
}

#[test]
fn the_switch_keeps_the_group_ids_the_streams_are_bound_to() {
    assert_eq!(
        input_group_ids(&chain(true), &registry()),
        input_group_ids(&chain(false), &registry()),
        "#967: equal group ids — the switch reuses every stream"
    );
    assert_eq!(input_group_ids(&chain(false), &registry()), vec![0, 1]);
}

/// Plays a steady guitar through `runtimes` the way the device streams do:
/// every runtime fed by the one input callback, every route popped from the
/// runtimes its stream holds (`owns_output`), the loop closed in software
/// (what leaves on the send comes back on the return next callback). Returns
/// every route's samples, callback after callback.
fn play(runtimes: &[Arc<ChainRuntimeState>]) -> Vec<Vec<f32>> {
    let mut input = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
    let mut outs = vec![vec![0.0_f32; FRAMES * DEVICE_CHANNELS]; ROUTES];
    let mut scratch = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
    let mut heard = vec![Vec::new(); ROUTES];
    for _ in 0..CALLBACKS {
        for (f, frame) in input.chunks_exact_mut(DEVICE_CHANNELS).enumerate() {
            frame[GUITAR] = 0.5;
            frame[RETURN] = outs[ROUTE_SEND][f * DEVICE_CHANNELS + SEND];
        }
        for runtime in runtimes {
            process_input_f32(runtime, 0, &input, DEVICE_CHANNELS);
        }
        for (route, out) in outs.iter_mut().enumerate() {
            let held: Vec<Arc<ChainRuntimeState>> = runtimes
                .iter()
                .filter(|rt| rt.owns_output(route))
                .cloned()
                .collect();
            process_output_f32_mixed(&held, route, out, DEVICE_CHANNELS, &mut scratch);
            heard[route].extend_from_slice(out);
        }
    }
    heard
}

/// Volume invariant #10 on the fixed slots: each tail sounds exactly as the
/// whole-chain build of the same state.
#[test]
fn every_tail_sounds_exactly_as_the_whole_chain_in_both_loop_states() {
    for loop_on in [false, true] {
        let whole = vec![Arc::new(
            build_chain_runtime_state(&chain(loop_on), SR, &[TARGET; ROUTES], &registry())
                .expect("the whole chain must build"),
        )];
        let slots: Vec<Arc<ChainRuntimeState>> =
            build(loop_on).into_iter().map(|(_, rt)| rt).collect();
        let (expected, got) = (play(&whole), play(&slots));
        for route in [ROUTE_MAIN, ROUTE_OUT2] {
            assert!(
                got[route] == expected[route],
                "loop {}: route {route} differs from the whole-chain build",
                state(loop_on)
            );
        }
    }
}
```

(b) Append to `crates/infra-cpal/src/io_topology_tests.rs`:

```rust

/// #980 P3: the owner's layout — one E/S with two outputs, plus a loop. Loop
/// off it runs one runtime per output pipeline (#980); loop on it keeps the
/// same slots (slot 0 runs the cut), so the switch keeps the structure the
/// streams were built for and stays a DSP rebuild (#967).
#[test]
fn a_loop_on_a_two_output_es_keeps_the_chains_structure() {
    use domain::ids::{BlockId, ChainId};
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
    use project::chain::Chain;

    const DEV: &str = "coreaudio:quantum";
    let ep = |name: &str, mode: ChannelMode, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(DEV.into()),
        mode,
        channels,
    };
    let registry = vec![
        IoBinding {
            id: "guitarra-1".into(),
            name: "Guitarra 1".into(),
            inputs: vec![ep("in", ChannelMode::Mono, vec![0])],
            outputs: vec![
                ep("main", ChannelMode::Stereo, vec![0, 1]),
                ep("out 2", ChannelMode::Stereo, vec![10, 11]),
            ],
        },
        IoBinding {
            id: "loop".into(),
            name: "Loop".into(),
            inputs: vec![ep("ret", ChannelMode::Mono, vec![13])],
            outputs: vec![ep("snd", ChannelMode::Mono, vec![12])],
        },
    ];
    let chain = |enabled| Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitarra-1".into()],
        blocks: vec![AudioBlock {
            id: BlockId("insert:loop".into()),
            enabled,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "external_loop".into(),
                io: "loop".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    };

    let off = super::chain_structure_signature(&chain(false), &registry);
    let on = super::chain_structure_signature(&chain(true), &registry);
    assert_eq!(
        off.last().map(String::as_str),
        Some("groups|[0, 1]"),
        "loop off: one runtime per output pipeline (#980)"
    );
    assert_eq!(
        on, off,
        "#980 P3: the loop switch must keep the chain's runtime slots — otherwise every \
         footswitch press reopens the streams (#967)"
    );
}
```

(c) Mount in `crates/infra-cpal/src/slot_processing.rs` right after the #947 mount (lines 128-130):

```rust
#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]
#[path = "issue_980_fixed_slot_binding_tests.rs"]
mod issue_980_fixed_slot_binding_tests;
```

and create `crates/infra-cpal/src/issue_980_fixed_slot_binding_tests.rs`:

```rust
//! #980 P3 — an output stream picks the slots it holds once, when it is built;
//! a loop switch then publishes new runtimes into those same slots. So the
//! slots each stream holds must not depend on the state the chain was in when
//! its streams were built, and after the switch each tail must have exactly
//! one writer among them.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_per_input_runtime_states, ChainRuntimeState};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;

use super::*;
use crate::LiveRuntimeSlot;

const DEV: &str = "coreaudio:quantum";
/// Main, Out 2, then the loop's send (#967 numbering).
const ROUTES: usize = 3;

fn endpoint(name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(DEV.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "guitarra-1".into(),
            name: "Guitarra 1".into(),
            inputs: vec![endpoint("in", ChannelMode::Mono, &[0])],
            outputs: vec![
                endpoint("main", ChannelMode::Stereo, &[0, 1]),
                endpoint("out 2", ChannelMode::Stereo, &[10, 11]),
            ],
        },
        IoBinding {
            id: "loop".into(),
            name: "Loop".into(),
            inputs: vec![endpoint("ret", ChannelMode::Mono, &[13])],
            outputs: vec![endpoint("snd", ChannelMode::Mono, &[12])],
        },
    ]
}

fn chain(loop_on: bool) -> Chain {
    Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitarra-1".into()],
        blocks: vec![AudioBlock {
            id: BlockId("insert:loop".into()),
            enabled: loop_on,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "external_loop".into(),
                io: "loop".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    }
}

fn runtimes(loop_on: bool) -> Vec<(usize, Arc<ChainRuntimeState>)> {
    build_per_input_runtime_states(
        &chain(loop_on),
        48_000.0,
        &HashMap::new(),
        &[64; ROUTES],
        &registry(),
    )
    .expect("the chain builds")
}

/// Per route, the positions of the slots a stream built NOW would hold.
fn held(slots: &[(usize, LiveRuntimeSlot)], map: &[Vec<String>]) -> Vec<Vec<usize>> {
    (0..ROUTES)
        .map(|route| {
            let bound = slots_for_output_stream(slots, map, DEV, route);
            slots
                .iter()
                .enumerate()
                .filter(|(_, (_, slot))| {
                    bound
                        .iter()
                        .any(|b| Arc::ptr_eq(&b.load(), &slot.load()))
                })
                .map(|(position, _)| position)
                .collect()
        })
        .collect()
}

#[test]
fn every_output_stream_holds_the_same_slots_whichever_state_built_it() {
    let heads = engine::runtime_endpoints::resolve_chain_io(&chain(false), &registry()).0;
    let map = crate::chain_resolve_io_map::output_devices_by_input_cpal(
        &chain(false),
        &registry(),
        &heads,
    );
    for built_on in [false, true] {
        let slots = build_chain_slots(&runtimes(built_on));
        let at_build = held(&slots, &map);
        let switched = runtimes(!built_on);
        assert_eq!(
            switched.len(),
            slots.len(),
            "#980 P3: the switched build must fill exactly the slots the streams hold \
             (streams built with the loop {})",
            if built_on { "on" } else { "off" }
        );
        for ((_, slot), (_, runtime)) in slots.iter().zip(switched) {
            drop(slot.publish(runtime));
        }
        assert_eq!(
            held(&slots, &map),
            at_build,
            "the slots each stream holds must not depend on the loop state"
        );
        for tail in 0..2 {
            let writers = slots
                .iter()
                .filter(|(_, slot)| slot.load().writes_output(tail))
                .count();
            assert_eq!(writers, 1, "tail {tail} must have exactly one writer after the switch");
        }
    }
}
```

(d) HW twin — mount in `crates/infra-cpal/src/lib.rs` right after the #967 HW mount (lines 69-71):

```rust
#[cfg(test)]
#[path = "issue_980_insert_fixed_slots_hw_tests.rs"]
mod issue_980_insert_fixed_slots_hw_tests;
```

and create `crates/infra-cpal/src/issue_980_insert_fixed_slots_hw_tests.rs`:

```rust
//! #980 P3 — the owner's layout on real streams: one E/S with TWO outputs
//! plus a loop. Loop off, each output pipeline runs in its own runtime slot
//! (#980); the switch must still open or close no stream (#967) and must
//! republish both slots. Runs on the BlackHole 16ch loopback, so the owner's
//! interface is never touched.
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release --lib \
//!     issue_980_insert_fixed_slots_hw -- --nocapture --test-threads=1
//! ```
#![cfg(target_os = "macos")]

use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;

use crate::ProjectRuntimeController;

const RATE: u32 = 48_000;
const BUFFER: u32 = 128;
/// Three channels at least: Main on 0, the loop on 1, Out 2 on 2.
const LOOPBACK: &str = "BlackHole 16ch";
const INSERT: &str = "issue-980:insert";
const ROUTE_MAIN: usize = 0;
const ROUTE_OUT2: usize = 1;
const ROUTE_SEND: usize = 2;

fn hw_enabled(name: &str) -> bool {
    if std::env::var("OPENRIG_HW_TESTS").is_ok() {
        return true;
    }
    eprintln!("[{name}] skipped — set OPENRIG_HW_TESTS=1 to run it (opens real streams)");
    false
}

fn loopback_device() -> Option<String> {
    crate::list_input_device_descriptors()
        .ok()?
        .into_iter()
        .find(|d| d.name.contains(LOOPBACK))
        .map(|d| d.id)
}

fn project(device: &str, insert_enabled: bool) -> (Project, Vec<IoBinding>) {
    let ep = |name: &str, ch: usize| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    };
    let registry = vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![ep("in", 0)],
            outputs: vec![ep("out", 0), ep("out 2", 2)],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![ep("ret", 1)],
            outputs: vec![ep("snd", 1)],
        },
    ];
    let project = Project {
        name: Some("issue-980".into()),
        device_settings: vec![DeviceSettings {
            device_id: DeviceId(device.into()),
            sample_rate: RATE,
            buffer_size_frames: BUFFER,
            bit_depth: 32,
        }],
        chains: vec![Chain {
            id: ChainId("issue-980-fixed-slots".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            // The loopback feeds the tails back into the head; volume 0 keeps
            // it silent without changing a single stream.
            volume: 0.0,
            io_binding_ids: vec!["main".into()],
            blocks: vec![AudioBlock {
                id: BlockId(INSERT.into()),
                enabled: insert_enabled,
                kind: AudioBlockKind::Insert(InsertBlock {
                    model: "external_loop".into(),
                    io: "fx".into(),
                }),
            }],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    };
    (project, registry)
}

fn open_streams(controller: &ProjectRuntimeController, chain: &ChainId) -> (usize, usize) {
    controller
        .active_chains
        .get(chain)
        .map(|a| (a._input_streams.len(), a._output_streams.len()))
        .unwrap_or((0, 0))
}

fn route_flowing(controller: &ProjectRuntimeController, chain: &ChainId, route: usize) -> bool {
    controller
        .chain_output_route_stats(chain)
        .iter()
        .flat_map(|(_, routes)| routes.iter())
        .any(|r| r.route == route && r.callbacks > 0)
}

/// The app's path for a loop switch (`sync_live_chain_runtime`). Returns how
/// long until the rebuild was live and how many slots it published.
fn switch(
    controller: &mut ProjectRuntimeController,
    project: &mut Project,
    enabled: bool,
) -> (Duration, usize) {
    project.chains[0].blocks[0].enabled = enabled;
    let chain = project.chains[0].clone();
    let started = Instant::now();
    assert!(
        !controller.chain_io_changed(project, &chain).expect("io check"),
        "#967: switching the loop read as a re-bind"
    );
    assert!(
        !controller
            .schedule_chain_activation(project, &chain)
            .expect("schedule"),
        "#980 P3: the loop switch regrouped the two-output chain — brand-new streams, \
         the 2–3 s of silence #967 removed"
    );
    assert!(
        controller
            .request_offthread_rebuild_if_live(project, &chain)
            .expect("live rebuild"),
        "the switch rebuilds the DSP on the live streams"
    );
    loop {
        let published = controller.poll_pending_rebuilds();
        if published > 0 {
            return (started.elapsed(), published);
        }
        assert!(started.elapsed() < Duration::from_secs(5), "the rebuild never landed");
        std::thread::sleep(Duration::from_micros(200));
    }
}

fn run(start_enabled: bool) {
    let Some(device) = loopback_device() else {
        eprintln!("skipped — needs the {LOOPBACK} loopback");
        return;
    };
    let (mut project, registry) = project(&device, start_enabled);
    let chain_id = project.chains[0].id.clone();
    let mut controller = ProjectRuntimeController::start_with_io_bindings(&project, registry)
        .expect("start real streams");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !(open_streams(&controller, &chain_id) != (0, 0)
        && route_flowing(&controller, &chain_id, ROUTE_MAIN)
        && route_flowing(&controller, &chain_id, ROUTE_OUT2))
    {
        assert!(Instant::now() < deadline, "the chain never started");
        controller.poll_pending_rebuilds();
        std::thread::sleep(Duration::from_millis(20));
    }
    let streams = open_streams(&controller, &chain_id);
    let generation = controller.stream_generation;
    assert_eq!(streams, (1, 3), "one input stream; Main, Out 2 and the loop's send");

    for enabled in [!start_enabled, start_enabled, !start_enabled, start_enabled] {
        let (took, published) = switch(&mut controller, &mut project, enabled);
        std::thread::sleep(Duration::from_millis(200));
        controller.poll_pending_rebuilds();
        eprintln!("[HW] #980 P3 loop -> {enabled}: live in {took:?}, {published} slot(s)");

        assert_eq!(published, 2, "#980 P3: the switch republishes BOTH slots in one tick");
        assert!(
            took < Duration::from_millis(100),
            "#967: the switch must land within a few ms, took {took:?}"
        );
        assert_eq!(
            (open_streams(&controller, &chain_id), controller.stream_generation),
            (streams, generation),
            "#980 P3: switching the loop must not build a single stream"
        );
        for route in [ROUTE_MAIN, ROUTE_OUT2] {
            assert!(
                route_flowing(&controller, &chain_id, route),
                "route {route} keeps playing through the switch"
            );
        }
        if enabled {
            assert!(
                route_flowing(&controller, &chain_id, ROUTE_SEND),
                "loop on: the send stream pulls the send route"
            );
        }
    }
}

#[test]
fn switching_the_loop_of_a_two_output_es_keeps_every_stream() {
    if !hw_enabled("switching_the_loop_of_a_two_output_es_keeps_every_stream") {
        return;
    }
    run(true);
}

#[test]
fn a_two_output_es_started_with_its_loop_off_keeps_every_stream() {
    if !hw_enabled("a_two_output_es_started_with_its_loop_off_keeps_every_stream") {
        return;
    }
    run(false);
}
```

(e) Old-topology test adjustments:

`crates/engine/src/issue_967_insert_streams_tests.rs` — in the doc of `a_disabled_insert_keeps_every_head_in_its_own_runtime`, `/// pipeline across both heads, one runtime.` becomes `/// pipeline across both heads, run by slot 0 of the same two slots (#980 P3).`; replace the whole `an_enabled_insert_makes_the_heads_one_runtime` test (doc + fn) with:

```rust
/// #980 P3: the loop's cut is still one pipeline across both heads, run by
/// slot 0 — and the chain keeps the slot B's loop-off pipeline runs in, idle,
/// so switching the loop of two E/S on one interface regroups nothing and
/// needs no new stream.
#[test]
fn an_enabled_insert_runs_the_whole_cut_in_slot_zero() {
    let registry = two_heads_registry();
    assert_eq!(
        crate::runtime_graph::input_group_ids(&two_heads_chain(true), &registry),
        vec![0, 1],
        "loop on: the same slots as loop off"
    );
    let runtimes = crate::runtime_graph::build_per_input_runtimes(
        &two_heads_chain(true),
        48_000.0,
        &std::collections::HashMap::new(),
        &[DEFAULT_ELASTIC_TARGET; 3],
        &registry,
    )
    .expect("the chain must build");
    assert_eq!(
        runtimes[0].1.stream_count(),
        3,
        "slot 0 runs the cut: both heads into the send, the return into both tails"
    );
    assert_eq!(runtimes[1].1.stream_count(), 0, "slot 1 idles while the loop cuts");
}
```

`crates/engine/src/issue_923_insert_tail_routes_tests.rs:131` — replace

```rust
    assert_eq!(runtimes.len(), 1, "an insert chain is one runtime");
```

with (the peaks this test measures are unchanged: slot 0 is the same cut runtime as before):

```rust
    assert_eq!(
        runtimes.len(),
        2,
        "#980 P3: one slot per loop-off pipeline (Main, ADAT 3/4); the cut runs in slot 0"
    );
    assert_eq!(runtimes[1].1.stream_count(), 0, "with the inserts on, slot 1 idles");
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
(cd $S && cargo test -p engine --test issue_980_insert_fixed_slots)
(cd $S && cargo test -p engine --lib -- issue_967_insert_streams issue_923_insert_tail_routes)
(cd $S && cargo test -p infra-cpal --lib -- a_loop_on_a_two_output_es_keeps_the_chains_structure issue_980_fixed_slot_binding)
```

Expected:
- `switching_the_loop_keeps_the_same_slots` FAILS "loop on: the SAME slots — the switch must not regroup the chain (#967)" `left: [0]`, `right: [0, 1]`.
- `with_the_loop_on_slot_zero_runs_the_whole_cut_and_the_other_idles` FAILS "loop on keeps both slots" `left: 1`, `right: 2`.
- `every_output_stream_holds_the_same_slots_in_both_states` FAILS for loop off `left: [[0], [1], []]`, `right: [[0], [0, 1], [0]]`.
- `an_idle_slot_runs_at_its_input_devices_rate` FAILS "loop on keeps both slots" `1` vs `2`.
- `the_switch_keeps_the_group_ids_the_streams_are_bound_to` FAILS `left: [0]`, `right: [0, 1]`.
- Already green (guards of Task 1's behaviour): `with_the_loop_off_each_slot_runs_one_pipeline`, `each_tail_has_exactly_one_writer_in_each_state`, `every_tail_sounds_exactly_as_the_whole_chain_in_both_loop_states`.
- `an_enabled_insert_runs_the_whole_cut_in_slot_zero` FAILS `left: [0]`, `right: [0, 1]`.
- `with_inserts_enabled_every_tail_output_plays` and `with_inserts_enabled_binding_order_does_not_matter` FAIL "#980 P3: one slot per loop-off pipeline …" `left: 1`, `right: 2`.
- `a_loop_on_a_two_output_es_keeps_the_chains_structure` FAILS `left: [..., "groups|[0]"]`, `right: [..., "groups|[0, 1]"]`.
- `every_output_stream_holds_the_same_slots_whichever_state_built_it` FAILS "#980 P3: the switched build must fill exactly the slots the streams hold (streams built with the loop off)" `left: 1`, `right: 2`.
- HW (idle machine, `OPENRIG_HW_TESTS=1`, BlackHole 16ch installed): both HW tests FAIL at "#980 P3: the loop switch regrouped the two-output chain — brand-new streams".

- [ ] **Step 3: Implement**

Create `crates/engine/src/insert_fixed_slots.rs`:

```rust
//! Responsibility: decides the runtime slots a chain keeps across an insert switch.
//!
//! #980 P3 (owner decision). A chain whose bound insert owns streams keeps the
//! SAME runtime slots whether the insert is on or off, so switching it is a DSP
//! rebuild published into slots the streams already hold — never new streams
//! (#967). Off, every (input × output) pipeline runs in its own slot (#980: one
//! runtime, one dsp-worker per pipeline). On, the loop's cut is one pipeline
//! through the gear: slot 0 runs all of it and the other slots idle. The slot
//! count is the loop-off pipeline count, in both states.
//!
//! Every slot keeps ONE identity in both states: the loop-off pipeline it is
//! stamped with, the input stream it sits on, that stream's clock (#736) and
//! the routes its output streams hold — slot 0 every route, slot k its own.
//!
//! Scope: every loop-off pipeline reads ONE input stream (one device). A chain
//! whose pipelines read several devices keeps the #967 rule — the switch
//! regroups it and it gets new streams — because fixed slots there would put
//! one device's input callback on another device's pipeline.
//!
//! Setup-time only.

use domain::ids::DeviceId;
use domain::io_binding::IoBinding;
use project::chain::Chain;

use crate::pipeline_grouping::{
    pipeline_key_of, pipeline_keys, uncut_segments, PipelineGroup, PipelineKey,
};
use crate::runtime_segments::ChainSegment;

/// The slots a chain keeps across its insert switch.
pub(crate) struct FixedSlots {
    /// The loop-off pipeline each slot is stamped with, in slot order.
    pub(crate) keys: Vec<PipelineKey>,
    /// The input stream (cpal index) every slot is bound to.
    pub(crate) cpal_input_index: usize,
    /// The device that stream reads. Every slot runs at its rate, the idle
    /// ones too, so a slot that becomes a pipeline again is already on its
    /// clock (#736).
    pub(crate) input_device: DeviceId,
}

impl FixedSlots {
    /// The routes an output stream binds to `slot` in BOTH states: slot 0
    /// every route of the chain (it writes them all while the insert cuts),
    /// slot k the route its own loop-off pipeline writes.
    pub(crate) fn owned_routes(&self, slot: usize, route_count: usize) -> Vec<usize> {
        if slot == 0 {
            return (0..route_count).collect();
        }
        self.keys
            .get(slot)
            .map(|key| vec![key.route])
            .unwrap_or_default()
    }
}

/// `Some` when `chain` keeps fixed slots across its insert switch (see the
/// module doc); `None` when it groups like any other chain.
pub(crate) fn fixed_slots(chain: &Chain, registry: &[IoBinding]) -> Option<FixedSlots> {
    let owns_insert_streams = chain
        .blocks
        .iter()
        .any(|block| crate::insert_cut::insert_owns_streams(block, registry));
    if !owns_insert_streams {
        return None;
    }
    let uncut = uncut_segments(chain, registry);
    let first = uncut.first()?;
    if uncut
        .iter()
        .any(|segment| segment.cpal_input_index != first.cpal_input_index)
    {
        return None;
    }
    Some(FixedSlots {
        keys: pipeline_keys(&uncut),
        cpal_input_index: first.cpal_input_index,
        input_device: first.input.device_id.clone(),
    })
}

/// The segments each slot runs: loop off, each segment in the slot of its
/// pipeline; loop on (`insert_cuts`), the whole cut in slot 0 and nothing in
/// the others.
pub(crate) fn fixed_slot_groups(
    slots: &FixedSlots,
    insert_cuts: bool,
    segments: Vec<ChainSegment>,
) -> Vec<PipelineGroup> {
    let mut groups: Vec<PipelineGroup> = slots
        .keys
        .iter()
        .enumerate()
        .map(|(slot, &key)| PipelineGroup {
            slot,
            key,
            segments: Vec::new(),
        })
        .collect();
    for segment in segments {
        let at = if insert_cuts {
            0
        } else {
            let key = pipeline_key_of(&segment);
            slots.keys.iter().position(|k| *k == key).unwrap_or(0)
        };
        groups[at].segments.push(segment);
    }
    groups
}
```

`crates/engine/src/lib.rs` — after line 27 (`mod insert_endpoints;`) add `mod insert_fixed_slots;`.

`crates/engine/src/pipeline_grouping.rs`:
- Add to the imports: `use crate::insert_fixed_slots::FixedSlots;`
- Replace the whole `group_segments_into_pipelines` function (doc included) with:

```rust
/// Partition a chain's segments into one [`PipelineGroup`] per runtime, in
/// slot order. Walking the runtimes by slot walks the streams in the order
/// `chain_stream_io_labels` names them.
///
/// - A chain with fixed slots (`insert_fixed_slots`, #980 P3): the same slots
///   whether its insert is on or off — the loop-off pipelines, or the whole
///   cut in slot 0 with the other slots idle.
/// - Any other chain an insert cuts: ONE runtime (the cut spans cpal
///   indices), stamped with the first pipeline of the uncut chain.
/// - Everything else: one runtime per (input × output) pipeline.
///
/// Every platform groups the same way: the JACK-direct client runs one
/// pipeline (ring + dsp-worker) per runtime too (#980).
pub(crate) fn group_segments_into_pipelines(
    chain: &Chain,
    registry: &[IoBinding],
    segments: Vec<ChainSegment>,
) -> Vec<PipelineGroup> {
    let fixed = crate::insert_fixed_slots::fixed_slots(chain, registry);
    group_segments_with(fixed.as_ref(), chain, registry, segments)
}

/// [`group_segments_into_pipelines`] with the chain's fixed slots already
/// looked up — the build needs them for each slot's identity too.
pub(crate) fn group_segments_with(
    fixed: Option<&FixedSlots>,
    chain: &Chain,
    registry: &[IoBinding],
    segments: Vec<ChainSegment>,
) -> Vec<PipelineGroup> {
    let insert_cuts = crate::runtime_graph::chain_has_insert_cut(chain, registry);
    if let Some(slots) = fixed {
        return crate::insert_fixed_slots::fixed_slot_groups(slots, insert_cuts, segments);
    }
    if segments.is_empty() || insert_cuts {
        let key = uncut_pipeline_keys(chain, registry)
            .first()
            .copied()
            .or_else(|| segments.first().map(pipeline_key_of))
            .unwrap_or(NO_PIPELINE);
        return vec![PipelineGroup {
            slot: 0,
            key,
            segments,
        }];
    }
    first_seen(segments, pipeline_key_of)
        .into_iter()
        .enumerate()
        .map(|(slot, (key, segments))| PipelineGroup {
            slot,
            key,
            segments,
        })
        .collect()
}
```

`crates/engine/src/runtime_graph.rs`:
- In the `RuntimeGraph` doc (Task 1 version), replace its last paragraph (`/// `.len()` = total pipelines. …`) with:

```rust
/// `.len()` = total runtimes: one per pipeline — or, for a chain whose bound
/// insert keeps fixed slots, one per loop-off pipeline in BOTH insert states
/// (#980 P3, `insert_fixed_slots`). A chain with one input and one output is
/// one runtime, byte-identical to the whole-chain build.
```

- Replace the doc of `chain_has_insert_cut` (Task 1 version) with:

```rust
/// Whether an insert cuts the chain. A cut forms a cross-cpal-index pipeline
/// (input → insert send → insert return → output), so the cut runs in ONE
/// runtime.
///
/// #967: only a real cut counts (`insert_cut::insert_cuts_chain`): a disabled
/// insert — or one with no E/S — leaves every pipeline in its own isolated
/// runtime (#980). A chain with fixed slots (`insert_fixed_slots`, #980 P3)
/// keeps its slots across the switch — the cut runs in slot 0, the others
/// idle — so the switch is a DSP rebuild into the slots its streams hold. Any
/// other chain the switch regroups (its pipelines read several devices) gets
/// new streams: `chain_structure_signature` (infra-cpal) says so.
```

- Replace the doc of `switch_owned_routes` with:

```rust
/// #967: the routes a runtime owns beyond the ones it writes, for a chain
/// WITHOUT fixed slots (fixed slots own theirs through
/// `FixedSlots::owned_routes`, #980 P3). One runtime that owns an insert's
/// streams owns every route of the chain: the routes the other state writes
/// (the send, a tail only the return feeds) must already be bound to it. A
/// chain split into several runtimes gets new streams when the switch
/// regroups it, so its runtimes own only what they write.
```

- In `build_per_input_runtimes`, replace everything from `let groups =` to `Ok(out)` (Task 2 version) with:

```rust
    let fixed = crate::insert_fixed_slots::fixed_slots(chain, registry);
    let groups =
        crate::pipeline_grouping::group_segments_with(fixed.as_ref(), chain, registry, all_segments);
    let runtime_count = groups.len();
    let mut out = Vec::with_capacity(runtime_count);
    for group in groups {
        // #980 P3: a fixed slot keeps ONE identity in both insert states — the
        // input stream it sits on, its clock and the routes its output streams
        // hold — so the streams bound to it stay right whichever state is
        // published into it; an idle slot has no segment to read them from.
        // Other runtimes: the stream is their segments' (#703), the clock
        // their input device's (#736; the chain scalar when absent) and the
        // owned routes `switch_owned_routes`.
        let (owned_routes, cpal_input_index, rate_device) = match &fixed {
            Some(slots) => (
                slots.owned_routes(group.slot, eff_outputs.len()),
                slots.cpal_input_index,
                Some(slots.input_device.clone()),
            ),
            None => (
                switch_owned_routes(chain, registry, runtime_count, eff_outputs.len()),
                group
                    .segments
                    .first()
                    .map(|s| s.cpal_input_index)
                    .unwrap_or(0),
                group.segments.first().map(|s| s.input.device_id.clone()),
            ),
        };
        let group_rate = rate_device
            .and_then(|device| device_rates.get(&device).copied())
            .unwrap_or(sample_rate);
        let mut state = assemble_chain_runtime_state(
            chain,
            &group.segments,
            &owned_routes,
            &eff_outputs,
            group_rate,
            device_rates,
            elastic_targets,
            None,
        )?;
        state.owned_pipeline = Some(crate::runtime_chain_state::OwnedPipeline {
            key: group.key,
            cpal_input_index,
        });
        out.push((group.slot, state));
    }
    Ok(out)
```

`docs/audio-config.md`:
- Replace the "Insert chains" bullet (Task 14 version) with:

```markdown
- **Insert chains** keep fixed runtime slots (#980 P3,
  `engine::insert_fixed_slots`) when their loop-off pipelines read one input
  stream: one slot per loop-off pipeline in both insert states; while the loop
  cuts, slot 0 runs the whole send/return pipeline (it spans cpal indices) and
  the other slots idle. An insert chain whose heads read several devices is
  one runtime while the loop cuts it and splits per pipeline while it does
  not. **Linux/JACK** groups exactly the same way — its client runs one
  pipeline per runtime (see "JACK-direct pipelines").
```

- Replace the #967 sentences from `The one exception is a chain with several pipelines` through `is already bound when the loop is switched on.` (Task 1 version) with:

```markdown
**The chain keeps the same runtime slots in both states (#980 P3,
`engine::insert_fixed_slots`).** With the loop off every (input × output)
pipeline runs in its own runtime slot, one dsp-worker each (#980); with it on,
the cut is one pipeline through the gear, so slot 0 runs all of it and the
other slots idle. The slot count is the loop-off pipeline count either way,
and every slot keeps the same stamp, input stream and clock in both states and
owns the same routes in both: slot 0 every route of the chain (it writes them
all while the loop cuts), slot k the route of its own pipeline. An output
stream therefore holds the same slots whichever state it was built in, and
exactly one of them writes it at a time; a held slot that writes nothing there
is skipped (`process_output_f32_mixed` picks its path by writer count), so the
writer's samples are byte-identical to hearing it alone. This covers every
chain whose loop-off pipelines read ONE input stream — one E/S with several
outputs (the owner's layout), or several E/S on one interface. A chain whose
heads read several devices keeps the old rule: its switch regroups the
runtimes, `chain_structure_signature` carries the grouping and it gets new
streams — fixed slots there would put one device's input callback on another
device's pipeline.
```

`docs/mcp.md` — at the end of the `openrig://routes` bullet (after `(JSON).` in `… repeated on each of its rows) (JSON).`), add: `While a loop cuts a chain with fixed slots (#980 P3), group 0 lists every route and its other groups list none; with the loop off each group lists its own pipeline's route.`

- [ ] **Step 4: Run the tests to verify they pass**

```bash
(cd $S && cargo test -p engine --test issue_980_insert_fixed_slots)          # 9 passed
(cd $S && cargo test -p engine --lib -- issue_967_insert_streams issue_923_insert_tail_routes issue_980_pipeline_split)
(cd $S && cargo test -p engine --test issue_980_one_worker_per_pipeline)
(cd $S && cargo test -p infra-cpal --lib -- io_topology_tests issue_947_output_stream_route_owner issue_980_fixed_slot_binding)
```

Expected: all PASS (`io_topology_tests::a_switch_that_regroups_the_chains_runtimes_is_a_structural_change` — heads on two devices — stays green: that chain has no fixed slots). Then run the Gate, then on the idle machine with BlackHole 16ch installed:

```bash
(cd $S && OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release --lib issue_980_insert_fixed_slots_hw -- --nocapture --test-threads=1)
(cd $S && OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release --lib issue_967_insert_toggle_streams -- --nocapture --test-threads=1)
```

Expected: PASS (a missing BlackHole prints "skipped" — say so in the issue comment).

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/engine/src/insert_fixed_slots.rs crates/engine/src/lib.rs crates/engine/src/pipeline_grouping.rs \
  crates/engine/src/runtime_graph.rs crates/engine/tests/issue_980_insert_fixed_slots.rs \
  crates/engine/src/issue_967_insert_streams_tests.rs crates/engine/src/issue_923_insert_tail_routes_tests.rs \
  crates/infra-cpal/src/io_topology_tests.rs crates/infra-cpal/src/slot_processing.rs \
  crates/infra-cpal/src/issue_980_fixed_slot_binding_tests.rs crates/infra-cpal/src/lib.rs \
  crates/infra-cpal/src/issue_980_insert_fixed_slots_hw_tests.rs docs/audio-config.md docs/mcp.md
git -C $S commit -m "feat(#980): a chain with a bound insert keeps the same runtime slots whether its loop is on or off" -m "Owner decision P3. Old-topology tests adjusted: issue_967_insert_streams_tests (an_enabled_insert_makes_the_heads_one_runtime -> an_enabled_insert_runs_the_whole_cut_in_slot_zero), issue_923_insert_tail_routes_tests (1 -> 2 slots; peak assertions unchanged).

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 17: An in-place update switches every slot of a chain together, sharing their processors

The in-place path (VST3 chains #779 — the owner's rig —, the JACK backend, the synchronous upsert) updates runtimes one at a time (`runtime_graph_impl.rs` in-place loop, `controller_offthread_live_rebuild.rs:102-124`). With fixed slots a loop switch MOVES a tail between slots, so slot-by-slot leaves Out 2 with two writers (double level) or none (silence) while the next slot builds — and a block that moves to another slot is rebuilt from scratch (a VST3 re-instantiated, a delay tail cut), which the one-runtime chain never did. Committing back to back is not enough either: each commit takes its runtime's `processing` lock, which a dsp-worker inside `process` holds for a whole period, so slot 0's routes could go live a period before slot 1's. And an update empties every runtime's blocks before it builds (`NodePool::take_live`; today `runtime_graph_update.rs:150-159` does it per runtime), and a segment with no blocks passes its input through unprocessed (`runtime_process_segment.rs:194-205`): every processor built in that window — loop off, the second instance of a block the cut ran once; a block whose model changed, once per pipeline runtime — is heard as the dry guitar on every output of the chain, and with every runtime taken at once the window lasts for all of their builds.

Fix, in five steps: (1) plan every runtime (fail before touching any); (2) while every runtime still plays with its own processors, walk each new segment over a snapshot of the live nodes and build NOW every processor no old node can stand in for (`runtime_node_plan`); (3) take every runtime's processors into one `NodePool` and assemble every runtime's next state from the old nodes the plan chose (its own segment first, then its other segments, then the other runtimes at the same rate) plus the ones just built — nodes only move; (4) take every runtime's `processing` lock in slot order and commit all of them under those locks; (5) drop the replaced processors after the last lock is released (#670). A failed build hands every node back by `instance_serial`. The single-runtime entry points delegate to the same path, so there is one implementation. The remaining in-window build is a block whose parameters changed on the same audio processor and that processor refuses the in-place retune (#358) — only the processor can tell, so it is tried as before (`try_in_place_param_update`, `runtime_block_builders.rs:250-287`).

**Files:**
- Create: `crates/engine/src/runtime_node_pool.rs`
- Create: `crates/engine/src/runtime_node_plan.rs`
- Create: `crates/engine/src/runtime_graph_update_together.rs`
- Modify (whole file): `crates/engine/src/runtime_graph_update.rs` (Step 1b adds a test seam to today's version first)
- Modify: `crates/engine/src/runtime_block_builders.rs` (Step 1b: test-only build counters and the count in `build_block_runtime_node`, `:289`; Step 3: `node_emits_mono_content` `:36` and `build_block_runtime_node` `:289` become `pub(crate)`)
- Modify: `crates/engine/src/runtime_graph_assemble.rs:271-301` (extract `segment_bus` from `build_input_processing_state`, behaviour-preserving)
- Modify: `crates/engine/src/runtime_graph_impl.rs` (module doc, imports, in-place branch of `upsert_chain_impl`, new `update_chain_in_place`)
- Modify: `crates/engine/src/lib.rs` (`mod runtime_graph_update_together;` after `mod runtime_graph_update;`, `mod runtime_node_plan;` and `mod runtime_node_pool;` after `mod runtime_mid_output_tap;`)
- Modify: `crates/engine/src/runtime_state.rs:194` (drop the now-stale `#[cfg_attr(not(test), allow(dead_code))]` on `instance_serial`)
- Modify: `crates/infra-cpal/src/controller_offthread_live_rebuild.rs:102-127` (VST3 in-place branch)
- Create: `crates/engine/src/issue_980_fixed_slot_switch_tests.rs`; modify `crates/engine/src/runtime.rs` (mount)
- Modify docs: `docs/audio-config.md` (after the P3 paragraph of Task 16)

**Interfaces:**
- Consumes: `lock_recover(&Mutex<T>, &str) -> MutexGuard<T>`, `BlockRuntimeNode { instance_serial, block_id, block_snapshot, input_layout, content_mono, output_layout, processor, … }`, `RuntimeProcessor::{Bypass, Audio}`, `AudioBlock::model_ref() -> Option<BlockModelRef { effect_type, model, params }>` (`project/src/block/audio_block_methods.rs:79`), `ChainProcessingState { input_states, input_to_segments, input_scratches, … }`, `InputProcessingState { blocks, outgoing, fade_in_remaining, … }`, `OutgoingTail { blocks, frames_remaining, scratch }` (`runtime_state.rs`), `crate::runtime::FADE_IN_FRAMES` (`runtime_state.rs:257`), `project::chain::processing_layout` (`runtime_graph_assemble.rs:271-272`), `group_segments_into_pipelines` (Task 16), `ChainRuntimeState::{owned_pipeline, sample_rate}`.
- Produces:
  - `pub(crate) struct NodePool` with `take_live(runtimes: &[Arc<ChainRuntimeState>]) -> Self`, `take_segment(&mut self, owner: usize, index: usize) -> Vec<BlockRuntimeNode>`, `take_serials(&mut self, serials: &[u64]) -> Vec<BlockRuntimeNode>`, `restore(self, runtimes: &[Arc<ChainRuntimeState>], built: Vec<Vec<InputProcessingState>>) -> Vec<BlockRuntimeNode>`, `into_leftovers(self) -> Vec<BlockRuntimeNode>`
  - `#[derive(Default)] pub(crate) struct SegmentNodes { pub(crate) lent: Vec<u64>, pub(crate) prebuilt: Vec<BlockRuntimeNode> }`; `pub(crate) fn plan_nodes(runtimes: &[Arc<ChainRuntimeState>], chain: &Chain, plans: &[UpdatePlan], spillover: bool) -> Vec<Vec<SegmentNodes>>` (in `runtime_node_plan`)
  - `pub(crate) fn segment_bus(input: &InputEntry, output_channels: &[usize]) -> (AudioChannelLayout, bool)` (in `runtime_graph_assemble`)
  - `pub(crate) struct UpdatePlan { pub(crate) segments: Vec<ChainSegment>, pub(crate) effective_outs: Vec<OutputEntry> }`; `pub(crate) fn plan_update(runtime: &ChainRuntimeState, chain: &Chain, registry: &[IoBinding]) -> Result<UpdatePlan>`; `pub(crate) fn segment_output_channels(plan: &UpdatePlan, segment: &ChainSegment) -> Vec<usize>`; `pub(crate) fn build_next_input_states(runtime: &ChainRuntimeState, chain: &Chain, plan: &UpdatePlan, pool: &mut NodePool, owner: usize, nodes: Vec<SegmentNodes>, spillover: bool, built: &mut Vec<InputProcessingState>) -> Result<()>`; `pub(crate) fn next_output_routes(runtime: &ChainRuntimeState, chain: &Chain, plan: &UpdatePlan, device_rates: &HashMap<DeviceId, f32>, reset_output_queue: bool, elastic_targets: &[usize]) -> Vec<Option<Arc<OutputRoutingState>>>`; `pub(crate) struct PreparedCommit`; `pub(crate) fn prepare_commit(runtime: &ChainRuntimeState, plan: UpdatePlan, input_states: Vec<InputProcessingState>, output_routes: Vec<Option<Arc<OutputRoutingState>>>) -> PreparedCommit`; `pub(crate) fn apply_commit(runtime: &ChainRuntimeState, processing: &mut ChainProcessingState, prepared: PreparedCommit, reset_output_queue: bool, volume: f32) -> Vec<InputProcessingState>`
  - `pub(crate) fn update_runtimes_together(runtimes: &[Arc<ChainRuntimeState>], chain: &Chain, device_rates: &HashMap<DeviceId, f32>, reset_output_queue: bool, elastic_targets: &[usize], spillover: bool, registry: &[IoBinding]) -> Result<()>`
  - Test-only (`#[cfg(test)]`, `runtime_block_builders`): `fn fresh_node_builds() -> usize`, `fn note_builds_while_taken(count: usize)`, `fn builds_while_taken() -> usize` — thread-local counters of block processors built from scratch, and of those built while an in-place update held its runtimes' processors.
  - `impl RuntimeGraph { pub fn update_chain_in_place(&self, chain: &Chain, device_rates: &HashMap<DeviceId, f32>, reset_output_queue: bool, elastic_targets: &[usize], registry: &[IoBinding]) -> Result<()> }`
  - Unchanged public signatures: `update_chain_runtime_state`, `update_chain_runtime_state_at_device_rates`, `update_chain_runtime_state_spillover`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/engine/src/runtime.rs`:

```rust

#[cfg(test)]
#[path = "issue_980_fixed_slot_switch_tests.rs"]
mod issue_980_fixed_slot_switch;
```

Create `crates/engine/src/issue_980_fixed_slot_switch_tests.rs`:

```rust
//! #980 P3 — the loop switch of a fixed-slot chain applied IN PLACE (the path
//! a VST3 chain, the JACK backend and the synchronous upsert take) publishes
//! every slot at one instant, keeps every block's processor and builds what
//! it adds before any runtime gives its processors up. Slot by slot, Out 2
//! had both pipelines writing it (double level) or neither (silence) for as
//! long as the other slot took to go live; a block that moved to another slot
//! was rebuilt (a VST3 re-instantiated, a delay tail cut); and a processor
//! built while the runtimes' own were taken left every output playing the
//! guitar unprocessed meanwhile. A dsp-worker inside `process` holds its
//! slot's lock for a whole period; the watcher here does the same to slot 1,
//! so a commit made slot by slot is caught between two slots.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InsertBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use crate::runtime::ChainRuntimeState;
use crate::runtime_graph::RuntimeGraph;

const DEVICE: &str = "coreaudio:quantum";
const SR: f32 = 44_100.0;
/// Two tails and the loop's send.
const TARGETS: [usize; 3] = [64; 3];
const OUT2: usize = 1;
/// The watcher's first hold on slot 1 — ample for a slot-by-slot update to
/// have published slot 0 — …
const FIRST_HOLD: Duration = Duration::from_millis(500);
/// … then its holds until the switch returns, …
const HOLD: Duration = Duration::from_millis(5);
/// … with this gap between them, so the switch can take the lock too.
const GAP: Duration = Duration::from_millis(1);

fn endpoint(name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(DEVICE.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn insert(loop_on: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId("insert:loop".into()),
        enabled: loop_on,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "external_loop".into(),
            io: "loop".into(),
        }),
    }
}

fn gain(id: &str) -> AudioBlock {
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

/// The owner's layout: one E/S (guitar in 0 → Main `[0,1]` + Out 2
/// `[10,11]`) and a mono loop on the same interface.
fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "guitarra-1".into(),
            name: "Guitarra 1".into(),
            inputs: vec![endpoint("in", ChannelMode::Mono, &[0])],
            outputs: vec![
                endpoint("main", ChannelMode::Stereo, &[0, 1]),
                endpoint("out 2", ChannelMode::Stereo, &[10, 11]),
            ],
        },
        IoBinding {
            id: "loop".into(),
            name: "Loop".into(),
            inputs: vec![endpoint("ret", ChannelMode::Mono, &[13])],
            outputs: vec![endpoint("snd", ChannelMode::Mono, &[12])],
        },
    ]
}

fn chain(loop_on: bool) -> Chain {
    Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitarra-1".into()],
        blocks: vec![insert(loop_on)],
        di_output: None,
        loopers: vec![],
    }
}

fn live_graph(chain: &Chain, registry: &[IoBinding]) -> RuntimeGraph {
    let mut graph = RuntimeGraph {
        chains: HashMap::new(),
    };
    graph
        .upsert_chain(chain, SR, &HashMap::new(), false, &TARGETS, registry)
        .expect("the chain builds");
    graph
}

/// Switch the owner's loop to `loop_on` in place while a watcher keeps taking
/// slot 1's processing lock — as a dsp-worker inside `process` does — first
/// for `FIRST_HOLD`, then `HOLD` at a time with a `GAP` between, until the
/// switch returns. `bad` is sampled on (slot 0, slot 1) only while the watcher
/// holds slot 1: a switch committed slot by slot is caught between two slots
/// (slot 0 live, slot 1 still waiting for its lock); one committed under
/// every slot's lock never is.
fn switch_while_slot_1_is_busy(
    graph: &mut RuntimeGraph,
    loop_on: bool,
    bad: fn(&ChainRuntimeState, &ChainRuntimeState) -> bool,
) -> bool {
    let slots = graph.runtimes_for(&chain(loop_on).id);
    assert_eq!(slots.len(), 2, "one slot per output pipeline");
    let (slot0, slot1) = (Arc::clone(&slots[0]), Arc::clone(&slots[1]));
    let switched = Arc::new(AtomicBool::new(false));
    let watcher_switched = Arc::clone(&switched);
    let (held_tx, held_rx) = mpsc::channel();
    let watcher = std::thread::spawn(move || {
        let mut seen = false;
        let mut hold = FIRST_HOLD;
        let mut first = Some(held_tx);
        while !watcher_switched.load(Ordering::Acquire) {
            {
                let _busy = slot1.processing.lock().expect("slot 1 lock");
                if let Some(held) = first.take() {
                    held.send(()).expect("signal the hold");
                }
                let until = Instant::now() + hold;
                while Instant::now() < until {
                    seen |= bad(&slot0, &slot1);
                    std::thread::yield_now();
                }
            }
            hold = HOLD;
            std::thread::sleep(GAP);
        }
        seen
    });
    held_rx.recv().expect("the watcher holds slot 1");
    graph
        .upsert_chain(&chain(loop_on), SR, &HashMap::new(), false, &TARGETS, &registry())
        .expect("the switch applies in place");
    switched.store(true, Ordering::Release);
    watcher.join().expect("watcher")
}

#[test]
fn switching_the_loop_on_never_gives_out_2_two_writers() {
    let mut graph = live_graph(&chain(false), &registry());
    let before = graph.runtimes_for(&chain(false).id);
    let two_writers = switch_while_slot_1_is_busy(&mut graph, true, |slot0, slot1| {
        slot0.writes_output(OUT2) && slot1.writes_output(OUT2)
    });
    assert!(
        !two_writers,
        "#980 P3: slot 0 went live as the cut while slot 1 still ran the Out 2 pipeline — \
         Out 2 played both until slot 1 was rebuilt"
    );
    let after = graph.runtimes_for(&chain(true).id);
    assert!(
        Arc::ptr_eq(&after[0], &before[0]) && Arc::ptr_eq(&after[1], &before[1]),
        "the switch is in place: the streams keep the runtimes they hold"
    );
    assert!(
        after[0].writes_output(OUT2) && !after[1].writes_output(OUT2),
        "loop on: Out 2 is the cut's (slot 0)"
    );
}

#[test]
fn switching_the_loop_off_never_leaves_out_2_without_a_writer() {
    let mut graph = live_graph(&chain(true), &registry());
    let no_writer = switch_while_slot_1_is_busy(&mut graph, false, |slot0, slot1| {
        !slot0.writes_output(OUT2) && !slot1.writes_output(OUT2)
    });
    assert!(
        !no_writer,
        "#980 P3: slot 0 dropped Out 2 while slot 1 was still idle — Out 2 went silent \
         until slot 1 was rebuilt"
    );
    let after = graph.runtimes_for(&chain(false).id);
    assert!(
        !after[0].writes_output(OUT2) && after[1].writes_output(OUT2),
        "loop off: Out 2 is its own pipeline's (slot 1)"
    );
}

/// Two E/S on one interface (A: in 0 → `[0,1]`, B: in 1 → `[2,3]`) and a
/// stereo loop on the same interface, so a head's processing bus is stereo on
/// both sides of the switch and its nodes may move.
fn two_heads_registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "a".into(),
            name: "A".into(),
            inputs: vec![endpoint("in", ChannelMode::Mono, &[0])],
            outputs: vec![endpoint("out", ChannelMode::Stereo, &[0, 1])],
        },
        IoBinding {
            id: "b".into(),
            name: "B".into(),
            inputs: vec![endpoint("in", ChannelMode::Mono, &[1])],
            outputs: vec![endpoint("out", ChannelMode::Stereo, &[2, 3])],
        },
        IoBinding {
            id: "loop".into(),
            name: "Loop".into(),
            inputs: vec![endpoint("ret", ChannelMode::Stereo, &[6, 7])],
            outputs: vec![endpoint("snd", ChannelMode::Stereo, &[4, 5])],
        },
    ]
}

fn two_heads_chain(loop_on: bool) -> Chain {
    Chain {
        id: ChainId("two-heads".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["a".into(), "b".into()],
        blocks: vec![gain("drive"), insert(loop_on), gain("amp")],
        di_output: None,
        loopers: vec![],
    }
}

/// The `instance_serial` of `block`'s node in segment `segment` of `runtime`.
fn serial_of(runtime: &ChainRuntimeState, segment: usize, block: &str) -> u64 {
    let processing = runtime.processing.lock().expect("processing lock");
    processing.input_states[segment]
        .blocks
        .iter()
        .find(|node| node.block_id.0 == block)
        .map(|node| node.instance_serial)
        .expect("the block runs in that segment")
}

/// #980 P3: an insert switch moves blocks between runtime slots — each keeps
/// its processor, as it did when the chain was one runtime (a VST3 is not
/// re-instantiated, a delay keeps its tail). Loop off, B's `drive` runs in
/// slot 1; loop on, slot 0 runs both heads into the send (segment 1 is head
/// B), and B's head must reuse that node.
#[test]
fn a_loop_switch_moves_each_block_with_its_processor() {
    let registry = two_heads_registry();
    let mut graph = live_graph(&two_heads_chain(false), &registry);
    let before = graph.runtimes_for(&two_heads_chain(false).id);
    assert_eq!(before.len(), 2, "loop off: one slot per E/S");
    let b_drive = serial_of(&before[1], 0, "drive");
    graph
        .upsert_chain(&two_heads_chain(true), SR, &HashMap::new(), false, &TARGETS, &registry)
        .expect("the switch applies in place");
    let after = graph.runtimes_for(&two_heads_chain(true).id);
    assert!(Arc::ptr_eq(&after[0], &before[0]), "the switch is in place");
    assert_eq!(
        serial_of(&after[0], 1, "drive"),
        b_drive,
        "head B's drive must keep the processor it had in slot 1 — not a new instance"
    );
}

/// The owner's E/S with a STEREO loop on the same interface, so the guitar's
/// processing bus is stereo on both sides of the switch and its processors
/// may move.
fn stereo_loop_registry() -> Vec<IoBinding> {
    let mut registry = registry();
    registry[1] = IoBinding {
        id: "loop".into(),
        name: "Loop".into(),
        inputs: vec![endpoint("ret", ChannelMode::Stereo, &[14, 15])],
        outputs: vec![endpoint("snd", ChannelMode::Stereo, &[12, 13])],
    };
    registry
}

/// `drive → loop → amp` on the owner's E/S.
fn drive_loop_amp(loop_on: bool) -> Chain {
    Chain {
        blocks: vec![gain("drive"), insert(loop_on), gain("amp")],
        ..chain(loop_on)
    }
}

/// #980 P3, loop off: the cut's head becomes slot 0's pipeline again and keeps
/// its `drive`; the Out 2 pipeline needs a `drive` of its own (the cut ran
/// one) — built BEFORE any runtime gives its processors up, so neither output
/// plays the guitar unprocessed while a VST3 or a NAM model loads.
#[test]
fn switching_the_loop_off_builds_the_missing_processors_before_taking_any() {
    let registry = stereo_loop_registry();
    let mut graph = live_graph(&drive_loop_amp(true), &registry);
    let before = graph.runtimes_for(&chain(true).id);
    assert_eq!(before.len(), 2, "fixed slots: one per output pipeline");
    let head_drive = serial_of(&before[0], 0, "drive");
    graph
        .upsert_chain(&drive_loop_amp(false), SR, &HashMap::new(), false, &TARGETS, &registry)
        .expect("the switch applies in place");
    assert_eq!(
        crate::runtime_block_builders::builds_while_taken(),
        0,
        "#980 P3: a processor was built while the runtimes had given theirs up — \
         every output played the guitar unprocessed meanwhile"
    );
    let after = graph.runtimes_for(&chain(false).id);
    assert!(Arc::ptr_eq(&after[0], &before[0]), "the switch is in place");
    assert_eq!(
        serial_of(&after[0], 0, "drive"),
        head_drive,
        "slot 0 keeps the cut head's drive — not a new instance"
    );
    assert_ne!(
        serial_of(&after[1], 0, "drive"),
        head_drive,
        "Out 2's pipeline runs a drive of its own"
    );
}
```

Step 1b — a test seam, behaviour-preserving, so the new test compiles and fails on its assertion. In `crates/engine/src/runtime_block_builders.rs`, right after the imports:

```rust
#[cfg(test)]
thread_local! {
    /// #980 P3 tests: block processors built from scratch on this thread.
    static FRESH_NODE_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// #980 P3 tests: of those, the ones an in-place update built while the
    /// runtimes it updates had given their processors up.
    static BUILDS_WHILE_TAKEN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// #980 P3 tests: block processors built from scratch on this thread so far.
#[cfg(test)]
pub(crate) fn fresh_node_builds() -> usize {
    FRESH_NODE_BUILDS.with(std::cell::Cell::get)
}

/// #980 P3 tests: an in-place update built `count` processors while its
/// runtimes' processors were taken (their outputs played the input
/// unprocessed meanwhile).
#[cfg(test)]
pub(crate) fn note_builds_while_taken(count: usize) {
    BUILDS_WHILE_TAKEN.with(|total| total.set(total.get() + count));
}

/// #980 P3 tests: processors built on this thread while an in-place update
/// held its runtimes' processors — zero when every build happens first.
#[cfg(test)]
pub(crate) fn builds_while_taken() -> usize {
    BUILDS_WHILE_TAKEN.with(std::cell::Cell::get)
}
```

and make the first statement of `fn build_block_runtime_node` (`:289`, before `Ok(match &block.kind {`):

```rust
    #[cfg(test)]
    FRESH_NODE_BUILDS.with(|builds| builds.set(builds.get() + 1));
```

In `crates/engine/src/runtime_graph_update.rs` (today's `update_chain_runtime_state_impl`), right after the `// Step 1: Extract existing blocks from all input states (brief lock)` statement (the `let mut existing_per_input … = { … };` block) add

```rust
    #[cfg(test)]
    let builds_at_take = crate::runtime_block_builders::fresh_node_builds();
```

and right after `runtime.output_routes.store(Arc::new(new_output_routes));` add

```rust
    #[cfg(test)]
    crate::runtime_block_builders::note_builds_while_taken(
        crate::runtime_block_builders::fresh_node_builds() - builds_at_take,
    );
```

(Step 3 rewrites this file; the seam moves into `update_runtimes_together`.)

- [ ] **Step 2: Run the tests to verify they fail**

```bash
(cd $S && cargo test -p engine --lib issue_980_fixed_slot_switch)
```

Expected (`upsert_chain` updates `(chain, 0)` fully during the watcher's first hold, then waits for slot 1; each slot only reuses its own nodes and builds the rest after taking them):
- `switching_the_loop_on_never_gives_out_2_two_writers` FAILS "#980 P3: slot 0 went live as the cut while slot 1 still ran the Out 2 pipeline — Out 2 played both until slot 1 was rebuilt".
- `switching_the_loop_off_never_leaves_out_2_without_a_writer` FAILS "#980 P3: slot 0 dropped Out 2 while slot 1 was still idle — Out 2 went silent until slot 1 was rebuilt".
- `a_loop_switch_moves_each_block_with_its_processor` FAILS "head B's drive must keep the processor it had in slot 1 — not a new instance" (`left` ≠ `right`: a fresh serial).
- `switching_the_loop_off_builds_the_missing_processors_before_taking_any` FAILS "#980 P3: a processor was built while the runtimes had given theirs up …" (`left` ≥ 1 — slot 1 built its `drive` after giving its processors up —, `right: 0`).

- [ ] **Step 3: Implement**

Create `crates/engine/src/runtime_node_pool.rs`:

```rust
//! Responsibility: lends a chain's old block processors to the runtimes being rebuilt.
//!
//! #967 / #980 P3: an in-place rebuild reuses each block's old node — the
//! processor and its state (a delay's tail, a VST3 instance) — instead of
//! building it again. A chain's runtimes are rebuilt together
//! (`runtime_graph_update_together`), and an insert switch moves a pipeline's
//! blocks between them (the loop's cut runs in slot 0, each loop-off pipeline
//! in its own slot). Which old node each rebuilt block takes is decided before
//! the pool exists (`runtime_node_plan`, over a snapshot of the live nodes);
//! the pool only hands those nodes over by `instance_serial`. It also
//! remembers which node sat where, so a failed rebuild puts every one back.
//!
//! Setup-time only.

use std::sync::Arc;

use crate::runtime::ChainRuntimeState;
use crate::runtime_state::{lock_recover, BlockRuntimeNode, InputProcessingState};

pub(crate) struct NodePool {
    /// Per runtime, per old segment, the nodes not lent yet.
    nodes: Vec<Vec<Vec<BlockRuntimeNode>>>,
    /// Per runtime, per old segment, the `instance_serial` of every node it held.
    serials: Vec<Vec<Vec<u64>>>,
}

impl NodePool {
    /// Take every processor out of every runtime's live segments (a brief
    /// `processing` lock each). From here to the commit the runtimes keep their
    /// segments with no blocks — they play their input unprocessed — so
    /// nothing is built in between: `runtime_node_plan` built it already.
    pub(crate) fn take_live(runtimes: &[Arc<ChainRuntimeState>]) -> Self {
        let mut nodes = Vec::with_capacity(runtimes.len());
        let mut serials = Vec::with_capacity(runtimes.len());
        for runtime in runtimes {
            let taken: Vec<Vec<BlockRuntimeNode>> = {
                let mut processing = lock_recover(&runtime.processing, "chain runtime");
                processing
                    .input_states
                    .iter_mut()
                    .map(|state| std::mem::take(&mut state.blocks))
                    .collect()
            };
            serials.push(
                taken
                    .iter()
                    .map(|segment| segment.iter().map(|node| node.instance_serial).collect())
                    .collect(),
            );
            nodes.push(taken);
        }
        Self { nodes, serials }
    }

    /// Spillover (#454-T5): old segment `index` of runtime `owner`, whole — it
    /// rings out as the new segment's tail.
    pub(crate) fn take_segment(&mut self, owner: usize, index: usize) -> Vec<BlockRuntimeNode> {
        self.nodes
            .get_mut(owner)
            .and_then(|segments| segments.get_mut(index))
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// The old nodes with these `instance_serial`s, in that order, wherever
    /// they sat — `runtime_node_plan` chose them. A serial the pool no longer
    /// holds is skipped (that block is then built by the assembly, as before).
    pub(crate) fn take_serials(&mut self, serials: &[u64]) -> Vec<BlockRuntimeNode> {
        let mut taken = Vec::with_capacity(serials.len());
        for serial in serials {
            let found = self.nodes.iter_mut().flatten().find_map(|segment| {
                segment
                    .iter()
                    .position(|node| node.instance_serial == *serial)
                    .map(|at| segment.remove(at))
            });
            taken.extend(found);
        }
        taken
    }

    /// A rebuild failed: give every runtime back exactly the nodes it held —
    /// from what the pool still has and from what the rebuild already took
    /// (`built`, per runtime). Returns the nodes no runtime held (built
    /// fresh), for the caller to drop outside every lock.
    pub(crate) fn restore(
        self,
        runtimes: &[Arc<ChainRuntimeState>],
        built: Vec<Vec<InputProcessingState>>,
    ) -> Vec<BlockRuntimeNode> {
        let mut loose: Vec<BlockRuntimeNode> = self.nodes.into_iter().flatten().flatten().collect();
        for states in built {
            for mut state in states {
                loose.append(&mut state.blocks);
                if let Some(mut tail) = state.outgoing.take() {
                    loose.append(&mut tail.blocks);
                }
            }
        }
        for (runtime, serials) in runtimes.iter().zip(&self.serials) {
            let restored: Vec<Vec<BlockRuntimeNode>> = serials
                .iter()
                .map(|segment| {
                    segment
                        .iter()
                        .filter_map(|serial| {
                            loose
                                .iter()
                                .position(|node| node.instance_serial == *serial)
                                .map(|at| loose.swap_remove(at))
                        })
                        .collect()
                })
                .collect();
            let mut processing = lock_recover(&runtime.processing, "chain runtime");
            for (state, blocks) in processing.input_states.iter_mut().zip(restored) {
                state.blocks = blocks;
            }
        }
        loose
    }

    /// What no rebuilt segment took — dropped by the caller after the last
    /// commit, outside every lock (#670).
    pub(crate) fn into_leftovers(self) -> Vec<BlockRuntimeNode> {
        self.nodes.into_iter().flatten().flatten().collect()
    }
}
```

Create `crates/engine/src/runtime_node_plan.rs`:

```rust
//! Responsibility: readies every processor a chain's rebuilt runtimes need before any runtime gives its own up.
//!
//! #980 P3: an in-place rebuild takes every runtime's processors
//! (`NodePool::take_live`), and a runtime plays its input unprocessed until
//! the commit. A processor built in that window — a second instance of a
//! block the loop's cut ran once, a block whose bus changed, a model on each
//! of four pipeline runtimes — kept every output of the chain dry for as long
//! as a VST3 or a NAM model took to load. So this plan walks each new segment
//! the way `build_runtime_block_nodes` will, over a snapshot of the live
//! nodes: a block takes an old node with its id, bus, content (#588) and rate
//! whose snapshot is the block or differs only in `enabled` — from its
//! runtime's old segment of the same index first (split-mono siblings keep
//! their own nodes), then its runtime's other old segments, then the other
//! runtimes at the same rate (a node is built for one rate). A block no old
//! node can stand in for is built HERE, while every runtime still plays with
//! its own processors — a block whose effect or model changed included. A
//! block whose parameters changed on the same audio processor keeps its old
//! node, which the rebuild retunes in place or rebuilds (#358), as before: only
//! the processor can tell whether a retune takes.
//!
//! Setup-time only.

use std::sync::Arc;

use block_core::AudioChannelLayout;
use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind};
use project::chain::Chain;

use crate::runtime::ChainRuntimeState;
use crate::runtime_block_builders::{build_block_runtime_node, node_emits_mono_content};
use crate::runtime_graph_assemble::segment_bus;
use crate::runtime_graph_update::{segment_output_channels, UpdatePlan};
use crate::runtime_segments::ChainSegment;
use crate::runtime_state::{lock_recover, BlockRuntimeNode, RuntimeProcessor};

/// What one new segment runs: the old nodes it takes (by `instance_serial`)
/// and the nodes built for it before the switch.
#[derive(Default)]
pub(crate) struct SegmentNodes {
    pub(crate) lent: Vec<u64>,
    pub(crate) prebuilt: Vec<BlockRuntimeNode>,
}

/// What one live node can stand in for — read under a brief lock.
struct LiveNode {
    serial: u64,
    block_id: BlockId,
    snapshot: AudioBlock,
    input_layout: AudioChannelLayout,
    content_mono: bool,
    output_layout: AudioChannelLayout,
    bypass: bool,
    /// Runs a real audio processor (`RuntimeProcessor::Audio`).
    audio: bool,
    /// `node_emits_mono_content` fed stereo `[0]` and mono `[1]` content.
    emits_mono: [bool; 2],
}

impl LiveNode {
    fn read(node: &BlockRuntimeNode) -> Self {
        Self {
            serial: node.instance_serial,
            block_id: node.block_id.clone(),
            snapshot: node.block_snapshot.clone(),
            input_layout: node.input_layout,
            content_mono: node.content_mono,
            output_layout: node.output_layout,
            bypass: matches!(node.processor, RuntimeProcessor::Bypass),
            audio: matches!(node.processor, RuntimeProcessor::Audio(_)),
            emits_mono: [
                node_emits_mono_content(node, false),
                node_emits_mono_content(node, true),
            ],
        }
    }

    fn emits_mono(&self, content_mono: bool) -> bool {
        self.emits_mono[usize::from(content_mono)]
    }

    /// Same block on the same bus and content.
    fn sits_like(&self, block: &AudioBlock, bus: AudioChannelLayout, content_mono: bool) -> bool {
        self.block_id == block.id && self.input_layout == bus && self.content_mono == content_mono
    }

    /// `try_reuse_block_node` keeps this node as it is for `block`: the same
    /// block, or the same block switched on or off — never a bypass node
    /// switched on, which needs a real processor.
    fn stands_in_for(&self, block: &AudioBlock) -> bool {
        if self.snapshot == *block {
            return true;
        }
        let mut toggled = self.snapshot.clone();
        toggled.enabled = block.enabled;
        toggled == *block && !(self.bypass && block.enabled)
    }

    /// The same block with other parameters, which `try_in_place_param_update`
    /// MAY retune in place (#358): an enabled audio processor of the same kind,
    /// effect and model. Anything else is rebuilt for certain.
    fn may_retune_to(&self, block: &AudioBlock) -> bool {
        let same_model = match (self.snapshot.model_ref(), block.model_ref()) {
            (Some(prev), Some(next)) => {
                prev.effect_type == next.effect_type && prev.model == next.model
            }
            _ => false,
        };
        self.audio
            && block.enabled
            && self.snapshot.enabled
            && std::mem::discriminant(&self.snapshot.kind) == std::mem::discriminant(&block.kind)
            && same_model
    }
}

/// Per runtime, per old segment, the live nodes not claimed yet.
type Live = Vec<Vec<Vec<Option<LiveNode>>>>;

/// Plan every runtime's nodes for `plans` (one per runtime, in `runtimes`
/// order), building now whatever no old node can stand in for. Returns, per
/// runtime, one [`SegmentNodes`] per new segment. `spillover` (#454-T5) reuses
/// no node — the old segments ring out as tails — so every processing block
/// is built now.
pub(crate) fn plan_nodes(
    runtimes: &[Arc<ChainRuntimeState>],
    chain: &Chain,
    plans: &[UpdatePlan],
    spillover: bool,
) -> Vec<Vec<SegmentNodes>> {
    let mut live: Live = runtimes
        .iter()
        .map(|runtime| {
            let processing = lock_recover(&runtime.processing, "chain runtime");
            processing
                .input_states
                .iter()
                .map(|state| state.blocks.iter().map(|node| Some(LiveNode::read(node))).collect())
                .collect()
        })
        .collect();
    let rates: Vec<u32> = runtimes
        .iter()
        .map(|runtime| runtime.sample_rate().to_bits())
        .collect();
    let mut readied = Vec::with_capacity(runtimes.len());
    for (owner, (runtime, plan)) in runtimes.iter().zip(plans).enumerate() {
        let mut segments = Vec::with_capacity(plan.segments.len());
        for (index, segment) in plan.segments.iter().enumerate() {
            let order = if spillover {
                Vec::new()
            } else {
                lookup_order(&live, &rates, owner, index)
            };
            segments.push(walk_segment(
                chain,
                runtime.sample_rate(),
                plan,
                segment,
                &mut live,
                &order,
            ));
        }
        readied.push(segments);
    }
    readied
}

/// Where a block of new segment `preferred` of runtime `owner` looks for an
/// old node: that runtime's old segment `preferred`, its other old segments in
/// order, then every other runtime at the same rate.
fn lookup_order(live: &Live, rates: &[u32], owner: usize, preferred: usize) -> Vec<(usize, usize)> {
    let own = live.get(owner).map_or(0, Vec::len);
    let mut order: Vec<(usize, usize)> = std::iter::once(preferred)
        .filter(|&p| p < own)
        .chain((0..own).filter(|&s| s != preferred))
        .map(|segment| (owner, segment))
        .collect();
    for runtime in 0..live.len() {
        if runtime != owner && rates[runtime] == rates[owner] {
            order.extend((0..live[runtime].len()).map(|segment| (runtime, segment)));
        }
    }
    order
}

/// The first unclaimed live node, in `order`, that `fits`.
fn claim(
    live: &mut Live,
    order: &[(usize, usize)],
    fits: impl Fn(&LiveNode) -> bool,
) -> Option<LiveNode> {
    order.iter().find_map(|&(runtime, segment)| {
        live[runtime][segment]
            .iter_mut()
            .find(|node| node.as_ref().is_some_and(|node| fits(node)))
            .and_then(Option::take)
    })
}

/// One new segment: the block walk `build_runtime_block_nodes` will do, over
/// the live nodes instead of the taken ones.
fn walk_segment(
    chain: &Chain,
    rate: f32,
    plan: &UpdatePlan,
    segment: &ChainSegment,
    live: &mut Live,
    order: &[(usize, usize)],
) -> SegmentNodes {
    let mut nodes = SegmentNodes::default();
    let (mut bus, mut content_mono) =
        segment_bus(&segment.input, &segment_output_channels(plan, segment));
    for block in segment.block_indices.iter().filter_map(|&b| chain.blocks.get(b)) {
        if !block.enabled {
            // Any old node of this id, else a bypass node — nothing to build.
            if let Some(node) = claim(live, order, |n| n.block_id == block.id) {
                content_mono = node.emits_mono(content_mono);
                nodes.lent.push(node.serial);
            }
            continue;
        }
        if matches!(
            block.kind,
            AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_)
        ) {
            continue;
        }
        let kept = if matches!(block.kind, AudioBlockKind::Select(_)) {
            // A select node is reused on its bus alone; it retunes its options.
            claim(live, order, |n| n.block_id == block.id && n.input_layout == bus)
        } else {
            claim(live, order, |n| n.sits_like(block, bus, content_mono) && n.stands_in_for(block))
                .or_else(|| {
                    // Parameters changed: retuned in place or rebuilt, as before.
                    claim(live, order, |n| {
                        n.sits_like(block, bus, content_mono) && n.may_retune_to(block)
                    })
                })
        };
        match kept {
            Some(node) => {
                bus = node.output_layout;
                content_mono = node.emits_mono(content_mono);
                nodes.lent.push(node.serial);
            }
            None => match build_block_runtime_node(chain, block, bus, content_mono, rate) {
                Ok(node) => {
                    bus = node.output_layout;
                    content_mono = node_emits_mono_content(&node, content_mono);
                    nodes.prebuilt.push(node);
                }
                // The assembly builds it — and bypasses it on the same error,
                // as before (#574); what follows is no longer foreseeable.
                Err(_) => break,
            },
        }
    }
    nodes
}
```

Create `crates/engine/src/runtime_graph_update_together.rs`:

```rust
//! Responsibility: switches every runtime slot of a chain to its new state at one instant.
//!
//! #980 P3: a chain with a bound insert keeps fixed runtime slots, and the loop
//! switch moves a tail between them (off: its own pipeline's slot; on: the cut
//! in slot 0). Updated one slot after another, that tail would sit with two
//! writers (double level) or none (silence) for as long as the next slot took
//! to go live — and a dsp-worker inside `process` holds its runtime's
//! `processing` lock for a whole period, so even back-to-back commits could
//! land a period apart. So:
//!
//! 1. every runtime is planned (a missing pipeline fails before any runtime
//!    is touched);
//! 2. every processor no old node can stand in for is built while every
//!    runtime still plays with its own (`runtime_node_plan`) — no output plays
//!    its input unprocessed while a VST3 or a NAM model loads;
//! 3. every runtime's processors go into one `NodePool` and every runtime's
//!    next state is assembled from it — nodes only move;
//! 4. every runtime's `processing` lock is taken, in slot order, and every
//!    runtime is committed under those locks — no dsp-worker runs any runtime
//!    of the chain between the first swap and the last route store;
//! 5. the replaced processors are dropped after the last lock is released,
//!    outside every lock (#670).
//!
//! A single runtime takes the same path.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use domain::ids::DeviceId;
use domain::io_binding::IoBinding;
use project::chain::Chain;

use crate::runtime::ChainRuntimeState;
use crate::runtime_graph_update::{
    apply_commit, build_next_input_states, next_output_routes, plan_update, prepare_commit,
};
use crate::runtime_node_plan::plan_nodes;
use crate::runtime_node_pool::NodePool;
use crate::runtime_state::{lock_recover, InputProcessingState};

/// Rebuild every runtime in `runtimes` in place for `chain` and publish them
/// together. A failed build changes no runtime: each one gets back exactly
/// the processors it had.
pub(crate) fn update_runtimes_together(
    runtimes: &[Arc<ChainRuntimeState>],
    chain: &Chain,
    device_rates: &HashMap<DeviceId, f32>,
    reset_output_queue: bool,
    elastic_targets: &[usize],
    spillover: bool,
    registry: &[IoBinding],
) -> Result<()> {
    // (1)
    let plans = runtimes
        .iter()
        .map(|runtime| plan_update(runtime, chain, registry))
        .collect::<Result<Vec<_>>>()?;
    // (2) Every runtime still plays with its own processors.
    let readied = plan_nodes(runtimes, chain, &plans, spillover);
    // (3) From here to the commit the runtimes play their input unprocessed.
    let mut pool = NodePool::take_live(runtimes);
    #[cfg(test)]
    let builds_at_take = crate::runtime_block_builders::fresh_node_builds();
    let mut built: Vec<Vec<InputProcessingState>> = Vec::with_capacity(runtimes.len());
    for (owner, ((runtime, plan), nodes)) in runtimes.iter().zip(&plans).zip(readied).enumerate() {
        let mut states = Vec::with_capacity(plan.segments.len());
        let outcome = build_next_input_states(
            runtime,
            chain,
            plan,
            &mut pool,
            owner,
            nodes,
            spillover,
            &mut states,
        );
        built.push(states);
        if let Err(e) = outcome {
            log::error!(
                "[engine] rebuild failed for chain '{}': {e} — restoring previous state",
                chain.id.0
            );
            drop(pool.restore(runtimes, built));
            return Err(e);
        }
    }
    let prepared: Vec<_> = runtimes
        .iter()
        .zip(plans)
        .zip(built)
        .map(|((runtime, plan), states)| {
            let routes = next_output_routes(
                runtime,
                chain,
                &plan,
                device_rates,
                reset_output_queue,
                elastic_targets,
            );
            prepare_commit(runtime, plan, states, routes)
        })
        .collect();
    // (4) Every runtime of the chain goes live at one instant: all of their
    // `processing` locks are held (taken in slot order — the only place that
    // holds two) until the last route is stored.
    let replaced: Vec<Vec<InputProcessingState>> = {
        let mut locked: Vec<_> = runtimes
            .iter()
            .map(|runtime| lock_recover(&runtime.processing, "chain runtime"))
            .collect();
        runtimes
            .iter()
            .zip(locked.iter_mut())
            .zip(prepared)
            .map(|((runtime, processing), prepared)| {
                apply_commit(runtime, processing, prepared, reset_output_queue, chain.volume)
            })
            .collect()
    };
    #[cfg(test)]
    crate::runtime_block_builders::note_builds_while_taken(
        crate::runtime_block_builders::fresh_node_builds() - builds_at_take,
    );
    // (5) Every lock is released — NOW the old nodes (NAM models, IR FFT
    // states) may run their multi-ms destructors without starving an audio
    // worker.
    drop(replaced);
    drop(pool.into_leftovers());
    Ok(())
}
```

Replace `crates/engine/src/runtime_graph_update.rs` with:

```rust
//! Responsibility: rebuilds a chain runtime in place.
//! In-place (lock-free) chain-runtime rebuild (issue #792 split from
//! `runtime_graph.rs`).
//!
//! Setup-time only — the swap into the live `ChainRuntimeState` is brief and
//! lock-guarded; the expensive work (building new nodes, dropping old NAM/IR
//! processors) happens OUTSIDE the audio worker's `processing` try_lock so a
//! param/preset edit never drops audio (issue #670). Reuses the shared
//! per-segment assembly helpers in `runtime_graph_assemble` and the pipeline
//! grouping in `pipeline_grouping`.
//!
//! #980 P3: the steps of one runtime's rebuild — resolve its pipeline's
//! segments, assemble its next state from the nodes readied for it, prepare
//! and apply its commit — live here; `runtime_graph_update_together` runs
//! them for every runtime of a chain at once, and a single-runtime update is
//! that same path with one runtime.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{anyhow, Result};

use domain::ids::{BlockId, DeviceId};
use domain::io_binding::IoBinding;
use project::chain::Chain;

use crate::pipeline_grouping::group_segments_into_pipelines;
use crate::runtime::{ChainRuntimeState, FADE_IN_FRAMES, PROBE_IDLE};
use crate::runtime_endpoints::{effective_inputs, effective_outputs, resolve_chain_io, OutputEntry};
use crate::runtime_graph_assemble::{
    build_input_processing_state, build_output_routing_state, collect_bypass_block_ids,
    output_entry_layout, target_for_route,
};
use crate::runtime_graph_update_together::update_runtimes_together;
use crate::runtime_node_plan::SegmentNodes;
use crate::runtime_node_pool::NodePool;
use crate::runtime_segments::{split_chain_into_segments, ChainSegment};
use crate::runtime_state::{
    lock_recover, ChainProcessingState, InputCallbackScratch, InputProcessingState, OutgoingTail,
    OutputRoutingState, SPILLOVER_FRAMES,
};

/// In-place lock-free rebuild (param/preset edit): old processors are reused
/// and dropped. Audio is click-safe via the per-segment fade-in.
pub fn update_chain_runtime_state(
    runtime: &Arc<ChainRuntimeState>,
    chain: &Chain,
    _sample_rate: f32, // #736: each runtime rebuilds at its own rate (`runtime.sample_rate()`)
    reset_output_queue: bool,
    elastic_targets: &[usize],
    registry: &[IoBinding],
) -> Result<()> {
    update_runtimes_together(
        std::slice::from_ref(runtime),
        chain,
        &HashMap::new(),
        reset_output_queue,
        elastic_targets,
        false,
        registry,
    )
}

/// [`update_chain_runtime_state`] knowing the rate of each device the chain
/// writes (#967): a route the edit writes for the first time — the tail of an
/// output-only E/S on another interface, which only an insert's cut feeds —
/// runs at ITS device's rate and gets the cross-rate cushion, exactly as the
/// initial build would give it, instead of the runtime's rate.
pub fn update_chain_runtime_state_at_device_rates(
    runtime: &Arc<ChainRuntimeState>,
    chain: &Chain,
    device_rates: &HashMap<DeviceId, f32>,
    reset_output_queue: bool,
    elastic_targets: &[usize],
    registry: &[IoBinding],
) -> Result<()> {
    update_runtimes_together(
        std::slice::from_ref(runtime),
        chain,
        device_rates,
        reset_output_queue,
        elastic_targets,
        false,
        registry,
    )
}

/// #454-T5: same lock-free swap, but the *previous* pipeline is retained as
/// a decaying [`OutgoingTail`] on the new state so its delay/reverb tail
/// rings out in parallel (spillover) instead of being cut. The new pipeline
/// is built fresh (no processor reuse) so it fades in cleanly while the old
/// one fades out.
pub fn update_chain_runtime_state_spillover(
    runtime: &Arc<ChainRuntimeState>,
    chain: &Chain,
    _sample_rate: f32, // #736: each runtime rebuilds at its own rate (`runtime.sample_rate()`)
    reset_output_queue: bool,
    elastic_targets: &[usize],
    registry: &[IoBinding],
) -> Result<()> {
    update_runtimes_together(
        std::slice::from_ref(runtime),
        chain,
        &HashMap::new(),
        reset_output_queue,
        elastic_targets,
        true,
        registry,
    )
}

/// What a runtime's rebuild resolves before it touches the runtime.
pub(crate) struct UpdatePlan {
    /// The segments of the pipeline the runtime runs (every segment, for a
    /// whole-chain runtime).
    pub(crate) segments: Vec<ChainSegment>,
    /// The chain's outputs — every route index a segment can write.
    pub(crate) effective_outs: Vec<OutputEntry>,
}

/// Resolve the segments `runtime` runs for `chain`.
///
/// #703 / #980: a pipeline runtime is refilled with ONLY its own pipeline's
/// segments, found by the pipeline it is stamped with — never by its slot:
/// slots are positions, and the pipeline at a position moves when a guitar
/// or an output joins. Refilling by position would feed this runtime's
/// output from another guitar, or run the same guitar twice on a shared
/// device callback (audible double volume). A whole-chain runtime
/// (`owned_pipeline == None`: probe, offline) keeps every segment.
pub(crate) fn plan_update(
    runtime: &ChainRuntimeState,
    chain: &Chain,
    registry: &[IoBinding],
) -> Result<UpdatePlan> {
    let (resolved_inputs, resolved_outputs) = resolve_chain_io(chain, registry);
    let (effective_ins, eff_input_cpal_indices, effective_split_positions, eff_entry_groups) =
        effective_inputs(chain, &resolved_inputs, registry);
    let effective_outs = effective_outputs(chain, &resolved_outputs, registry);
    let all_segments = split_chain_into_segments(
        chain,
        &effective_ins,
        &eff_input_cpal_indices,
        &effective_split_positions,
        &eff_entry_groups,
        &effective_outs,
        registry,
    );
    let segments: Vec<ChainSegment> = match runtime.owned_pipeline {
        Some(owned) => group_segments_into_pipelines(chain, registry, all_segments)
            .into_iter()
            .find(|group| group.key == owned.key)
            .map(|group| group.segments)
            .ok_or_else(|| {
                anyhow!(
                    "chain '{}' in-place update: pipeline {:?} no longer exists \
                     (a topology change must take the full-rebuild path)",
                    chain.id.0,
                    owned.key
                )
            })?,
        None => all_segments,
    };
    Ok(UpdatePlan {
        segments,
        effective_outs,
    })
}

/// The device channels `segment` writes — what its processing bus is sized
/// for. Shared by the assembly and by `runtime_node_plan`, which must foresee
/// the bus each block will see.
pub(crate) fn segment_output_channels(plan: &UpdatePlan, segment: &ChainSegment) -> Vec<usize> {
    segment
        .output_route_indices
        .iter()
        .filter_map(|&idx| plan.effective_outs.get(idx))
        .flat_map(|e| e.channels.iter().copied())
        .collect()
}

/// Step 2: assemble `runtime`'s next segments into `built` from the nodes
/// readied for it — `nodes`, one entry per new segment: the old nodes it takes
/// from `pool` by serial and the ones `runtime_node_plan` built before any
/// runtime gave its processors up. `owner` is this runtime's position in
/// `pool`. Nothing is built here unless the plan could not foresee it (a
/// parameter change that cannot be retuned in place, #358). On an error
/// `built` keeps what was already assembled, so the caller can hand every
/// node back.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_next_input_states(
    runtime: &ChainRuntimeState,
    chain: &Chain,
    plan: &UpdatePlan,
    pool: &mut NodePool,
    owner: usize,
    nodes: Vec<SegmentNodes>,
    spillover: bool,
    built: &mut Vec<InputProcessingState>,
) -> Result<()> {
    let mut nodes = nodes.into_iter();
    for (i, segment) in plan.segments.iter().enumerate() {
        let SegmentNodes { lent, prebuilt } = nodes.next().unwrap_or_default();
        // #454-T5 spillover: the old segment rings out as the new one's tail
        // and the new pipeline runs only processors built for it. Otherwise
        // (#967, #980 P3) each block takes the old node the plan chose — from
        // any old segment of any runtime of the switch at the same rate — so a
        // loop switch moves a block with its processor (no VST3
        // re-instantiated, no delay tail cut).
        let (lent, tail_blocks) = if spillover {
            let old = pool.take_segment(owner, i);
            (Vec::new(), (!old.is_empty()).then_some(old))
        } else {
            (pool.take_serials(&lent), None)
        };
        // A segment no old node carries over into fades in, exactly as when it
        // was built here from scratch.
        let fades_in = lent.is_empty();
        let mut existing = lent;
        existing.extend(prebuilt);
        let existing = (!existing.is_empty()).then_some(existing);
        // #736: rebuild at the runtime's OWN built rate, not the chain scalar
        let mut input_state = build_input_processing_state(
            chain,
            &segment.input,
            &segment_output_channels(plan, segment),
            runtime.sample_rate(),
            existing,
            Some(&segment.block_indices),
            segment.output_route_indices.clone(),
            segment.mid_output_taps.clone(),
            segment.split_mono_sibling_count,
        )?;
        if fades_in {
            input_state.fade_in_remaining = FADE_IN_FRAMES;
        }
        if let Some(blocks) = tail_blocks {
            input_state.outgoing = Some(Box::new(OutgoingTail {
                blocks,
                frames_remaining: SPILLOVER_FRAMES,
                scratch: Vec::with_capacity(2048),
            }));
        }
        built.push(input_state);
    }
    // #85: keep the DI-loop marking across a live rebuild, or the mid pipeline
    // goes silent the first time the user turns a knob with the loop playing.
    crate::runtime_graph_assemble::mark_di_loop_pipelines(&plan.segments, built);
    Ok(())
}

/// The routes `runtime` writes after the rebuild.
///
/// Output routes (#670): REUSE the existing route when its endpoint shape
/// is unchanged (the param-edit / block-toggle case). A fresh empty buffer
/// here used to (a) discard the in-flight audio — the audible gap on every
/// edit — and (b) restart the standing cushion at zero, which never
/// refills in producer/consumer lockstep, leaving the chain permanently
/// fragile after the first edit (owner-reported underruns while playing,
/// reproduced by rebuild_while_playing_keeps_the_cushion). Reusing the
/// Arc keeps both the buffered audio and the cushion. A genuinely changed
/// endpoint (or an explicit queue reset) still gets a fresh route.
pub(crate) fn next_output_routes(
    runtime: &ChainRuntimeState,
    chain: &Chain,
    plan: &UpdatePlan,
    device_rates: &HashMap<DeviceId, f32>,
    reset_output_queue: bool,
    elastic_targets: &[usize],
) -> Vec<Option<Arc<OutputRoutingState>>> {
    let segments = &plan.segments;
    let old_output_routes = runtime.output_routes.load();
    plan.effective_outs
        .iter()
        .enumerate()
        .map(|(route_idx, o)| {
            // #947: a route exists only for an output this runtime writes.
            if !crate::runtime_graph_assemble::route_is_written(segments, route_idx) {
                return None;
            }
            let old_route = old_output_routes.get(route_idx).and_then(Option::as_ref);
            // #85: keep the route on its own device's rate across a rebuild —
            // the old route knows it, and a rebuild never changes a device —
            // and so its deeper cross-rate cushion too: a tap rebuilt at the
            // lockstep depth starved on the first bunched callback after
            // every live edit ("mudei a ordem e deu merda").
            // #967: a route this edit writes for the first time runs at its
            // OWN device's rate when the caller knows it, like the initial build.
            let route_rate = old_route
                .map(|old| old.sample_rate)
                .or_else(|| device_rates.get(&o.device_id).copied())
                .unwrap_or_else(|| runtime.sample_rate());
            let cushion = crate::route_cushion::route_cushion(
                target_for_route(elastic_targets, route_idx),
                route_rate,
                runtime.sample_rate(),
                crate::route_convolution::route_has_convolution(chain, segments, route_idx),
                crate::route_clock::route_on_producer_clock(segments, route_idx, &o.device_id),
            );
            if !reset_output_queue {
                if let Some(old) = old_route {
                    if old.output_channels == o.channels
                        && old.buffer.layout() == output_entry_layout(o)
                        && old.buffer.target_level() == cushion.target
                        && old.buffer.capacity() == cushion.capacity
                    {
                        return Some(Arc::clone(old));
                    }
                }
            }
            // Fresh route on a rebuild: born at its resting cushion like any
            // other (#965). An unprimed route here left the chain permanently
            // fragile (#670: fill ~0, every scheduling wobble on a real USB
            // interface popped the output empty — the owner's random clicks
            // after adding/swapping a cab).
            let fresh = build_output_routing_state(o, cushion, route_rate);
            if let Some(old) = old_route {
                fresh.buffer.seed_last_frame_from(&old.buffer);
            }
            Some(Arc::new(fresh))
        })
        .collect()
}

/// One runtime's rebuilt state, ready to go live (`apply_commit`).
pub(crate) struct PreparedCommit {
    input_states: Vec<InputProcessingState>,
    input_to_segments: Vec<Vec<usize>>,
    output_routes: Vec<Option<Arc<OutputRoutingState>>>,
    bypass_block_ids: HashSet<BlockId>,
}

/// Step 2.5, before any `processing` lock: refresh `runtime`'s stream handles
/// and derive everything its commit publishes, so the commit only swaps.
pub(crate) fn prepare_commit(
    runtime: &ChainRuntimeState,
    plan: UpdatePlan,
    input_states: Vec<InputProcessingState>,
    output_routes: Vec<Option<Arc<OutputRoutingState>>>,
) -> PreparedCommit {
    // Refresh stream_handles — picks up new handles from rebuilt blocks
    // (e.g. block param changed → new processor → new Arc; old Arc in map would be stale)
    {
        let mut handles = lock_recover(&runtime.stream_handles, "stream_handles");
        handles.clear();
        for input_state in &input_states {
            for block in &input_state.blocks {
                if let Some(ref handle) = block.stream_handle {
                    handles.insert(block.block_id.clone(), Arc::clone(handle));
                }
            }
        }
    }
    // The cpal input index → segments dispatch map of the new segments.
    let max_input_idx = plan
        .segments
        .iter()
        .map(|s| s.cpal_input_index)
        .max()
        .unwrap_or(0);
    let mut input_to_segments: Vec<Vec<usize>> = vec![Vec::new(); max_input_idx + 1];
    for (seg_idx, segment) in plan.segments.iter().enumerate() {
        if segment.cpal_input_index < input_to_segments.len() {
            input_to_segments[segment.cpal_input_index].push(seg_idx);
        }
    }
    PreparedCommit {
        bypass_block_ids: collect_bypass_block_ids(&input_states),
        input_states,
        input_to_segments,
        output_routes,
    }
}

/// Step 3, under `runtime`'s `processing` lock — the caller holds the lock of
/// every runtime of the chain, so all of them go live together: swap in the
/// new segments and dispatch map, then publish the routes and the volume.
///
/// Returns the replaced segments. Their nodes (NAM models, IR FFT states) run
/// multi-ms destructors, so the caller drops them after every lock is
/// released: the audio worker only try_locks `processing`, and holding the
/// lock through those destructors made it emit silence for 3-6 buffers on
/// every cab/model swap (issue #670, owner's click when switching the CAB/IR —
/// reproduced on the real interface, 64-384 underruns per swap).
pub(crate) fn apply_commit(
    runtime: &ChainRuntimeState,
    processing: &mut ChainProcessingState,
    prepared: PreparedCommit,
    reset_output_queue: bool,
    volume: f32,
) -> Vec<InputProcessingState> {
    let PreparedCommit {
        input_states,
        input_to_segments,
        output_routes,
        bypass_block_ids,
    } = prepared;
    let old_input_states = std::mem::replace(&mut processing.input_states, input_states);
    // Issue #580: keep the lock-free `stream_count` mirror in sync with the
    // new Vec length. Updated INSIDE the same critical section that swaps the
    // Vec so any concurrent reader sees a consistent (new Vec length, new
    // count) pair after the lock releases. Relaxed ordering — the value is
    // purely advisory for the meter timer's subscription loop.
    runtime.stream_count.store(
        processing.input_states.len(),
        std::sync::atomic::Ordering::Relaxed,
    );
    // Issue #580 follow-up: refresh the lock-free bypass mirror from the new
    // nodes so re-enabling a (still) born-disabled block keeps declining the
    // fast path. Swapped inside the same critical section as the Vec so a
    // reader never sees a stale (nodes, bypass-set) pair.
    runtime.bypass_block_ids.store(Arc::new(bypass_block_ids));
    // #967: publish which device callbacks have work, in the same critical
    // section, so an idle stream never takes this lock again.
    runtime.fed_inputs.store(
        crate::runtime_chain_state::fed_inputs_mask(&input_to_segments),
        std::sync::atomic::Ordering::Relaxed,
    );
    processing.input_to_segments = input_to_segments;
    // Cancel any in-flight latency probe — its beep was pushed into the old
    // queue that we're about to discard, so leaving the state Fired would wait
    // forever for a detection that will never happen.
    runtime
        .probe_state
        .store(PROBE_IDLE, std::sync::atomic::Ordering::Release);
    // Resize scratches to match the new input count, preserving existing
    // allocated capacity for slots that still exist.
    let new_len = processing.input_to_segments.len();
    processing
        .input_scratches
        .resize_with(new_len, InputCallbackScratch::default);
    // #967: a route converter's phase and history belong to the route it fed.
    // Keep it only where the route itself survives this update (the same
    // Arc); a route built fresh — or gone — starts clean, so a few samples
    // from before the edit are never played in front of it.
    let old_routes = runtime.output_routes.load();
    for scratch in processing.input_scratches.iter_mut() {
        scratch.route_resamplers.retain(|route_idx, _| {
            match (
                output_routes.get(*route_idx).and_then(Option::as_ref),
                old_routes.get(*route_idx).and_then(Option::as_ref),
            ) {
                (Some(new), Some(old)) => Arc::ptr_eq(new, old),
                _ => false,
            }
        });
    }
    // Seed each new buffer with the previous buffer's last pushed frame so a
    // brief underrun during the transition repeats the tail of the old audio
    // rather than jumping to silence. No producer pushes while the chain's
    // `processing` locks are held, so this is the old route's true last frame.
    if !reset_output_queue {
        for (new_route, old_route) in output_routes.iter().zip(old_routes.iter()) {
            if let (Some(new_route), Some(old_route)) = (new_route, old_route) {
                new_route.buffer.seed_last_frame_from(&old_route.buffer);
            }
        }
    }
    runtime.output_routes.store(Arc::new(output_routes));
    // Issue #440: chain edits (the volume slider included) re-apply the
    // preset volume on the master output without destroying the runtime —
    // an atomic store the audio thread sees on its next callback.
    runtime.set_volume_pct(volume);
    old_input_states
}
```

`crates/engine/src/lib.rs` — after `mod runtime_graph_update;` add `mod runtime_graph_update_together;`; after `mod runtime_mid_output_tap;` add `mod runtime_node_plan;` and `mod runtime_node_pool;`.

`crates/engine/src/runtime_block_builders.rs` — `fn node_emits_mono_content` (`:36`) and `fn build_block_runtime_node` (`:289`) become `pub(crate) fn` (the node plan walks and builds with them); the Step 1b counters stay.

`crates/engine/src/runtime_graph_assemble.rs` — the plan must start each segment's walk on the bus the build will use, so the bus decision moves into one function. Add, right above `pub(crate) fn build_input_processing_state`:

```rust
/// The bus a segment's blocks process on, and whether the signal entering its
/// first block is effectively mono. Issue #588: a mono source is broadcast to
/// identical stereo channels (`Stereo([s, s])`), so the content entering the
/// first block is effectively mono; a DualMono/Stereo source carries
/// independent channels and is not. #980 P3: the in-place node plan
/// (`runtime_node_plan`) walks a segment from this same starting point.
pub(crate) fn segment_bus(
    input: &InputEntry,
    output_channels: &[usize],
) -> (AudioChannelLayout, bool) {
    let bus = match project::chain::processing_layout(&input.channels, output_channels, input.mode)
    {
        project::chain::ProcessingLayout::Mono => AudioChannelLayout::Mono,
        project::chain::ProcessingLayout::Stereo | project::chain::ProcessingLayout::DualMono => {
            AudioChannelLayout::Stereo
        }
    };
    (bus, matches!(input.mode, ChainInputMode::Mono))
}
```

and in `build_input_processing_state` replace `let processing_layout_channel = match proc_layout { … };` (`:277-282`) with `let (processing_layout_channel, source_is_mono) = segment_bus(input, output_channels);`, and delete the `// Issue #588: a mono source is broadcast …` comment with `let source_is_mono = matches!(input_read_layout, AudioChannelLayout::Mono);` (`:297-301`) — `input_read_layout` is Mono exactly when `input.mode` is `ChainInputMode::Mono` (`:273-276`), so the build is unchanged; `proc_layout` and `input_read_layout` stay for the log line.

`crates/engine/src/runtime_state.rs` — on the `instance_serial` field of `BlockRuntimeNode` delete the line `#[cfg_attr(not(test), allow(dead_code))]` (the node pool reads it in production now).

`crates/engine/src/runtime_graph_impl.rs`:
- Module doc lines 4-6 become: `//! The container's add / swap / remove operations. \`upsert_chain\` takes the` / `//! fast in-place path (\`update_runtimes_together\`) while the live runtimes` / `//! run the chain's pipelines, else a full rebuild. Setup-time only.`
- Replace the import `use crate::runtime_graph_update::{update_chain_runtime_state_at_device_rates, update_chain_runtime_state_spillover};` with `use crate::runtime_graph_update_together::update_runtimes_together;`.
- Add inside `impl RuntimeGraph`, after `runs_pipelines_of`:

```rust
    /// #980 P3: rebuild every runtime of `chain` in place and publish them at
    /// one instant (`update_runtimes_together`) — the live-edit path of a
    /// chain holding a VST3 (#779). The `Arc`s the live streams hold stay.
    pub fn update_chain_in_place(
        &self,
        chain: &Chain,
        device_rates: &HashMap<DeviceId, f32>,
        reset_output_queue: bool,
        elastic_targets: &[usize],
        registry: &[IoBinding],
    ) -> Result<()> {
        update_runtimes_together(
            &self.runtimes_for(&chain.id),
            chain,
            device_rates,
            reset_output_queue,
            elastic_targets,
            false,
            registry,
        )
    }
```

- In `upsert_chain_impl`, replace the body of `if !existing_groups.is_empty() && self.runs_pipelines_of(chain, registry) { … }` (Task 3 version: the `for group in &existing_groups { … }` loop and the `if let Some(rt) = … existing_groups[0] …` return) with:

```rust
            // #980 P3: every runtime of the chain is rebuilt and published at
            // one instant — a fixed-slot chain's loop switch moves a tail
            // between slots, and one slot at a time left it with two writers
            // or none while the next slot built. #454-T5 spillover builds the
            // new pipeline fresh and keeps each route at the runtime's rate,
            // as before (no device-rate map); #967: otherwise new routes run
            // at their own device's rate.
            let runtimes = self.runtimes_for(&chain.id);
            let no_device_rates = HashMap::new();
            let rates = if spillover {
                &no_device_rates
            } else {
                device_rates
            };
            update_runtimes_together(
                &runtimes,
                chain,
                rates,
                reset_output_queue,
                elastic_targets,
                spillover,
                registry,
            )?;
            if let Some(first) = runtimes.first() {
                return Ok(first.clone());
            }
```

`crates/infra-cpal/src/controller_offthread_live_rebuild.rs` — replace the VST3 branch (lines 102-127, `if chain_contains_vst3(chain) { let groups … return Ok(true); }`) with:

```rust
        if chain_contains_vst3(chain) {
            // #967: a route the edit writes for the first time (an insert
            // switched on feeding a tail on another interface) runs at its own
            // device's rate. #980 P3: every slot of the chain switches at one
            // instant, sharing its processors (`RuntimeGraph::update_chain_in_place`)
            // — slot by slot, a loop switch left a tail with both pipelines, or
            // none, writing it while the next slot rebuilt its VST3s.
            self.runtime_graph.update_chain_in_place(
                chain,
                &device_sample_rates,
                false,
                &elastic_targets,
                &self.io_bindings,
            )?;
            self.rearm_di_stream_after_rebuild(chain);
            return Ok(true);
        }
```

`docs/audio-config.md` — append to the P3 paragraph written in Task 16:

```markdown
An in-place update — a VST3 chain (#779), the JACK backend, the synchronous
upsert — rebuilds every runtime of the chain together
(`engine::runtime_graph_update_together`): every runtime is planned first, so
a missing pipeline fails before any runtime is touched; then, while every
runtime still plays with its own processors, a walk over a snapshot of the
live nodes (`engine::runtime_node_plan`) chooses each block's old node — in
its own runtime first, then in the other runtimes at the same rate, so a loop
switch moves a block with its processor (no VST3 re-instantiated, no delay
tail cut) — and builds every processor no old node can stand in for (loop
off, the second instance of a block the cut ran once). Only then are the
processors taken (`engine::runtime_node_pool`) — a runtime with no blocks
plays its input unprocessed, so that window only moves nodes — and every
runtime is committed under the `processing` locks of all of them, taken in
slot order: a dsp-worker holds its runtime's lock for a whole period, so
commits made one after another could land a period apart, leaving a tail with
two writers (double level) or none (silence). The replaced processors are
dropped after the last lock is released.
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
(cd $S && cargo test -p engine --lib issue_980_fixed_slot_switch)   # 4 passed (~1 s: the holds)
(cd $S && cargo test -p engine --lib -- issue_967_insert_streams issue_947 stream_isolation runtime_lock_recovery audio_under_block_toggle rig_spillover rt_block_assembly issue_980)
(cd $S && cargo test -p engine --test issue_588_no_model_reload_on_chain_edit)
(cd $S && cargo test -p infra-cpal --lib controller_live_edit_replicates_user_report)
(cd $S && cargo test -p infra-cpal --test issue_779_vst3_live_param_no_reinstantiate)
git -C $S grep -n "update_chain_runtime_state_at_device_rates\|update_chain_runtime_state_spillover" -- crates/infra-cpal/src crates/engine/src/runtime_graph_impl.rs
```

Expected: all PASS (`an_in_place_update_keeps_the_processors_that_changed_segment` pins #967/#779 node reuse; `rig_spillover` pins #454-T5, whose new pipeline is now built before the take and still fades in; `issue_588_no_model_reload_on_chain_edit` pins that a plain edit builds nothing); the last grep prints nothing (no per-slot loop left on a live chain). Then run the Gate and, for JACK parity, `$LINUX cargo test -p engine --lib issue_980` and `$LINUX cargo build --workspace`.

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/engine/src/runtime_node_pool.rs crates/engine/src/runtime_node_plan.rs \
  crates/engine/src/runtime_graph_update_together.rs crates/engine/src/runtime_graph_update.rs \
  crates/engine/src/runtime_block_builders.rs crates/engine/src/runtime_graph_assemble.rs \
  crates/engine/src/runtime_graph_impl.rs crates/engine/src/lib.rs \
  crates/engine/src/runtime_state.rs crates/engine/src/runtime.rs \
  crates/engine/src/issue_980_fixed_slot_switch_tests.rs \
  crates/infra-cpal/src/controller_offthread_live_rebuild.rs docs/audio-config.md
git -C $S commit -m "fix(#980): an in-place update switches every slot of a chain together, sharing their processors" -m "Every processor the switch adds is built before any runtime gives its own up; every slot is committed under all the chain's processing locks.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 18: A pipeline change that keeps every stream signature rebuilds the chain's streams

The streams (cpal) and the JACK client hold a chain's runtimes by slot, each slot bound to one pipeline. `upsert_chain_with_resolved` rebuilds them only when a stream signature changes (`controller_upsert.rs:111-127`), but an edit can move the pipelines and keep every signature. Example: two E/S re-paired over the same I/O — X {in 0 → A} + Y {in 1 → B} becoming X {in 0, in 1 → A} + Y {→ B}. The pipelines go from (A, in 0), (B, in 1) to (A, in 0), (A, in 1); the slot ids (`[0, 1]`), so `chain_structure_signature` (`io_topology.rs:114-121`, which carries `input_group_ids` only), the flat I/O lists (`bound_io_signature`), the cpal `stream_signature` and the JACK signature (counts/device/rate, `stream_signature_types.rs:46-76`) all stay equal. Task 7 sends the edit to the synchronous path, the graph answers with a full rebuild (new `Arc`s, `runtime_graph_impl.rs` full-rebuild branch) — and the streams, their slots and the JACK `JackPeriod` keep the old runtimes: the chain plays what the graph dropped, and every later edit goes to runtimes nobody plays.

Fix: decide BEFORE the graph upsert whether the live runtimes still run the chain's pipelines (`RuntimeGraph::runs_pipelines_of`, Task 3), and rebuild the streams when they do not — on both platforms. The teardown then runs before the upsert, so it drains the runtimes the streams play (Task 4), and the new streams / JACK client are built on the graph's new runtimes (`install_chain_slots`, Task 5). `chain_structure_signature` carries `pipeline_slots` (slot + pipeline) instead of the slot ids, so the off-thread path's `chain_io_changed` (`controller_offthread_live_rebuild.rs:163`) reads such an edit as structural too; a fixed-slot insert switch keeps its signature, because its `pipeline_slots` are equal in both states (Task 16). With the decision made before the upsert, a kept JACK client always plays the graph's runtimes: the graph replaces a chain's `Arc`s only in a full rebuild, which now always comes with a new client, so no republish into a live JACK client is needed (JACK never publishes into its slots — the JACK RT thread never ends up the last owner of a runtime through `JackPeriod::loaded`). The JACK test below pins that.

**Files:**
- Create: `crates/infra-cpal/src/issue_980_pipeline_pairing_tests.rs`
- Create: `crates/infra-cpal/src/controller_jack_slots_tests.rs`
- Modify: `crates/infra-cpal/src/controller_upsert.rs:1-5` (module doc), after `:127` (the decision), end of file (two test mounts)
- Modify: `crates/infra-cpal/src/io_topology.rs:88-121` (`chain_structure_signature`: doc and the `groups|` entry)
- Modify: `crates/infra-cpal/src/io_topology_tests.rs` (one test appended)
- Modify docs: `docs/audio-config.md` (one bullet in "Per-pipeline stream isolation")

**Interfaces:**
- Consumes: `RuntimeGraph::{runs_pipelines_of, runtimes_for, runtimes_with_groups_for}` (Task 3), `engine::runtime_graph::pipeline_slots(&Chain, &[IoBinding]) -> Vec<(usize, PipelineKey)>` (Task 3; `PipelineKey: Debug`), `teardown_active_chain_for_rebuild` (Task 4), `install_chain_slots` (Task 5), `ProjectRuntimeController::{for_testing, set_io_bindings}` (`controller.rs:149, 241`), `upsert_chain_with_resolved(&mut self, &Chain, ResolvedChainAudioConfig, bool) -> Result<()>` (`controller_upsert.rs:83`), `ResolvedChainAudioConfig { inputs, outputs, sample_rate, by_device, output_devices_by_input_cpal, stream_signature }` (`resolved.rs:18-36`), `crate::host::jack_server_is_running() -> bool` (`host.rs:53`, Linux+JACK only).
- Produces: nothing new. `upsert_chain_with_resolved` rebuilds the streams in one more case; `chain_structure_signature` names pipelines instead of slot ids.

- [ ] **Step 1: Write the failing tests**

Append to `crates/infra-cpal/src/controller_upsert.rs`:

```rust

// `pub(crate)`: `io_topology_tests` shares its fixtures.
#[cfg(test)]
#[path = "issue_980_pipeline_pairing_tests.rs"]
pub(crate) mod pipeline_pairing_tests;

#[cfg(all(test, target_os = "linux", feature = "jack"))]
#[path = "controller_jack_slots_tests.rs"]
mod jack_slots_tests;
```

Create `crates/infra-cpal/src/issue_980_pipeline_pairing_tests.rs`:

```rust
//! #980 — a change of pipelines that keeps every stream signature rebuilds the
//! chain's streams.
//!
//! Re-pairing two E/S over the same I/O — X {in 0 → A} + Y {in 1 → B} becoming
//! X {in 0, in 1 → A} + Y {→ B} — keeps the slot ids, the flat I/O lists and
//! every stream signature, but moves the pipelines: the graph answers it with
//! new runtimes. Streams kept open played the runtimes the graph dropped, and
//! every later edit went to runtimes nobody played.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::RuntimeGraph;
use project::chain::Chain;

use crate::controller::ProjectRuntimeController;
use crate::resolved::{ChainStreamSignature, ResolvedChainAudioConfig};

fn endpoint(name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("quantum".into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn binding(id: &str, inputs: Vec<IoEndpoint>, outputs: Vec<IoEndpoint>) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.into(),
        inputs,
        outputs,
    }
}

fn in_(channel: usize) -> IoEndpoint {
    endpoint(&format!("in {channel}"), ChannelMode::Mono, &[channel])
}

fn out_a() -> IoEndpoint {
    endpoint("a", ChannelMode::Stereo, &[0, 1])
}

fn out_b() -> IoEndpoint {
    endpoint("b", ChannelMode::Stereo, &[10, 11])
}

/// X {in 0 → A} + Y {in 1 → B}: pipelines (A, in 0), (B, in 1).
pub(crate) fn paired() -> Vec<IoBinding> {
    vec![
        binding("x", vec![in_(0)], vec![out_a()]),
        binding("y", vec![in_(1)], vec![out_b()]),
    ]
}

/// X {in 0, in 1 → A} + Y {→ B}: pipelines (A, in 0), (A, in 1) — the same
/// slot ids and flat I/O, other pipelines.
pub(crate) fn repaired() -> Vec<IoBinding> {
    vec![
        binding("x", vec![in_(0), in_(1)], vec![out_a()]),
        binding("y", vec![], vec![out_b()]),
    ]
}

pub(crate) fn chain() -> Chain {
    Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["x".into(), "y".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

/// The same resolved device config before and after: no device, no stream
/// opened, every signature equal.
fn resolved() -> ResolvedChainAudioConfig {
    ResolvedChainAudioConfig {
        inputs: Vec::new(),
        outputs: Vec::new(),
        sample_rate: 48_000.0,
        by_device: HashMap::new(),
        output_devices_by_input_cpal: Vec::new(),
        stream_signature: ChainStreamSignature {
            inputs: vec![],
            outputs: vec![],
        },
    }
}

/// A running jackd must not get a client from a test.
fn jackd_runs_here() -> bool {
    #[cfg(all(target_os = "linux", feature = "jack"))]
    return crate::host::jack_server_is_running();
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    false
}

/// RED — the streams must play the runtimes the graph holds after a
/// re-pairing, not the ones before it.
#[test]
fn a_repairing_that_keeps_every_stream_signature_rebuilds_the_streams() {
    if jackd_runs_here() {
        eprintln!("skipped: a jackd runs on this machine — this test must not open a client on it");
        return;
    }
    let id = chain().id;
    let mut controller = ProjectRuntimeController::for_testing(RuntimeGraph {
        chains: HashMap::new(),
    });
    controller.set_io_bindings(paired());
    controller
        .upsert_chain_with_resolved(&chain(), resolved(), false)
        .expect("the paired E/S builds");
    let before = controller.runtime_graph.runtimes_for(&id);
    assert_eq!(before.len(), 2, "precondition: two pipelines");

    controller.set_io_bindings(repaired());
    controller
        .upsert_chain_with_resolved(&chain(), resolved(), false)
        .expect("the re-paired E/S builds");

    let graph = controller.runtime_graph.runtimes_with_groups_for(&id);
    assert_eq!(graph.len(), 2, "precondition: still two pipelines");
    assert!(
        graph
            .iter()
            .all(|(_, runtime)| !before.iter().any(|old| Arc::ptr_eq(old, runtime))),
        "precondition: the graph answered the re-pairing with new runtimes"
    );
    for (slot, runtime) in &graph {
        let playing = controller
            .chain_slots
            .get(&(id.clone(), *slot))
            .expect("every pipeline has a live slot")
            .load();
        assert!(
            Arc::ptr_eq(&playing, runtime),
            "slot {slot}: the streams must play the runtime the graph holds, not the one \
             before the re-pairing"
        );
    }
}
```

Append to `crates/infra-cpal/src/io_topology_tests.rs`:

```rust

/// #980: the streams are bound to the chain's runtimes slot by slot, each slot
/// to one pipeline. Re-pairing two E/S over the same I/O keeps the slot ids
/// but moves the pipelines — a structural change (new streams).
#[test]
fn a_repairing_over_the_same_io_is_a_structural_change() {
    use crate::controller_upsert::pipeline_pairing_tests::{chain, paired, repaired};

    assert_ne!(
        super::chain_structure_signature(&chain(), &paired()),
        super::chain_structure_signature(&chain(), &repaired()),
        "#980: the pipelines moved — the streams bound to them must be rebuilt"
    );
}
```

Create `crates/infra-cpal/src/controller_jack_slots_tests.rs` (JACK pin — no production change expected):

```rust
//! #980 — a JACK client kept across an edit plays what the graph holds.
//!
//! Switching a bound insert does not change the JACK port shape, so the client
//! is kept, and the switch runs in place (Tasks 16–17: same keys, same `Arc`s).
//! Any edit the graph answers with other runtimes gets a new client
//! (`upsert_chain_with_resolved` decides that before the graph upsert), so a
//! kept client never plays a runtime the graph dropped.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{ChainRuntimeState, RuntimeGraph};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;

use crate::controller::ProjectRuntimeController;
use crate::resolved::{ChainStreamSignature, ResolvedChainAudioConfig};

fn registry() -> Vec<IoBinding> {
    let ep = |name: &str, mode: ChannelMode, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("jack:quantum".into()),
        mode,
        channels,
    };
    vec![
        IoBinding {
            id: "guitarra-1".into(),
            name: "Guitarra 1".into(),
            inputs: vec![ep("in", ChannelMode::Mono, vec![0])],
            outputs: vec![
                ep("main", ChannelMode::Stereo, vec![0, 1]),
                ep("out 2", ChannelMode::Stereo, vec![10, 11]),
            ],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![ep("ret", ChannelMode::Mono, vec![2])],
            outputs: vec![ep("snd", ChannelMode::Mono, vec![2])],
        },
    ]
}

fn chain(insert_on: bool) -> Chain {
    Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitarra-1".into()],
        blocks: vec![AudioBlock {
            id: BlockId("insert:0".into()),
            enabled: insert_on,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "fx".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    }
}

/// The same JACK port shape before and after the switch.
fn resolved() -> ResolvedChainAudioConfig {
    ResolvedChainAudioConfig {
        inputs: Vec::new(),
        outputs: Vec::new(),
        sample_rate: 48_000.0,
        by_device: HashMap::new(),
        output_devices_by_input_cpal: Vec::new(),
        stream_signature: ChainStreamSignature {
            inputs: vec![],
            outputs: vec![],
        },
    }
}

#[test]
fn a_kept_jack_client_plays_the_runtimes_the_graph_holds_after_an_insert_switch() {
    if crate::host::jack_server_is_running() {
        eprintln!("skipped: a jackd runs on this machine — this test must not open a client on it");
        return;
    }
    let mut controller = ProjectRuntimeController::for_testing(RuntimeGraph {
        chains: HashMap::new(),
    });
    controller.set_io_bindings(registry());
    controller
        .upsert_chain_with_resolved(&chain(false), resolved(), false)
        .expect("insert off builds");
    controller
        .upsert_chain_with_resolved(&chain(true), resolved(), false)
        .expect("insert on builds");

    let id = chain(true).id;
    let graph = controller.runtime_graph.runtimes_with_groups_for(&id);
    let mut live: Vec<(usize, Arc<ChainRuntimeState>)> = controller
        .chain_slots
        .iter()
        .filter(|((chain, _), _)| chain == &id)
        .map(|((_, group), slot)| (*group, slot.load()))
        .collect();
    live.sort_by_key(|(group, _)| *group);

    let live_groups: Vec<usize> = live.iter().map(|(g, _)| *g).collect();
    let graph_groups: Vec<usize> = graph.iter().map(|(g, _)| *g).collect();
    assert_eq!(
        live_groups, graph_groups,
        "the client binds one slot per pipeline the graph holds"
    );
    for ((group, playing), (_, held)) in live.iter().zip(graph.iter()) {
        assert!(
            Arc::ptr_eq(playing, held),
            "pipeline {group}: the JACK callback must play the graph's runtime, not the one before the switch"
        );
    }
}
```

(`controller_upsert` is a private module of the crate root, so `crate::controller_upsert::pipeline_pairing_tests` is reachable from anywhere in the crate once the mount is `pub(crate)`.)

- [ ] **Step 2: Run the tests to verify they fail**

```bash
(cd $S && cargo test -p infra-cpal --lib -- pipeline_pairing_tests a_repairing_over_the_same_io_is_a_structural_change)
$LINUX cargo test -p infra-cpal --features jack --lib -- pipeline_pairing_tests jack_slots_tests a_repairing_over_the_same_io
```

Expected (macOS and Linux alike):
- `a_repairing_that_keeps_every_stream_signature_rebuilds_the_streams` FAILS "slot 0: the streams must play the runtime the graph holds, not the one before the re-pairing" (every signature equal → no stream rebuild; the graph rebuilt the runtimes behind the slots' back).
- `a_repairing_over_the_same_io_is_a_structural_change` FAILS `assertion left != right failed` (both sides end in `groups|[0, 1]`).
- `a_kept_jack_client_plays_the_runtimes_the_graph_holds_after_an_insert_switch` PASSES (Linux pin: the switch is in place since Task 17).

- [ ] **Step 3: Implement**

`crates/infra-cpal/src/controller_upsert.rs` — module doc lines 3-5 become:

```rust
//! The stream signature decides the shape: an unchanged signature keeps the
//! streams up (a knob edit must not drop audio), a changed one tears the
//! chain's streams down before the replacement is built — and so does an edit
//! that moves the chain's pipelines under an unchanged signature (#980).
```

and right after the `#[cfg(not(all(target_os = "linux", feature = "jack")))] let needs_stream_rebuild = …;` statement (`:122-127`), before `// #669: track the real device sample rate …`, insert:

```rust
        // #980: the streams (and the JACK client) hold the chain's runtimes by
        // slot, each slot bound to one pipeline. An edit whose pipelines no
        // longer match the live runtimes gets NEW runtimes from the graph (a
        // full rebuild), so it needs new streams even when every signature is
        // unchanged — two E/S re-paired over the same I/O keep the slot ids,
        // the I/O lists and the signatures. Decided BEFORE the graph upsert,
        // so the teardown below silences the runtimes the streams still play.
        let needs_stream_rebuild = needs_stream_rebuild
            || !self.runtime_graph.runs_pipelines_of(chain, &self.io_bindings);
```

(A chain with no runtime yet has no live pipelines, so the check reads "moved" — and such a chain has no `active_chains` entry, so `needs_stream_rebuild` was already `true`.)

`crates/infra-cpal/src/io_topology.rs` — in the doc of `chain_structure_signature` (`:88-92`), `/// \`Output\`), whose on/off state changes the streams the chain owns — and the` / `/// runtime grouping those streams are bound to (#967). Parameter values are` becomes `/// \`Output\`), whose on/off state changes the streams the chain owns — and the` / `/// pipelines those streams are bound to, slot by slot (#967, #980). Parameter values are`; and replace the final `.chain(std::iter::once(format!( … "groups|{:?}", engine::runtime_graph::input_group_ids(chain, registry) )))` (`:114-121`) with:

```rust
        .chain(std::iter::once(format!(
            // #967 / #980: the runtimes the chain is split into are bound to its
            // streams by slot, each slot to one pipeline. A change that moves
            // them — an insert switched on a chain whose pipelines read several
            // input streams, two E/S re-paired over the same I/O — needs new
            // streams; one that does not (a fixed-slot insert switch, whose
            // pipelines are the same in both states) stays a DSP rebuild.
            "pipelines|{:?}",
            engine::runtime_graph::pipeline_slots(chain, registry)
        )))
```

`docs/audio-config.md` — add a bullet to "Per-pipeline stream isolation", after the "One device, several pipelines" bullet:

```markdown
- **Pipelines moved, signatures equal** (#980): the streams hold a chain's
  runtimes slot by slot, each slot bound to one pipeline. An edit whose
  pipelines no longer match the live runtimes — two E/S re-paired over the
  same I/O keep the slot ids and every stream signature — gets new runtimes
  from the graph and new streams (a new JACK client), decided before the graph
  is touched (`upsert_chain_with_resolved`); `chain_structure_signature`
  carries the `(slot, pipeline)` list, so the off-thread path reads it as a
  structural change too.
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
(cd $S && cargo test -p infra-cpal --lib -- pipeline_pairing_tests io_topology issue_980 issue_881 controller)
(cd $S && cargo test -p infra-cpal --test controller_schedule_rebuild --test issue_762_live_rebuild_offthread)
$LINUX cargo test -p infra-cpal --features jack --lib -- pipeline_pairing_tests jack_slots_tests io_topology
$LINUX cargo build --workspace   # zero warnings
```

Expected: all PASS — `io_topology_tests::a_switch_that_regroups_the_chains_runtimes_is_a_structural_change` pins that a regrouping switch still reads as structural, and `a_loop_on_a_two_output_es_keeps_the_chains_structure` (Task 16) plus `issue_980_fixed_slot_binding_tests` that a fixed-slot switch still does not (no new streams on the owner's footswitch). Then run the Gate.

- [ ] **Step 5: Commit**

```bash
git -C $S add crates/infra-cpal/src/issue_980_pipeline_pairing_tests.rs crates/infra-cpal/src/controller_jack_slots_tests.rs \
  crates/infra-cpal/src/controller_upsert.rs crates/infra-cpal/src/io_topology.rs \
  crates/infra-cpal/src/io_topology_tests.rs docs/audio-config.md
git -C $S commit -m "fix(#980): an edit that moves the pipelines rebuilds the streams, whatever the signatures say" -m "Decided before the graph upsert (so the teardown drains what the streams play) on both backends; chain_structure_signature carries the (slot, pipeline) list. Pins that a kept JACK client plays the graph's runtimes.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Then Push.

---

### Task 19: Comment sweep, real-hardware battery, owner hand-off

Comment and doc wording only — no behaviour change, no TDD step (`cargo test --workspace` must give the same result before and after the edits). Each edit replaces the quoted phrase exactly (case by case — no sed/regex migration, project rule).

**Files:** every file in the table below; `crates/engine/tests/issue_947_unfed_route_no_underrun.rs` (comment only).

**Interfaces:** none.

- [ ] **Step 1: No new test** — record the baseline:

```bash
(cd $S && cargo test --workspace 2>&1 | grep "test result" | sed -E 's/; finished in [0-9.]+s//' | sort | uniq -c > target/980_before.txt)
```

- [ ] **Step 2: Confirm the stale phrases are still there**

```bash
git -C $S grep -n "per-input runtime\|per-entry runtime\|per-group runtime\|input-entry group\|runtime group, route" -- crates ':!*_tests.rs' ':!crates/*/tests/*'
```

Expected: the sites of the table below (and `adapter-gui/src/tuner_session.rs:166`, which is the pre-existing "input k = stream k" mapping — out of scope, left untouched).

- [ ] **Step 3: Edit**

| File:line | Stale phrase (exact) | New phrase |
|---|---|---|
| `crates/infra-cpal/src/slot_processing.rs:17` | `Wrap each per-group runtime in a fresh` | `Wrap each pipeline runtime in a fresh` |
| `crates/infra-cpal/src/slot_processing.rs:32-38` | `/// every per-entry runtime whose cpal input index equals the stream's` … `/// their group id, which historically WAS the cpal index.` (the whole paragraph) | `/// every pipeline runtime whose cpal input index equals the stream's` / `/// device order (#703, #980): the other outputs of the same guitar and the` / `/// other entries on the interface are separate runtimes bound to the SAME` / `/// stream, and the single callback fans out to all of them. A runtime with` / `/// no pipeline stamp (whole-chain: tests, legacy fixtures) falls back to its` / `/// slot id, which is its cpal index only in those shapes; every runtime` / `/// infra builds is stamped.` |
| `crates/infra-cpal/src/slot_processing.rs:58` | `/// Each per-input runtime writes its binding's output route and nothing else.` | `/// Each pipeline runtime writes ONE route (#980) — the one runtime of a chain` / `/// an insert cuts writes them all — so an output holds exactly the runtimes` / `/// that own it (\`owns_output\`) and whose binding feeds this device.` |
| `crates/infra-cpal/src/slot_processing.rs:107` | `/// Mix the chain's live per-group output runtimes into \`out\`.` | `/// Mix the live runtimes this output stream holds into \`out\`.` |
| `crates/infra-cpal/src/stream_builder.rs:36-47` | from `/// Issue #350 phase 3: \`runtimes\` is the chain's ordered list of` to `/// (the only mix point invariant #4 permits).` | `/// Issue #350 phase 3 / #980: \`slots\` is the chain's ordered list of` / `/// pipeline runtimes — \`(slot, runtime)\` in segment order (see` / `/// \`RuntimeGraph::runtimes_with_groups_for\`). The engine's` / `/// \`effective_inputs\` assigns cpal indices by first-seen distinct device` / `/// over the chain's raw input entries; \`resolved.inputs\` is in that same` / `/// raw-entry order, so deduplicating it by device in iteration order` / `/// yields the Nth distinct device == cpal index N. Each physical input` / `/// device gets ONE cpal stream bound to every pipeline runtime it feeds` / `/// (\`slots_for_input_stream\`); each output device stream holds the runtimes` / `/// that write it and sums them at the backend (the only mix point` / `/// invariant #4 permits).` |
| `crates/infra-cpal/src/stream_builder.rs:59` | `// Fallback used only if a chain somehow has no per-input runtime for a` | `// Fallback used only if a chain somehow has no pipeline runtime for a` |
| `crates/infra-cpal/src/stream_builder.rs:67-69` | `is cpal index N. Issue #703: that one stream binds EVERY per-entry` / `// runtime fed by this device (two entries on one interface are two` / `// isolated runtimes sharing the stream) — \`slots_for_input_stream\`` | `is cpal index N. Issue #703 / #980: that one stream binds EVERY` / `// pipeline runtime fed by this device (two entries on one interface, or` / `// one entry into two outputs) — \`slots_for_input_stream\`` |
| `crates/infra-cpal/src/stream_builder.rs:94` | `"chain '{}' cpal input {} has no per-input runtime",` | `"chain '{}' cpal input {} has no pipeline runtime",` |
| `crates/infra-cpal/src/stream_builder_input.rs:34-37` | `/// Issue #703: \`slots\` is every per-entry runtime this device's single` / `/// stream feeds (two input entries on one interface are two isolated` / `/// runtimes sharing the stream). The callback fans the buffer out to each` | `/// Issue #703 / #980: \`slots\` is every pipeline runtime this device's` / `/// single stream feeds (two entries on one interface, or one entry into two` / `/// outputs, are separate runtimes sharing the stream). The callback fans the buffer out to each` |
| `crates/infra-cpal/src/stream_builder_input.rs:82-84` | `// Issue #703: one worker PER per-entry runtime — each entry's` / `// chain DSP runs on its own realtime thread, so a heavy entry` / `// cannot starve its sibling sharing this device stream.` | `// Issue #703 / #980: one worker PER pipeline runtime — each` / `// pipeline's DSP runs on its own realtime thread, so a heavy` / `// pipeline cannot starve its sibling sharing this device stream.` |
| `crates/infra-cpal/src/stream_builder_output.rs:3-4` | `//! The stream is handed every runtime slot the chain owns and sums them` / `//! at the backend — the only mix point CLAUDE.md invariant #4 permits.` | `//! The stream is handed the runtime slots that own its route and sums` / `//! their writers at the backend — the only mix point invariant #4 permits.` |
| `crates/infra-cpal/src/stream_builder_output.rs:36-39` | `/// #350 phase 3: a chain may own N per-input runtimes (one isolated` / `/// \`ChainRuntimeState\` per physical input device). This single physical` / `/// output device must SUM all of them — the backend mix CLAUDE.md` | `/// #350 phase 3 / #980: a chain owns one runtime per pipeline. This` / `/// output device SUMS the runtimes that write its route (two guitars into` / `/// one output) — the backend mix CLAUDE.md` |
| `crates/infra-cpal/src/stream_builder_project.rs:50-55` | `// Issue #350 phase 3: a chain owns N per-input runtimes (one` … `// here and take the byte-identical fast path.` | `// Issue #350 phase 3 / #980: a chain owns one runtime per` / `// pipeline. Pass the full ordered (slot, runtime) list so each` / `// input cpal stream feeds the pipelines it carries and each output` / `// stream mixes the runtimes that write it. A one-input, one-output` / `// chain has exactly one entry here and takes the byte-identical` / `// fast path.` |
| `crates/infra-cpal/src/controller.rs:56-57` | `/// Issue #703: a build yields one runtime per input-entry group, each` / `/// published into its own \`(chain, group)\` slot.` | `/// Issue #703 / #980: a build yields one runtime per pipeline, each` / `/// published into its own \`(chain, slot)\` slot.` |
| `crates/infra-cpal/src/controller.rs:67` | `(issue #703: one per input-entry group);` | `(#703 / #980: one per pipeline);` |
| `crates/infra-cpal/src/build_request.rs:38-42` | `/// Build the fresh per-entry chain runtimes from \`req\`.` … `/// chains get exactly one \`(0, runtime)\` pair (the legacy shape).` | `/// Build the fresh pipeline runtimes of a chain from \`req\`. Worker-runnable: this` / `/// is the heavy DSP-assembly step that must not run on the frontend thread` / `/// (issue #672). Issue #703 / #980: one isolated runtime per pipeline —` / `/// the caller publishes each into its \`(chain, slot)\` slot; a one-input,` / `/// one-output chain gets exactly one \`(0, runtime)\` pair.` |
| `crates/infra-cpal/src/controller_rebuild_queue.rs:33` | `// #703: a chain owns one slot per input-entry group.` | `// #703 / #980: a chain owns one slot per pipeline.` |
| `crates/infra-cpal/src/controller_upsert.rs:147-151` (≈`:156-160` after Task 18's insert) | `// upsert_chain (re)builds every per-input runtime for this chain and` … `// (issue #350 phase 3).` | `// upsert_chain (re)builds every pipeline runtime for this chain and` / `// returns the first; fetch the full ordered (slot, runtime) list from` / `// the graph so the stream layer binds each device stream to the` / `// pipelines it feeds and each output to the runtimes that write it` / `// (#350, #980).` |
| `crates/infra-cpal/src/controller_block_toggle.rs:24` | `every per-input runtime` | `every pipeline runtime` |
| `crates/infra-cpal/src/controller_chain_activation.rs:32` | `/// per-input runtimes (the heavy NAM/IR load)` | `/// pipeline runtimes (the heavy NAM/IR load)` |
| `crates/infra-cpal/src/controller_taps.rs:68` | `// lowest-group (group 0) per-input runtime, which IS the primary` | `// slot-0 pipeline runtime (first route × first input), which IS the primary` |
| `crates/infra-cpal/src/controller_taps.rs:109` | `/// per-input runtimes (same convention as` | `/// pipeline runtimes (same convention as` |
| `crates/infra-cpal/src/controller_taps.rs:163` | `/// 1. When several streams share one per-input runtime (e.g. two` | `/// 1. When several streams share one runtime (e.g. two` |
| `crates/infra-cpal/src/controller_taps.rs:175` | `/// \`(per-input runtime, local segment)\` pair` | `/// \`(pipeline runtime, local segment)\` pair` |
| `crates/infra-cpal/src/controller_taps.rs:229` | `// Issue #350 phase 3: a chain owns N per-input runtimes, each with` | `// Issue #350 phase 3 / #980: a chain owns N pipeline runtimes, each with` |
| `crates/infra-cpal/src/controller_taps.rs:232` | `so \`stream_index\` is GLOBAL. Walk the per-input runtimes in` | `so \`stream_index\` is GLOBAL. Walk the pipeline runtimes in` |
| `crates/infra-cpal/src/controller_taps.rs:233` | `// group order, subtracting` | `// slot order, subtracting` |
| `crates/infra-cpal/src/controller_taps.rs:252` | `// Issue #350: a chain may own N per-input runtimes; the chain's` | `// Issue #350 / #980: a chain owns N pipeline runtimes; the chain's` |
| `crates/infra-cpal/src/controller_taps.rs:278` | `/// chain's per-input runtimes (issue #670).` | `/// chain's pipeline runtimes (issue #670).` |
| `crates/infra-cpal/src/controller_taps.rs:291` | `/// per-input runtimes (issue #670 instrumentation).` | `/// pipeline runtimes (issue #670 instrumentation).` |
| `crates/infra-cpal/src/controller_taps.rs:303` | `/// #923: every output route of every per-input runtime of this chain, as` | `/// #923: every output route of every pipeline runtime of this chain, as` |
| `crates/engine/src/segment_types.rs:43-46` | `/// (issue #703). The runtime graph partitions segments by this id:` / `/// distinct raw entries become isolated runtimes even on one shared` | `/// (issue #703). The runtime graph partitions segments by (route, this` / `/// id) (#980): every pipeline becomes an isolated runtime even on one shared` |
| `crates/engine/src/effective_endpoints.rs:29-30` | `///   g02/g03); distinct raw endpoints get distinct groups (own isolated` / `///   runtime) even on the same device.` | `///   g02/g03); distinct raw endpoints get distinct groups (own isolated` / `///   pipelines, #980) even on the same device.` |
| `crates/engine/src/runtime_graph_assemble.rs:8` | `which is what makes two per-input runtimes structurally isolated,` | `which is what makes two pipeline runtimes structurally isolated,` |
| `crates/engine/src/runtime_graph_assemble.rs:52-54` | `/// and \`build_per_input_runtimes\` (one segment group → one isolated` / `/// per-input runtime).` | `/// and \`build_per_input_runtimes\` (one pipeline's segments → one isolated` / `/// runtime, #980).` |
| `crates/engine/src/runtime_graph_assemble.rs:57` | `/// what makes two per-input runtimes structurally isolated (issue #350).` | `/// what makes two pipeline runtimes structurally isolated (issue #350).` |
| `crates/engine/src/runtime_graph_assemble.rs:331` | `/// one route; it stays silent, exactly as #699 left it.` | `/// one route; it stays silent, exactly as #699 left it. (#980: a pipeline` / `/// runtime holds one pipeline, so the loop plays once per runtime;` / `/// \`arm_di_loop_per_output_stream\` arms one runtime per route.)` |
| `crates/engine/src/runtime_graph_impl.rs` (full-rebuild comment) | `// Full rebuild: drop every stale per-input runtime for this chain` | `// Full rebuild: drop every stale pipeline runtime for this chain` |
| `crates/engine/src/runtime_graph_impl.rs` (`remove_chain`) | `// Issue #350: a chain may own N per-input runtimes; drop them all.` | `// Issue #350 / #980: a chain owns N pipeline runtimes; drop them all.` |
| `crates/engine/src/runtime_block_toggle.rs:116` | `every per-input runtime of the chain` | `every pipeline runtime of the chain` |
| `crates/application/src/query_output_routes.rs:13` | `/// One output route of one per-input runtime, as its device stream saw it.` | `/// One output route of one pipeline runtime, as its device stream saw it.` |
| `crates/application/src/query_output_routes.rs:17` | `/// The per-input runtime (cpal input group) that owns the route.` | `/// The pipeline runtime (slot) that owns the route (#980).` |
| `crates/application/src/query_output_routes.rs:46-47` | `/// One chain's runtime groups, flattened into the rows \`openrig://routes\`` / `/// lists: group order, then route order` | `/// One chain's pipeline runtimes, flattened into the rows \`openrig://routes\`` / `/// lists: slot order, then route order` |
| `crates/application/src/query_kind.rs:105` | `per (chain, runtime group, route)` | `per (chain, pipeline slot, route)` |
| `crates/application/src/live_source.rs:86` | `at least one live per-input runtime` | `at least one live pipeline runtime` |
| `crates/adapter-mcp/src/resources.rs:26` | `per (chain, runtime group, route)` | `per (chain, pipeline slot, route)` |
| `crates/adapter-gui/src/live_source_gui.rs:67` | `every per-input runtime` | `every pipeline runtime` |
| `crates/adapter-gui/src/meter_invalidation.rs:50` | `/// per-input runtimes (issue #350) and drops to 0 when the engine` | `/// pipeline runtimes (#350, #980) and drops to 0 when the engine` |
| `crates/adapter-gui/src/meter_wiring_poll.rs:99` | `// when the engine rebuilds the per-input runtimes (toggle` | `// when the engine rebuilds the pipeline runtimes (toggle` |
| `crates/engine/tests/issue_947_unfed_route_no_underrun.rs:57` | `// Each runtime carries a route for both chain outputs (route 0, route 1).` | `// The output stream pops every runtime on both routes; each runtime owns only its own (#947, #980).` |

Leave untouched (still valid shapes, listed on the issue as "no longer model production"): `application/src/query_output_routes_tests.rs:59-88`, `engine/tests/issue_85_di_reaches_every_pipeline.rs:139-160`, `infra-cpal/tests/issue_323_controller_loopers.rs:65-86`, `adapter-gui/tests/issue_323_looper_wiring.rs:81-91`, `infra-cpal/tests/issue_771_di_playback_routing.rs:48-111`, `adapter-gui/tests/issue_771_output_select_wiring.rs:83-91`, `infra-cpal/src/controller_per_stream_input_tap_tests.rs:337`, and the test-message wording in `infra-cpal/src/tests_regression.rs`.

- [ ] **Step 4: Verify nothing changed, then run every battery**

```bash
(cd $S && cargo test --workspace 2>&1 | grep "test result" | sed -E 's/; finished in [0-9.]+s//' | sort | uniq -c > target/980_after.txt && diff target/980_before.txt target/980_after.txt)   # no diff (the timing suffix is stripped: every line ends in "; finished in N.NNs")
```

Then the Gate, then Linux, then the real-hardware battery on an idle machine (the owner's interface and the BlackHole loopback connected; nothing else playing):

```bash
$LINUX cargo test --workspace
(cd $S && OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
    --test issue_670_cab_swap --test issue_670_real_streams_no_xruns \
    --test issue_698_pitch_shifter_live --test issue_698_owner_64_dual_chain \
    --test issue_85_mid_output_reaches_its_device --test issue_85_heavy_rig_mid_output \
    --test issue_85_mid_output_other_rate_real --test issue_85_owner_interfaces_tap -- --nocapture --test-threads=1)
(cd $S && OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release --lib \
    -- issue_980_insert_fixed_slots_hw issue_967_insert_toggle_streams --nocapture --test-threads=1)
```

Expected: every test green with its assertions untouched. Record the xrun/underrun numbers the battery prints (R3: dsp-workers per device doubled, `issue_698_owner_64_dual_chain` goes from 2 to 4 workers). Any new xrun, dropout or latency step is a STOP (invariants #3/#6/#7): report on the issue, do not tune it away.

- [ ] **Step 5: Commit, push, hand off**

```bash
git -C $S add crates/infra-cpal/src/slot_processing.rs crates/infra-cpal/src/stream_builder.rs \
  crates/infra-cpal/src/stream_builder_input.rs crates/infra-cpal/src/stream_builder_output.rs \
  crates/infra-cpal/src/stream_builder_project.rs crates/infra-cpal/src/controller.rs \
  crates/infra-cpal/src/build_request.rs crates/infra-cpal/src/controller_rebuild_queue.rs \
  crates/infra-cpal/src/controller_upsert.rs crates/infra-cpal/src/controller_block_toggle.rs \
  crates/infra-cpal/src/controller_chain_activation.rs crates/infra-cpal/src/controller_taps.rs \
  crates/engine/src/segment_types.rs crates/engine/src/effective_endpoints.rs \
  crates/engine/src/runtime_graph_assemble.rs crates/engine/src/runtime_graph_impl.rs \
  crates/engine/src/runtime_block_toggle.rs crates/application/src/query_output_routes.rs \
  crates/application/src/query_kind.rs crates/application/src/live_source.rs \
  crates/adapter-mcp/src/resources.rs crates/adapter-gui/src/live_source_gui.rs \
  crates/adapter-gui/src/meter_invalidation.rs crates/adapter-gui/src/meter_wiring_poll.rs \
  crates/engine/tests/issue_947_unfed_route_no_underrun.rs
git -C $S commit -m "docs(#980): name pipeline runtimes in the comments that still said per-input" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Push, then complete the solver workspace and take its run line:

```bash
(cd $S && scripts/solver-setup.sh 980 bug/issue-980)   # prints "run: cd … && OPENRIG_PLUGINS_ROOT=… cargo run -p adapter-gui -- --mcp"
```

Hand-off (chat AND `gh issue comment 980`, nothing else around it): the two commands, each in its own code block — the main folder one, and the `run:` line the script printed, verbatim with its absolute path — then:

```bash
git fetch && git checkout bug/issue-980 && git pull
```

1. [ ] ANAL+DIG, both guitars, 5 minutes: Main and Out 2 play with no click or dropout
2. [ ] Main and Out 2 sound at the same level as before
3. [ ] SYN-2/pedais footswitch on and off several times: no silence gap on either output
4. [ ] A knob on a VST3 block while playing: heard on both outputs, no pop
5. [ ] Orange Pi: an E/S with two outputs plays on both
6. [ ] Orange Pi: two guitars on one interface both play, no clicks for a few minutes
