# #328 Part 4: Engine: Y -> A/B routing — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Y → A/B chain sends each output exactly the split paths whose output node has that output checked, the input/output checklist disables endpoints for every chain kind, and a chain whose checklist leaves it nothing to play is simply off — on macOS, Windows and Linux/JACK.

**Architecture:** The checklist is applied once, where every consumer reads a chain's ports (`project::binding_discovery::resolve_chain_ports`), through a new pure rule module (`project::endpoint_feeds`). The engine keeps its one-segment-per-(input × output) model: each `ChainSegment` now carries `paths: SegmentPaths`, computed per output route (`engine::segment_paths`). The runtime builder then builds a Y split, per segment, as a **neutral Split → Mix** of only the paths that segment runs (`engine::split_segment_view`), so both-paths-on-one-output is summed inside the segment by Part 3's aligned mixer code — never by two segments on one route, never by two runtimes. The stream layer gates on one predicate (`engine::runtime_graph::chain_plays`), `chain_structure_signature` sees path-set changes, and the Linux/JACK-direct callback plays every route of its runtime.

**Tech Stack:** Rust (edition 2021), crates `project`, `engine`, `infra-cpal`; `arc_swap`; tests are plain `#[test]` modules mounted with `#[path = "..._tests.rs"]` plus crate `tests/` integration files.

**Spec:** `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328/docs/superpowers/specs/2026-09-28-issue-328-chain-split-graph-design.md` — sections covered: §1.3 (runtime application of `EndpointDisables`: "The projection … filters the discovered endpoints with this set before segments are built"), §4.2 (Y → A/B: one segment per input × checked output, `SegmentPaths`, `classify_output_routes`/`blocks_between` mapping, `chain_structure_signature`, Linux/JACK output index fix), §5.3 engine/stream side ("Unchecking every input or every output of a node is allowed; that node's segments simply are not built"), §6 (volume, stream isolation, latency), §7 ("Engine structure: Y builds one segment per checked output with the right path set; both paths on one output sum inside the segment; no allocation after warm-up"), §8 (`docs/audio-config.md`: Y outputs, endpoint checklist).

**Depends on:**
- **Part 1 (model):** `AudioBlockKind::Split(SplitBlock)`, `SplitEnd`, `split_params`, `EndpointDisables`/`EndpointRef`/`EndpointNode`, `Chain.disabled_endpoints` copied by `rig_to_chains`.
- **Part 3 (engine Split → Mix):** the Split runtime node built inside `build_runtime_block_nodes`, its per-callback mixer and path alignment, and the walkers that recurse into split paths (`block_is_convolution`, offline `apply_block_offline`).
- Part 2 (commands) is not needed by this part. This part adds no `Command`, so the "chain id field name" question of the contract does not arise here.

### Interfaces consumed from other parts (exact names this plan compiles against)

From Part 1 — import paths assumed; if Part 1 re-exported elsewhere, fix only the `use` lines:
```rust
project::block::{AudioBlock, AudioBlockKind, SplitBlock, SplitEnd}        // SplitBlock { end, params, a, b }; SplitEnd::{Mix, Y}
project::block::split_params::{default_split_params, LEVEL_TO_A, MIX_B_POLARITY, MIX_LEVEL_A,
    MIX_LEVEL_B, MIX_MASTER, MIX_MASTER_SUM, MIX_PAN_A, MIX_PAN_B}          // &str consts, fn default_split_params() -> ParameterSet
project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef}  // EndpointRef { io: String, endpoint: String }
EndpointDisables { inputs, outputs, path_a_outputs, path_b_outputs }: Default + Clone + PartialEq
EndpointDisables::is_enabled(&self, node: EndpointNode, r: &EndpointRef) -> bool
EndpointNode::{Input, Output, PathAOutput, PathBOutput}
project::chain::Chain { …, pub disabled_endpoints: EndpointDisables }
AudioBlock / AudioBlockKind / SplitBlock derive Debug + Clone + PartialEq (block_snapshot equality already requires PartialEq)
```
Name collision to keep in mind: `project::endpoint_ref::EndpointRef { binding_id, endpoint }` already exists (#323/#717). This part always names the checklist type by its full path `project::endpoint_disables::EndpointRef`.

From Part 1 as its plan is written today (read `part-1-model.md` before Task 1):
- `project::block::find_split(&[AudioBlock]) -> Option<(usize, &SplitBlock)>` (Part 1 Task 4) — this part uses it to recognise a Y chain, so there is one definition of "the chain's split".
- **Part 1 Task 11 already filters `resolve_chain_ports` by the checklist** (head inputs unchecked on `Input`; tail outputs unchecked on `Output`, or on both path nodes for a Y chain) and writes the `### Endpoint checklist (issue #328)` section of `docs/audio-config.md` (before `### Mid-chain ports (issue #85)`). Task 2 below therefore only verifies it and re-implements it solely if Part 1 dropped that task.
- Part 1's validation (`validate_split_layout`) lets the chain's own `Input`/`Output` ports follow a Y split; only processing blocks may not.

### Pre-flight: the `EndpointRef` field name (run once, before Task 1)

The shared contract says `EndpointRef { io, endpoint }`, and this plan's struct literals use `io:`. The reviewed Part 1 plan (Task 8) defines exactly that struct in `crates/project/src/endpoint_disables.rs` (an earlier draft re-exported the looper's `binding_id` type instead), so the check below is expected to print `pub io: String`. Check what actually landed:

```bash
grep -n "pub use crate::endpoint_ref::EndpointRef\|pub io: String" /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328/crates/project/src/endpoint_disables.rs
```

- Prints `pub io: String` → use the code below as written.
- Prints `pub use crate::endpoint_ref::EndpointRef;` → in every `EndpointRef { … }` literal of this plan write `binding_id:` where it says `io:` (the value is unchanged). The literals are: Task 1 `r` (tests) and `endpoint_ref` (production), Task 2 fallback `disables`, Task 3 `off`, Task 4 `r`, Task 6 `endpoint`, Task 7 `off`, Task 8 `off`, Task 9 `off`, the rig test's `every_output_off` and the controller test's literal. Nothing else changes.

From Part 3 — behaviour only, no internal signature is called:
- A **Split → Mix** block reaching `build_runtime_block_nodes` builds `RuntimeProcessor::Split(SplitRuntimeState)` and processes it as spec §4.1: an **empty path is its split input**; path level, mix level and master are linear `x / 100`; `pan_gains(0.0) == (1.0, 1.0)` (centre = unity both sides); alignment delays **only the shorter path** (an empty path has latency 0).
- `route_convolution::block_is_convolution` recurses into `SplitBlock.a/b` (spec §4.1 walker list). If Part 3 did not add that arm, Task 7 adds it.
- `offline::apply_block_offline` processes `RuntimeProcessor::Split`.

## Global Constraints

Solver root for every command: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328` (abbreviated `$S` in prose only — every command below spells it out). Git always as `git -C <solver root>`; never in the main folder; never `git add -A`.

- "`.rs` 600, `.slint` 500" line caps; "Todo arquivo de produção faz UMA coisa."
- "todo arquivo de produção DECLARA sua responsabilidade no cabeçalho — `//! Responsibility: <uma frase>`"; `scripts/validate.sh` check 1 rejects a declaration containing ` and `, ` e `, `plus`, `also`, `,`, `;`, `+`, `&` or `/` (so no "E/S", no "A/B" in headers).
- No inline test module: "`#[cfg(test)] mod tests { ... }` — extract to `<file>_tests.rs`"; mount with `#[cfg(test)] #[path = "..._tests.rs"] mod …;`.
- "Zero alocação, lock, syscall ou I/O no audio thread. Sem exceção."
- Stream isolation: "Mixing entre streams NUNCA acontece no nosso código — só no backend (cpal/JACK)." / spec §6: "The mix is DSP inside one segment. Y outputs are separate segments that each run the shared blocks." Never sum two segments of one output in `mixed_per_route`, never sum runtimes in `process_output_f32_mixed`.
- "Volume por stream IMUTÁVEL … `crates/engine/src/volume_invariants_tests.rs`" stays byte-for-byte unchanged; spec §6: "Chains without a split must produce bit-identical output."
- Spec §6: "No chain gains latency. Alignment only delays the shorter path."
- "TDD red-first OBRIGATÓRIO" — the failing ASSERTION is seen before production code (a compile error is not the red; add a compiling skeleton, watch the assertion fail, then implement).
- "`#[ignore]` é PROIBIDO."
- "Zero warnings (`cargo build` limpo)."
- "Conteúdo de repo sempre em inglês."
- "Fix de Linux/Orange Pi/JACK fica atrás de `cfg` guards" — the crate's guard is `#[cfg(all(target_os = "linux", feature = "jack"))]`.
- #716 pairing preserved: "a HEAD input only pairs with its own binding's TAIL output".
- Builds: `nice -n 19 cargo … -j 2`. Per step: `cargo test -p <crate> <filter>`; with several filters they go after `--` (`cargo test -p engine --lib -j 2 -- a b c` — cargo itself accepts only one positional filter).
- Push gate — CLAUDE.md: "Antes de TODO push: `cargo test --workspace`, nunca por crate" — every task's push step runs, from the solver root and after `pull --rebase`: `cargo fmt --all -- --check` (silent), `cargo test --workspace` (0 failed), `cargo build --workspace` (no line starting with `warning`), `VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates` (0 errors). A fmt hunk is fixed with the Edit tool and amended into the task's commit before the push.
- Commit then push immediately; after each push `gh issue comment 328 -R jpfaria/OpenRig` with hash, files, tests run.
- Commit messages: `feat(#328): …` in English, **no `Co-Authored-By` trailer** (`.claude/skills/openrig-code-quality/SKILL.md:446`, `docs/development/gitflow.md:39`); all six parts follow the project rule.
- Docs that describe changed behaviour go in the same commit (spec §8 → `docs/audio-config.md`).

## Review Focus

The five failure modes most likely to bite a player, each pinned by a named test:

1. **An output checked on path A only also plays path B or the dry split input** (bleed through the unused path or its mixer knob). Expect: that output carries path A alone. → Task 6 `each_output_carries_only_the_paths_that_feed_it` (+ `mixer_knobs_on_a_y_split_change_no_output`).
2. **An output checked on both paths plays at the wrong level** — doubled (two pipelines summed on one route) or halved (master default 50). Expect: exactly one pipeline, A + B at unity. → Task 4 `each_output_runs_the_paths_that_check_it` (one AB segment) + Task 6 `each_output_carries_only_the_paths_that_feed_it` (0.375 = 0.25 + 0.125).
3. **Unchecking the last output (or input) errors the activation, stops every other chain's sync, or sends audio to the legacy fallback device.** Expect: the chain is simply off, the rest keeps playing. → Task 3 `every_output_unchecked_invents_no_fallback_output`, Task 9 `unchecking_every_output_takes_the_chain_down_like_a_switch_off` + `every_stream_gate_reads_chain_plays`.
4. **Linux/JACK (Orange Pi): path B's output is silent** because the JACK-direct callback pops route 0 only. Expect: every route of the runtime reaches its channels. → Task 10 `every_route_of_the_runtime_reaches_its_own_channels` + `the_jack_callback_plays_every_route`.
5. **Checking path B on an output path A already feeds is treated as a knob edit** (no device stream is added, so the I/O signature is unchanged and the chain rides the off-thread DSP rebuild instead of #881's "a structural change gets brand-new streams"). Expect: a path-set change is structural (spec §4.2) and rebuilds the chain's streams. → Task 8 `a_path_set_change_is_a_structural_change`.

---

## File Structure

| File | Action | Responsibility (header) | LOC now |
|---|---|---|---|
| `crates/project/src/endpoint_feeds.rs` | create | decides which chain node uses each endpoint of the chain's bindings | — |
| `crates/project/src/endpoint_feeds_tests.rs` | create (test) | — | — |
| `crates/project/src/lib.rs` | modify: `pub mod endpoint_feeds;` | routes the project crate's public surface | 47 |
| `crates/project/src/binding_discovery.rs` | Task 2 fallback only (Part 1 Task 11 owns this edit): skip unchecked head/tail endpoints | resolves which ports a chain gets from the bindings it selected | 234 |
| `crates/project/tests/issue_328_checklist_ports.rs` | Task 2 fallback only: create (test) | — | — |
| `crates/engine/src/segment_types.rs` | modify: `SegmentPaths`, `ChainSegment.paths` | describes one segment of a chain | 48 |
| `crates/engine/src/segment_paths.rs` | create | maps each output route of a chain to the split paths it runs | — |
| `crates/engine/src/segment_paths_tests.rs` | create (test) | — | — |
| `crates/engine/src/split_segment_view.rs` | create | shapes a Y split into the split one segment runs | — |
| `crates/engine/src/split_segment_view_tests.rs` | create (test) | — | — |
| `crates/engine/src/lib.rs` | modify: 2 module lines | routes the engine crate's public surface | 97 |
| `crates/engine/src/runtime_segments.rs` | modify: paths per segment, no-source early return, insert grouping | cuts a chain into segments at its insert points | 425 |
| `crates/engine/src/effective_endpoints.rs` | modify: no fallback when the checklist emptied a node | expands the resolved endpoints into the streams the runtime opens | 174 |
| `crates/engine/src/runtime_graph_assemble.rs` | modify: build from the segment's view | assembles the runtime graph of a chain | 402 |
| `crates/engine/src/runtime_graph_update.rs` | modify: pass `segment.paths` | rebuilds a chain runtime in place | 497 |
| `crates/engine/src/offline.rs` | modify: offline renders a Y chain as both paths | drives a chain through its DSP with no audio device | 258 |
| `crates/engine/src/route_convolution.rs` | modify: per-segment paths | tells whether a convolution block feeds an output route | 60 |
| `crates/engine/src/route_convolution_tests.rs` | modify (test) | — | 169 |
| `crates/engine/src/runtime_graph.rs` | modify: `chain_plays` + graph gate | constructs the audio runtime graph | 496 |
| `crates/engine/src/input_conflicts.rs` | modify: gate on `chain_plays` | (existing header kept) | 133 |
| `crates/engine/src/rig_runtime.rs` | modify: `build` / `enable_input` skip a chain that does not play | turns a rig into the chains the engine runs | 249 |
| `crates/engine/tests/issue_328_endpoint_disables.rs` | modify (test, created by Part 1) | — | — |
| `crates/engine/src/runtime_output_process.rs` | modify: `process_output_f32_all_routes` | fills one physical output device callback | 174 |
| `crates/engine/src/runtime.rs` | modify: re-export line 50 | (existing header kept) | 537 |
| `crates/engine/src/audio_alloc_invariant_tests.rs` | modify (test) | — | 334 |
| `crates/engine/src/issue_328_checklist_segments_tests.rs` | create (test) | — | — |
| `crates/engine/src/issue_328_y_segments_tests.rs` | create (test) | — | — |
| `crates/engine/src/issue_328_y_audio_tests.rs` | create (test) | — | — |
| `crates/engine/src/issue_328_silenced_chain_tests.rs` | create (test) | — | — |
| `crates/engine/src/runtime_output_all_routes_tests.rs` | create (test) | — | — |
| `crates/infra-cpal/src/io_topology.rs` | modify: `paths|…` row | computes the topology signatures a rebuild decision compares | 123 |
| `crates/infra-cpal/src/io_topology_tests.rs` | modify (test) | — | 327 |
| `crates/infra-cpal/src/chain_resolve.rs` | modify: 3 gates | (existing header kept) | 476 |
| `crates/infra-cpal/src/validation.rs` | modify: 1 gate | (existing header kept) | 255 |
| `crates/infra-cpal/src/stream_builder_project.rs` | modify: 1 gate | builds every enabled chain's cpal streams for a whole project | 76 |
| `crates/infra-cpal/src/controller_sync.rs` | modify: 4 gates | syncs the live runtime with the project's enabled chains | 182 |
| `crates/infra-cpal/src/controller_upsert.rs` | modify: 1 gate | (existing header kept) | 214 |
| `crates/infra-cpal/src/controller_disable_kills_streams_tests.rs` | modify (test) | — | 205 |
| `crates/infra-cpal/src/jack_handlers.rs` | modify: pop every route | serves the JACK process callback | 271 |
| `crates/infra-cpal/src/jack_direct.rs` | modify: preallocate route scratch | assembles the live JACK client of a chain | 367 |
| `crates/infra-cpal/tests/issue_328_stream_gates_read_chain_plays.rs` | create (test) | — | — |
| `crates/infra-cpal/tests/issue_328_jack_pops_every_route.rs` | create (test) | — | — |
| `docs/audio-config.md` | modify: paragraphs appended to Part 1's `### Endpoint checklist (issue #328)` section, and a new `### Y → A/B outputs (issue #328)` section right after it (both before `### Mid-chain ports (issue #85)`, line 325 today) | — | 1023 |

Note (order Part 3 → Part 4, see the plan README): Part 3 takes `crates/engine/src/lib.rs` from 97 to 99 (`mod runtime_split;`, `mod runtime_block_reuse;`), so BOTH module lines of this part need the fallback, not only the second one. Before Task 4 Step 5 (where `pub mod segment_paths;` is added) run `wc -l crates/engine/src/lib.rs`; if it prints 99, declare the module instead at the end of `crates/engine/src/runtime_graph.rs` (already a `pub` module — infra-cpal calls `engine::runtime_graph::input_group_ids` and this part's `chain_plays` there) as `#[path = "segment_paths.rs"] pub mod segment_paths;`, and write every `crate::segment_paths::` path of this plan (Task 4 `runtime_segments.rs`, Task 5 `group_routes_by_paths`) as `crate::runtime_graph::segment_paths::` and every `engine::segment_paths::` path (Task 8 `io_topology.rs`) as `engine::runtime_graph::segment_paths::`; nothing else changes. Then, before Task 6 Step 3 run `wc -l crates/engine/src/lib.rs`; if adding `mod split_segment_view;` would take it past 99 lines (the "< 100 LOC router" rule), declare it instead at the end of `crates/engine/src/runtime_graph_assemble.rs` as `#[path = "split_segment_view.rs"] pub(crate) mod split_segment_view;` and write every `crate::split_segment_view::` path of this plan (Task 6 `runtime_graph_assemble.rs` and `offline.rs`, Task 7 `route_convolution.rs`) as `crate::runtime_graph_assemble::split_segment_view::` — nothing else changes.

Contract notes: `ChainSegment.paths` is declared `pub(crate)` like every other field of the `pub(crate) struct ChainSegment` (the contract says `pub`; visibility is identical because the struct itself is crate-private). `classify_output_routes` and `blocks_between` keep their signatures: the per-route path map is `segment_paths::route_paths`, built from the same `resolve_chain_ports` order, and `blocks_between` already includes the split's index (the split is one top-level block; the builder, not the index list, expands it into paths).

---

### Task 1: Endpoint feed rules (`project::endpoint_feeds`)

**Files:**
- Create: `crates/project/src/endpoint_feeds.rs`
- Create: `crates/project/src/endpoint_feeds_tests.rs`
- Modify: `crates/project/src/lib.rs` (between `pub mod endpoint_disables;`, which Part 1 Task 8 adds before line 27, and `pub mod endpoint_ref;`)

**Interfaces:**
- Consumes: Part 1 `Chain.disabled_endpoints`, `EndpointDisables::is_enabled`, `EndpointNode`, `EndpointRef`, `find_split`, `SplitEnd`.
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum TailFeed { Off, Chain, Paths { a: bool, b: bool } }
  pub fn head_input_enabled(chain: &Chain, io: &str, endpoint: &str) -> bool
  pub fn tail_feed(chain: &Chain, io: &str, endpoint: &str) -> TailFeed
  pub fn inputs_all_unchecked(chain: &Chain, registry: &[IoBinding]) -> bool
  pub fn outputs_all_unchecked(chain: &Chain, registry: &[IoBinding]) -> bool
  pub fn checklist_silences(chain: &Chain, registry: &[IoBinding]) -> bool
  ```

- [ ] **Step 1: Write the failing tests** — `crates/project/src/endpoint_feeds_tests.rs`:

```rust
//! #328 — the endpoint checklist of a chain's input and output nodes: which
//! node uses which endpoint of the chain's own E/S.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};

use super::{
    checklist_silences, head_input_enabled, inputs_all_unchecked, outputs_all_unchecked,
    tail_feed, TailFeed,
};
use crate::block::split_params::default_split_params;
use crate::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock, SplitBlock, SplitEnd};
use crate::chain::Chain;
use crate::endpoint_disables::{EndpointDisables, EndpointRef};

fn ep(name: &str, ch: usize) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    }
}

