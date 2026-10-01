# Unlimited Split Graph Implementation Plan (#328, spec §11)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. The owner chose **native** execution (no subagents).

**Goal:** A split opens N paths (N ≥ 2, no upper bound). Splits nest in any path at any depth. Every Y leaf gets its own outputs, and the graph view lays the tree out automatically.

**Architecture:**
- The model widens `SplitBlock{a, b}` to `paths: Vec<Vec<AudioBlock>>` and `PathRef{split, side}` to `PathRef{split, path: usize}`.
- Knobs come from a schema generated per path count (`_<i>` suffix).
- The engine generalizes the 2-way mix and alignment to N, and replaces `SegmentPaths{A,B,AB}` with a set of Y leaves found in the tree.
- The GUI turns `ChainStage::Parallel` into a recursive tree with N lanes.

**Tech Stack:** Rust workspace (crates `project`, `infra-yaml`, `application`, `engine`, `infra-cpal`, `adapter-gui`), Slint, serde_yaml, schemars.

**Spec:** `docs/superpowers/specs/2026-09-28-issue-328-chain-split-graph-design.md` §11 (§10 and earlier sections hold the 2-path design this generalizes).

## Global Constraints

- **Work location:** work only in `.solvers/issue-328` on branch `feature/issue-328`. Never touch the main folder.
- **Commit and push:** every commit is pushed immediately. Stage explicit paths only.
- **Compile once (owner's call, 2026-10-01):** write every task's code in one pass, without cargo. In each task the test is written before the production code. After the last task, run one `cargo check --workspace --tests`, fix what it reports, then run the targeted tests. The per-task "Run, see FAILED" and "PASS" steps below are deferred to that single compile.
- **No local full builds or tests:**
  - Run only the targeted test: `nice -n 19 cargo test -p <crate> -j 2 <filter>`, one cargo at a time.
  - Before each push: `cargo fmt --all -- --check` and `VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates`.
  - CI runs the workspace.
- **File headers:** every touched production file carries `//! Responsibility: <one sentence, no "and", no comma>`.
- **File size:** `.rs` ≤ 600 lines and `.slint` ≤ 500 lines. A new responsibility goes in a new file.
- **Audio thread:** zero allocation, lock, syscall or I/O.
- **Stream isolation (LAW ZERO):** each chain input runs its own copy of the graph, and nothing is mixed across streams in our code.
- **Paths:** a path holds no `Input`, `Output` or `Insert`. `Select` and `Split` are allowed.
- **Y rule:** a Y ends the list it sits in. At top level only the chain's `Input`/`Output` may follow it.
- **No cycles:** a split never contains itself (structurally impossible with an owned tree; no edge drawing exists).
- **Timing:** every sum is time-aligned, and the longest path is never delayed.
- **Volume:** volume invariants (`crates/engine/src/volume_invariants_tests.rs`) never change.
- **Language:** repo content is in English, and the GUI shows paths as letters A, B, C, ….

## Review Focus

- **Removing a middle path renumbers knobs, MIDI and scenes.** Removing path 1 of 4 must move `mix_level_2`→`mix_level_1` and `mix_level_3`→`mix_level_2`. A MIDI CC mapped to `mix_level_3` must still drive the same (now `_2`) path. Pinned in Task 5.
- **Legacy files with `a`/`b` and `*_a`/`*_b` keys load unchanged.** A project saved by this branch before §11 must load with identical sound. The same holds for `disabled_endpoints.path_a_outputs`. Pinned in Tasks 3 and 4.
- **A Y nested in a Mix path, plus a top-level output.**
  - Each leaf reaches only the outputs it checked.
  - The Mix's other paths still sum into the top-level output.
  - Pinned in Task 10 (`y_inside_mix_path_routes_leaf_and_mix`).
- **A toggle deep in a nested path re-aligns every ancestor.** Bypassing a high-latency block 2 levels down must shrink the outer Mix's alignment too, so there is no comb filtering. Pinned in Task 9.
- **The audio thread does not allocate with many paths.** Pinned in Task 8 with an allocation-counting test over a 5-path Mix after warm-up.

---

## File Structure

### Part 1 — model, persistence, commands

`crates/project/src/block/`

| File | Change |
|---|---|
| `split_block.rs` | `paths` field, `SplitBlock::new`, `SplitBlock::with_paths` |
| `path_ref.rs` | `PathRef{split, path: usize}` and `path_letter(i)`; `PathSide` deleted |
| `block_walk.rs`, `split_lookup.rs` | iterate `paths` |
| `split_block_methods.rs` | fewer-than-two refusal; Select allowed |
| `split_params.rs` | generated schema; every fn takes `path_count` |
| `split_param_keys.rs` (new) | key builders and the `*_a`/`*_b` legacy rename |
| `split_param_renumber.rs` (new) | drop path `i` and renumber the keys above it |
| `y_leaves.rs` (new) | the Y leaves of a block list, at any depth |

`crates/project/src/`

| File | Change |
|---|---|
| `endpoint_disables.rs` | `path_outputs` and `EndpointNode::PathOutput` |
| `endpoint_feeds.rs` | `TailFeed::Leaves` |
| `rig_write_back.rs`, `rig_model_swap.rs`, `block/audio_block_methods.rs` | iterate `paths` |

`crates/infra-yaml/src/`

| File | Change |
|---|---|
| `block_yaml_split.rs` | `paths` plus the legacy `a`/`b` load |
| `endpoint_disables_yaml` | wherever `path_a_outputs` is (de)serialized: the new shape plus legacy keys |

`crates/application/src/`

| File | Change |
|---|---|
| `command.rs` | `AddSplitPath`, `RemoveSplitPath`, `SetChainEndpointEnabled` node |
| `split_path_commands.rs` (new) | the handlers of the two new commands |
| `block_path.rs`, `validate_split.rs`, `query_ids.rs` | `PathRef.path` |
| MCP tool registry | two tools; the variant count test grows by 2 |

### Part 2 — engine

`crates/engine/src/`

| File | Change |
|---|---|
| `runtime_split_align.rs` | `plan_alignment(&[usize]) -> AlignPlan{delays, clamped}` |
| `runtime_split_mix.rs` | per-path `PathKnobs`, `path_input`, `accumulate_path` |
| `runtime_split_knobs.rs` | `Vec<PathKnobAtomics>` |
| `runtime_split_state.rs` | `paths`, `bufs`, `aligns`, `values` (all preallocated) |
| `runtime_split_process.rs` | N-way chunk processing |
| `runtime_split_builder.rs`, `runtime_split_latency.rs`, `runtime_split_walk.rs` | N paths |
| `runtime_block_reuse.rs`, `runtime_block_toggle.rs` | recurse the whole tree; re-align every ancestor |
| `segment_types.rs` | `SegmentPaths` becomes `LeafSet(Vec<PathRef>)` |
| `segment_paths.rs` | per-output leaf sets |
| `split_segment_view.rs` | recursive prune to a leaf set |
| `runtime_segments.rs`, `runtime_graph_assemble.rs`, `route_convolution.rs`, `offline.rs` | leaf sets |

`crates/infra-cpal/src/controller_offthread_live_rebuild.rs`: the signature carries leaf sets.

### Part 3 — GUI

`crates/adapter-gui/src/graph_view_model/`

| File | Change |
|---|---|
| `types.rs` | `Parallel{split_id, lanes: Vec<Vec<ChainStage>>, end}` |
| `chain_builder.rs` | recursive layout |
| `anchors.rs` | `AnchorSlot{path: Option<PathRef>, index}` |
| `routing_ids.rs` | ids keyed by the split's block id |
| `validation.rs` | recursive validation |

`crates/adapter-gui/src/`

| File | Change |
|---|---|
| `chain_graph_adapter.rs` | tree stages; size from the tree |
| `chain_graph_ids.rs` | resolve at depth; `__out_<id>_<i>` |
| `graph_anchor.rs` | slot to `InsertTarget` at depth |
| `split_picker_entries.rs` | everything at every "+"; Y rule only |
| `split_path_gestures.rs` (new) | "+ path" and remove-path events, as pure functions |
| `split_editor_items.rs` | generated groups per path |
| `endpoint_checklist_items.rs`, `endpoint_checklist_wiring.rs` | one checklist per leaf |
| `compact_row_address.rs`, `chain_block_lists.rs`, `chain_block_helpers.rs` | depth plus index paths |
| `ui/**` | Slint: "+ path" button on the split node, remove-path on each lane, row size from the model |

### Part 4 — docs

- `docs/blocks-catalog.md`, `docs/audio-config.md`, `docs/screens.md`, `docs/gui/graph-view.md`, `docs/mcp.md`
- `README.md`, `README.pt-BR.md`, `README.es-ES.md`
- the 9 translation files

---

## Part 1 — Model, persistence, commands

### Task 1: `SplitBlock.paths` and `PathRef.path`

**Files:**
- Modify:
  - `crates/project/src/block/split_block.rs`
  - `crates/project/src/block/path_ref.rs`
  - `crates/project/src/block/block_walk.rs`
  - `crates/project/src/block/split_lookup.rs`
  - `crates/project/src/block/split_block_methods.rs`
  - `crates/project/src/block/mod.rs`
- Modify (mechanical, compile only, 2-path behavior unchanged):
  - every file that matched `split.a`, `split.b` or `PathSide` (the `grep` list in the recon)
  - engine code that builds a/b keeps using `paths[0]`/`paths[1]` until Task 8 replaces it
- Test: `crates/project/tests/issue_328_split_rules.rs` (fixtures move to `SplitBlock::with_paths`)

**Interfaces:**
- Produces:
  - `SplitBlock{end, params, paths: Vec<Vec<AudioBlock>>}`
  - `SplitBlock::new(end) -> SplitBlock` (2 empty paths)
  - `SplitBlock::with_paths(end, Vec<Vec<AudioBlock>>) -> SplitBlock` (params for that count)
  - `PathRef{split: BlockId, path: usize}`
  - `project::block::path_letter(usize) -> String` (0→"A", 25→"Z", 26→"AA")
  - `pub const MIN_SPLIT_PATHS: usize = 2`

- [ ] **Step 1: Write the failing tests** (append to `issue_328_split_rules.rs`; change the `split` fixture to `SplitBlock::with_paths(end, vec![a, vec![]])`)

```rust
#[test]
fn a_split_holds_any_number_of_paths() {
    let s = SplitBlock::with_paths(
        SplitEnd::Mix,
        vec![vec![delay("p0")], vec![delay("p1")], vec![delay("p2")], vec![]],
    );
    assert_eq!(s.paths.len(), 4);
    assert!(s.validate_structure().is_ok());
}

#[test]
fn a_split_refuses_fewer_than_two_paths() {
    let s = SplitBlock::with_paths(SplitEnd::Mix, vec![vec![delay("only")]]);
    let err = s.validate_structure().expect_err("one path");
    assert!(err.contains("at least 2 paths"), "got: {err}");
}

#[test]
fn a_select_may_sit_in_a_path() {
    let s = SplitBlock::with_paths(SplitEnd::Mix, vec![vec![select("sel")], vec![]]);
    assert!(s.validate_structure().is_ok(), "spec §11: Select is allowed in a path");
}

#[test]
fn path_letters_name_any_index() {
    use project::block::path_letter;
    assert_eq!(path_letter(0), "A");
    assert_eq!(path_letter(2), "C");
    assert_eq!(path_letter(25), "Z");
    assert_eq!(path_letter(26), "AA");
}
```

In `a_path_holds_processing_blocks_only`, drop the `(select("sel"), "select")` row from `forbidden` (spec §11 now allows it). In `a_rig_refuses_a_port_or_select_in_the_y_path_behind_a_mix`, drop the select row as well.

- [ ] **Step 2: Run, see FAILED**

Run: `nice -n 19 cargo test -p project -j 2 --test issue_328_split_rules`
Expected: compile error `no function or associated item named with_paths`. After a stub returning `a`/`b`, the run gives `assertion failed: s.paths.len() == 4`.

- [ ] **Step 3: Implement the model**

```rust
// split_block.rs
pub const MIN_SPLIT_PATHS: usize = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SplitBlock {
    pub end: SplitEnd,
    pub params: ParameterSet,
    pub paths: Vec<Vec<AudioBlock>>,
}

impl SplitBlock {
    pub fn new(end: SplitEnd) -> Self {
        Self::with_paths(end, vec![Vec::new(); MIN_SPLIT_PATHS])
    }

    pub fn with_paths(end: SplitEnd, paths: Vec<Vec<AudioBlock>>) -> Self {
        Self { end, params: default_split_params(paths.len()), paths }
    }
}

// path_ref.rs
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PathRef {
    pub split: BlockId,
    /// 0-based; the GUI shows it as a letter (`path_letter`).
    pub path: usize,
}

pub fn path_letter(index: usize) -> String {
    let mut n = index + 1;
    let mut out = Vec::new();
    while n > 0 {
        let rem = (n - 1) % 26;
        out.push(b'A' + rem as u8);
        n = (n - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).expect("ascii")
}
```

`default_split_params(path_count)` gets its argument in Task 2. In this task, call the existing `default_split_params()` and keep `params` as is: the knob keys are still `_a`/`_b`, and they stay that way until Task 2.

In `validate_structure`:
- First, `if self.paths.len() < MIN_SPLIT_PATHS { return Err(format!("split has {} path(s); a split needs at least 2 paths", self.paths.len())) }`.
- Then iterate `&self.paths`.
- Remove `AudioBlockKind::Select(_)` from the forbidden match.

`block_walk.rs`, `split_lookup.rs`, `block_path::list_holding`/`lane_mut`:
- Replace each `[&split.a, &split.b]` / `split.a … or_else split.b` with `split.paths.iter()` / `iter_mut().find_map(..)`.
- `lane_mut` indexes `split.paths.get_mut(path.path).ok_or_else(|| anyhow!("split {:?} has no path {}", path.split, path.path))`.

- [ ] **Step 4: Fix every compile error, behavior-preserving**

Apply these replacements one file at a time with Edit, never sed:
- `split.a` → `split.paths[0]`, `split.b` → `split.paths[1]`
- `PathSide::A` → `0`, `PathSide::B` → `1`
- `side` → `path`

Then check that each crate compiles: `nice -n 19 cargo check -p <crate> -j 2 --tests`, one crate at a time, in this order: project, infra-yaml, application, engine, infra-cpal, adapter-gui.

- [ ] **Step 5: Run the targeted tests, see PASS**

Run: `nice -n 19 cargo test -p project -j 2 --test issue_328_split_rules`
Expected: all PASS.

- [ ] **Step 6: Gate, commit, push**

```bash
cargo fmt --all -- --check && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git add <each touched path>
git commit -m "feat(#328): a split holds N paths (spec §11.1)"
git push
```

### Task 2: Per-path knob schema

**Files:**
- Modify: `crates/project/src/block/split_params.rs`
- Create: `crates/project/src/block/split_param_keys.rs`
- Test: `crates/project/tests/issue_328_split_params.rs`

**Interfaces:**
- Produces:
  - `split_param_specs(path_count: usize) -> Vec<ParameterSpec>`
  - `default_split_params(path_count) -> ParameterSet`
  - `normalize_split_params(params, path_count) -> Result<ParameterSet, String>`
  - `check_split_knob(path, value, path_count) -> Result<(), String>`
  - `split_param_descriptors(block_id, params, path_count) -> Result<Vec<BlockParameterDescriptor>, String>`
- Produces, in `split_param_keys.rs`:
  - key builders `level_to(i)`, `balance(i)`, `mix_level(i)`, `mix_pan(i)`, `mix_polarity(i)`, each returning `String`
  - `path_of_key(&str) -> Option<(&'static str /*stem*/, usize)>`
  - `migrate_legacy_split_keys(params: ParameterSet) -> ParameterSet`

Legacy map:

| Old key | New key |
|---|---|
| `level_to_a/b` | `level_to_0/1` |
| `balance_a/b` | `balance_0/1` |
| `mix_level_a/b` | `mix_level_0/1` |
| `mix_pan_a/b` | `mix_pan_0/1` |
| `mix_b_polarity` | `mix_polarity_1` |

Labels are "Level to A", "Balance B", "Level C", "Pan C" and "C polarity", via `path_letter`. Groups are unchanged ("Split" and "Mixer"). The editor groups by path in Task 18.

- [ ] **Step 1: Failing tests**

```rust
use project::block::{default_split_params, migrate_legacy_split_keys, normalize_split_params, split_param_specs};
use project::param::ParameterSet;
use domain::value_objects::ParameterValue;

#[test]
fn the_schema_has_one_knob_set_per_path() {
    let keys: Vec<String> = split_param_specs(3).into_iter().map(|s| s.path).collect();
    for i in 0..3 {
        for stem in ["level_to_", "balance_", "mix_level_", "mix_pan_", "mix_polarity_"] {
            assert!(keys.contains(&format!("{stem}{i}")), "{stem}{i} missing");
        }
    }
    assert!(!keys.contains(&"level_to_3".to_string()));
    for k in ["split_mode", "mix_master", "mix_master_sum"] {
        assert!(keys.contains(&k.to_string()));
    }
}

#[test]
fn three_paths_default_to_unity_levels() {
    let p = default_split_params(3);
    for i in 0..3 {
        assert_eq!(p.get(&format!("level_to_{i}")), Some(&ParameterValue::Float(100.0)));
        assert_eq!(p.get(&format!("mix_level_{i}")), Some(&ParameterValue::Float(100.0)));
    }
}

#[test]
fn legacy_a_b_keys_load_as_paths_0_and_1() {
    let mut old = ParameterSet::default();
    old.insert("level_to_b", ParameterValue::Float(40.0));
    old.insert("mix_b_polarity", ParameterValue::String("invert".into()));
    let new = migrate_legacy_split_keys(old);
    assert_eq!(new.get("level_to_1"), Some(&ParameterValue::Float(40.0)));
    assert_eq!(new.get("mix_polarity_1"), Some(&ParameterValue::String("invert".into())));
    assert!(new.get("level_to_b").is_none());
    assert!(normalize_split_params(new, 2).is_ok());
}

#[test]
fn a_knob_of_a_missing_path_is_refused() {
    use project::block::check_split_knob;
    assert!(check_split_knob("mix_level_2", ParameterValue::Float(50.0), 3).is_ok());
    assert!(check_split_knob("mix_level_3", ParameterValue::Float(50.0), 3).is_err());
}
```

Check how `ParameterValue` floats and strings are spelled in `domain::value_objects` before writing the test, and match it exactly. Existing tests in this file that use `LEVEL_TO_A` and the other constants move to the key builders (`level_to(0)`).

- [ ] **Step 2: Run, see FAILED**

Run: `nice -n 19 cargo test -p project -j 2 --test issue_328_split_params`

- [ ] **Step 3: Implement**

`split_param_specs(n)`:
- Push `split_mode`.
- For each `i in 0..n`, push `percent(level_to(i), format!("Level to {}", path_letter(i)), split)` and `side(balance(i), …)`.
- Then, for each `i`, push `mix_level(i)`, `mix_pan(i)` and the `mix_polarity(i)` enum.
- Last, push `mix_master` and `mix_master_sum`.

`float_parameter` and `enum_parameter` take `&str` paths. If they need `&'static str`, switch them to `impl Into<String>`. Check `crates/project/src/param` first and change only if required.

`unknown keys` stay lenient. `normalize_split_params(params, n)` runs `migrate_legacy_split_keys` first, so every load and every command path sees new keys.

Update every caller:
- `split_block.rs`, `audio_block_methods.rs` (validate_params)
- application `validate_split.rs`, `query_ids.rs`
- engine `runtime_split_knobs.rs` (temporarily reads `level_to(0)`/`(1)`)
- adapter-gui `split_editor_items.rs`

Each caller passes `split.paths.len()`.

- [ ] **Step 4: PASS, then `cargo check` the dependent crates one at a time**
- [ ] **Step 5: Gate, commit "feat(#328): split knobs per path (spec §11.2)", push**

### Task 3: YAML `paths` and the a/b migration

**Files:**
- Modify: `crates/infra-yaml/src/block_yaml_split.rs` and the `!Split` serde struct it uses
- Test: `crates/infra-yaml/tests/issue_328_split_yaml.rs`

**Interfaces:**
- Consumes: `SplitBlock::with_paths`, `migrate_legacy_split_keys`
- Produces:
  - on-disk shape `!Split { end, params, paths: [[…], […], …] }`
  - path block ids `<split>::p<i>:<j>`
  - the legacy `a:`/`b:` shape still loads

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn a_three_path_split_round_trips() {
    // Build a project whose preset holds Split{Mix, paths: [[delay],[delay],[]]},
    // save it with the existing helper this file already uses, reload, assert
    // paths.len() == 3 and the block ids are "<split>::p0:0", "<split>::p1:0".
}

#[test]
fn a_legacy_a_b_split_loads_as_paths_0_and_1() {
    let yaml = r#"
!Split
end: mix
params: { level_to_b: 40.0 }
a: [ <one delay block in this file's existing fixture form> ]
b: []
"#;
    // load → paths.len()==2, paths[0].len()==1, params["level_to_1"]==40.0
}
```

Write both bodies with the fixture helpers that already exist in `issue_328_split_yaml.rs`. Read the file first and copy its delay-block YAML and its load/save helpers exactly. Do not invent new helpers.

- [ ] **Step 2: FAILED** — `nice -n 19 cargo test -p infra-yaml -j 2 --test issue_328_split_yaml`
- [ ] **Step 3: Implement**
  - The serde struct gets `#[serde(default)] paths: Vec<Vec<Value>>`, `#[serde(default)] a: Option<Vec<Value>>` and `#[serde(default)] b: Option<Vec<Value>>`.
  - If `paths` is empty and `a`/`b` are present, use `vec![a.unwrap_or_default(), b.unwrap_or_default()]`.
  - `split_to_yaml` writes `paths` only.
  - `split_from_yaml(id, enabled, end, params, paths: Vec<Vec<Value>>)` names block `j` of path `i` as `format!("{}::p{}:{}", id, i, j)` when the stored id is missing. This matches the rule for the old `::a:` ids.
- [ ] **Step 4: PASS**
- [ ] **Step 5: Gate, commit "feat(#328): N-path split on disk with a/b migration (spec §11.3)", push**

### Task 4: Y leaves, endpoint disables per leaf, `TailFeed::Leaves`

**Files:**
- Create: `crates/project/src/block/y_leaves.rs`
- Modify:
  - `crates/project/src/endpoint_disables.rs`
  - `crates/project/src/endpoint_feeds.rs`
  - the infra-yaml file that (de)serializes `disabled_endpoints`, found with `grep -rn path_a_outputs crates/infra-yaml`
- Test:
  - `crates/project/tests/issue_328_endpoint_disables.rs`
  - `crates/project/src/endpoint_feeds_tests.rs`
  - `crates/project/tests/issue_328_y_leaves.rs` (new)

**Interfaces:**
- Produces:
  - `y_leaves(blocks: &[AudioBlock]) -> Vec<PathRef>`, in depth-first order. A leaf is a path of a Y that holds no Y. A Y nested in a Mix path counts. A Y in a Y path replaces that path with its own leaves.
  - `EndpointNode::{Input, Output, PathOutput(PathRef)}`
  - `EndpointDisables{inputs, outputs, path_outputs: Vec<PathOutputDisables>}`
  - `PathOutputDisables{split: BlockId, path: usize, disabled: Vec<EndpointRef>}`
  - `EndpointDisables::leaf_output_enabled(&PathRef, &EndpointRef) -> bool`
  - `TailFeed::{Off, Chain, Leaves(Vec<PathRef>)}`: the leaves that feed one output endpoint. `Chain` means no Y anywhere in the chain.

- [ ] **Step 1: Failing tests**

```rust
// issue_328_y_leaves.rs
#[test]
fn a_y_in_a_y_path_gives_three_leaves() {
    // Y outer(paths: [[Y inner(paths:[[],[]])], [delay]])
    // leaves == [inner/0, inner/1, outer/1]
}
#[test]
fn a_y_inside_a_mix_path_has_its_own_leaves() {
    // [Mix m(paths:[[Y y(paths:[[],[]])],[delay]]), delay]
    // leaves == [y/0, y/1]
}
#[test]
fn a_chain_without_a_y_has_no_leaves() { /* [Mix, delay] → [] */ }

// issue_328_endpoint_disables.rs
#[test]
fn a_leaf_output_can_be_unchecked() {
    // set_enabled(PathOutput(PathRef{split:"y",path:2}), ep, false)
    // → !leaf_output_enabled(&PathRef{y,2}, ep), leaf 1 still enabled
}
```

Add a YAML test in infra-yaml for the old-key migration: `path_a_outputs: [..]` loads as `path_outputs: [{split: <root Y id>, path: 0, disabled: [..]}]`. The root Y is the first top-level Y (`find_split_with_end(blocks, Y)`). This needs the chain's blocks at load time; do the migration in the chain loader, after the blocks are loaded.

In `endpoint_feeds_tests.rs`, add a test where `tail_feed` returns `Leaves([y/1])` for an output that only leaf `y/1` has enabled.

- [ ] **Step 2: FAILED** for each file (`-p project --test issue_328_y_leaves`, `--test issue_328_endpoint_disables`, `--lib endpoint_feeds`).
- [ ] **Step 3: Implement**

```rust
// y_leaves.rs
pub fn y_leaves(blocks: &[AudioBlock]) -> Vec<PathRef> {
    let mut out = Vec::new();
    collect(blocks, &mut out);
    out
}

fn collect(blocks: &[AudioBlock], out: &mut Vec<PathRef>) {
    for block in blocks {
        let AudioBlockKind::Split(split) = &block.kind else { continue };
        for (i, path) in split.paths.iter().enumerate() {
            let before = out.len();
            collect(path, out);
            if split.end == SplitEnd::Y && out.len() == before && !has_y_split(path) {
                out.push(PathRef { split: block.id.clone(), path: i });
            }
        }
    }
}
```

`has_y_split` must become recursive. It currently checks the top level only, and is used by `tail_feed`.

`EndpointDisables::retain_known` drops `path_outputs` entries whose leaf is no longer in `y_leaves`.

`tail_feed(chain, io, endpoint)` behaves as follows:
- With no leaves, it returns `Chain` or `Off` as today.
- With leaves, it returns `Leaves(leaves enabled for this endpoint)`, or `Off` when that set is empty.

`checklist_silences` iterates `path_outputs`.
- [ ] **Step 4: PASS**
- [ ] **Step 5: Gate, commit "feat(#328): every Y leaf owns its outputs (spec §11.3, §11.5)", push**

### Task 5: `AddSplitPath`, `RemoveSplitPath`, `RemoveSplit`

**Files:**
- Modify: `crates/application/src/command.rs`, the dispatcher match, and the MCP tool registry (`grep -rn "AddSplit" crates/adapter-mcp crates/application` locates them)
- Create:
  - `crates/application/src/split_path_commands.rs`
  - `crates/project/src/block/split_param_renumber.rs`
- Test: `crates/application/src/ld_split_paths_tests.rs` (new), `crates/project/tests/issue_328_split_params.rs`

**Interfaces:**
- Consumes: `block_path::split_mut`, the key builders, `path_of_key`
- Produces:
  - `Command::AddSplitPath{chain: ChainId, split_id: BlockId}` and `Command::RemoveSplitPath{chain, split_id, path: usize}`
  - `drop_path_keys(params: &ParameterSet, removed: usize, path_count: usize) -> ParameterSet`
  - `renumbered_key(key: &str, removed: usize) -> Option<String>`: `None` when the key belongs to the removed path, the shifted key when its index is above `removed`, the same key otherwise

- [ ] **Step 1: Failing tests**

```rust
// issue_328_split_params.rs
#[test]
fn removing_a_middle_path_renumbers_the_keys_above_it() {
    let mut p = default_split_params(4);
    p.insert("mix_level_3", ParameterValue::Float(12.0));
    let q = drop_path_keys(&p, 1, 4);
    assert_eq!(q.get("mix_level_2"), Some(&ParameterValue::Float(12.0)));
    assert!(q.get("mix_level_3").is_none());
    assert!(normalize_split_params(q, 3).is_ok());
    assert_eq!(renumbered_key("mix_pan_1", 1), None);
    assert_eq!(renumbered_key("mix_pan_3", 1).as_deref(), Some("mix_pan_2"));
    assert_eq!(renumbered_key("mix_master", 1).as_deref(), Some("mix_master"));
}

// ld_split_paths_tests.rs — copy the dispatcher setup of ld_split_nested_tests.rs
#[test]
fn add_split_path_appends_an_empty_path_with_default_knobs() { /* 2 → 3 paths, level_to_2 == 100 */ }
#[test]
fn remove_split_path_refuses_to_go_below_two() { /* err contains "at least 2" */ }
#[test]
fn remove_split_path_moves_midi_and_scene_keys() {
    // 4-path Mix, a MIDI mapping on "<split>::mix_level_3" and a scene value on
    // mix_level_3; remove path 1 → mapping and scene now name mix_level_2.
}
#[test]
fn remove_split_path_drops_its_leaf_output_disables() { /* Y with 3 paths, path_outputs on path 2, remove path 1 → entry now path 1 */ }
#[test]
fn remove_split_keeps_path_0_in_place() { /* [pre, Mix(paths:[[x],[y],[z]]), post] → [pre, x, post] */ }
#[test]
fn blocks_add_and_move_at_depth() { /* AddBlock into PathRef{inner, 2} where inner sits in outer path 1 */ }
```

Before writing the MIDI/scene test, read how MIDI mappings and scenes address a block parameter (`grep -rn "struct MidiMapping\|scene" crates/project/src | head`). Assert against that real shape.

Add `AddSplitPath` and `RemoveSplitPath` to the MCP variant-count test, so the expected count grows by 2.

- [ ] **Step 2: FAILED** — `nice -n 19 cargo test -p application -j 2 ld_split_paths` and `-p project --test issue_328_split_params`
- [ ] **Step 3: Implement**
  - `AddSplitPath`:
    1. `split_mut`.
    2. `paths.push(vec![])`.
    3. `params = normalize_split_params(params, paths.len())`, which fills the new path's defaults.
  - `RemoveSplitPath`:
    1. Refuse if `paths.len() <= MIN_SPLIT_PATHS` (`"a split keeps at least 2 paths"`) or if `path >= len`.
    2. `paths.remove(path)`, then `params = drop_path_keys(...)`.
    3. Rewrite MIDI mapping keys and scene parameter keys of this block through `renumbered_key`. A mapping or scene value whose key returns `None` is dropped.
    4. Rewrite `path_outputs` entries of this split: drop `path`, decrement those above it.
    5. Drop `path_outputs` entries of every Y nested in the removed path.
  - `RemoveSplit`: replace the split block with `paths[0]`'s blocks at its position.

  Each MCP tool mirrors `add_split`'s tool definition.
- [ ] **Step 4: PASS**
- [ ] **Step 5: Gate, commit "feat(#328): add and remove split paths (spec §11.4)", push**
- [ ] **Step 6:** `gh issue comment 328` with the Part 1 hashes. CI must be green on the push before Part 2 starts.

---

## Part 2 — Engine

### Task 6: N-way alignment plan

**Files:** Modify `crates/engine/src/runtime_split_align.rs`. Test: `crates/engine/src/runtime_split_align_tests.rs`

**Interfaces:**
- Produces:
  - `AlignPlan{delays: Vec<usize>, clamped: bool}`
  - `plan_alignment(latencies: &[usize]) -> AlignPlan`, where `delays[i] = min(max - latencies[i], MAX_ALIGN_SAMPLES)`

- [ ] **Step 1: Failing test**

```rust
#[test]
fn every_path_is_delayed_up_to_the_longest() {
    let plan = plan_alignment(&[10, 0, 64]);
    assert_eq!(plan.delays, vec![54, 64, 0]);
    assert!(!plan.clamped);
}
#[test]
fn a_difference_over_the_cap_is_clamped() {
    let plan = plan_alignment(&[0, MAX_ALIGN_SAMPLES + 5]);
    assert_eq!(plan.delays, vec![MAX_ALIGN_SAMPLES, 0]);
    assert!(plan.clamped);
}
```

Rewrite the existing 2-arg tests as slice calls; their expected values are unchanged.
- [ ] **Step 2: FAILED** — `nice -n 19 cargo test -p engine -j 2 runtime_split::align`
- [ ] **Step 3: Implement**

```rust
pub(crate) struct AlignPlan { pub(crate) delays: Vec<usize>, pub(crate) clamped: bool }

pub(crate) fn plan_alignment(latencies: &[usize]) -> AlignPlan {
    let longest = latencies.iter().copied().max().unwrap_or(0);
    let mut clamped = false;
    let delays = latencies.iter().map(|&l| {
        let d = longest - l;
        clamped |= d > MAX_ALIGN_SAMPLES;
        d.min(MAX_ALIGN_SAMPLES)
    }).collect();
    AlignPlan { delays, clamped }
}
```

`plan_alignment` runs at build and toggle time, off the audio thread, so its `Vec` is fine. `refresh_alignment` is called on the control side (verify in `runtime_block_toggle.rs`). If it runs on the audio thread, write into a preallocated slice instead: `plan_alignment_into(&[usize], &mut [usize]) -> bool`.
- [ ] **Step 4: PASS**. **Step 5:** commit "feat(#328): N-way split alignment", push.

### Task 7: N-way knob math

**Files:** Modify `crates/engine/src/runtime_split_mix.rs`, `runtime_split_knobs.rs`. Test: their `_tests.rs`, `issue_328_split_mix_tests.rs`

**Interfaces:**
- Produces:
  - `PathKnobs{level_to: f32, balance: f32, mix_level: f32, mix_pan: f32, invert: bool}`
  - `MixKnobs{dual_mono: bool, master: f32, master_sum: bool}`
  - `path_input(frame: [f32;2], path: &PathKnobs, mix: &MixKnobs) -> [f32;2]`: the split side of path i, the §4.1 math for one path
  - `accumulate_path(acc: &mut [f32;2], out: [f32;2], path: &PathKnobs)`: applies mix level, pan and polarity, then adds
  - `finish_mix(acc: [f32;2], mix: &MixKnobs) -> [f32;2]`: the master stage
  - `SplitKnobs::from_params(params, path_count)`
  - `SplitKnobs::load_into(&self, mixes: bool, paths: &mut [PathKnobs]) -> MixKnobs`: no allocation
  - `PathKnobs::neutral()`: level 100, pan 0, normal
  - `MixKnobs::neutral()`: master 100, no sum

- [ ] **Step 1: Failing tests** (in `issue_328_split_mix_tests.rs`)

```rust
#[test]
fn three_paths_at_defaults_sum_to_unity() {
    // the split's default master and level must produce, through
    // path_input → identity path → accumulate_path ×3 → finish_mix,
    // the same output the 2-path defaults produce today for the same input
    // (read the existing 2-path unity test and extend it to 3 paths: same
    // expected value scaled exactly as the existing per-path sum rule does).
}
#[test]
fn per_path_polarity_inverts_only_that_path() {
    // path 2 invert, paths 0/1 normal, identical inputs → acc == 1 path's worth
}
#[test]
fn per_path_pan_moves_only_that_path() { /* path 1 pan +50 → its L share 0 */ }
```

Read the existing 2-path unity and pan tests first. The N-path tests must call the same `pan_gains` and `balance_pick` helpers and assert against the same numbers. The 2-path tests stay as they are, rewritten to use `PathKnobs`, with expected values unchanged.
- [ ] **Step 2: FAILED** — `nice -n 19 cargo test -p engine -j 2 issue_328_split_mix runtime_split::mix runtime_split::knobs`
- [ ] **Step 3: Implement.** `SplitKnobs` holds `paths: Vec<PathKnobAtomics>` (one `AtomicU32` per f32 and one `AtomicBool` each), built off-thread. `load_into` writes into the caller's preallocated slice. The math moves verbatim from `split_inputs` and `mix_frame` into the per-path functions. The 2-path result must be bit-identical.
- [ ] **Step 4: PASS** (old and new). **Step 5:** commit "feat(#328): split and mixer math per path", push.

### Task 8: N-path `SplitRuntimeState` and process

**Files:**
- Modify: `runtime_split_state.rs`, `runtime_split_process.rs`, `runtime_split_builder.rs`, `runtime_split_latency.rs`, `runtime_split_walk.rs`
- Test: `runtime_split_process_tests.rs`, `runtime_split_state_tests.rs`, and a new `runtime_split_alloc_tests.rs`

**Interfaces:**
- Produces:

```rust
pub(crate) struct SplitRuntimeState {
    pub(crate) mixes: bool,
    pub(crate) paths: Vec<Vec<BlockRuntimeNode>>,
    pub(crate) bufs: Vec<Vec<AudioFrame>>,      // one per path, capacity SEGMENT_FRAME_CAPACITY
    pub(crate) aligns: Vec<AlignDelay>,         // one per path
    pub(crate) values: Vec<PathKnobs>,          // scratch, len == paths.len()
    pub(crate) knobs: SplitKnobs,
}
SplitRuntimeState::new(mixes, paths, knobs, block_id)
```

`process_chunk`:
1. `let mix = knobs.load_into(mixes, &mut values)`.
2. For each path `i`, clear `bufs[i]` and push `path_input(frame, &values[i], &mix)` for every frame.
3. Run path `i`'s nodes over `bufs[i]`, then `aligns[i].process(&mut bufs[i])`.
4. If `mixes`: `frames[k] = finish_mix(Σ_i accumulate_path(bufs[i][k]))`.
5. Otherwise, for a Y, which only runs inside a pruned segment as a neutral Mix (Task 10), use the same code: Y is never processed un-pruned.

`path_latency` and `path_latency_ceiling` take the max over `paths`. `for_each_node` recurses `paths`.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn a_three_path_mix_runs_every_path() {
    // test_support builds a split with three paths, each holding a gain node
    // (test_support already has a gain/latency fake node — reuse it) with
    // gains 1, 2, 3 and mix levels at default; process one frame of 1.0 →
    // output == the 2-path rule extended: (1+2+3) × the same per-path scaling.
}
#[test]
fn a_nested_mix_is_aligned_inside_its_path() {
    // outer Mix paths: [ [inner Mix(paths: [[latency 32],[]])], [] ]
    // impulse in → the outer output holds ONE impulse peak (both outer paths
    // aligned to the inner's 32), not two peaks 32 apart.
}
#[test]
fn no_allocation_after_warm_up_with_five_paths() {
    // use the allocation-counting harness the engine already uses
    // (grep -rn "GlobalAlloc\|alloc_counter" crates/engine/src) — process 64
    // callbacks of a 5-path Mix; allocations during the last 63 == 0.
}
```

- [ ] **Step 2: FAILED** — `nice -n 19 cargo test -p engine -j 2 runtime_split`
- [ ] **Step 3: Implement.** The builder builds every path with `build_nodes_for`. `adopt_history` adopts per index when the path count matches; otherwise it adopts per index up to the min. `refresh_alignment` uses `plan_alignment(&paths.iter().map(path_latency).collect::<Vec<_>>())` and sets each `aligns[i]` delay.
- [ ] **Step 4: PASS**. **Step 5:** commit "feat(#328): the engine runs N split paths", push.

### Task 9: Reuse and toggle at any depth

**Files:** Modify `crates/engine/src/runtime_block_reuse.rs`, `runtime_block_toggle.rs`. Test: their tests plus `runtime_split_dispatch_tests.rs`

**Interfaces:**
- Produces:
  - `toggle_block(...)` refreshes `refresh_alignment()` on every split from the toggled node up to the root, deepest first.
  - Reuse matches nodes by `BlockId` anywhere in the tree.

- [ ] **Step 1: Failing test**

```rust
#[test]
fn bypassing_a_deep_block_realigns_every_ancestor() {
    // outer Mix [ [inner Mix [ [lat 64], [] ]], [] ]; bypass the lat-64 node →
    // inner delays become [0,0] and outer delays become [0,0] (was [0,64]).
}
#[test]
fn a_block_two_levels_down_is_reused_on_rebuild() {
    // rebuild with an unchanged tree → the deep node's runtime pointer is the same
}
```

- [ ] **Step 2: FAILED**. **Step 3: Implement.** Use a recursive walk that returns `bool found` and refreshes on the way out. **Step 4: PASS**. **Step 5:** commit "feat(#328): toggle and reuse at any depth", push.

### Task 10: Leaf sets replace `SegmentPaths`

**Files:**
- Modify:
  - `segment_types.rs`, `segment_paths.rs`, `split_segment_view.rs`
  - `runtime_segments.rs`, `runtime_graph_assemble.rs`, `route_convolution.rs`, `offline.rs`
  - `crates/infra-cpal/src/controller_offthread_live_rebuild.rs`
  - `chain_structure_signature` (find it with `grep -rn "fn chain_structure_signature" crates`)
- Test: `segment_paths_tests.rs`, `split_segment_view_tests.rs`, `issue_328_y_segments_tests.rs`, `issue_328_y_audio_tests.rs`, `route_convolution_tests.rs`, infra-cpal `io_topology_tests.rs`

**Interfaces:**
- Produces:
  - `SegmentPaths` becomes `pub(crate) enum SegmentLeaves { All, Only(Vec<PathRef>) }`.
    - `All` covers a chain without a Y, and offline rendering.
    - `Only(..)` holds the leaves feeding this output, sorted.
  - `ChainSegment.leaves: SegmentLeaves` replaces `.paths`.
  - `route_leaves(chain, registry) -> Vec<SegmentLeaves>`
  - `chain_for_segment(chain, &SegmentLeaves) -> Cow<Chain>`
  - `block_for_segment(block, &SegmentLeaves) -> AudioBlock`, which recurses. Each Y becomes a Mix with neutral mixer knobs (`mix_level_i = 100` if path `i` holds a kept leaf or a kept descendant leaf, else 0; master 100). Paths that hold no kept leaf are emptied. A Mix keeps its knobs, and its paths are pruned recursively, so a Y nested inside a Mix path is pruned while the Mix's other paths stay.
  - `is_routing(block)`: true when the block's subtree holds a Y.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn one_segment_per_output_with_its_leaf_set() {
    // Y y(paths:[[],[],[]]); leaf 0 → out A, leaves 1,2 → out B
    // route_leaves == [Only([y/0]), Only([y/1, y/2])]
}
#[test]
fn two_leaves_on_one_output_sum_inside_its_segment() {
    // audio test (issue_328_y_audio_tests): paths gain 1 and gain 2 both on
    // out B → out B carries 3× the input, aligned (one impulse peak).
}
#[test]
fn a_y_in_a_y_gives_three_outputs() { /* Y[ [Y[ [],[] ]], [] ] each leaf to its own output → 3 segments */ }
#[test]
fn y_inside_mix_path_routes_leaf_and_mix() {
    // [Mix m[ [Y y[[gain 2],[gain 3]]], [gain 5] ], Output top]; leaf y/0 → out X.
    // out X segment: Mix kept, y pruned to path 0 → X gets 2 + 5 (path 1 of m
    // still sums), no 3.
}
#[test]
fn the_signature_changes_when_a_leaf_moves_output() { /* chain_structure_signature differs */ }
```

The existing A/B/AB tests are rewritten as `Only([y/0])`, `Only([y/1])` and `Only([y/0, y/1])`, with their expected audio unchanged.
- [ ] **Step 2: FAILED** — `nice -n 19 cargo test -p engine -j 2 segment_paths split_segment_view issue_328_y`
- [ ] **Step 3: Implement.** `route_leaves` uses `tail_feed(...)`:
  - `Leaves(v)` → `Only(v)`
  - `Chain` → `All`
  - `Off` → no segment

  `group_routes_by_leaves` keys on the `Vec<PathRef>`. `offline.rs` uses `All`, which renders the whole tree with every leaf summed (current offline behavior: every path plays). In infra-cpal and JACK, compile against the new type; the JACK output-index service is unchanged in logic and stays under its existing `cfg`.
- [ ] **Step 4: PASS**, then `nice -n 19 cargo test -p infra-cpal -j 2 io_topology`.
- [ ] **Step 5:** commit "feat(#328): outputs fed by Y leaves at any depth (spec §11.5)", push. Then `gh issue comment 328` with the Part 2 hashes. CI must be green before Part 3.

---

## Part 3 — GUI

Before the first `.slint` edit, invoke `claude-plugin:ux-ui`, `slint:slint` and `slint-best-practices`. Render with `tools/slint-render`. Prove every click with an `i-slint-backend-testing` interaction test, and use no `PopupWindow` (use a root-level overlay).

### Task 11: Recursive `ChainStage` and layout

**Files:**
- Modify: `graph_view_model/types.rs`, `chain_builder.rs`, `routing_ids.rs`, `validation.rs`
- Test: `graph_view_model_tests.rs`

**Interfaces:**
- Produces:
  - `ChainStage::Parallel{split_id: String, lanes: Vec<Vec<ChainStage>>, end: ParallelEnd}`
  - `split_node_id(split_id: &str) -> String` (`__split_<id>`)
  - `merge_node_id(split_id) -> String` (`__merge_<id>`)
  - `linear_chain_layout(stages, metrics) -> (Vec<GraphNode>, Vec<GraphEdge>)`, recursive
  - `stage_extent(stages) -> (columns: usize, rows: usize)`:
    - a Single is 1×1;
    - a Parallel is 2 + the max lane width (1 + max for a Fan) by the sum of its lanes' row counts (min 1 per lane);
    - a list's width is the sum of its stages and its height is their max.
  - Lane `i` of a Parallel starts at the row offset equal to the sum of the earlier lanes' heights. The block is centred on its parent's row band.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn a_three_lane_mix_puts_each_lane_on_its_own_row() { /* 3 lanes → 3 distinct y, split and merge on the centre */ }
#[test]
fn a_nested_split_takes_the_rows_it_needs() {
    // outer Mix lanes: [[inner Mix lanes [[a],[b]]], [c]] → rows == 3,
    // a, b, c have three distinct y; inner merge sits before outer merge (x).
}
#[test]
fn a_linear_chain_keeps_one_row() { /* stage_extent == (n, 1) */ }
```

The existing tests move to the `split_id` field. Their numbers are unchanged for 2 lanes.
- [ ] **Step 2: FAILED** — `nice -n 19 cargo test -p adapter-gui -j 2 graph_view_model`
- [ ] **Step 3: Implement** a recursive `place(stages, col, row_top, rows, metrics, prev_tail, nodes, edges) -> (end_col, tail)`.
- [ ] **Step 4: PASS**. **Step 5:** commit, push.

### Task 12: Anchors and node ids at depth

**Files:**
- Modify: `graph_view_model/anchors.rs`, `chain_graph_ids.rs`, `graph_anchor.rs`
- Test: `graph_anchor_tests.rs`, `chain_graph_ids_tests.rs`

**Interfaces:**
- Produces:
  - `AnchorSlot{path: Option<PathRef>, index: usize}`, with `anchor_id()` equal to `"top:{index}"` or `"path:{split}:{path}:{index}"`
  - `parse_anchor` as its inverse. Split ids contain `::` and a `:`-free tail, so parse with `rsplitn(3, ':')`.
  - `insert_target(chain, slot)` uses `list_at(chain, slot.path)` at any depth.
  - `resolve_node` resolves `__split_<id>`, `__merge_<id>` and `__out_<id>_<i>` (→ `Endpoints(PathOutput(PathRef))`), and blocks at any depth with their `PathRef`.

- [ ] **Step 1: Failing tests**

The existing anchor tests are rewritten to the new slot shape, with expected targets unchanged. Add:

```rust
#[test]
fn an_anchor_inside_a_nested_path_names_that_path() { /* inner split path 2, index 1 */ }
#[test]
fn an_anchor_id_with_a_split_id_containing_colons_parses_back() {}
#[test]
fn a_block_three_levels_down_resolves_with_its_path() {}
#[test]
fn a_leaf_output_node_resolves_to_its_path_output() {}
```

- [ ] **Step 2: FAILED**. **Step 3: Implement.** **Step 4: PASS**. **Step 5:** commit, push.

### Task 13: Adapter builds the tree; row size from the tree

**Files:** Modify `chain_graph_adapter.rs` and the row Slint height/width binding. Test: `chain_graph_adapter_tests.rs`, `tests/chain_row_*.rs` (unchanged, must stay green)

**Interfaces:**
- Produces:
  - `chain_stages(chain, labels)`, recursive
  - `ChainGraph{nodes, edges, anchors, rows, columns}`: `lanes` becomes `rows` from `stage_extent`
  - `grid_metrics(rows)`
  - a Y path gets the terminal `__out_<split>_<i>` labelled via `labels.path_output(i)` (`"Out " + path_letter(i)`)

- [ ] **Step 1: Failing tests:** `a_nested_tree_reports_its_rows_and_columns`, `every_y_leaf_gets_its_own_output_node`.
- [ ] **Steps 2–5:** FAILED → implement → PASS (plus `nice -n 19 cargo test -p adapter-gui -j 2 --test 'chain_row_*'`) → commit, push.

### Task 14: Picker everywhere

**Files:** Modify `split_picker_entries.rs`. Test: `split_picker_entries_tests.rs`

- [ ] **Step 1: Failing tests:**
  - `split_y_and_mix_are_offered_inside_a_nested_path`
  - `split_y_is_refused_when_blocks_follow_in_that_list`
  - `select_is_offered_in_a_path`
  - `insert_is_not_offered_in_a_path`
- [ ] **Steps 2–5:** FAILED → implement (judge rule 1 against `list_at(chain, path)` from position on) → PASS → commit, push.

### Task 15: "+ path" and remove-path gestures

**Files:**
- Create: `split_path_gestures.rs`
- Modify:
  - the graph node Slint card for the split node (a "+ path" SVG button)
  - the lane card or lane header (a remove-path SVG button)
  - the confirm overlay used for non-empty paths (reuse the existing root-level confirm overlay; `grep -rn "confirm" crates/adapter-gui/ui | head`)
  - the callback wiring
- Test: `split_path_gestures_tests.rs`, and an interaction test next to `issue_328_graph_row_interaction_tests.rs`

**Interfaces:**
- Produces:
  - `add_path_command(chain_id, node_id, chain) -> Option<Command>`
  - `remove_path_request(chain, split_id, path) -> RemovePathRequest{Direct(Command) | Confirm(Command)}`: `Confirm` when the path holds blocks
  - Slint callbacks `split-add-path(string)` and `lane-remove-path(string, int)`

- [ ] **Step 1: Failing tests:**
  - pure tests for both functions
  - an interaction test that clicks the "+ path" on a rendered split node and asserts `AddSplitPath` was dispatched
  - an interaction test that clicks remove on a non-empty lane and asserts the confirm overlay is shown
- [ ] **Step 2: FAILED.** **Step 3: Implement.** **Step 4: PASS.** Then run `tools/slint-render` on a 3-path Mix and on a Y nested in a Mix, and check both PNGs.
- [ ] **Step 5:** commit, push.

### Task 16: Split and mixer editors per path

**Files:** Modify `split_editor_items.rs`. Test: `split_editor_items_tests.rs`, `issue_328_path_block_editor_tests.rs`

- [ ] **Step 1: Failing tests:**
  - `the_mixer_editor_shows_one_group_per_path` (3 paths → "A", "B", "C" groups, each with level, pan and polarity, then master)
  - `the_split_editor_shows_level_and_balance_per_path`
- [ ] **Steps 2–5:** FAILED → implement from `split_param_descriptors(.., paths.len())`, grouping by `path_of_key` → PASS → commit, push.

### Task 17: Endpoint checklist per leaf

**Files:** Modify `endpoint_checklist_items.rs`, `endpoint_checklist_wiring.rs`. Test: `endpoint_checklist_items_tests.rs`

- [ ] **Step 1: Failing tests:**
  - `each_leaf_output_node_lists_every_chain_output`
  - `unchecking_on_leaf_2_dispatches_path_output_2`
  - `two_leaves_may_check_the_same_output`
- [ ] **Steps 2–5:** FAILED → implement → PASS → commit, push.

### Task 18: Compact and touch views at depth

**Files:** Modify `compact_row_address.rs`, `chain_block_lists.rs`, `chain_block_helpers.rs`, and the compact row Slint indentation. Test: `compact_row_address_tests.rs`, `issue_328_compact_path_row_actions_tests.rs`

- [ ] **Step 1: Failing tests:**
  - `a_split_row_is_followed_by_its_paths_tagged_a_b_c`
  - `a_nested_path_row_carries_its_depth`
  - `a_compact_row_address_round_trips_at_depth`
- [ ] **Steps 2–5:** FAILED → implement (`side_from_index` becomes `PathRef{path: index}`; add a `depth` field to the row model and indent by `depth * 16px`) → PASS → render check → commit, push.
- [ ] **Step 6:** `gh issue comment 328` with the Part 3 hashes. CI must be green.

---

## Part 4 — Docs and translations

### Task 19: Docs

**Files:** `docs/blocks-catalog.md`, `docs/audio-config.md`, `docs/screens.md`, `docs/gui/graph-view.md`, `docs/mcp.md`, `README.md`, `README.pt-BR.md`, `README.es-ES.md`

- [ ] Rewrite the split sections to the N-path model:
  - knob keys `_<i>`
  - leaves and outputs
  - "+ path" and remove-path
  - the two MCP tools
  - the migration note for `a`/`b` files
- [ ] Gate (`VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates`), commit "docs(#328): N-path split graph", push.

### Task 20: Translations

- [ ] Run `scripts/extract-translations.sh`. Fill every new string ("+ path", "Remove path", "Out {}", "Level {}", "Pan {}", "{} polarity", "Level to {}", "Balance {}", the confirm text) in all 9 `.po`/`.yml` files, with English as the reference.
- [ ] Commit "i18n(#328): N-path split strings", push.
- [ ] Final validation checklist in chat and as a `gh issue comment 328`:
  - the main-folder checkout command
  - the absolute solver `run:` line from `scripts/solver-setup.sh 328 feature/issue-328`
  - numbered `[ ]` items covering only ear, eye and hardware: a 3-path Mix sounds summed and aligned, "+ path" and remove-path, a Y in a Mix routes to its outputs, an old project loads the same, the graph grows to fit