/// One E/S with two inputs and two outputs.
fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![ep("in-1", 0), ep("in-2", 1)],
        outputs: vec![ep("out-1", 0), ep("out-2", 1)],
    }]
}

fn r(endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: "main".into(),
        endpoint: endpoint.into(),
    }
}

fn disables(inputs: &[&str], outputs: &[&str], path_a: &[&str], path_b: &[&str]) -> EndpointDisables {
    let refs = |names: &[&str]| names.iter().map(|name| r(name)).collect::<Vec<_>>();
    EndpointDisables {
        inputs: refs(inputs),
        outputs: refs(outputs),
        path_a_outputs: refs(path_a),
        path_b_outputs: refs(path_b),
    }
}

fn split(end: SplitEnd) -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end,
            params: default_split_params(),
            a: vec![],
            b: vec![],
        }),
    }
}

fn mid_input() -> AudioBlock {
    AudioBlock {
        id: BlockId("mid-in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "aux-in".into(),
        }),
    }
}

fn mid_output() -> AudioBlock {
    AudioBlock {
        id: BlockId("mid-out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "aux-out".into(),
        }),
    }
}

fn chain(blocks: Vec<AudioBlock>, disabled_endpoints: EndpointDisables) -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks,
        di_output: None,
        loopers: vec![],
        disabled_endpoints,
    }
}

#[test]
fn an_unchecked_input_is_off_for_the_input_node_only() {
    let chain = chain(vec![], disables(&["in-1"], &[], &[], &[]));
    assert!(
        !head_input_enabled(&chain, "main", "in-1"),
        "#328: in-1 is unchecked on the input node"
    );
    assert!(head_input_enabled(&chain, "main", "in-2"), "in-2 stays checked");
    assert_eq!(
        tail_feed(&chain, "main", "out-1"),
        TailFeed::Chain,
        "the output node is untouched"
    );
}

#[test]
fn a_linear_chain_sends_to_what_its_output_node_checks() {
    let chain = chain(vec![], disables(&[], &["out-2"], &["out-1"], &["out-1"]));
    assert_eq!(
        tail_feed(&chain, "main", "out-2"),
        TailFeed::Off,
        "#328: out-2 is unchecked on the chain output node"
    );
    assert_eq!(
        tail_feed(&chain, "main", "out-1"),
        TailFeed::Chain,
        "#328: the path lists mean nothing without a Y split"
    );
}

#[test]
fn a_mix_split_sends_through_the_chain_output_node() {
    let chain = chain(vec![split(SplitEnd::Mix)], disables(&[], &["out-1"], &[], &[]));
    assert_eq!(tail_feed(&chain, "main", "out-1"), TailFeed::Off);
    assert_eq!(
        tail_feed(&chain, "main", "out-2"),
        TailFeed::Chain,
        "#328: a Split → Mix ends in the chain output node"
    );
}

#[test]
fn a_y_split_sends_each_output_from_the_paths_that_check_it() {
    let chain = chain(
        vec![split(SplitEnd::Y)],
        disables(&[], &["out-1", "out-2"], &["out-2"], &["out-1"]),
    );
    assert_eq!(
        tail_feed(&chain, "main", "out-1"),
        TailFeed::Paths { a: true, b: false },
        "#328: path A has out-1 checked, path B does not"
    );
    assert_eq!(
        tail_feed(&chain, "main", "out-2"),
        TailFeed::Paths { a: false, b: true },
        "#328: the chain output list means nothing on a Y chain"
    );
}

#[test]
fn a_y_output_both_paths_uncheck_is_off() {
    let chain = chain(vec![split(SplitEnd::Y)], disables(&[], &[], &["out-1"], &["out-1"]));
    assert_eq!(
        tail_feed(&chain, "main", "out-1"),
        TailFeed::Off,
        "#328: no path sends to out-1"
    );
    assert_eq!(
        tail_feed(&chain, "main", "out-2"),
        TailFeed::Paths { a: true, b: true },
        "#328: checked on both paths by default"
    );
}

#[test]
fn a_reference_the_bindings_no_longer_have_is_ignored() {
    let chain = chain(vec![], disables(&["gone"], &["gone"], &[], &[]));
    assert!(head_input_enabled(&chain, "main", "in-1"));
    assert!(
        !inputs_all_unchecked(&chain, &registry()),
        "#328: a stale ref must not count as an unchecked input"
    );
    assert!(!outputs_all_unchecked(&chain, &registry()));
}

#[test]
fn unchecking_every_input_silences_the_chain_unless_a_mid_input_feeds_it() {
    let silenced = chain(vec![], disables(&["in-1", "in-2"], &[], &[], &[]));
    assert!(
        inputs_all_unchecked(&silenced, &registry()),
        "#328: no head input is left"
    );
    assert!(checklist_silences(&silenced, &registry()));
    let fed = chain(vec![mid_input()], disables(&["in-1", "in-2"], &[], &[], &[]));
    assert!(
        !inputs_all_unchecked(&fed, &registry()),
        "#328: a mid Input is its own node and still feeds the chain"
    );
}

#[test]
fn unchecking_every_output_silences_the_chain_unless_a_mid_output_takes_it() {
    let linear = chain(vec![], disables(&[], &["out-1", "out-2"], &[], &[]));
    assert!(outputs_all_unchecked(&linear, &registry()));
    assert!(checklist_silences(&linear, &registry()));
    let tapped = chain(vec![mid_output()], disables(&[], &["out-1", "out-2"], &[], &[]));
    assert!(
        !outputs_all_unchecked(&tapped, &registry()),
        "#328: a mid Output still plays the chain"
    );
    let y_silent = chain(
        vec![split(SplitEnd::Y)],
        disables(&[], &[], &["out-1", "out-2"], &["out-1", "out-2"]),
    );
    assert!(
        outputs_all_unchecked(&y_silent, &registry()),
        "#328: no path sends anywhere"
    );
    let y_one = chain(
        vec![split(SplitEnd::Y)],
        disables(&[], &[], &["out-1", "out-2"], &["out-1"]),
    );
    assert!(
        !outputs_all_unchecked(&y_one, &registry()),
        "#328: path B still sends to out-2"
    );
}

#[test]
fn a_chain_without_bindings_is_never_silenced_by_the_checklist() {
    let mut legacy = chain(vec![], disables(&["in-1", "in-2"], &["out-1", "out-2"], &[], &[]));
    legacy.io_binding_ids.clear();
    assert!(
        !checklist_silences(&legacy, &registry()),
        "#328: nothing to uncheck — the legacy fallback endpoints stay"
    );
}
```

- [ ] **Step 2: Run to see the compile red**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p project --lib endpoint_feeds -j 2`
Expected: FAIL to compile — the test module is not mounted yet / `error[E0432]: unresolved import super::…` once mounted. Not the red that counts; go on.

- [ ] **Step 3: Compiling skeleton** — `crates/project/src/endpoint_feeds.rs`:

```rust
//! Responsibility: decides which chain node uses each endpoint of the chain's bindings.

use domain::io_binding::IoBinding;

use crate::chain::Chain;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailFeed {
    Off,
    Chain,
    Paths { a: bool, b: bool },
}

pub fn head_input_enabled(_chain: &Chain, _io: &str, _endpoint: &str) -> bool {
    true
}

pub fn tail_feed(_chain: &Chain, _io: &str, _endpoint: &str) -> TailFeed {
    TailFeed::Chain
}

pub fn inputs_all_unchecked(_chain: &Chain, _registry: &[IoBinding]) -> bool {
    false
}

pub fn outputs_all_unchecked(_chain: &Chain, _registry: &[IoBinding]) -> bool {
    false
}

pub fn checklist_silences(_chain: &Chain, _registry: &[IoBinding]) -> bool {
    false
}

#[cfg(test)]
#[path = "endpoint_feeds_tests.rs"]
mod tests;
```

and in `crates/project/src/lib.rs`, right after `pub mod endpoint_disables;` (so the list stays alphabetical, before `pub mod endpoint_ref;`):

```rust
pub mod endpoint_feeds;
```

- [ ] **Step 4: Run to see the assertion red**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p project --lib endpoint_feeds -j 2`
Expected: FAIL — e.g. `endpoint_feeds::tests::a_linear_chain_sends_to_what_its_output_node_checks` panics with ``assertion `left == right` failed: #328: out-2 is unchecked on the chain output node`` / `left: Chain` / `right: Off`; `an_unchecked_input_is_off_for_the_input_node_only` panics with `#328: in-1 is unchecked on the input node`. Two tests pass on the skeleton because they pin the "nothing is off" answer it returns: `a_reference_the_bindings_no_longer_have_is_ignored` and `a_chain_without_bindings_is_never_silenced_by_the_checklist` (7 failed, 2 passed).

- [ ] **Step 5: Implementation** — replace the whole body of `crates/project/src/endpoint_feeds.rs`:

```rust
//! Responsibility: decides which chain node uses each endpoint of the chain's bindings.
//!
//! #328 — the input and output nodes of a chain's graph each carry a checklist
//! of the endpoints of the chain's own E/S (`Chain.disabled_endpoints`, copied
//! from `RigInput`). Every endpoint is checked unless the node's list names
//! it. A linear or Split → Mix chain has one output node; a Y → A/B chain has
//! two (path A's, path B's) and no chain output node. A reference to an
//! endpoint the E/S no longer has matches nothing, so it is ignored.

use domain::io_binding::IoBinding;

use crate::block::{find_split, AudioBlockKind, SplitEnd};
use crate::chain::Chain;
use crate::endpoint_disables::{EndpointNode, EndpointRef};

/// Which output node of a chain sends to one tail endpoint of its E/S.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailFeed {
    /// No node sends here: the endpoint is unchecked on every output node.
    Off,
    /// The chain output node (linear and Split → Mix chains).
    Chain,
    /// Y → A/B: the path output nodes that have the endpoint checked — at
    /// least one of `a`, `b` is true.
    Paths { a: bool, b: bool },
}

/// Whether the chain's input node reads head endpoint `endpoint` of E/S `io`.
pub fn head_input_enabled(chain: &Chain, io: &str, endpoint: &str) -> bool {
    chain
        .disabled_endpoints
        .is_enabled(EndpointNode::Input, &endpoint_ref(io, endpoint))
}

/// Which output node of `chain` sends to tail endpoint `endpoint` of E/S `io`.
pub fn tail_feed(chain: &Chain, io: &str, endpoint: &str) -> TailFeed {
    let reference = endpoint_ref(io, endpoint);
    let disables = &chain.disabled_endpoints;
    if !has_y_split(chain) {
        return if disables.is_enabled(EndpointNode::Output, &reference) {
            TailFeed::Chain
        } else {
            TailFeed::Off
        };
    }
    let a = disables.is_enabled(EndpointNode::PathAOutput, &reference);
    let b = disables.is_enabled(EndpointNode::PathBOutput, &reference);
    if a || b {
        TailFeed::Paths { a, b }
    } else {
        TailFeed::Off
    }
}

/// Whether the input node's checklist is what leaves the chain with no
/// source: its E/S have head inputs, every one is unchecked, and no enabled
/// mid `Input` block brings a signal of its own.
pub fn inputs_all_unchecked(chain: &Chain, registry: &[IoBinding]) -> bool {
    let mut heads = selected_bindings(chain, registry)
        .flat_map(|binding| {
            binding
                .inputs
                .iter()
                .map(move |ep| (binding.id.as_str(), ep.name.as_str()))
        })
        .peekable();
    heads.peek().is_some()
        && heads.all(|(io, endpoint)| !head_input_enabled(chain, io, endpoint))
        && !has_enabled_block(chain, |kind| matches!(kind, AudioBlockKind::Input(_)))
}

/// Whether the output nodes' checklists are what leave the chain with no
/// output: its E/S have tail outputs, no output node sends to any of them,
/// and no enabled mid `Output` block takes the signal elsewhere.
pub fn outputs_all_unchecked(chain: &Chain, registry: &[IoBinding]) -> bool {
    let mut tails = selected_bindings(chain, registry)
        .flat_map(|binding| {
            binding
                .outputs
                .iter()
                .map(move |ep| (binding.id.as_str(), ep.name.as_str()))
        })
        .peekable();
    tails.peek().is_some()
        && tails.all(|(io, endpoint)| tail_feed(chain, io, endpoint) == TailFeed::Off)
        && !has_enabled_block(chain, |kind| matches!(kind, AudioBlockKind::Output(_)))
}

/// Whether the checklist leaves the chain nothing to play (spec §5.3:
/// allowed — the chain simply builds no segment).
pub fn checklist_silences(chain: &Chain, registry: &[IoBinding]) -> bool {
    inputs_all_unchecked(chain, registry) || outputs_all_unchecked(chain, registry)
}

/// Whether the chain's one split (Part 1 `find_split`) ends in Y → A/B.
fn has_y_split(chain: &Chain) -> bool {
    find_split(&chain.blocks).is_some_and(|(_, split)| split.end == SplitEnd::Y)
}

fn has_enabled_block(chain: &Chain, is_port: fn(&AudioBlockKind) -> bool) -> bool {
    chain
        .blocks
        .iter()
        .any(|block| block.enabled && is_port(&block.kind))
}

fn selected_bindings<'a>(
    chain: &'a Chain,
    registry: &'a [IoBinding],
) -> impl Iterator<Item = &'a IoBinding> {
    chain
        .io_binding_ids
        .iter()
        .filter_map(move |id| registry.iter().find(|binding| &binding.id == id))
}

fn endpoint_ref(io: &str, endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: io.to_string(),
        endpoint: endpoint.to_string(),
    }
}

#[cfg(test)]
#[path = "endpoint_feeds_tests.rs"]
mod tests;
```

- [ ] **Step 6: Run to see green**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p project --lib endpoint_feeds -j 2`
Expected: PASS — `test result: ok. 9 passed; 0 failed`.

- [ ] **Step 7: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/project/src/endpoint_feeds.rs crates/project/src/endpoint_feeds_tests.rs crates/project/src/lib.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "$(cat <<'EOF'
feat(#328): endpoint feed rules for the input and output node checklists
EOF
)"
```

- [ ] **Step 8: Push**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch && git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push
gh issue comment 328 -R jpfaria/OpenRig --body "Part 4 · Task 1 pushed $(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD): project::endpoint_feeds (+tests). cargo test -p project --lib endpoint_feeds: 9 passed."
```

---

### Task 2: `resolve_chain_ports` applies the checklist (owned by Part 1 Task 11 — verify, re-implement only if missing)

Part 1 Task 11 ("Discovery honours the checklists") already makes `resolve_chain_ports` drop an unchecked head input and a tail output no output node keeps, pins it in `crates/project/tests/issue_328_endpoint_discovery.rs`, and documents it. Implementing it again here would add a second filter and a test that is green before any code — not a red. So this task first checks that Part 1 delivered it.

**Files:**
- Read only (normal case): `crates/project/src/binding_discovery.rs`, `crates/project/tests/issue_328_endpoint_discovery.rs`
- Fallback only (Part 1 dropped its Task 11): Modify `crates/project/src/binding_discovery.rs:45-80` (doc + head/tail loops); Create `crates/project/tests/issue_328_checklist_ports.rs`; Modify `docs/audio-config.md` — insert a section before line 325 `### Mid-chain ports (issue #85)`

**Interfaces:**
- Consumes: Part 1 Task 11 (normal case); Task 1 `head_input_enabled`, `tail_feed`, `TailFeed` (fallback).
- Produces: `resolve_chain_ports(chain, registry)` omits an unchecked head input and a tail output no output node sends to. Every consumer (engine segmentation, infra-cpal stream open, looper/DI pickers, meters) inherits it.

- [ ] **Step 0: Check that Part 1 delivered the filter**

Run: `grep -n "disabled_endpoints" /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328/crates/project/src/binding_discovery.rs && cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p project --test issue_328_endpoint_discovery -j 2`
Expected (normal case): the grep prints `let disabled = &chain.disabled_endpoints;` and the test binary ends `test result: ok. 6 passed`. Then this task is done — nothing to write, nothing to commit; the `### Endpoint checklist (issue #328)` section Part 1 wrote is the one later tasks append to. Go to Task 3.
If the grep prints nothing (Part 1 shipped without its Task 11), run Steps 1–7 below.

- [ ] **Step 1 (fallback only): Write the failing test** — `crates/project/tests/issue_328_checklist_ports.rs`:

```rust
//! #328 — the endpoint checklist decides which endpoints of the chain's own
//! E/S are ports. An unchecked head input, and a tail output no output node
//! sends to, are no port: no stream opens for them and no route is built.
//! Mid `Input`/`Output` blocks are nodes of their own and ignore it.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::binding_discovery::{resolve_chain_ports, ChainPort, PortDirection};
use project::block::split_params::default_split_params;
use project::block::{AudioBlock, AudioBlockKind, OutputBlock, SplitBlock, SplitEnd};
use project::chain::Chain;
use project::endpoint_disables::{EndpointDisables, EndpointRef};

fn ep(name: &str, ch: usize) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    }
}

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![ep("in-1", 0), ep("in-2", 1)],
        outputs: vec![ep("out-1", 0), ep("out-2", 1)],
    }]
}

fn disables(inputs: &[&str], outputs: &[&str], path_a: &[&str], path_b: &[&str]) -> EndpointDisables {
    let refs = |names: &[&str]| {
        names
            .iter()
            .map(|name| EndpointRef {
                io: "main".into(),
                endpoint: (*name).into(),
            })
            .collect::<Vec<_>>()
    };
    EndpointDisables {
        inputs: refs(inputs),
        outputs: refs(outputs),
        path_a_outputs: refs(path_a),
        path_b_outputs: refs(path_b),
    }
}

fn y_split() -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Y,
            params: default_split_params(),
            a: vec![],
            b: vec![],
        }),
    }
}

fn chain(blocks: Vec<AudioBlock>, disabled_endpoints: EndpointDisables) -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks,
        di_output: None,
        loopers: vec![],
        disabled_endpoints,
    }
}

fn names(ports: &[ChainPort], direction: PortDirection, from_block: bool) -> Vec<String> {
    ports
        .iter()
        .filter(|p| p.direction == direction && p.from_block == from_block)
        .map(|p| p.endpoint.name.clone())
        .collect()
}

#[test]
fn an_unchecked_input_is_no_port() {
    let ports = resolve_chain_ports(&chain(vec![], disables(&["in-1"], &[], &[], &[])), &registry());
    assert_eq!(
        names(&ports, PortDirection::Input, false),
        vec!["in-2"],
        "#328: in-1 is unchecked on the input node — no port, so no stream opens for it"
    );
}

#[test]
fn an_unchecked_output_is_no_port() {
    let ports = resolve_chain_ports(&chain(vec![], disables(&[], &["out-2"], &[], &[])), &registry());
    assert_eq!(
        names(&ports, PortDirection::Output, false),
        vec!["out-1"],
        "#328: out-2 is unchecked on the chain output node — no port, no route"
    );
}

#[test]
fn a_y_output_is_a_port_while_either_path_sends_to_it() {
    let one_path = chain(vec![y_split()], disables(&[], &[], &["out-1"], &[]));
    assert_eq!(
        names(&resolve_chain_ports(&one_path, &registry()), PortDirection::Output, false),
        vec!["out-1", "out-2"],
        "#328: path B still sends to out-1"
    );
    let no_path = chain(vec![y_split()], disables(&[], &[], &["out-1"], &["out-1"]));
    assert_eq!(
        names(&resolve_chain_ports(&no_path, &registry()), PortDirection::Output, false),
        vec!["out-2"],
        "#328: no path sends to out-1 — it is no port"
    );
}

#[test]
fn a_mid_output_is_its_own_node() {
    let mid = AudioBlock {
        id: BlockId("mid-out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "main".into(),
            endpoint: "out-1".into(),
        }),
    };
    let ports = resolve_chain_ports(&chain(vec![mid], disables(&[], &["out-1"], &[], &[])), &registry());
    assert_eq!(
        names(&ports, PortDirection::Output, false),
        vec!["out-2"],
        "#328: the tail out-1 is unchecked"
    );
    assert_eq!(
        names(&ports, PortDirection::Output, true),
        vec!["out-1"],
        "#328: the mid Output on out-1 ignores the output node's checklist"
    );
}

#[test]
fn with_nothing_unchecked_every_endpoint_is_a_port() {
    let ports = resolve_chain_ports(&chain(vec![], EndpointDisables::default()), &registry());
    assert_eq!(names(&ports, PortDirection::Input, false), vec!["in-1", "in-2"]);
    assert_eq!(names(&ports, PortDirection::Output, false), vec!["out-1", "out-2"]);
}
```

- [ ] **Step 2 (fallback only): Run to see the red**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p project --test issue_328_checklist_ports -j 2`
Expected: FAIL — `an_unchecked_input_is_no_port` panics ``assertion `left == right` failed: #328: in-1 is unchecked on the input node — no port, so no stream opens for it`` (`left: ["in-1", "in-2"]`, `right: ["in-2"]`); `an_unchecked_output_is_no_port`, the second assertion of `a_y_output_is_a_port_while_either_path_sends_to_it` and the first of `a_mid_output_is_its_own_node` fail the same way. `with_nothing_unchecked_every_endpoint_is_a_port` passes (it pins the unchanged default).

- [ ] **Step 3 (fallback only): Implementation** — `crates/project/src/binding_discovery.rs`. Replace the doc lines 45-51 and the head/tail loops at lines 62-79:

```rust
/// Resolve every I/O port of `chain` against the binding `registry`.
///
/// - Head inputs / tail outputs come from the bindings the chain selects
///   (`io_binding_ids`) — these are never persisted in the chain. #328: an
///   endpoint the input node's checklist unchecks, and one no output node
///   sends to (`endpoint_feeds`), is no port — so no stream opens for it and
///   every consumer numbers the rest exactly as the engine numbers its routes.
/// - Mid `Input` / `Output` blocks resolve their `io`/`endpoint` reference.
///
/// Ports whose binding (or endpoint) is absent from the registry are skipped.
```

```rust
        for ep in &binding.inputs {
            // #328: unchecked on the input node → no port.
            if !crate::endpoint_feeds::head_input_enabled(chain, &binding.id, &ep.name) {
                continue;
            }
            ports.push(ChainPort {
                direction: PortDirection::Input,
                offset: 0,
                from_block: false,
                binding_id: binding.id.clone(),
                endpoint: ep.clone(),
            });
        }
        for ep in &binding.outputs {
            // #328: no output node (the chain's, or a Y split's two) sends here.
            if crate::endpoint_feeds::tail_feed(chain, &binding.id, &ep.name)
                == crate::endpoint_feeds::TailFeed::Off
            {
                continue;
            }
            ports.push(ChainPort {
                direction: PortDirection::Output,
                offset: tail,
                from_block: false,
                binding_id: binding.id.clone(),
                endpoint: ep.clone(),
            });
        }
```

- [ ] **Step 4 (fallback only): Run to see green**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p project --test issue_328_checklist_ports --test issue_716_binding_discovery --test issue_871_disabled_io_block_has_no_port -j 2`
Expected: PASS — all three binaries `ok`.

- [ ] **Step 5 (fallback only): Docs** — insert into `docs/audio-config.md` immediately before line 325 `### Mid-chain ports (issue #85)` (where Part 1 Task 11 would have put it):

```markdown
### Endpoint checklist (issue #328)

The input and output nodes of a chain's graph each carry a checklist of the
endpoints of the chain's own E/S. Every endpoint is listed and checked by
default. Unchecking one disables that endpoint **for that node only**: it stays
listed and the E/S itself is untouched. The state lives on the rig input
(`RigInput.disabled_endpoints`, copied onto the synthetic `Chain` by
`rig_to_chains`), because the same preset can be reused by several inputs.

`resolve_chain_ports` applies it (`project::endpoint_feeds`), so every consumer
reads the same answer: an unchecked head input, and a tail output no output
node sends to, are **no port at all** — no stream opens for it, no route is
built, and the looper and DI pickers number the remaining endpoints exactly as
the engine numbers its routes. Mid `Input`/`Output` blocks are nodes of their
own and ignore the checklist. A reference to an endpoint the E/S no longer has
is ignored.

| Chain | Output node(s) | List read |
|---|---|---|
| linear, Split → Mix | the chain output | `outputs` |
| Y → A/B | path A's, path B's | `path_a_outputs`, `path_b_outputs` — the endpoint is a port while either path has it checked |

The graph's checklist must list endpoints from the E/S itself, not from
`resolve_chain_ports`, which no longer returns the unchecked ones.
```

- [ ] **Step 6 (fallback only): Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/project/src/binding_discovery.rs crates/project/tests/issue_328_checklist_ports.rs docs/audio-config.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "$(cat <<'EOF'
feat(#328): an unchecked endpoint is no port of the chain
EOF
)"
```

- [ ] **Step 7 (fallback only): Push**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch && git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push
gh issue comment 328 -R jpfaria/OpenRig --body "Part 4 · Task 2 pushed $(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD): binding_discovery.rs, tests/issue_328_checklist_ports.rs, docs/audio-config.md. project tests issue_328/716/871 green."
```

---

### Task 3: No phantom endpoint when the checklist empties a node

**Files:**
- Modify: `crates/engine/src/effective_endpoints.rs:109-111` and `:138-140`; mount the test at the end of the file
- Modify: `crates/engine/src/runtime_segments.rs:52-53` (early return at the top of `split_chain_into_segments`)
- Create: `crates/engine/src/issue_328_checklist_segments_tests.rs`
- Modify: `docs/audio-config.md` (append to the `### Endpoint checklist (issue #328)` section)

**Interfaces:**
- Consumes: Task 1 `inputs_all_unchecked`, `outputs_all_unchecked`; the checklist filter in `resolve_chain_ports` (Part 1 Task 11, or Task 2's fallback).
- Produces: `effective_inputs`/`effective_outputs` never invent their legacy fallback entry for a node the checklist emptied; `split_chain_into_segments` returns `vec![]` when `inputs_all_unchecked`.

- [ ] **Step 1: Write the failing test** — `crates/engine/src/issue_328_checklist_segments_tests.rs`:

```rust
//! #328 — unchecking every input (or every output) of a chain is allowed and
//! means "nothing to play" (spec §5.3). The engine used to fill an empty node
//! with its legacy fallback endpoint — mono channel 0 of device "" — and the
//! insert walker let a loop's return stand in for a missing input, feeding the
//! pedal into itself.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;
use project::endpoint_disables::{EndpointDisables, EndpointRef};

use crate::runtime_endpoints::{effective_inputs, effective_outputs, resolve_chain_io};
use crate::runtime_graph::chain_stream_count;
use crate::runtime_segments::split_chain_into_segments;

fn endpoint(name: &str, mode: ChannelMode, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode,
        channels,
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![endpoint("in", ChannelMode::Mono, vec![0])],
            outputs: vec![endpoint("out", ChannelMode::Stereo, vec![0, 1])],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![endpoint("ret", ChannelMode::Mono, vec![3])],
            outputs: vec![endpoint("snd", ChannelMode::Mono, vec![3])],
        },
    ]
}

fn off(endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: "main".into(),
        endpoint: endpoint.into(),
    }
}

fn chain(blocks: Vec<AudioBlock>, disabled_endpoints: EndpointDisables) -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks,
        di_output: None,
        loopers: vec![],
        disabled_endpoints,
    }
}

fn insert() -> AudioBlock {
    AudioBlock {
        id: BlockId("loop".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".into(),
            io: "fx".into(),
        }),
    }
}

/// `(effective inputs, effective outputs)` the runtime would open.
fn endpoints(chain: &Chain) -> (usize, usize) {
    let reg = registry();
    let (ri, ro) = resolve_chain_io(chain, &reg);
    (
        effective_inputs(chain, &ri, &reg).0.len(),
        effective_outputs(chain, &ro, &reg).len(),
    )
}

fn segment_count(chain: &Chain) -> usize {
    let reg = registry();
    let (ri, ro) = resolve_chain_io(chain, &reg);
    let (ei, ci, sp, eg) = effective_inputs(chain, &ri, &reg);
    let eo = effective_outputs(chain, &ro, &reg);
    split_chain_into_segments(chain, &ei, &ci, &sp, &eg, &eo, &reg).len()
}

#[test]
fn every_output_unchecked_invents_no_fallback_output() {
    let chain = chain(
        vec![],
        EndpointDisables {
            outputs: vec![off("out")],
            ..EndpointDisables::default()
        },
    );
    assert_eq!(
        endpoints(&chain).1,
        0,
        "#328: the checklist emptied the output node — the legacy fallback output (device \"\") must not appear"
    );
    assert_eq!(
        chain_stream_count(&chain, &registry()),
        0,
        "#328: no output, no stream"
    );
}

#[test]
fn every_input_unchecked_invents_no_fallback_input() {
    let chain = chain(
        vec![],
        EndpointDisables {
            inputs: vec![off("in")],
            ..EndpointDisables::default()
        },
    );
    assert_eq!(
        endpoints(&chain).0,
        0,
        "#328: the checklist emptied the input node — the legacy fallback input must not appear"
    );
    assert_eq!(segment_count(&chain), 0);
}

#[test]
fn an_insert_return_never_stands_in_for_an_unchecked_input() {
    let chain = chain(
        vec![insert()],
        EndpointDisables {
            inputs: vec![off("in")],
            ..EndpointDisables::default()
        },
    );
    assert_eq!(
        segment_count(&chain),
        0,
        "#328: with every input unchecked the chain has no source — feeding the pre-insert blocks from the insert's own return would loop the pedal into itself"
    );
}

#[test]
fn a_chain_without_bindings_keeps_its_fallback_endpoints() {
    let mut legacy = chain(vec![], EndpointDisables::default());
    legacy.io_binding_ids.clear();
    assert_eq!(
        endpoints(&legacy),
        (1, 1),
        "the pre-#328 fallback stays for a chain that selects no E/S"
    );
}
```

Mount it at the end of `crates/engine/src/effective_endpoints.rs` (after line 174):

```rust

#[cfg(test)]
#[path = "issue_328_checklist_segments_tests.rs"]
mod issue_328_checklist_segments_tests;
```

- [ ] **Step 2: Run to see the red**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib issue_328_checklist_segments -j 2`
Expected: FAIL — `every_output_unchecked_invents_no_fallback_output` panics ``assertion `left == right` failed: #328: the checklist emptied the output node — the legacy fallback output (device "") must not appear`` (`left: 1`, `right: 0`); `every_input_unchecked_invents_no_fallback_input` `left: 1 right: 0`; `an_insert_return_never_stands_in_for_an_unchecked_input` `left: 2 right: 0`. `a_chain_without_bindings_keeps_its_fallback_endpoints` passes (pins the legacy fallback).

- [ ] **Step 3: Implementation**

`crates/engine/src/effective_endpoints.rs:109-111` becomes:

```rust
    // #328: a node the checklist emptied stays empty — the fallback below is
    // for a chain with no E/S, never a way to reach device "" behind the
    // user's unchecked inputs.
    if !entries.is_empty() || project::endpoint_feeds::inputs_all_unchecked(chain, registry) {
        return (entries, cpal_indices, split_positions, entry_groups);
    }
```

`crates/engine/src/effective_endpoints.rs:138-140` becomes:

```rust
    // #328: same rule on the output side.
    if !entries.is_empty() || project::endpoint_feeds::outputs_all_unchecked(chain, registry) {
        return entries;
    }
```

`crates/engine/src/runtime_segments.rs`, first statement of `split_chain_into_segments` (before the `insert_positions` comment at line 53):

```rust
    // #328: the input node's checklist left the chain no head input and no mid
    // `Input` either — it has no source, so it builds nothing. An insert's
    // return must never stand in for the missing input (the loop would feed
    // itself).
    if project::endpoint_feeds::inputs_all_unchecked(chain, registry) {
        return Vec::new();
    }
```

- [ ] **Step 4: Run to see green**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib -j 2 -- issue_328_checklist_segments issue_85 issue_928 issue_967`
Expected: PASS for all four filters.

- [ ] **Step 5: Docs** — append to the `### Endpoint checklist (issue #328)` section of `docs/audio-config.md`:

```markdown
Unchecking **every** input (with no mid `Input`) or **every** output (with no
mid `Output`) is allowed: the chain simply has nothing to play. The engine does
not invent its legacy fallback endpoint for it (`effective_inputs` /
`effective_outputs` keep that fallback for a chain that selects no E/S), and
with no input it builds no segment at all — an insert's return must never stand
in for the missing input.
```

- [ ] **Step 6: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/engine/src/effective_endpoints.rs crates/engine/src/runtime_segments.rs crates/engine/src/issue_328_checklist_segments_tests.rs docs/audio-config.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "$(cat <<'EOF'
feat(#328): a node the checklist empties gets no fallback endpoint
EOF
)"
```

- [ ] **Step 7: Push**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch && git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push
gh issue comment 328 -R jpfaria/OpenRig --body "Part 4 · Task 3 pushed $(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD): effective_endpoints.rs, runtime_segments.rs (+tests), docs. engine issue_328_checklist_segments/85/928/967 green."
```

---

### Task 4: Each segment carries the split paths its output runs

**Files:**
- Modify: `crates/engine/src/segment_types.rs` (append `SegmentPaths`; add field to `ChainSegment` at lines 29-48)
- Create: `crates/engine/src/segment_paths.rs`
- Modify: `crates/engine/src/lib.rs:84` (after `mod segment_binding;` add `pub mod segment_paths;`)
- Modify: `crates/engine/src/runtime_segments.rs:32`, `:80`, `:83-94`, `:183-194`, `:278-286`, `:348-361`, `:366-380`, `:408-422`, test mount at the end
- Create: `crates/engine/src/issue_328_y_segments_tests.rs`
- Modify: `docs/audio-config.md` (new `### Y → A/B outputs (issue #328)` section after the checklist section)

**Interfaces:**
- Consumes: Task 1 `tail_feed`, `TailFeed`; the checklist filter in `resolve_chain_ports` (Part 1 Task 11, or Task 2's fallback).
- Produces:
  ```rust
  // crates/engine/src/segment_types.rs
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum SegmentPaths { None, A, B, AB }
  pub(crate) struct ChainSegment { …, pub(crate) paths: SegmentPaths }
  // crates/engine/src/segment_paths.rs
  pub use crate::segment_types::SegmentPaths;
  pub fn route_paths(chain: &Chain, registry: &[IoBinding]) -> Vec<SegmentPaths>   // index = route idx
  ```

- [ ] **Step 1: Write the failing test** — `crates/engine/src/issue_328_y_segments_tests.rs`:

```rust
//! #328 — Y → A/B routing keeps the one-segment-per-(input × output) model
//! (#85). The segment of output O runs the shared blocks and then the paths
//! whose output node has O checked: `ChainSegment.paths`. Both paths on one
//! output are ONE segment (the builder sums them inside it, aligned) — two
//! segments would be summed on the route, outside the mixer.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::split_params::default_split_params;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, OutputBlock, SplitBlock, SplitEnd};
use project::chain::Chain;
use project::endpoint_disables::{EndpointDisables, EndpointRef};
use project::param::ParameterSet;

use crate::runtime_endpoints::{effective_inputs, effective_outputs, resolve_chain_io};
use crate::runtime_segments::{split_chain_into_segments, ChainSegment};
use crate::segment_types::SegmentPaths;

fn mono(name: &str, device: &str, ch: usize) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    }
}

fn stereo(name: &str, device: &str, channels: [usize; 2]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Stereo,
        channels: channels.to_vec(),
    }
}

/// `main` is the chain's E/S; `aux` is another interface a mid Output uses.
fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![mono("in", "dev", 0)],
            outputs: vec![
                stereo("out-a", "dev", [0, 1]),
                stereo("out-b", "dev", [2, 3]),
                stereo("out-ab", "dev", [4, 5]),
            ],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: vec![],
            outputs: vec![stereo("aux-out", "aux", [6, 7])],
        },
    ]
}

fn effect(id: &str) -> AudioBlock {
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

fn y_split() -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Y,
            params: default_split_params(),
            a: vec![effect("amp-a")],
            b: vec![effect("amp-b")],
        }),
    }
}

fn mid_output() -> AudioBlock {
    AudioBlock {
        id: BlockId("mid-out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "aux-out".into(),
        }),
    }
}

fn r(io: &str, endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: io.into(),
        endpoint: endpoint.into(),
    }
}

/// Path A → out-a + out-ab; path B → out-b + out-ab.
fn y_disables() -> EndpointDisables {
    EndpointDisables {
        inputs: vec![],
        outputs: vec![],
        path_a_outputs: vec![r("main", "out-b")],
        path_b_outputs: vec![r("main", "out-a")],
    }
}

fn chain(bindings: &[&str], blocks: Vec<AudioBlock>, disabled_endpoints: EndpointDisables) -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: bindings.iter().map(|b| b.to_string()).collect(),
        blocks,
        di_output: None,
        loopers: vec![],
        disabled_endpoints,
    }
}

fn segments(chain: &Chain, registry: &[IoBinding]) -> (Vec<ChainSegment>, Vec<Vec<usize>>) {
    let (ri, ro) = resolve_chain_io(chain, registry);
    let (ei, ci, sp, eg) = effective_inputs(chain, &ri, registry);
    let eo = effective_outputs(chain, &ro, registry);
    let segs = split_chain_into_segments(chain, &ei, &ci, &sp, &eg, &eo, registry);
    let outs = eo.iter().map(|o| o.channels.clone()).collect();
    (segs, outs)
}

/// `(output channels, paths)` per segment — the routing, readably.
fn routing(chain: &Chain, registry: &[IoBinding]) -> Vec<(Vec<usize>, SegmentPaths)> {
    let (segs, outs) = segments(chain, registry);
    segs.iter()
        .map(|s| (outs[s.output_route_indices[0]].clone(), s.paths))
        .collect()
}

#[test]
fn each_output_runs_the_paths_that_check_it() {
    let chain = chain(&["main"], vec![mid_output(), effect("pre"), y_split()], y_disables());
    assert_eq!(
        routing(&chain, &registry()),
        vec![
            (vec![0, 1], SegmentPaths::A),
            (vec![2, 3], SegmentPaths::B),
            (vec![4, 5], SegmentPaths::AB),
            (vec![6, 7], SegmentPaths::None),
        ],
        "#328: out-a runs path A, out-b path B, out-ab ONE pipeline with both; the mid \
         Output sits before the split and runs no path"
    );
    let (segs, _) = segments(&chain, &registry());
    assert!(
        segs.iter()
            .filter(|s| s.paths != SegmentPaths::None)
            .all(|s| s.block_indices == vec![1, 2]),
        "#328: every Y output runs the shared blocks, then the split the builder shapes"
    );
}

#[test]
fn an_output_no_path_checks_has_no_pipeline() {
    let mut disables = y_disables();
    disables.path_b_outputs.push(r("main", "out-b"));
    let chain = chain(&["main"], vec![y_split()], disables);
    assert_eq!(
        routing(&chain, &registry()),
        vec![(vec![0, 1], SegmentPaths::A), (vec![4, 5], SegmentPaths::AB)],
        "#328: out-b is checked on no path — no route, no segment"
    );
}

/// Two E/S on one Y chain: a head input still pairs only with its own E/S's
/// outputs (#716); each pair still runs the paths its output checks.
#[test]
fn a_head_input_still_pairs_only_with_its_own_e_s() {
    let registry = vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![mono("in-main", "dev", 0)],
            outputs: vec![stereo("out-main", "dev", [0, 1])],
        },
        IoBinding {
            id: "second".into(),
            name: "SECOND".into(),
            inputs: vec![mono("in-2", "dev", 1)],
            outputs: vec![stereo("out-2", "dev", [2, 3])],
        },
    ];
    let disables = EndpointDisables {
        inputs: vec![],
        outputs: vec![],
        path_a_outputs: vec![r("second", "out-2")],
        path_b_outputs: vec![r("main", "out-main")],
    };
    let chain = chain(&["main", "second"], vec![y_split()], disables);
    let (segs, outs) = segments(&chain, &registry);
    let pairing: Vec<(usize, Vec<usize>, SegmentPaths)> = segs
        .iter()
        .map(|s| (s.entry_group, outs[s.output_route_indices[0]].clone(), s.paths))
        .collect();
    assert_eq!(
        pairing,
        vec![(0, vec![0, 1], SegmentPaths::A), (1, vec![2, 3], SegmentPaths::B)],
        "#716: MAIN's input feeds only MAIN's output, SECOND's only SECOND's; #328: \
         each through the path its output checks"
    );
}
```

Mount it at the end of `crates/engine/src/runtime_segments.rs` (after line 425):

```rust

#[cfg(test)]
#[path = "issue_328_y_segments_tests.rs"]
mod issue_328_y_segments_tests;
```

- [ ] **Step 2: Run to see the compile red**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib issue_328_y_segments -j 2`
Expected: FAIL to compile — `error[E0432]: unresolved import crate::segment_types::SegmentPaths` and `no field paths on type &ChainSegment`.

- [ ] **Step 3: Compiling skeleton**

Append to `crates/engine/src/segment_types.rs`:

```rust

/// Which paths of the chain's Y → A/B split one segment runs (#328). A
/// segment writes one output, and that output's node checklist decides the
/// set; the builder shapes the split into exactly those paths
/// (`split_segment_view`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentPaths {
    /// The chain has no Y split, or this segment ends before it.
    None,
    /// Only path A feeds this segment's output.
    A,
    /// Only path B feeds this segment's output.
    B,
    /// Both paths feed it: summed at unity inside the segment.
    AB,
}
```

and add as the last field of `ChainSegment` (after `entry_group` at line 47):

```rust
    /// #328: the split paths this segment runs — `SegmentPaths::None` unless
    /// the chain ends in a Y → A/B split and this segment reaches it.
    pub(crate) paths: SegmentPaths,
```

In `crates/engine/src/runtime_segments.rs`, line 32 becomes:

```rust
pub(crate) use crate::segment_types::{ChainSegment, MidOutputTap, SegmentPaths, SegmentTap};
```

and add `paths: SegmentPaths::None,` as the last field of each of the four `ChainSegment { … }` literals (lines 278-286, 348-361, 366-380, 408-422).

- [ ] **Step 4: Run to see the assertion red**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib issue_328_y_segments -j 2`
Expected: FAIL — `each_output_runs_the_paths_that_check_it` panics ``assertion `left == right` failed: #328: out-a runs path A, out-b path B, out-ab ONE pipeline with both; …`` with `left: [([0, 1], None), ([2, 3], None), ([4, 5], None), ([6, 7], None)]`; `an_output_no_path_checks_has_no_pipeline` and `a_head_input_still_pairs_only_with_its_own_e_s` fail on the `None` paths the same way.

- [ ] **Step 5: Implementation**

Create `crates/engine/src/segment_paths.rs`:

```rust
//! Responsibility: maps each output route of a chain to the split paths it runs.

use domain::io_binding::IoBinding;
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::chain::Chain;
use project::endpoint_feeds::{tail_feed, TailFeed};

pub use crate::segment_types::SegmentPaths;

/// The split paths each resolved output route runs, in route order — the
/// order `resolve_chain_ports` numbers outputs with, which is the runtime's
/// route order (`classify_output_routes` walks the same list). A tail route
/// of a Y → A/B chain runs the paths whose output node has that endpoint
/// checked. A mid `Output` has no path node: before the split its pipeline
/// never reaches the split, and after a Y split (Part 1 lets ports follow
/// it) `None` makes the builder play both paths (`split_segment_view`).
/// Every route of a chain without a Y split is `None`.
pub fn route_paths(chain: &Chain, registry: &[IoBinding]) -> Vec<SegmentPaths> {
    let tail = chain.blocks.len();
    resolve_chain_ports(chain, registry)
        .into_iter()
        .filter(|port| port.direction == PortDirection::Output)
        .map(|port| {
            if port.offset < tail {
                return SegmentPaths::None;
            }
            match tail_feed(chain, &port.binding_id, &port.endpoint.name) {
                TailFeed::Paths { a: true, b: true } => SegmentPaths::AB,
                TailFeed::Paths { a: true, b: false } => SegmentPaths::A,
                TailFeed::Paths { a: false, b: true } => SegmentPaths::B,
                TailFeed::Paths { a: false, b: false } | TailFeed::Chain | TailFeed::Off => {
                    SegmentPaths::None
                }
            }
        })
        .collect()
}
```

`crates/engine/src/lib.rs`: after line 84 `mod segment_binding;` add:

```rust
pub mod segment_paths;
```

`crates/engine/src/runtime_segments.rs`, right after line 80 (`let (tail_routes, mid_taps, resolved_output_count) = classify_output_routes(chain, registry);`):

```rust
    // #328: the split paths each route runs (a Y → A/B chain's outputs).
    let route_paths = crate::segment_paths::route_paths(chain, registry);
```

In the `segments_without_inserts(` call at lines 83-94 add `&route_paths,` after `registry,`. In its signature (lines 183-194) add the last parameter:

```rust
    registry: &[IoBinding],
    route_paths: &[SegmentPaths],
) -> Vec<ChainSegment> {
```

and in its `ChainSegment` literal (lines 278-286) replace `paths: SegmentPaths::None,` with:

```rust
                // #328: the paths this output's node checks (none before the split).
                paths: route_paths
                    .get(out_entry_idx)
                    .copied()
                    .unwrap_or(SegmentPaths::None),
```

- [ ] **Step 6: Run to see green**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib -j 2 -- issue_328_y_segments issue_85 issue_928 runtime_graph`
Expected: PASS.

- [ ] **Step 7: Docs** — insert into `docs/audio-config.md` after the `### Endpoint checklist (issue #328)` section:

```markdown
### Y → A/B outputs (issue #328)

A Y → A/B chain keeps the stream model above: one segment per (input × output)
pair (`split_chain_into_segments`). The segment of output `O` runs the shared
blocks and then the paths whose output node has `O` checked — its
`ChainSegment.paths` (`segment_paths::route_paths`): `A`, `B` or `AB`. Both
paths on one output are **one** segment; they are summed inside it, never by
two segments on one route. An output no path checks is no port, so it has no
route and no segment. The #716 pairing is unchanged: a head input still pairs
only with its own E/S's outputs, so a path can only reach outputs of the E/S
whose input feeds it.
```

- [ ] **Step 8: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/engine/src/segment_types.rs crates/engine/src/segment_paths.rs crates/engine/src/lib.rs crates/engine/src/runtime_segments.rs crates/engine/src/issue_328_y_segments_tests.rs docs/audio-config.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "$(cat <<'EOF'
feat(#328): every segment carries the split paths its output runs
EOF
)"
```

- [ ] **Step 9: Push**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch && git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push
gh issue comment 328 -R jpfaria/OpenRig --body "Part 4 · Task 4 pushed $(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD): segment_types.rs, segment_paths.rs, runtime_segments.rs, lib.rs (+tests), docs. engine issue_328_y_segments/85/928/runtime_graph green."
```

---

### Task 5: A Y chain behind an insert — one return pipeline per path set

**Files:**
- Modify: `crates/engine/src/segment_paths.rs` (add `group_routes_by_paths` + test mount)
- Create: `crates/engine/src/segment_paths_tests.rs`
- Modify: `crates/engine/src/runtime_segments.rs` — `segments_with_inserts` signature (lines 296-308), its call (lines 111-123), the final push (lines 407-422)
- Modify: `crates/engine/src/issue_328_y_segments_tests.rs` (append)
- Modify: `docs/audio-config.md` (append to the Y section)

**Interfaces:**
- Consumes: Task 4 `SegmentPaths`, `route_paths`.
- Produces: `pub(crate) fn group_routes_by_paths(routes: &[usize], paths: &[SegmentPaths]) -> Vec<(SegmentPaths, Vec<usize>)>`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/engine/src/issue_328_y_segments_tests.rs`:

```rust
/// An insert in the shared blocks: the return feeds the tail. With a Y split
/// its outputs run different paths, so the return feeds one pipeline per path
/// set — and a mid Output after the insert rides only ONE of them, or its
/// route would be written twice (doubled level).
#[test]
fn behind_an_insert_the_return_feeds_one_pipeline_per_path_set() {
    let registry = vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![mono("in", "dev", 0)],
            outputs: vec![stereo("out-a", "dev", [0, 1]), stereo("out-b", "dev", [2, 3])],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![mono("ret", "dev", 4)],
            outputs: vec![mono("snd", "dev", 4)],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: vec![],
            outputs: vec![stereo("aux-out", "aux", [6, 7])],
        },
    ];
    let insert = AudioBlock {
        id: BlockId("loop".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(project::block::InsertBlock {
            model: "standard".into(),
            io: "fx".into(),
        }),
    };
    let disables = EndpointDisables {
        inputs: vec![],
        outputs: vec![],
        path_a_outputs: vec![r("main", "out-b")],
        path_b_outputs: vec![r("main", "out-a")],
    };
    let chain = chain(&["main"], vec![insert, mid_output(), y_split()], disables);
    let (segs, outs) = segments(&chain, &registry);

    let finals: Vec<(Vec<Vec<usize>>, SegmentPaths)> = segs
        .iter()
        .filter(|s| s.block_indices == vec![2])
        .map(|s| {
            (
                s.output_route_indices.iter().map(|&r| outs[r].clone()).collect(),
                s.paths,
            )
        })
        .collect();
    assert_eq!(
        finals,
        vec![
            (vec![vec![0, 1]], SegmentPaths::A),
            (vec![vec![2, 3]], SegmentPaths::B),
        ],
        "#328: the return feeds one pipeline per path set — out-a through A, out-b through B"
    );
    let tap_writers = segs
        .iter()
        .filter(|s| s.mid_output_taps.iter().any(|t| outs[t.route_idx] == vec![6, 7]))
        .count();
    assert_eq!(
        tap_writers, 1,
        "#328: the mid Output after the insert is written by ONE pipeline — two would double it on its route"
    );
}
```

Create `crates/engine/src/segment_paths_tests.rs`:

```rust
//! #328 — grouping a return's tail routes by the split paths they run.

use super::{group_routes_by_paths, SegmentPaths};

#[test]
fn routes_group_by_path_set_in_first_seen_order() {
    let paths = [SegmentPaths::B, SegmentPaths::A, SegmentPaths::B, SegmentPaths::AB];
    assert_eq!(
        group_routes_by_paths(&[0, 1, 2, 3], &paths),
        vec![
            (SegmentPaths::B, vec![0, 2]),
            (SegmentPaths::A, vec![1]),
            (SegmentPaths::AB, vec![3]),
        ]
    );
}

#[test]
fn a_split_free_chain_is_one_group_with_every_route() {
    let paths = [SegmentPaths::None, SegmentPaths::None];
    assert_eq!(
        group_routes_by_paths(&[0, 1], &paths),
        vec![(SegmentPaths::None, vec![0, 1])],
        "#328: a chain with no Y split keeps its one return pipeline, byte-identical"
    );
}

#[test]
fn no_routes_is_one_empty_group() {
    assert_eq!(
        group_routes_by_paths(&[], &[]),
        vec![(SegmentPaths::None, vec![])],
        "the return pipeline with no tail still exists, as before #328"
    );
}
```

- [ ] **Step 2: Run to see the red**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib issue_328_y_segments -j 2`
Expected: FAIL — `behind_an_insert_the_return_feeds_one_pipeline_per_path_set` panics ``assertion `left == right` failed: #328: the return feeds one pipeline per path set — out-a through A, out-b through B`` with `left: [([[0, 1], [2, 3]], None)]`. (`segment_paths_tests.rs` is not mounted yet.)

- [ ] **Step 3: Compiling skeleton** — append to `crates/engine/src/segment_paths.rs`:

```rust

pub(crate) fn group_routes_by_paths(
    routes: &[usize],
    _paths: &[SegmentPaths],
) -> Vec<(SegmentPaths, Vec<usize>)> {
    vec![(SegmentPaths::None, routes.to_vec())]
}

#[cfg(test)]
#[path = "segment_paths_tests.rs"]
mod tests;
```

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib segment_paths -j 2`
Expected: FAIL — `routes_group_by_path_set_in_first_seen_order` panics `left: [(None, [0, 1, 2, 3])]`; `no_routes_is_one_empty_group` passes; `a_split_free_chain_is_one_group_with_every_route` passes (both pin the unchanged shape).

- [ ] **Step 4: Implementation**

Replace the skeleton in `crates/engine/src/segment_paths.rs`:

```rust
/// Groups `routes` (indices into `paths`) by the path set they run, in
/// first-seen order, so one pipeline serves every route that runs the same
/// paths. A split-free chain is one group holding every route, and no routes
/// is one empty group — the exact shape the insert return had before #328.
pub(crate) fn group_routes_by_paths(
    routes: &[usize],
    paths: &[SegmentPaths],
) -> Vec<(SegmentPaths, Vec<usize>)> {
    let mut groups: Vec<(SegmentPaths, Vec<usize>)> = Vec::new();
    for &route in routes {
        let set = paths.get(route).copied().unwrap_or(SegmentPaths::None);
        match groups.iter_mut().find(|(group, _)| *group == set) {
            Some((_, members)) => members.push(route),
            None => groups.push((set, vec![route])),
        }
    }
    if groups.is_empty() {
        groups.push((SegmentPaths::None, Vec::new()));
    }
    groups
}
```

`crates/engine/src/runtime_segments.rs`: in the `segments_with_inserts(` call (lines 111-123) add `&route_paths,` as the last argument; in its signature (lines 296-308) add the last parameter `route_paths: &[SegmentPaths],`. Replace the final push (lines 407-424, from `let last_return_idx = …` to the closing `segments`) with:

```rust
    let last_return_idx = return_idx(insert_positions.len() - 1);
    // #328: a Y → A/B split runs different paths per output, so the return
    // feeds one pipeline per path set; a split-free chain is one group with
    // every tail route, exactly as before. Mid taps ride the first pipeline
    // only — two would write the tap's route twice.
    let mut taps = Some(taps);
    for (paths, routes) in crate::segment_paths::group_routes_by_paths(tail_routes, route_paths) {
        segments.push(ChainSegment {
            input: effective_ins[last_return_idx].clone(),
            cpal_input_index: cpal_indices
                .get(last_return_idx)
                .copied()
                .unwrap_or(last_return_idx),
            block_indices: block_indices.clone(),
            output_route_indices: routes,
            mid_output_taps: taps.take().unwrap_or_default(),
            split_mono_sibling_count: None,
            entry_group: entry_groups
                .get(last_return_idx)
                .copied()
                .unwrap_or(last_return_idx),
            paths,
        });
    }

    segments
```

- [ ] **Step 5: Run to see green**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib -j 2 -- segment_paths issue_328_y_segments issue_967 issue_979 issue_881 issue_928`
Expected: PASS — the insert suites (#881/#967/#979) are unchanged.

- [ ] **Step 6: Docs** — append to `### Y → A/B outputs (issue #328)` in `docs/audio-config.md`:

```markdown
With an insert cutting the shared blocks, the insert's return feeds one
pipeline per distinct path set (routes that run the same paths share it). A mid
`Output` after the insert rides only the first of them, so its route is still
written once.
```

- [ ] **Step 7: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/engine/src/segment_paths.rs crates/engine/src/segment_paths_tests.rs crates/engine/src/runtime_segments.rs crates/engine/src/issue_328_y_segments_tests.rs docs/audio-config.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "$(cat <<'EOF'
feat(#328): an insert's return feeds one pipeline per Y path set
EOF
)"
```

- [ ] **Step 8: Push**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch && git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push
gh issue comment 328 -R jpfaria/OpenRig --body "Part 4 · Task 5 pushed $(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD): segment_paths.rs (+tests), runtime_segments.rs, docs. engine segment_paths/issue_328/967/979/881/928 green."
```

---

### Task 6: The builder expands the Y node into the segment's paths

**Files:**
- Create: `crates/engine/src/split_segment_view.rs`
- Create: `crates/engine/src/split_segment_view_tests.rs`
- Create: `crates/engine/src/issue_328_y_audio_tests.rs`
- Modify: `crates/engine/src/lib.rs` (after `mod segment_types;` — line 87 once Task 4 added `pub mod segment_paths;` — add `mod split_segment_view;`)
- Modify: `crates/engine/src/runtime_graph_assemble.rs:88-98` (call), `:254-264` (signature), `:308-315` (build from the view)
- Modify: `crates/engine/src/runtime_graph_update.rs:204-214` (call)
- Modify: `crates/engine/src/offline.rs:80-87`, `:136-143`, `:160-167`
- Modify: `docs/audio-config.md` (append to the Y section)

**Interfaces:**
- Consumes: Task 4 `SegmentPaths`, `ChainSegment.paths`; Part 1 `split_params`; Part 3 Split → Mix runtime semantics (empty path = its split input; gains `x/100`; `pan_gains(0.0) == (1.0, 1.0)`; alignment delays only the shorter path).
- Produces:
  ```rust
  pub(crate) fn block_for_segment(block: &AudioBlock, paths: SegmentPaths) -> Cow<'_, AudioBlock>
  pub(crate) fn chain_for_segment(chain: &Chain, paths: SegmentPaths) -> Cow<'_, Chain>
  pub(crate) fn build_input_processing_state(chain, input, output_channels, sample_rate,
      existing_blocks, block_indices, output_route_indices, mid_output_taps,
      split_mono_sibling_count, paths: SegmentPaths) -> anyhow::Result<InputProcessingState>
  ```

- [ ] **Step 1: Write the failing view tests** — `crates/engine/src/split_segment_view_tests.rs`:

```rust
//! #328 — a Y → A/B split, as one segment builds it.

use std::borrow::Cow;

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::split_params::{
    default_split_params, LEVEL_TO_A, MIX_B_POLARITY, MIX_LEVEL_A, MIX_LEVEL_B, MIX_MASTER,
    MIX_MASTER_SUM, MIX_PAN_A, MIX_PAN_B,
};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, SplitBlock, SplitEnd};
use project::chain::Chain;
use project::endpoint_disables::EndpointDisables;
use project::param::ParameterSet;

use super::{block_for_segment, chain_for_segment};
use crate::segment_types::SegmentPaths;

fn effect(id: &str) -> AudioBlock {
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

fn split(end: SplitEnd, params: ParameterSet) -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end,
            params,
            a: vec![effect("amp-a")],
            b: vec![effect("amp-b")],
        }),
    }
}

/// A user's mixer settings — everything a Y → A/B output must ignore.
fn user_params() -> ParameterSet {
    let mut params = default_split_params();
    params.insert(LEVEL_TO_A, ParameterValue::Float(40.0));
    params.insert(MIX_LEVEL_A, ParameterValue::Float(10.0));
    params.insert(MIX_LEVEL_B, ParameterValue::Float(20.0));
    params.insert(MIX_PAN_A, ParameterValue::Float(-50.0));
    params.insert(MIX_PAN_B, ParameterValue::Float(50.0));
    params.insert(MIX_B_POLARITY, ParameterValue::String("invert".into()));
    params.insert(MIX_MASTER, ParameterValue::Float(5.0));
    params.insert(MIX_MASTER_SUM, ParameterValue::Bool(true));
    params
}

fn view(block: &AudioBlock, paths: SegmentPaths) -> SplitBlock {
    match block_for_segment(block, paths).into_owned().kind {
        AudioBlockKind::Split(split) => split,
        _ => panic!("a split must stay a split"),
    }
}

fn ids(blocks: &[AudioBlock]) -> Vec<&str> {
    blocks.iter().map(|b| b.id.0.as_str()).collect()
}

fn assert_neutral_knobs(shaped: &SplitBlock) {
    let defaults = default_split_params();
    for key in [MIX_PAN_A, MIX_PAN_B, MIX_B_POLARITY, MIX_MASTER_SUM] {
        assert_eq!(
            shaped.params.get(key),
            defaults.get(key),
            "#328: {key} takes its neutral default — Y has no mixer"
        );
    }
    assert_eq!(
        shaped.params.get_f32(MIX_MASTER),
        Some(100.0),
        "#328: master at unity (its default 50 would halve the output)"
    );
}

#[test]
fn a_y_output_fed_by_path_a_runs_path_a_alone_at_unity() {
    let shaped = view(&split(SplitEnd::Y, user_params()), SegmentPaths::A);
    assert!(
        matches!(shaped.end, SplitEnd::Mix),
        "#328: the segment runs the split through the Split → Mix code"
    );
    assert_eq!(ids(&shaped.a), vec!["amp-a"]);
    assert!(shaped.b.is_empty(), "#328: path B does not feed this output — it is not built");
    assert_eq!(shaped.params.get_f32(MIX_LEVEL_A), Some(100.0), "path A at unity");
    assert_eq!(
        shaped.params.get_f32(MIX_LEVEL_B),
        Some(0.0),
        "#328: the empty path B contributes nothing — not even the dry split input"
    );
    assert_neutral_knobs(&shaped);
    assert_eq!(
        shaped.params.get_f32(LEVEL_TO_A),
        Some(40.0),
        "the split's own knobs still apply"
    );
}

#[test]
fn a_y_output_fed_by_path_b_runs_path_b_alone_at_unity() {
    let shaped = view(&split(SplitEnd::Y, user_params()), SegmentPaths::B);
    assert!(matches!(shaped.end, SplitEnd::Mix));
    assert!(shaped.a.is_empty(), "#328: path A does not feed this output — it is not built");
    assert_eq!(ids(&shaped.b), vec!["amp-b"]);
    assert_eq!(shaped.params.get_f32(MIX_LEVEL_A), Some(0.0));
    assert_eq!(shaped.params.get_f32(MIX_LEVEL_B), Some(100.0));
    assert_neutral_knobs(&shaped);
}

#[test]
fn a_y_output_fed_by_both_paths_sums_them_at_unity() {
    let shaped = view(&split(SplitEnd::Y, user_params()), SegmentPaths::AB);
    assert_eq!((ids(&shaped.a), ids(&shaped.b)), (vec!["amp-a"], vec!["amp-b"]));
    assert_eq!(
        (shaped.params.get_f32(MIX_LEVEL_A), shaped.params.get_f32(MIX_LEVEL_B)),
        (Some(100.0), Some(100.0)),
        "#328: A + B at unity, aligned by the Split → Mix code"
    );
    assert_neutral_knobs(&shaped);
}

#[test]
fn a_render_with_no_routing_plays_both_paths() {
    let block = split(SplitEnd::Y, user_params());
    assert_eq!(
        view(&block, SegmentPaths::None),
        view(&block, SegmentPaths::AB),
        "#328: an offline render hears the chain as an output with both paths checked"
    );
}

#[test]
fn a_mix_split_and_every_other_block_are_themselves() {
    let mix = split(SplitEnd::Mix, user_params());
    assert!(
        matches!(block_for_segment(&mix, SegmentPaths::A), Cow::Borrowed(_)),
        "#328: a Split → Mix keeps its own mixer"
    );
    let amp = effect("amp");
    assert!(matches!(block_for_segment(&amp, SegmentPaths::AB), Cow::Borrowed(_)));
}

#[test]
fn a_chain_is_shaped_only_where_its_y_split_sits() {
    let chain = |blocks: Vec<AudioBlock>| Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
        loopers: vec![],
        disabled_endpoints: EndpointDisables::default(),
    };
    let linear = chain(vec![effect("pre")]);
    assert!(
        matches!(chain_for_segment(&linear, SegmentPaths::A), Cow::Borrowed(_)),
        "a chain with no Y split is itself — nothing is cloned"
    );
    let y = chain(vec![effect("pre"), split(SplitEnd::Y, user_params())]);
    let shaped = chain_for_segment(&y, SegmentPaths::B);
    assert!(
        matches!(shaped, Cow::Owned(_)),
        "#328: a chain with a Y split is shaped for each segment"
    );
    assert_eq!(shaped.blocks.len(), 2);
    assert_eq!(shaped.blocks[0], y.blocks[0], "the shared blocks are untouched");
    assert_eq!(
        shaped.blocks[1],
        block_for_segment(&y.blocks[1], SegmentPaths::B).into_owned(),
        "#328: the split is shaped for the segment"
    );
}
```

- [ ] **Step 2: Run to see the compile red**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib split_segment_view -j 2`
Expected: FAIL to compile — no `split_segment_view` module yet.

- [ ] **Step 3: Compiling skeleton** — `crates/engine/src/split_segment_view.rs`:

```rust
//! Responsibility: shapes a Y split into the split one segment runs.

use std::borrow::Cow;

use project::block::AudioBlock;
use project::chain::Chain;

use crate::segment_types::SegmentPaths;

pub(crate) fn chain_for_segment(chain: &Chain, _paths: SegmentPaths) -> Cow<'_, Chain> {
    Cow::Borrowed(chain)
}

pub(crate) fn block_for_segment(block: &AudioBlock, _paths: SegmentPaths) -> Cow<'_, AudioBlock> {
    Cow::Borrowed(block)
}

#[cfg(test)]
#[path = "split_segment_view_tests.rs"]
mod tests;
```

and in `crates/engine/src/lib.rs`, after `mod segment_types;`:

```rust
mod split_segment_view;
```

- [ ] **Step 4: Run to see the assertion red**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib split_segment_view -j 2`
Expected: FAIL — `a_y_output_fed_by_path_a_runs_path_a_alone_at_unity` panics `#328: the segment runs the split through the Split → Mix code`; `a_y_output_fed_by_path_b_runs_path_b_alone_at_unity` panics `assertion failed: matches!(shaped.end, SplitEnd::Mix)`; `a_y_output_fed_by_both_paths_sums_them_at_unity` panics `#328: A + B at unity, aligned by the Split → Mix code` with `left: (Some(10.0), Some(20.0))`; `a_chain_is_shaped_only_where_its_y_split_sits` panics `#328: a chain with a Y split is shaped for each segment`. Two pass on the skeleton: `a_mix_split_and_every_other_block_are_themselves` (it pins the untouched cases) and `a_render_with_no_routing_plays_both_paths` (the skeleton returns the same borrowed block for `None` and `AB`; after Step 5 it pins that both views are the both-paths mixer).

- [ ] **Step 5: Implement the view** — replace `crates/engine/src/split_segment_view.rs`:

```rust
//! Responsibility: shapes a Y split into the split one segment runs.
//!
//! #328 — a Y → A/B split has no mixer: every output runs the paths whose
//! output node checks it (`segment_paths`). Each segment therefore builds its
//! split as a Split → Mix (spec §4.2: "same code as 4.1, with neutral mixer
//! knobs") whose mixer passes the running paths at unity — centred, B not
//! inverted, master at unity, no sum — and whose other path is left empty at
//! level zero. One path: the output carries exactly that path, and because an
//! empty path has no latency the running path is never delayed. Both paths:
//! their unity sum, aligned by the Split → Mix code. The split's own knobs
//! (mode, level into each path, balance) still apply. Setup-time only.

use std::borrow::Cow;

use domain::value_objects::ParameterValue;
use project::block::split_params::{
    default_split_params, MIX_B_POLARITY, MIX_LEVEL_A, MIX_LEVEL_B, MIX_MASTER, MIX_MASTER_SUM,
    MIX_PAN_A, MIX_PAN_B,
};
use project::block::{AudioBlock, AudioBlockKind, SplitBlock, SplitEnd};
use project::chain::Chain;
use project::param::ParameterSet;

use crate::segment_types::SegmentPaths;

/// A 0–100 gain knob at unity: the split knobs are linear gain `x / 100`
/// (spec §1.2).
const UNITY_PCT: f32 = 100.0;

/// `chain` as one segment builds it: its Y split replaced by
/// [`block_for_segment`]. A chain without a Y split is itself.
pub(crate) fn chain_for_segment(chain: &Chain, paths: SegmentPaths) -> Cow<'_, Chain> {
    if !chain.blocks.iter().any(is_y_split) {
        return Cow::Borrowed(chain);
    }
    let mut shaped = chain.clone();
    shaped.blocks = chain
        .blocks
        .iter()
        .map(|block| block_for_segment(block, paths).into_owned())
        .collect();
    Cow::Owned(shaped)
}

/// The block one segment builds in place of `block`: a Y split becomes the
/// neutral Split → Mix of the paths the segment runs; every other block is
/// itself. `SegmentPaths::None` reaches a Y split only in a render with no
/// per-output routing (offline, tone doctor): it plays both paths, as an
/// output with both checked would.
pub(crate) fn block_for_segment(block: &AudioBlock, paths: SegmentPaths) -> Cow<'_, AudioBlock> {
    let AudioBlockKind::Split(split) = &block.kind else {
        return Cow::Borrowed(block);
    };
    if !matches!(split.end, SplitEnd::Y) {
        return Cow::Borrowed(block);
    }
    let (runs_a, runs_b) = match paths {
        SegmentPaths::A => (true, false),
        SegmentPaths::B => (false, true),
        SegmentPaths::AB | SegmentPaths::None => (true, true),
    };
    Cow::Owned(AudioBlock {
        id: block.id.clone(),
        enabled: block.enabled,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Mix,
            params: neutral_mixer(&split.params, runs_a, runs_b),
            a: if runs_a { split.a.clone() } else { Vec::new() },
            b: if runs_b { split.b.clone() } else { Vec::new() },
        }),
    })
}

fn is_y_split(block: &AudioBlock) -> bool {
    matches!(&block.kind, AudioBlockKind::Split(split) if matches!(split.end, SplitEnd::Y))
}

/// `params` with the mixer set to pass the running paths through at unity.
/// Pan, polarity and sum take their defaults (centre, normal, off — spec
/// §1.2), copied so their value type is whatever the split's schema declares.
fn neutral_mixer(params: &ParameterSet, runs_a: bool, runs_b: bool) -> ParameterSet {
    let defaults = default_split_params();
    let mut params = params.clone();
    for key in [MIX_PAN_A, MIX_PAN_B, MIX_B_POLARITY, MIX_MASTER_SUM] {
        if let Some(value) = defaults.get(key) {
            params.insert(key, value.clone());
        }
    }
    let level = |runs: bool| ParameterValue::Float(if runs { UNITY_PCT } else { 0.0 });
    params.insert(MIX_LEVEL_A, level(runs_a));
    params.insert(MIX_LEVEL_B, level(runs_b));
    params.insert(MIX_MASTER, ParameterValue::Float(UNITY_PCT));
    params
}

#[cfg(test)]
#[path = "split_segment_view_tests.rs"]
mod tests;
```

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib split_segment_view::tests -j 2`
Expected: PASS — `6 passed`. (`chain_for_segment` has no production caller until Step 8; the intermediate `dead_code` warning is gone before the commit.)

- [ ] **Step 6: Write the failing audio tests** — mount them at the end of `crates/engine/src/split_segment_view.rs`:

```rust

#[cfg(test)]
#[path = "issue_328_y_audio_tests.rs"]
mod issue_328_y_audio_tests;
```

and create `crates/engine/src/issue_328_y_audio_tests.rs`:

```rust
//! #328 — what a Y → A/B chain sounds like on each output. Input 0.5 on a mono
//! channel; path A is a volume at 50 % (×0.5), path B a volume at 25 % (×0.25).
//! Path A feeds out-a and out-ab, path B feeds out-b and out-ab, so
//! out-a = 0.25, out-b = 0.125 and out-ab = 0.25 + 0.125 = 0.375 — each output
//! hears exactly its paths, and both paths on one output sum at unity.

use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use project::block::split_params::{
    default_split_params, LEVEL_TO_A, MIX_B_POLARITY, MIX_LEVEL_A, MIX_LEVEL_B, MIX_MASTER,
    MIX_MASTER_SUM, MIX_PAN_A,
};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, SplitBlock, SplitEnd};
use project::chain::Chain;
use project::endpoint_disables::{EndpointDisables, EndpointRef};
use project::param::ParameterSet;

use crate::runtime::{
    build_chain_runtime_state, process_input_f32, process_output_f32, ChainRuntimeState,
    DEFAULT_ELASTIC_TARGET,
};
use crate::runtime_graph::update_chain_runtime_state;

const CHANNELS: usize = 6;
const FRAMES: usize = 64;
const INPUT: f32 = 0.5;
const TOLERANCE: f32 = 1e-3;

fn stereo_out(name: &str, channels: [usize; 2]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels: channels.to_vec(),
    }
}

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![
            stereo_out("out-a", [0, 1]),
            stereo_out("out-b", [2, 3]),
            stereo_out("out-ab", [4, 5]),
        ],
    }]
}

fn volume(id: &str, pct: f32) -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert("volume", ParameterValue::Float(pct));
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params,
        }),
    }
}

fn y_split(params: ParameterSet, enabled: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Y,
            params,
            a: vec![volume("amp-a", 50.0)],
            b: vec![volume("amp-b", 25.0)],
        }),
    }
}

fn endpoint(name: &str) -> EndpointRef {
    EndpointRef {
        io: "main".into(),
        endpoint: name.into(),
    }
}

/// Path A → out-a + out-ab; path B → out-b + out-ab.
fn y_chain(split: AudioBlock) -> Chain {
    Chain {
        id: ChainId("issue-328-y".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![split],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: EndpointDisables {
            inputs: vec![],
            outputs: vec![],
            path_a_outputs: vec![endpoint("out-b")],
            path_b_outputs: vec![endpoint("out-a")],
        },
    }
}

fn runtime(chain: &Chain) -> Arc<ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET; 3], &registry())
            .expect("the Y chain must build"),
    )
}

/// Left-channel peak of out-a, out-b and out-ab once the fades settled.
fn route_peaks(runtime: &Arc<ChainRuntimeState>) -> [f32; 3] {
    let mut input = vec![0.0_f32; FRAMES * CHANNELS];
    for frame in input.chunks_exact_mut(CHANNELS) {
        frame[0] = INPUT;
    }
    let mut out = vec![0.0_f32; FRAMES * CHANNELS];
    let mut peaks = [0.0_f32; 3];
    for callback in 0..256 {
        process_input_f32(runtime, 0, &input, CHANNELS);
        for (route, peak) in peaks.iter_mut().enumerate() {
            out.fill(0.0);
            process_output_f32(runtime, route, &mut out, CHANNELS);
            if callback >= 128 {
                for frame in out.chunks_exact(CHANNELS) {
                    *peak = peak.max(frame[route * 2].abs());
                }
            }
        }
    }
    peaks
}

fn assert_peaks(peaks: [f32; 3], expected: [f32; 3], why: &str) {
    for (route, (got, want)) in peaks.iter().zip(expected).enumerate() {
        assert!(
            (got - want).abs() < TOLERANCE,
            "{why} — route {route} (out-a, out-b, out-ab) got {got}, expected {want}; all: {peaks:?}"
        );
    }
}

#[test]
fn each_output_carries_only_the_paths_that_feed_it() {
    let runtime = runtime(&y_chain(y_split(default_split_params(), true)));
    assert_peaks(
        route_peaks(&runtime),
        [0.25, 0.125, 0.375],
        "#328: out-a hears path A only, out-b path B only, out-ab both at unity"
    );
}

#[test]
fn mixer_knobs_on_a_y_split_change_no_output() {
    let mut params = default_split_params();
    params.insert(MIX_LEVEL_A, ParameterValue::Float(0.0));
    params.insert(MIX_LEVEL_B, ParameterValue::Float(0.0));
    params.insert(MIX_PAN_A, ParameterValue::Float(50.0));
    params.insert(MIX_B_POLARITY, ParameterValue::String("invert".into()));
    params.insert(MIX_MASTER, ParameterValue::Float(10.0));
    params.insert(MIX_MASTER_SUM, ParameterValue::Bool(true));
    let runtime = runtime(&y_chain(y_split(params, true)));
    assert_peaks(
        route_peaks(&runtime),
        [0.25, 0.125, 0.375],
        "#328: Y has no mixer — mixer knobs set over MCP or a scene must not reach any output"
    );
}

#[test]
fn a_knob_edit_rebuilds_each_output_on_its_own_paths() {
    let chain = y_chain(y_split(default_split_params(), true));
    let runtime = runtime(&chain);
    let _ = route_peaks(&runtime);
    let mut params = default_split_params();
    params.insert(LEVEL_TO_A, ParameterValue::Float(50.0));
    let edited = y_chain(y_split(params, true));
    update_chain_runtime_state(
        &runtime,
        &edited,
        48_000.0,
        false,
        &[DEFAULT_ELASTIC_TARGET; 3],
        &registry(),
    )
    .expect("the in-place rebuild succeeds");
    assert_peaks(
        route_peaks(&runtime),
        [0.125, 0.125, 0.25],
        "#328: after a live edit each output still runs its own paths; level-to-A halves path A only"
    );
}

#[test]
fn a_bypassed_y_split_passes_the_shared_signal_once_to_every_output() {
    let runtime = runtime(&y_chain(y_split(default_split_params(), false)));
    assert_peaks(
        route_peaks(&runtime),
        [0.5, 0.5, 0.5],
        "#328: a bypassed split is a passthrough — out-ab must not get the signal twice"
    );
}

#[test]
fn an_offline_render_of_a_y_chain_plays_both_paths() {
    let chain = y_chain(y_split(default_split_params(), true));
    let input = vec![[INPUT, INPUT]; 4096];
    let outcome = crate::offline::render_chain(&chain, 48_000.0, &input, 64, 0)
        .expect("the offline render succeeds");
    let last = outcome.samples.last().copied().expect("rendered samples");
    assert!(
        (last[0] - 0.375).abs() < TOLERANCE,
        "#328: an offline render hears a Y chain as both paths summed at unity, got {last:?}"
    );
}
```

- [ ] **Step 7: Run to see the audio red**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib issue_328_y_audio -j 2`
Expected: FAIL — the builder still receives the un-viewed `end: Y` split. `each_output_carries_only_the_paths_that_feed_it` panics `#328: out-a hears path A only, out-b path B only, out-ab both at unity — route 0 (out-a, out-b, out-ab) got <x>, expected 0.25` (with Part 3 treating an un-viewed split as a mixer, every route plays the same mixed value, e.g. 0.1875 at master 50). `mixer_knobs_on_a_y_split_change_no_output`, `a_knob_edit_rebuilds_each_output_on_its_own_paths` and `an_offline_render_of_a_y_chain_plays_both_paths` fail the same way. `a_bypassed_y_split_passes_the_shared_signal_once_to_every_output` already passes — a disabled block is a bypass node before any split code runs; it pins that the view never doubles a bypassed split. Paste the exact FAILED lines into the task log.

- [ ] **Step 8: Wire the view into the builder**

`crates/engine/src/runtime_graph_assemble.rs`, signature at lines 254-264 gets a last parameter:

```rust
    split_mono_sibling_count: Option<usize>,
    paths: crate::segment_types::SegmentPaths,
) -> anyhow::Result<InputProcessingState> {
```

and lines 308-315 become:

```rust
    // #328: a Y → A/B split is built as the split THIS segment runs.
    let segment_chain = crate::split_segment_view::chain_for_segment(chain, paths);
    let (blocks, _output_layout) = build_runtime_block_nodes(
        &segment_chain,
        processing_layout_channel,
        source_is_mono,
        sample_rate,
        existing_blocks,
        block_indices,
    )?;
```

Its call in `assemble_chain_runtime_state` (lines 88-98) gets `segment.paths,` after `segment.split_mono_sibling_count,`. The call in `crates/engine/src/runtime_graph_update.rs:204-214` gets `segment.paths,` after `segment.split_mono_sibling_count,`.

`crates/engine/src/offline.rs`: add after line 23 `use crate::runtime_state::{BlockRuntimeNode, RuntimeProcessor};`:

```rust
use crate::segment_types::SegmentPaths;
use crate::split_segment_view::chain_for_segment;
```

and in the three `build_runtime_block_nodes(` calls (lines 80-87, 136-143, 160-167) replace the first argument `chain,` with:

```rust
        // #328: an offline render has no per-output routing — a Y split plays
        // both paths, as an output with both checked would.
        &chain_for_segment(chain, SegmentPaths::None),
```

- [ ] **Step 9: Run to see green**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib -j 2 -- split_segment_view issue_328 volume_invariants runtime_graph offline`
Expected: PASS — including `volume_invariants` (split-free chains take `Cow::Borrowed`, bit-identical).

- [ ] **Step 10: Docs** — append to `### Y → A/B outputs (issue #328)` in `docs/audio-config.md`:

```markdown
The builder then shapes the Y split, per segment, into the split that segment
runs (`split_segment_view`): a Split → Mix of only its paths, with a neutral
mixer — each running path at unity, centred, B not inverted, master at unity,
no sum — and the other path empty at level zero. One path: the output carries
exactly that path, never delayed (an empty path has no latency). Both paths:
their unity sum, time-aligned by the Split → Mix code. The split's own knobs
(mode, level into each path, balance) apply; its mixer knobs are ignored, since
Y has no mixer. A bypassed Y split passes the shared signal once to every
checked output. An offline render (no per-output routing) hears both paths.
```

- [ ] **Step 11: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/engine/src/split_segment_view.rs crates/engine/src/split_segment_view_tests.rs crates/engine/src/issue_328_y_audio_tests.rs crates/engine/src/lib.rs crates/engine/src/runtime_graph_assemble.rs crates/engine/src/runtime_graph_update.rs crates/engine/src/offline.rs docs/audio-config.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "$(cat <<'EOF'
feat(#328): each Y output runs exactly the paths that feed it
EOF
)"
```

- [ ] **Step 12: Push**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch && git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push
gh issue comment 328 -R jpfaria/OpenRig --body "Part 4 · Task 6 pushed $(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD): split_segment_view.rs (+tests), issue_328_y_audio_tests.rs, runtime_graph_assemble.rs, runtime_graph_update.rs, offline.rs, docs. engine split_segment_view/issue_328/volume_invariants/runtime_graph/offline green."
```

---

### Task 7: Path B's cab does not deepen the cushion of an output only path A feeds

**Files:**
- Modify: `crates/engine/src/route_convolution.rs:10-13`, `:15-27`, `:32-56`
- Modify: `crates/engine/src/route_convolution_tests.rs` (append)
- Modify: `docs/audio-config.md` (append to the Y section)

**Interfaces:**
- Consumes: Task 4 `ChainSegment.paths`; Task 6 `block_for_segment`; Part 3's `AudioBlockKind::Split` arm in `block_is_convolution` (added here if missing).
- Produces: `route_has_convolution` sees only the split paths each segment runs.

- [ ] **Step 1: Write the failing test** — append to `crates/engine/src/route_convolution_tests.rs`:

```rust
/// #328 — a Y → A/B output hears only the paths that feed it. Path B's cab
/// must not give the output path A alone feeds the convolution cushion (a
/// deeper cushion is latency that output never needed); path B's own output
/// keeps it.
#[test]
fn a_y_output_counts_only_the_paths_that_feed_it() {
    use domain::ids::{ChainId, DeviceId};
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use project::block::split_params::default_split_params;
    use project::block::{SplitBlock, SplitEnd};
    use project::chain::Chain;
    use project::endpoint_disables::{EndpointDisables, EndpointRef};

    use super::route_has_convolution;
    use crate::runtime_endpoints::{effective_inputs, effective_outputs, resolve_chain_io};
    use crate::runtime_segments::split_chain_into_segments;

    let out = |name: &str, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels,
    };
    let registry = vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![out("out-a", vec![0, 1]), out("out-b", vec![2, 3])],
    }];
    let off = |name: &str| EndpointRef {
        io: "main".into(),
        endpoint: name.into(),
    };
    let split = AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Y,
            params: default_split_params(),
            a: vec![core("gain", "volume")],
            b: vec![core(block_core::EFFECT_TYPE_CAB, "ir_marshall_4x12_v30")],
        }),
    };
    let chain = Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![split],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: EndpointDisables {
            inputs: vec![],
            outputs: vec![],
            path_a_outputs: vec![off("out-b")],
            path_b_outputs: vec![off("out-a")],
        },
    };
    let (ri, ro) = resolve_chain_io(&chain, &registry);
    let (ei, ci, sp, eg) = effective_inputs(&chain, &ri, &registry);
    let eo = effective_outputs(&chain, &ro, &registry);
    let segments = split_chain_into_segments(&chain, &ei, &ci, &sp, &eg, &eo, &registry);

    assert!(
        !route_has_convolution(&chain, &segments, 0),
        "#328: out-a hears path A only — path B's cab must not deepen its cushion"
    );
    assert!(
        route_has_convolution(&chain, &segments, 1),
        "#328: out-b hears path B's cab — it keeps the convolution cushion"
    );
}
```

- [ ] **Step 2: Run to see the red**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib route_convolution -j 2`
Expected: FAIL — with Part 3's split arm present: `#328: out-a hears path A only — path B's cab must not deepen its cushion`; without it: `#328: out-b hears path B's cab — it keeps the convolution cushion`. Either line is the red.

- [ ] **Step 3: Implementation** — `crates/engine/src/route_convolution.rs`. Imports (lines 10-13):

```rust
use project::block::{AudioBlock, AudioBlockKind};
use project::chain::Chain;

use crate::segment_types::{ChainSegment, SegmentPaths};
use crate::split_segment_view::block_for_segment;
```

`block_is_convolution` (lines 15-27) — keep Part 3's `Split` arm if it already exists; otherwise the function becomes:

```rust
/// Whether `block` is an enabled convolution (IR / cab) block, or a split
/// with one in either path.
pub(crate) fn block_is_convolution(block: &AudioBlock) -> bool {
    block.enabled
        && match &block.kind {
            AudioBlockKind::Core(core) => {
                core.effect_type == block_core::EFFECT_TYPE_CAB
                    || core.effect_type == block_core::EFFECT_TYPE_IR
                    || core.model.starts_with("ir_")
            }
            AudioBlockKind::Nam(nam) => nam.model.starts_with("ir_"),
            AudioBlockKind::Split(split) => {
                split.a.iter().chain(split.b.iter()).any(block_is_convolution)
            }
            _ => false,
        }
}
```

`route_has_convolution` (lines 32-56):

```rust
pub(crate) fn route_has_convolution(
    chain: &Chain,
    segments: &[ChainSegment],
    route_idx: usize,
) -> bool {
    // #328: a segment hears only the split paths it runs.
    let convolves = |indices: &[usize], paths: SegmentPaths| {
        indices
            .iter()
            .filter_map(|&idx| chain.blocks.get(idx))
            .any(|block| block_is_convolution(&block_for_segment(block, paths)))
    };
    segments.iter().any(|segment| {
        let at_tail = segment.output_route_indices.contains(&route_idx)
            && convolves(&segment.block_indices, segment.paths);
        let at_tap = segment
            .mid_output_taps
            .iter()
            .filter(|tap| tap.route_idx == route_idx)
            .any(|tap| {
                let before = tap.blocks_before.min(segment.block_indices.len());
                convolves(&segment.block_indices[..before], segment.paths)
            });
        at_tail || at_tap
    })
}
```

- [ ] **Step 4: Run to see green**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib -j 2 -- route_convolution issue_592 issue_965`
Expected: PASS.

- [ ] **Step 5: Docs** — append to `### Y → A/B outputs (issue #328)`:

```markdown
A route's convolution cushion (#592/#965) counts only the paths its segment
runs: path B's cab does not deepen the cushion — and so the latency — of an
output path A alone feeds.
```

- [ ] **Step 6: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/engine/src/route_convolution.rs crates/engine/src/route_convolution_tests.rs docs/audio-config.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "$(cat <<'EOF'
feat(#328): a Y output's cushion counts only the paths that feed it
EOF
)"
```

- [ ] **Step 7: Push**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch && git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push
gh issue comment 328 -R jpfaria/OpenRig --body "Part 4 · Task 7 pushed $(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD): route_convolution.rs (+tests), docs. engine route_convolution/592/965 green."
```

---

### Task 8: A path-set change is a structural change of the chain

**Files:**
- Modify: `crates/infra-cpal/src/io_topology.rs:114-122`
- Modify: `crates/infra-cpal/src/io_topology_tests.rs` (append)
- Modify: `docs/audio-config.md` (append to the Y section)

**Interfaces:**
- Consumes: Task 4 `engine::segment_paths::route_paths` (pub).
- Produces: `chain_structure_signature` ends with a `paths|[…]` row.

- [ ] **Step 1: Write the failing test** — append to `crates/infra-cpal/src/io_topology_tests.rs`:

```rust
// ── #328: which split paths feed each output is part of the structure ─────

/// A Y → A/B chain whose two outputs both stay open: checking path B on the
/// output path A already feeds opens no new device stream (the I/O signature
/// is unchanged), yet that output's pipeline now runs both paths. Only the
/// structure signature can see it; without the row `schedule_chain_activation`
/// takes the edit for a knob turn instead of giving the chain brand-new
/// streams (#881, spec §4.2).
#[test]
fn a_path_set_change_is_a_structural_change() {
    use domain::ids::{BlockId, ChainId};
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use project::block::split_params::default_split_params;
    use project::block::{AudioBlock, AudioBlockKind, SplitBlock, SplitEnd};
    use project::chain::Chain;
    use project::endpoint_disables::{EndpointDisables, EndpointRef};

    let ep = |name: &str, ch: usize| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    };
    let registry = vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![ep("in", 0)],
        outputs: vec![ep("out-a", 0), ep("out-b", 1)],
    }];
    let off = |name: &str| EndpointRef {
        io: "main".into(),
        endpoint: name.into(),
    };
    let chain = |path_b_outputs: Vec<EndpointRef>| Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![AudioBlock {
            id: BlockId("split".into()),
            enabled: true,
            kind: AudioBlockKind::Split(SplitBlock {
                end: SplitEnd::Y,
                params: default_split_params(),
                a: vec![],
                b: vec![],
            }),
        }],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: EndpointDisables {
            inputs: vec![],
            outputs: vec![],
            path_a_outputs: vec![off("out-b")],
            path_b_outputs,
        },
    };
    // Path A → out-a. Path B → out-b only, then → out-a too.
    let before = chain(vec![off("out-a")]);
    let after = chain(vec![]);

    assert_eq!(
        super::bound_io_signature(&before, &registry),
        super::bound_io_signature(&after, &registry),
        "fixture: both outputs stay open, so the I/O signature cannot tell"
    );
    assert_ne!(
        super::chain_structure_signature(&before, &registry),
        super::chain_structure_signature(&after, &registry),
        "#328: out-a now runs path A AND path B — its pipeline changed, so the chain needs new streams (#881)"
    );
}
```

- [ ] **Step 2: Run to see the red**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p infra-cpal --lib io_topology -j 2`
Expected: FAIL — ``assertion `left != right` failed: #328: out-a now runs path A AND path B — its pipeline changed, so the chain needs new streams (#881)``.

- [ ] **Step 3: Implementation** — `crates/infra-cpal/src/io_topology.rs`, insert before the final `.collect()` (after the `groups|` chain at lines 114-121):

```rust
        .chain(std::iter::once(format!(
            // #328: the split paths each output runs. Checking path B on an
            // output path A already feeds opens no new device stream, but
            // that output's pipeline changes — a structural edit, so the
            // chain gets new streams (#881).
            "paths|{:?}",
            engine::segment_paths::route_paths(chain, registry)
        )))
```

- [ ] **Step 4: Run to see green**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p infra-cpal --lib -j 2 -- io_topology stream_builder issue_979`
Expected: PASS — the existing signature tests only compare signatures of chains with no Y split, where the new row is constant.

- [ ] **Step 5: Docs** — append to `### Y → A/B outputs (issue #328)`:

```markdown
`chain_structure_signature` carries each output's path set, so checking or
unchecking a path on an output that stays open is a structural edit: the chain
gets brand-new streams (#881), never an in-place knob-style rebuild. On
Linux/JACK the structure signature is not consulted (#672); there the edit
is an in-place rebuild, which already runs the new path sets.
```

- [ ] **Step 6: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/infra-cpal/src/io_topology.rs crates/infra-cpal/src/io_topology_tests.rs docs/audio-config.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "$(cat <<'EOF'
feat(#328): a Y path-set change rebuilds the chain's streams
EOF
)"
```

- [ ] **Step 7: Push**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch && git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push
gh issue comment 328 -R jpfaria/OpenRig --body "Part 4 · Task 8 pushed $(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD): io_topology.rs (+tests), docs. infra-cpal io_topology/stream_builder/979 green."
```

---

### Task 9: A chain the checklist silences is off everywhere

**Files:**
- Modify: `crates/engine/src/runtime_graph.rs` — new `chain_plays` (after `chain_has_insert_cut`, line 98), gate at line 201, test mount at the end (after line 496)
- Modify: `crates/engine/src/input_conflicts.rs:70`, `:121`
- Modify: `crates/engine/src/rig_runtime.rs:73-74` (`RigRuntime::build`), `:120-124` (`enable_input`) — the rig side (#924: every tap detector agrees)
- Create: `crates/engine/src/issue_328_silenced_chain_tests.rs`
- Modify: `crates/engine/tests/issue_328_endpoint_disables.rs` (append; Part 1 Tasks 10–11 created it with the `rig`, `input`, `shared_binding` helpers)
- Modify: `crates/infra-cpal/src/chain_resolve.rs:90`, `:103`, `:394`
- Modify: `crates/infra-cpal/src/validation.rs:69`
- Modify: `crates/infra-cpal/src/stream_builder_project.rs:49`
- Modify: `crates/infra-cpal/src/controller_sync.rs:41`, `:102`, `:155`, `:172`
- Modify: `crates/infra-cpal/src/controller_upsert.rs:48`
- Modify: `crates/infra-cpal/src/controller_disable_kills_streams_tests.rs` (append)
- Create: `crates/infra-cpal/tests/issue_328_stream_gates_read_chain_plays.rs`
- Modify: `docs/audio-config.md` (append to the checklist section)

**Interfaces:**
- Consumes: Task 1 `checklist_silences`; Task 3 (a silenced chain builds zero segments).
- Produces: `pub fn chain_plays(chain: &Chain, registry: &[IoBinding]) -> bool` in `engine::runtime_graph`.

- [ ] **Step 1: Write the failing engine tests** — `crates/engine/src/issue_328_silenced_chain_tests.rs`:

```rust
//! #328 — a chain whose checklist leaves it no input or no output has nothing
//! to play (spec §5.3: allowed). It is off: no runtime, no stream, and it
//! claims no input tap another chain wants.

use std::collections::HashMap;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;
use project::endpoint_disables::{EndpointDisables, EndpointRef};
use project::project::Project;

use super::{build_runtime_graph, chain_plays};
use crate::input_conflicts::{conflicting_input_channel, input_conflicting_chains};

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn off(endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: "main".into(),
        endpoint: endpoint.into(),
    }
}

fn chain(id: &str, enabled: bool, disabled_endpoints: EndpointDisables) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints,
    }
}

fn no_outputs() -> EndpointDisables {
    EndpointDisables {
        outputs: vec![off("out")],
        ..EndpointDisables::default()
    }
}

fn no_inputs() -> EndpointDisables {
    EndpointDisables {
        inputs: vec![off("in")],
        ..EndpointDisables::default()
    }
}

#[test]
fn a_chain_plays_only_when_on_with_an_input_and_an_output() {
    let reg = registry();
    assert!(chain_plays(&chain("a", true, EndpointDisables::default()), &reg));
    assert!(!chain_plays(&chain("b", false, EndpointDisables::default()), &reg), "switched off");
    assert!(!chain_plays(&chain("c", true, no_outputs()), &reg), "#328: every output unchecked");
    assert!(!chain_plays(&chain("d", true, no_inputs()), &reg), "#328: every input unchecked");
}

#[test]
fn the_graph_leaves_out_a_chain_with_every_output_unchecked() {
    let silenced = chain("rig:input-1", true, no_outputs());
    let mut rates = HashMap::new();
    rates.insert(silenced.id.clone(), 48_000.0_f32);
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![silenced],
        midi: None,
    };
    let graph = build_runtime_graph(&project, &rates, &HashMap::new(), &registry())
        .expect("the graph builds");
    assert!(
        graph.chains.is_empty(),
        "#328: a chain with nothing to play gets no runtime — got {} runtime(s)",
        graph.chains.len()
    );
}

#[test]
fn a_silenced_chain_claims_no_input_tap() {
    let silenced = chain("rig:input-1", true, no_outputs());
    let player = chain("rig:input-2", true, EndpointDisables::default());
    let skipped = input_conflicting_chains([&silenced, &player], &registry());
    assert!(
        skipped.is_empty(),
        "#328: the chain with every output unchecked plays nothing — it must not keep input ch 0 from the chain that does, got {skipped:?}"
    );
    assert!(
        conflicting_input_channel(&player, [&silenced, &player], &registry()).is_none(),
        "#328: enabling the player must not be refused because of a silenced chain"
    );
}
```

Mount it at the end of `crates/engine/src/runtime_graph.rs`:

```rust

#[cfg(test)]
#[path = "issue_328_silenced_chain_tests.rs"]
mod issue_328_silenced_chain_tests;
```

The rig side (`RigRuntime`, used by `adapter-console-rig`) has its own tap detector, `rig_tap_conflict::tap_conflict`, which holds the taps of every input it brought up. If a silenced input is still brought up there, it keeps the taps the chain side now says are free — the #924 disagreement Part 1's Review Focus 5 pins for unchecked inputs. Append to `crates/engine/tests/issue_328_endpoint_disables.rs` (its `rig`, `shared_binding`, `rig_to_chains`, `EndpointDisables`, `EndpointRef` are already in scope there):

```rust
/// #328 — a rig input whose checklist leaves it no output plays nothing. The
/// rig side must treat it as off, like the chain side (`chain_plays`): no
/// runtime, no tap held, so the input sharing its E/S still comes up (#924:
/// every detector agrees).
#[test]
fn a_silenced_input_holds_no_tap_on_the_rig_side() {
    use engine::rig_runtime::RigRuntime;
    use engine::runtime_endpoints::input_conflicting_chains;

    // g1 has its only output unchecked; g2 plays. Both select the same E/S.
    let every_output_off = EndpointDisables {
        outputs: vec![EndpointRef {
            io: "shared".into(),
            endpoint: "out".into(),
        }],
        ..EndpointDisables::default()
    };
    let r = rig(every_output_off, EndpointDisables::default());
    let registry = vec![shared_binding()];

    assert_eq!(
        input_conflicting_chains(rig_to_chains(&r).iter(), &registry),
        Vec::<domain::ids::ChainId>::new(),
        "chain side: the silenced g1 claims no tap"
    );
    let rt = RigRuntime::build(r, 48_000.0, registry).expect("the rig builds");
    assert!(
        !rt.is_enabled("g1"),
        "#328: g1 has every output unchecked — nothing to play, so it is not brought up"
    );
    assert!(
        rt.is_enabled("g2"),
        "#328: rig side: g2 must not be refused a tap the silenced g1 does not play (#924: the detectors agree)"
    );
}
```

- [ ] **Step 2: Run to see the compile red, then add the skeleton**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib issue_328_silenced_chain -j 2`
Expected: FAIL to compile — `cannot find function chain_plays in super`.

Skeleton in `crates/engine/src/runtime_graph.rs`, right after `chain_has_insert_cut` (after line 98):

```rust

pub fn chain_plays(chain: &Chain, _registry: &[IoBinding]) -> bool {
    chain.enabled
}
```

- [ ] **Step 3: Run to see the assertion red**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib issue_328_silenced_chain -j 2`
Expected: FAIL — `a_chain_plays_only_when_on_with_an_input_and_an_output` panics `#328: every output unchecked`; `the_graph_leaves_out_a_chain_with_every_output_unchecked` panics `#328: a chain with nothing to play gets no runtime — got 1 runtime(s)` (a chain with no segment still gets the one empty group `group_segments_by_input` returns); `a_silenced_chain_claims_no_input_tap` panics `… got [ChainId("rig:input-2")]`.

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --test issue_328_endpoint_disables -j 2`
Expected: FAIL — `a_silenced_input_holds_no_tap_on_the_rig_side` panics ``assertion `left == right` failed: chain side: the silenced g1 claims no tap`` with `left: [ChainId("rig:g2")]`. Part 1's tests in that binary pass.

- [ ] **Step 4: Engine implementation**

Replace the skeleton in `crates/engine/src/runtime_graph.rs`:

```rust

/// #328: whether a chain gets a runtime and streams — switched on, and its
/// endpoint checklist leaves it an input and an output
/// (`endpoint_feeds::checklist_silences`). The graph, the input-tap claims
/// and every stream gate in infra-cpal read this ONE rule, so a chain whose
/// every output is unchecked is off, never an activation error.
pub fn chain_plays(chain: &Chain, registry: &[IoBinding]) -> bool {
    chain.enabled && !project::endpoint_feeds::checklist_silences(chain, registry)
}
```

`build_runtime_graph`, line 201 `if !chain.enabled {` becomes:

```rust
        if !chain_plays(chain, registry) {
```

`crates/engine/src/input_conflicts.rs:70` becomes:

```rust
        .filter(|other| {
            crate::runtime_graph::chain_plays(other, registry) && other.id != candidate.id
        })
```

and line 121 `if !chain.enabled {` becomes:

```rust
        if !crate::runtime_graph::chain_plays(chain, registry) {
```

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib -j 2 -- issue_328_silenced_chain input_conflict runtime_graph`
Expected: PASS.

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --test issue_328_endpoint_disables -j 2`
Expected: FAIL, now on the rig side — `a_silenced_input_holds_no_tap_on_the_rig_side` panics `#328: g1 has every output unchecked — nothing to play, so it is not brought up` (the chain-side assertion passes).

Rig side, `crates/engine/src/rig_runtime.rs`. In `RigRuntime::build`, line 73 (`if let Some(chain) = rig_to_chains(&project).into_iter().find(|c| c.id == id) {`) is followed by the `graph.upsert_chain(` call at line 74; insert between them:

```rust
                // #328: an input whose checklist leaves it nothing to play is
                // off — no runtime, and it holds no tap another input wants
                // (the chain side's rule, `chain_plays`; #924).
                if !crate::runtime_graph::chain_plays(&chain, &registry) {
                    continue;
                }
```

In `enable_input`, right after the `let chain = rig_to_chains(&self.project) … .ok_or_else(…)?;` statement (lines 120-123), insert:

```rust
        // #328: nothing to play → it stays off and holds no tap.
        if !crate::runtime_graph::chain_plays(&chain, &self.registry) {
            return Ok(());
        }
```

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --test issue_328_endpoint_disables --test issue_716_input_conflict -j 2`
Expected: PASS.

- [ ] **Step 5: Write the failing infra-cpal tests**

Append to `crates/infra-cpal/src/controller_disable_kills_streams_tests.rs`:

```rust
/// #328 — unchecking every output of a chain leaves it nothing to play. The
/// controller must take it down like a switch-off (#929), not fail the edit
/// trying to open devices for a chain with no output.
#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn unchecking_every_output_takes_the_chain_down_like_a_switch_off() {
    use domain::ids::DeviceId;
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use project::endpoint_disables::{EndpointDisables, EndpointRef};

    let chain_id = ChainId("rig:input-5".into());
    let (mut controller, _runtime) = controller_with_open_streams(&chain_id);
    controller.io_bindings = vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];
    let mut silenced = chain(&chain_id.0, true);
    silenced.io_binding_ids = vec!["main".into()];
    silenced.disabled_endpoints = EndpointDisables {
        inputs: vec![],
        outputs: vec![EndpointRef {
            io: "main".into(),
            endpoint: "out".into(),
        }],
        path_a_outputs: vec![],
        path_b_outputs: vec![],
    };

    let result = controller.upsert_chain(&project(), &silenced);

    assert!(
        result.is_ok(),
        "#328: a chain with every output unchecked is off, not an error — got {result:?}"
    );
    assert!(
        !controller.active_chains.contains_key(&chain_id),
        "#328: its streams die like a switch-off (#929)"
    );
    assert!(controller.streams.owned(&chain_id).is_none());
}
```

Create `crates/infra-cpal/tests/issue_328_stream_gates_read_chain_plays.rs`:

```rust
//! #328 — every place the stream layer decides whether a chain gets streams
//! must read ONE rule, `engine::runtime_graph::chain_plays`: switched on and
//! the endpoint checklist leaves it an input and an output. A site still
//! gating on `chain.enabled` alone tries to open a chain whose every output is
//! unchecked — and one such chain fails the sync of the whole project.

use std::path::Path;

const GATE_FILES: &[&str] = &[
    "src/chain_resolve.rs",
    "src/validation.rs",
    "src/stream_builder_project.rs",
    "src/controller_sync.rs",
    "src/controller_upsert.rs",
];

const BARE_GATES: &[&str] = &[
    "if !chain.enabled {",
    "if chain.enabled {",
    "|c| c.enabled",
    "c.enabled && c.id",
];

#[test]
fn every_stream_gate_reads_chain_plays() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for file in GATE_FILES {
        let source = std::fs::read_to_string(root.join(file))
            .unwrap_or_else(|e| panic!("read {file}: {e}"));
        for gate in BARE_GATES {
            assert!(
                !source.contains(gate),
                "#328: {file} still gates on `{gate}` — use `engine::runtime_graph::chain_plays(chain, registry)`"
            );
        }
        assert!(
            source.contains("chain_plays("),
            "#328: {file} must decide through `chain_plays`"
        );
    }
}
```

- [ ] **Step 6: Run to see the red**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p infra-cpal --lib unchecking_every_output -j 2 && nice -n 19 cargo test -p infra-cpal --test issue_328_stream_gates_read_chain_plays -j 2`
Expected: FAIL — `unchecking_every_output_takes_the_chain_down_like_a_switch_off` panics `#328: a chain with every output unchecked is off, not an error — got Err(…)` (the pre-fix upsert asks the audio host for device "dev" — a read-only device query that fails); `every_stream_gate_reads_chain_plays` panics `#328: src/chain_resolve.rs still gates on \`if !chain.enabled {\``.

- [ ] **Step 7: infra-cpal implementation** — one line each, `registry` is the local slice, `self.io_bindings` inside the controller:

`crates/infra-cpal/src/chain_resolve.rs:90` (Linux/JACK branch): `if chain.enabled {` →
```rust
            if engine::runtime_graph::chain_plays(chain, registry) {
```
`crates/infra-cpal/src/chain_resolve.rs:103` and `:394`: `if !chain.enabled {` →
```rust
            if !engine::runtime_graph::chain_plays(chain, registry) {
```
`crates/infra-cpal/src/validation.rs:69` and `crates/infra-cpal/src/stream_builder_project.rs:49`: `if !chain.enabled {` →
```rust
        if !engine::runtime_graph::chain_plays(chain, registry) {
```
`crates/infra-cpal/src/controller_sync.rs:41` (JACK): `let needs_audio = project.chains.iter().any(|c| c.enabled);` →
```rust
            let needs_audio = project
                .chains
                .iter()
                .any(|chain| engine::runtime_graph::chain_plays(chain, &self.io_bindings));
```
`crates/infra-cpal/src/controller_sync.rs:102` and `:172`: `if !chain.enabled {` →
```rust
                if !engine::runtime_graph::chain_plays(chain, &self.io_bindings) {
```
`crates/infra-cpal/src/controller_sync.rs:155` (JACK): →
```rust
            let still_exists = project.chains.iter().any(|chain| {
                engine::runtime_graph::chain_plays(chain, &self.io_bindings) && chain.id == chain_id
            });
```
`crates/infra-cpal/src/controller_upsert.rs:48`: `if !chain.enabled {` →
```rust
        // #328: a chain the checklist leaves with no input or no output is off.
        if !engine::runtime_graph::chain_plays(chain, &self.io_bindings) {
```

- [ ] **Step 8: Run to see green**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p infra-cpal --lib controller_disable_kills_streams -j 2 && nice -n 19 cargo test -p infra-cpal --test issue_328_stream_gates_read_chain_plays -j 2 && nice -n 19 cargo test -p engine --lib issue_328 -j 2`
Expected: PASS. The Linux/JACK lines (`controller_sync.rs:41/155/172`, `chain_resolve.rs:90`) only compile under `cfg(all(target_os = "linux", feature = "jack"))`; the source test pins them on every platform, and the CI Linux job compiles them.

- [ ] **Step 9: Docs** — append to `### Endpoint checklist (issue #328)`:

```markdown
The stream layer treats such a chain as switched off: the graph, the input-tap
claims (#716) and every activation gate read one rule,
`engine::runtime_graph::chain_plays`. Its streams die like a switch-off (#929),
it claims no input channel another chain wants, and it never fails the
activation of the other chains. The rig runtime (`RigRuntime::build` /
`enable_input`) reads the same rule, so its tap detector agrees (#924).
```

- [ ] **Step 10: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/engine/src/runtime_graph.rs crates/engine/src/input_conflicts.rs crates/engine/src/rig_runtime.rs crates/engine/src/issue_328_silenced_chain_tests.rs crates/engine/tests/issue_328_endpoint_disables.rs crates/infra-cpal/src/chain_resolve.rs crates/infra-cpal/src/validation.rs crates/infra-cpal/src/stream_builder_project.rs crates/infra-cpal/src/controller_sync.rs crates/infra-cpal/src/controller_upsert.rs crates/infra-cpal/src/controller_disable_kills_streams_tests.rs crates/infra-cpal/tests/issue_328_stream_gates_read_chain_plays.rs docs/audio-config.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "$(cat <<'EOF'
feat(#328): a chain the checklist silences is off, never an error
EOF
)"
```

- [ ] **Step 11: Push**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch && git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push
gh issue comment 328 -R jpfaria/OpenRig --body "Part 4 · Task 9 pushed $(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD): runtime_graph.rs chain_plays, input_conflicts.rs, rig_runtime.rs (rig side), infra-cpal gates (chain_resolve/validation/stream_builder_project/controller_sync/controller_upsert) + tests, docs. engine issue_328 + infra-cpal controller_disable_kills_streams + issue_328_stream_gates green."
```

---

### Task 10: Linux/JACK-direct plays every route of its runtime

**Files:**
- Modify: `crates/engine/src/runtime_output_process.rs` (append `process_output_f32_all_routes` + test mount after line 174)
- Modify: `crates/engine/src/runtime.rs:50`
- Create: `crates/engine/src/runtime_output_all_routes_tests.rs`
- Modify: `crates/engine/src/audio_alloc_invariant_tests.rs` (per-thread counter + one test)
- Modify: `crates/infra-cpal/src/jack_handlers.rs:28`, struct field after `:151`, output block `:249-260`
- Modify: `crates/infra-cpal/src/jack_direct.rs:218`
- Create: `crates/infra-cpal/tests/issue_328_jack_pops_every_route.rs`
- Modify: `docs/audio-config.md` (append to the Y section)

**Interfaces:**
- Consumes: `process_output_f32`, `output_limiter` (`runtime_dsp.rs:80`, already imported by `runtime_output_process.rs:17`), `ChainRuntimeState.output_routes` (`ArcSwap<Vec<Option<Arc<OutputRoutingState>>>>`, `runtime_chain_state.rs:44`), `OutputRoutingState.output_channels` (`runtime_state.rs:143`).
- Produces: `pub fn process_output_f32_all_routes(runtime: &Arc<ChainRuntimeState>, out: &mut [f32], output_total_channels: usize, scratch: &mut [f32])`, re-exported as `engine::runtime::process_output_f32_all_routes`.

- [ ] **Step 1: Write the failing tests** — `crates/engine/src/runtime_output_all_routes_tests.rs`:

```rust
//! #328 — Linux/JACK-direct runs ONE client per chain and popped route 0 only,
//! so a Y → A/B chain's path-B output (route 1) — like any second output of a
//! chain — was never played there. `process_output_f32_all_routes` plays every
//! route the runtime writes, each on its own channels.

use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;
use project::endpoint_disables::EndpointDisables;

use super::{process_output_f32, process_output_f32_all_routes};
use crate::runtime::{
    build_chain_runtime_state, process_input_f32, ChainRuntimeState, DEFAULT_ELASTIC_TARGET,
};

const CHANNELS: usize = 4;
const FRAMES: usize = 64;

fn stereo_out(name: &str, channels: [usize; 2]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels: channels.to_vec(),
    }
}

fn registry(outputs: Vec<IoEndpoint>) -> Vec<IoBinding> {
    vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs,
    }]
}

fn runtime(registry: &[IoBinding]) -> Arc<ChainRuntimeState> {
    let chain = Chain {
        id: ChainId("issue-328-jack".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: EndpointDisables::default(),
    };
    Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET; 2], registry)
            .expect("the chain builds"),
    )
}

fn input() -> Vec<f32> {
    let mut input = vec![0.0_f32; FRAMES * CHANNELS];
    for frame in input.chunks_exact_mut(CHANNELS) {
        frame[0] = 0.5;
    }
    input
}

#[test]
fn every_route_of_the_runtime_reaches_its_own_channels() {
    let runtime = runtime(&registry(vec![
        stereo_out("out-a", [0, 1]),
        stereo_out("out-b", [2, 3]),
    ]));
    let input = input();
    let mut out = vec![0.0_f32; FRAMES * CHANNELS];
    let mut scratch = vec![0.0_f32; FRAMES * CHANNELS];
    let mut peaks = [0.0_f32; CHANNELS];
    for callback in 0..256 {
        process_input_f32(&runtime, 0, &input, CHANNELS);
        out.fill(0.0);
        process_output_f32_all_routes(&runtime, &mut out, CHANNELS, &mut scratch);
        if callback >= 128 {
            for frame in out.chunks_exact(CHANNELS) {
                for (ch, peak) in peaks.iter_mut().enumerate() {
                    *peak = peak.max(frame[ch].abs());
                }
            }
        }
    }
    assert!(
        (peaks[0] - 0.5).abs() < 1e-3,
        "#328: route 0 plays on ch 0, got {peaks:?}"
    );
    assert!(
        (peaks[2] - 0.5).abs() < 1e-3,
        "#328: route 1 (a Y chain's path-B output) must play on ch 2 — JACK-direct popped route 0 only, got {peaks:?}"
    );
}

/// The single-output chain — every JACK chain before #328 — must come out of
/// the new call bit-for-bit as it came out of `process_output_f32(…, 0, …)`.
#[test]
fn a_single_route_is_bit_identical_to_the_route_0_pop() {
    let registry = registry(vec![stereo_out("out", [0, 1])]);
    let old = runtime(&registry);
    let new = runtime(&registry);
    let input = input();
    let mut old_out = vec![0.0_f32; FRAMES * CHANNELS];
    let mut new_out = vec![0.0_f32; FRAMES * CHANNELS];
    let mut scratch = vec![0.0_f32; FRAMES * CHANNELS];
    for _ in 0..256 {
        process_input_f32(&old, 0, &input, CHANNELS);
        process_input_f32(&new, 0, &input, CHANNELS);
        old_out.fill(0.0);
        new_out.fill(0.0);
        process_output_f32(&old, 0, &mut old_out, CHANNELS);
        process_output_f32_all_routes(&new, &mut new_out, CHANNELS, &mut scratch);
        let bits = |v: &[f32]| v.iter().map(|s| s.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&old_out), bits(&new_out), "#328: one route must be byte-identical");
    }
}

/// On cpal each route is its own stream and the device plays each route's
/// samples as the route wrote them. Routes on their own channels must reach
/// JACK the same way, bit for bit: a hot signal (0.99 in, above the 0.95
/// limiter knee) on out-b must not be squeezed a second time just because
/// out-a exists — otherwise a Y chain sounds different on Linux.
#[test]
fn routes_on_their_own_channels_keep_their_samples_bit_for_bit() {
    let registry = registry(vec![
        stereo_out("out-a", [0, 1]),
        stereo_out("out-b", [2, 3]),
    ]);
    let per_route = runtime(&registry);
    let all_routes = runtime(&registry);
    let mut hot = vec![0.0_f32; FRAMES * CHANNELS];
    for frame in hot.chunks_exact_mut(CHANNELS) {
        frame[0] = 0.99;
    }
    let mut expected = vec![0.0_f32; FRAMES * CHANNELS];
    let mut route_out = vec![0.0_f32; FRAMES * CHANNELS];
    let mut got = vec![0.0_f32; FRAMES * CHANNELS];
    let mut scratch = vec![0.0_f32; FRAMES * CHANNELS];
    for _ in 0..256 {
        process_input_f32(&per_route, 0, &hot, CHANNELS);
        process_input_f32(&all_routes, 0, &hot, CHANNELS);
        expected.fill(0.0);
        for route in 0..2 {
            route_out.fill(0.0);
            process_output_f32(&per_route, route, &mut route_out, CHANNELS);
            for (dst, src) in expected.iter_mut().zip(&route_out) {
                *dst += *src;
            }
        }
        got.fill(0.0);
        process_output_f32_all_routes(&all_routes, &mut got, CHANNELS, &mut scratch);
        let bits = |v: &[f32]| v.iter().map(|s| s.to_bits()).collect::<Vec<_>>();
        assert_eq!(
            bits(&got),
            bits(&expected),
            "#328: a route on its own channels is played as it was written — no second limiter pass"
        );
    }
}
```

Create `crates/infra-cpal/tests/issue_328_jack_pops_every_route.rs`:

```rust
//! #328 — the Linux/JACK-direct callback (`jack_handlers.rs`, compiled only
//! with `cfg(all(target_os = "linux", feature = "jack"))`) must play every
//! route of the chain's runtime. Popping route 0 alone left a Y → A/B chain's
//! path-B output silent on the Orange Pi. Pinned by source so every platform's
//! test run guards the JACK-only file.

use std::path::Path;

#[test]
fn the_jack_callback_plays_every_route() {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/jack_handlers.rs"),
    )
    .expect("read jack_handlers.rs");
    assert!(
        !source.contains("process_output_f32(&self.runtime, 0,"),
        "#328: the JACK callback still pops route 0 only"
    );
    assert!(
        source.contains("process_output_f32_all_routes("),
        "#328: the JACK callback must play every route through process_output_f32_all_routes"
    );
}
```

- [ ] **Step 2: Run to see the compile red, then add the skeleton**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p infra-cpal --test issue_328_jack_pops_every_route -j 2`
Expected: FAIL — `#328: the JACK callback still pops route 0 only` (the engine test file is mounted by the skeleton below; before it, it does not compile).

Skeleton appended to `crates/engine/src/runtime_output_process.rs` (it also mounts the engine test file):

```rust

pub fn process_output_f32_all_routes(
    runtime: &Arc<ChainRuntimeState>,
    out: &mut [f32],
    output_total_channels: usize,
    _scratch: &mut [f32],
) {
    process_output_f32(runtime, 0, out, output_total_channels);
}

#[cfg(test)]
#[path = "runtime_output_all_routes_tests.rs"]
mod runtime_output_all_routes_tests;
```

(The module name carries `runtime_output_all_routes`, so the `cargo test … runtime_output_all_routes` filter below matches its tests — a test path is `runtime_output_process::<module>::<fn>`, and the file name is not part of it.)

and `crates/engine/src/runtime.rs:50` becomes:

```rust
pub use crate::runtime_output_process::{
    process_output_f32, process_output_f32_all_routes, process_output_f32_mixed,
};
```

- [ ] **Step 3: Run to see the assertion red**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib runtime_output_all_routes -j 2 && nice -n 19 cargo test -p infra-cpal --test issue_328_jack_pops_every_route -j 2`
Expected: FAIL — `every_route_of_the_runtime_reaches_its_own_channels` panics `#328: route 1 (a Y chain's path-B output) must play on ch 2 — JACK-direct popped route 0 only, got [0.5, 0.5, 0.0, 0.0]`; `routes_on_their_own_channels_keep_their_samples_bit_for_bit` panics `#328: a route on its own channels is played as it was written — no second limiter pass` (route 1's channels are 0 in `got`); `the_jack_callback_plays_every_route` panics `#328: the JACK callback still pops route 0 only`. `a_single_route_is_bit_identical_to_the_route_0_pop` passes on the skeleton by construction — it pins the fast path the real implementation must keep.

- [ ] **Step 4: Engine implementation** — replace the skeleton function in `crates/engine/src/runtime_output_process.rs`:

```rust

/// Linux/JACK-direct (#328): ONE client carries every route of the chain's
/// runtime on one device, so its callback plays them all — each route is
/// rendered into `scratch` and added to `out`, what a cpal device does with
/// the one stream it opens per route. This is the backend mix of ONE
/// runtime's own routes, never of two runtimes (`process_output_f32_mixed`).
/// A runtime writing a single route takes `process_output_f32` straight into
/// `out`, byte-identical to the single-output chain it always was. A channel
/// only one route writes keeps that route's samples bit for bit; only a
/// channel two or more routes share takes the backend-mix limiter. `scratch`
/// is preallocated by the caller to at least `out.len()`: zero allocation,
/// zero locking.
pub fn process_output_f32_all_routes(
    runtime: &Arc<ChainRuntimeState>,
    out: &mut [f32],
    output_total_channels: usize,
    scratch: &mut [f32],
) {
    let routes = runtime.output_routes.load();
    let written = routes.iter().flatten().count();
    if written <= 1 {
        let only = routes.iter().position(Option::is_some).unwrap_or(0);
        process_output_f32(runtime, only, out, output_total_channels);
        return;
    }
    out.fill(0.0);
    let n = out.len();
    for route_idx in (0..routes.len()).filter(|&idx| routes[idx].is_some()) {
        let buf = &mut scratch[..n];
        process_output_f32(runtime, route_idx, buf, output_total_channels);
        for (dst, src) in out.iter_mut().zip(buf.iter()) {
            *dst += *src;
        }
    }
    // Only a channel two routes share can sum past 1.0 — the guard the
    // backend mix of several runtimes already uses, applied there alone.
    for ch in 0..output_total_channels {
        let writers = routes
            .iter()
            .flatten()
            .filter(|route| route.output_channels.contains(&ch))
            .count();
        if writers < 2 {
            continue;
        }
        for frame in out.chunks_mut(output_total_channels) {
            if let Some(sample) = frame.get_mut(ch) {
                *sample = output_limiter(*sample);
            }
        }
    }
}
```

- [ ] **Step 5: JACK handler** — `crates/infra-cpal/src/jack_handlers.rs`. Line 28:

```rust
use engine::runtime::{process_input_f32, process_output_f32_all_routes, ChainRuntimeState};
```

After the `pub(crate) output_buf: Vec<f32>,` field (line 151):

```rust
    /// #328: where one route renders before it is added to `output_buf` —
    /// preallocated like it, so the callback never allocates.
    pub(crate) route_scratch: Vec<f32>,
```

Output block (lines 249-260, from `let needed = …` to the closing `}));`) becomes:

```rust
            let needed = n_frames * total_out_ports;
            if self.output_buf.len() < needed {
                self.output_buf.resize(needed, 0.0);
            }
            if self.route_scratch.len() < needed {
                self.route_scratch.resize(needed, 0.0);
            }
            let buf = &mut self.output_buf[..needed];
            let scratch = &mut self.route_scratch[..needed];
            buf.fill(0.0);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // #328: this one client carries EVERY route of the chain's
                // runtime — a Y → A/B chain writes its path-B output on its
                // own route. Popping route 0 alone left that output silent.
                process_output_f32_all_routes(&self.runtime, buf, total_out_ports, scratch);
                for cell in &self.di_cells {
                    crate::di_playback::mix_di_playback(cell, buf, total_out_ports);
                }
            }));
```

`crates/infra-cpal/src/jack_direct.rs`, right after line 218 `output_buf: vec![0.0f32; MAX_JACK_FRAMES * output_ports.len().max(1)],`:

```rust
        route_scratch: vec![0.0f32; MAX_JACK_FRAMES * output_ports.len().max(1)],
```

- [ ] **Step 6: Zero-allocation pin** — `crates/engine/src/audio_alloc_invariant_tests.rs`. After the `ALLOC_GUARD` thread-local (line 76), add:

```rust

thread_local! {
    /// #328: the same count kept per thread — a measurement window no other
    /// test measuring in parallel can move, so the test below needs no
    /// `#[ignore]`.
    static THREAD_ALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
}
```

In `alloc`, `alloc_zeroed` and `realloc`, inside each `if ALLOC_GUARD.with(|g| g.get()) {` block, after `ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);` add:

```rust
            THREAD_ALLOC_COUNT.with(|c| c.set(c.get() + 1));
```

After `measure_allocs` (line 114) add:

```rust

pub(super) fn measure_allocs_on_this_thread<F: FnOnce()>(f: F) -> usize {
    THREAD_ALLOC_COUNT.with(|c| c.set(0));
    ALLOC_GUARD.with(|g| g.set(true));
    f();
    ALLOC_GUARD.with(|g| g.set(false));
    THREAD_ALLOC_COUNT.with(Cell::get)
}
```

Append the test:

```rust
/// #328 — Linux/JACK-direct plays every route of a chain's runtime through
/// `process_output_f32_all_routes` (a Y → A/B chain's outputs are separate
/// routes). The route mix renders into the scratch the JACK handler
/// preallocates: steady-state callbacks allocate nothing.
#[test]
fn every_route_output_does_not_allocate() {
    let out = |name: &str, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels,
    };
    let registry = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![out("out-a", vec![0, 1]), out("out-b", vec![2, 3])],
    }];
    let runtime = Arc::new(
        build_chain_runtime_state(&chain(), 48_000.0_f32, &[DEFAULT_ELASTIC_TARGET; 2], &registry)
            .expect("runtime should build"),
    );
    let frames = 32_usize;
    let channels = 4_usize;
    let input_buf = vec![0.5_f32; frames * channels];
    let mut output_buf = vec![0.0_f32; frames * channels];
    let mut scratch = vec![0.0_f32; frames * channels];
    for _ in 0..256 {
        process_input_f32(&runtime, 0, &input_buf, channels);
        crate::runtime::process_output_f32_all_routes(&runtime, &mut output_buf, channels, &mut scratch);
    }
    let allocs = measure_allocs_on_this_thread(|| {
        for _ in 0..1_000 {
            process_input_f32(&runtime, 0, &input_buf, channels);
            crate::runtime::process_output_f32_all_routes(
                &runtime,
                &mut output_buf,
                channels,
                &mut scratch,
            );
        }
    });
    assert_eq!(
        allocs, 0,
        "CLAUDE.md invariant #8 broken by the JACK route mix: {allocs} heap allocations in 1000 steady-state callbacks"
    );
}
```

(`chain()` in this file builds a `Chain` literal; Part 1 already gave it `disabled_endpoints`.)

- [ ] **Step 7: Run to see green**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p engine --lib -j 2 -- runtime_output_all_routes every_route_output_does_not_allocate volume_invariants && nice -n 19 cargo test -p infra-cpal --test issue_328_jack_pops_every_route -j 2`
Expected: PASS. The edits in `jack_handlers.rs` / `jack_direct.rs` compile only under `cfg(all(target_os = "linux", feature = "jack"))`: the source test pins them here, the CI Linux job builds them.

- [ ] **Step 8: Docs** — append to `### Y → A/B outputs (issue #328)`:

```markdown
On Linux with JACK-direct, one JACK client carries a chain's runtime on one
device. Its callback plays **every** route the runtime writes
(`process_output_f32_all_routes`: each route rendered into a preallocated
scratch and added; the backend-mix limiter touches only a channel two or more
routes share, so a route on its own channels sounds exactly as on cpal); it
used to pop route 0 only, which left path B's output — and any chain's
second output or insert send — silent. A runtime with a single route is
byte-identical to before.
```

- [ ] **Step 9: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/engine/src/runtime_output_process.rs crates/engine/src/runtime.rs crates/engine/src/runtime_output_all_routes_tests.rs crates/engine/src/audio_alloc_invariant_tests.rs crates/infra-cpal/src/jack_handlers.rs crates/infra-cpal/src/jack_direct.rs crates/infra-cpal/tests/issue_328_jack_pops_every_route.rs docs/audio-config.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "$(cat <<'EOF'
feat(#328): Linux/JACK-direct plays every route of the chain's runtime
EOF
)"
```

- [ ] **Step 10: Push**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch && git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push
gh issue comment 328 -R jpfaria/OpenRig --body "Part 4 · Task 10 pushed $(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD): runtime_output_process.rs process_output_f32_all_routes (+tests, alloc pin), runtime.rs, jack_handlers.rs, jack_direct.rs, source pin, docs. engine runtime_output_all_routes/every_route_output_does_not_allocate/volume_invariants + infra-cpal issue_328_jack_pops_every_route green."
```

---

### Task 11: Part 4 whole-workspace gate

**Files:** none new — verification of everything above.

**Interfaces:**
- Consumes: Tasks 1–10.
- Produces: a green, warning-free workspace pushed to `feature/issue-328`.

- [ ] **Step 1: Format**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && cargo fmt --all -- --check`
Expected: no output, exit 0. On a diff: `cargo fmt --all`, re-run, and commit the formatted files with the Step 5 commit.

- [ ] **Step 2: Whole-workspace tests**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test --workspace -j 2 2>&1 | grep -E "^test result|FAILED|panicked"`
Expected: every line `test result: ok`, with `N ignored` counts no higher than before this part (only the pre-existing ignored alloc tests); no `FAILED`, no `panicked`. `crates/engine/src/volume_invariants_tests.rs` is unchanged and green.

- [ ] **Step 3: Build, zero warnings**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^warning|^error" ; echo "exit=$?"`
Expected: no `warning`/`error` lines (grep exit 1).

- [ ] **Step 4: Static checks (whole repo)**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates`
Expected: exit 0; `endpoint_feeds.rs`, `segment_paths.rs`, `split_segment_view.rs` listed `✓ OK` under "Single Responsibility"; every touched file under its cap; no inline test module.

- [ ] **Step 5: Commit (only if Steps 1–4 changed files) and push**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status --short
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add <each path cargo fmt touched, listed by the status above>
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "$(cat <<'EOF'
feat(#328): format the Y routing changes
EOF
)"
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch && git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase && git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push
```

If `pull --rebase` brought commits, repeat Steps 2–4 before the push.

- [ ] **Step 6: Hardware battery (the agent runs it, idle machine)**

Run (the battery as `docs/testing.md` → "Real-hardware battery" lists it): `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && OPENRIG_HW_TESTS=1 nice -n 19 cargo test -p infra-cpal --release -j 2 --test issue_670_cab_swap --test issue_670_real_streams_no_xruns --test issue_698_pitch_shifter_live --test issue_698_owner_64_dual_chain -- --test-threads=1 2>&1 | grep -E "^test result|xrun|underrun|FAILED"`
Expected: every binary `test result: ok`, zero xruns, zero underruns — this part must not regress the split-free rigs the battery plays (spec §6). Record the counts in the issue comment.

- [ ] **Step 7: Issue comment with the validation checklist** (CLAUDE.md LEI ZERO — needs the owner's ear on real gear). The second command is the exact `run:` line `scripts/solver-setup.sh 328 feature/issue-328` prints — copy it from that output, never from memory. The same checklist goes in the chat reply.

~~~bash
gh issue comment 328 -R jpfaria/OpenRig --body "$(cat <<'EOF'
Part 4 (Y → A/B routing) pushed. fmt/test --workspace/build (0 warnings)/validate.sh crates green; hardware battery: <xruns>/<underruns>.

```
git fetch && git checkout feature/issue-328 && git pull
```
```
<the exact run: line printed by scripts/solver-setup.sh 328 feature/issue-328>
```
1. [ ] Y chain, path A → Main out only, path B → second out only: each out plays only its amp.
2. [ ] Check path B on Main too: Main plays both amps summed, no comb/flanging.
3. [ ] Uncheck every output of a chain: it goes silent, the other chains keep playing, no error.
4. [ ] Orange Pi (JACK): path B's output is audible.
EOF
)"
~~~

---

## Self-review notes

- Spec coverage: §1.3 runtime filtering → Part 1 Task 11 (verified by Task 2, re-implemented there only as a fallback), Task 3; §4.2 `SegmentPaths` + segmentation → Tasks 4, 5; "both paths on one output summed inside the segment at unity with alignment (same code as 4.1, neutral mixer)" → Task 6; "If no path feeds O, no segment" → the port filter + Task 4 (`an_output_no_path_checks_has_no_pipeline`); `chain_structure_signature` → Task 8; Linux/JACK route 0 → Task 10; §5.3 "Unchecking every input or every output … allowed" → Tasks 3, 9 (graph, tap claims, every infra-cpal gate and the rig runtime); §6 isolation (no route/runtime sums), latency (Task 7, empty path never delays), volume (`volume_invariants` in Tasks 6, 10, 11); §7 engine structure + alloc → Tasks 4, 6, 10; §8 docs → every task appends its paragraph to `docs/audio-config.md`.
- `blocks_between` is intentionally unchanged (the split is one block index; the builder expands it).
- Every new type/function name used in a later task is defined in an earlier one: `TailFeed`/`tail_feed`/`head_input_enabled`/`inputs_all_unchecked`/`outputs_all_unchecked`/`checklist_silences` (T1), `SegmentPaths`/`route_paths` (T4), `group_routes_by_paths` (T5), `block_for_segment`/`chain_for_segment` (T6), `chain_plays` (T9), `process_output_f32_all_routes` (T10).
