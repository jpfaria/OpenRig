# #328 Part 2: Commands and MCP — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every transport (GUI, MCP, MIDI map, gRPC) can build, edit and tear down a chain split — blocks addressed inside path A / path B, the split's knobs edited as ordinary block parameters, the endpoint checklist toggled — through the `Command` bus, with every structural edit refused unless the result obeys the split rules, and every edit captured into `project.yaml`.

**Architecture:** Two small pure modules in `crates/application` carry the new logic: `block_path.rs` addresses a block anywhere in the one-level block tree (top level + the split's two paths), and `split_rules.rs` says whether a block list obeys the three split rules. A new dispatcher helper, `edit_chain_blocks`, runs every structural edit on a copy of the chain's blocks and commits it only when `split_rules` accepts the result, so a refused command never leaves a half-applied chain. New variants live in a new `SplitCommand` sub-enum (`AddSplit`, `SetSplitEnd`, `RemoveSplit`) and in `ChainCommand` (`SetChainEndpointEnabled`); their handlers live in new `local_dispatcher_*` files. MCP tools are derived from the `Command` schema automatically — this part bumps the parity pin and proves each tool builds its command.

**Tech Stack:** Rust 2021, serde / serde_json, schemars, anyhow; `cargo test` (unit tests beside the module via `#[cfg(test)] #[path = "…_tests.rs"]`, integration tests in `crates/application/tests/`).

**Spec:** `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328/docs/superpowers/specs/2026-09-28-issue-328-chain-split-graph-design.md` — sections covered: **§3 Commands** (all bullets), **§1.1** (the split rules, enforced at the command layer), **§1.2** (knobs edited through `SetBlockParameter*`), **§1.3** (the checklist state written back into `RigInput`), **§1.4** (the command-layer walkers: `with_block`, the read side, the rig write-back of `disabled_endpoints`), **§7** "Commands" bullet, **§8** `docs/mcp.md` (plus `docs/midi.md`, which documents the same payloads).

**Depends on:** **Part 1 (model)** — must be merged into `feature/issue-328` first. This part consumes, by the contract names:

- `project::block::split_block::{SplitBlock, SplitEnd}`, `project::block::path_ref::{PathRef, PathSide}` (Part 1 Task 2 puts them in their own file; both pairs are also re-exported as `project::block::{SplitBlock, SplitEnd, PathRef, PathSide}`, which is how this plan imports `PathRef`/`PathSide`) and `AudioBlockKind::Split(SplitBlock)`, with `AudioBlockKind::label()` returning `"split"` for it.
- `project::block::split_params::{default_split_params, SPLIT_MODE, MIX_PAN_A, MIX_MASTER_SUM}`.
- `project::endpoint_disables::{EndpointDisables, EndpointRef, EndpointNode}` with `EndpointDisables::is_empty` / `is_enabled`.
- `Chain.disabled_endpoints` and `RigInput.disabled_endpoints` (both `EndpointDisables`).
- Derives this part relies on: `PathRef`, `PathSide`, `SplitEnd`, `EndpointNode` — `Debug, Clone, Serialize, Deserialize, schemars::JsonSchema` (the `Command` schema, hence the MCP tools, needs `JsonSchema`); `EndpointDisables` — `Default, Clone, Debug, PartialEq`; `EndpointRef` — `Clone, Debug, PartialEq`; `SplitBlock` — `Clone, Debug, PartialEq`.
- Part 1 behaviours the capture/nav tests exercise end to end: `AudioBlockKind::model_identity()` encodes both paths (so an edit inside a path is structural), `RigPreset::apply_scene` recurses into paths, and `engine::rig_runtime::rig_to_chains` copies `RigInput.disabled_endpoints` onto the projected chain.

Parts 3–4 (engine) and 5–6 (GUI) are not needed by any test here. Part 6 (chain row) consumes this part's commands.

**Naming against the shared contract:** the existing command variants name the chain id field `chain` (`AddBlock { chain, … }`, `SetChainIoBindings { chain, … }`), so the new variants use `chain` instead of the contract's `chain_id`: `AddSplit { chain, position, end }`, `SetSplitEnd { chain, split_id, end }`, `RemoveSplit { chain, split_id }`, `SetChainEndpointEnabled { chain, node, io, endpoint, enabled }`. Every other contract name is used as-is. The new `path` fields carry `#[serde(default, skip_serializing_if = "Option::is_none")]` — the contract's `#[serde(default)]` plus the skip, so a path-less command serializes byte-identical to today's wire format.

**`EndpointRef` name clash:** `project` already has `endpoint_ref::EndpointRef { binding_id, endpoint }` (the looper's, re-exported as `application::command::EndpointRef`). The contract's checklist ref is `project::endpoint_disables::EndpointRef { io, endpoint }`. This part always imports the checklist one by its full path and never glob-imports both.

## Global Constraints

- **Work only in** `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328` (a `git clone`, branch `feature/issue-328`). Never touch the main folder; every git command is `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 …`. `git worktree` is forbidden.
- **File caps:** `.rs` 600 lines, `.slint` 500 lines. Test files (`*_tests.rs`, `*_tests_*.rs`, `tests/`) are the only exemption.
- **One file, one responsibility.** Every production file declares it in its first 20 lines as `//! Responsibility: <one sentence>`. `scripts/validate.sh` check 1 rejects a missing header and a sentence containing ` and `, ` e `, ` plus `, ` also `, `,`, `;`, `+`, `&` or `/`. A new responsibility goes into a new file, never the end of an open one. `lib.rs` / `mod.rs` stay routers.
- **Zero allocation, lock, syscall or I/O on the audio thread.** Nothing in this part runs on the audio thread: `edit_chain_blocks` clones the block list on the dispatcher (UI) thread.
- **Stream isolation:** N streams = N isolated pipelines. No handler here touches a runtime; each emits an event naming exactly one chain, and the drain (`runtime_sync_policy`) re-syncs exactly that chain.
- **Volume invariants:** `crates/engine/src/volume_invariants_tests.rs` is never edited. Chains without a split behave bit-identically.
- **TDD red-first:** write the test, run it, see the ASSERTION fail and paste the `FAILED`/panic line, only then write production code. A test that passes before the change is a characterization pin and is labelled as one in its step. Never edit an existing test to make it pass (the `path: None` additions of Tasks 4–5 are compile fixes for a new struct field, not behaviour changes).
- **Zero warnings:** `cargo build --workspace` clean; no unused import, `mut`, or dead fixture.
- **Builds:** always `nice -n 19 cargo … -j 2`, one cargo process at a time.
- **Push gate (before EVERY push, CLAUDE.md "Antes de TODO push"):** `cargo fmt --all -- --check` (clean) + `cargo test --workspace` + `cargo build --workspace` (zero warnings) + `VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates`. Every task's push block runs it as one line before `git push`; a failure stops the push. Per step, the targeted `cargo test -p <crate> <filter>` shown in the task is the red/green loop; Task 11 is the final full gate of the part.
- **English** for every repo artefact (code, comments, docs, commits, issue comments).
- **Linux/JACK** fixes go behind `cfg(all(target_os = "linux", feature = "jack"))`. This part has none.
- **No regex/sed migrations:** every construction site is changed with the Edit tool, one by one.
- **Stage explicit paths** (never `git add -A`); every commit is followed by fetch + push; after each push post the `gh issue comment 328` shown in the task (CLAUDE.md gitflow).
- **Docs in the same commit** as the behaviour they describe (spec §8).
- **Commit message:** `feat(#328): …` in English, with **no `Co-Authored-By` trailer** — `.claude/skills/openrig-code-quality/SKILL.md:446` and `docs/development/gitflow.md:39` ("Commits in English, no `Co-Authored-By` trailers"); the project rule outranks the workflow default, and all six parts follow it.

## Review Focus

1. **Payloads written before #328** — an MCP client, a `midi-map.yaml` line, or GUI code that sends `AddBlock` / `InsertPrebuiltBlock` / `MoveBlock` without `path` — must keep landing at the chain's top level and serialize byte-identical. Pinned by Task 4 `add_block_without_path_serializes_exactly_as_before` + `add_block_without_a_path_still_lands_at_the_top_level`, Task 5 `move_block_without_path_serializes_exactly_as_before`.
2. **A forbidden structure reaching the project through any door** (a second split; a split, select, input, output or insert inside a path; a processing block after a Y split; `RemoveBlock` on the split silently dropping both paths) must be refused with the chain untouched — otherwise the saved project no longer reopens. Pinned by Task 3 `remove_block_refuses_the_split_and_leaves_the_chain_untouched` / `overwrite_block_refuses_to_put_an_input_port_inside_a_path`, Task 4 `insert_prebuilt_block_refuses_a_second_split` / `add_block_refuses_a_processing_block_after_a_y_split`, Task 5 `move_block_refuses_to_put_the_split_inside_its_own_path`, Task 6 `add_split_refuses_a_second_split`, Task 8 `configure_chain_refuses_a_select_inside_a_path` / `load_chain_preset_refuses_a_block_after_a_y_split`.
3. **An edit inside a path lost on save** — a block added into path A, or a new split, must reach the rig preset that `project.yaml` persists. Pinned by Task 4 `add_block_into_a_path_reaches_the_rig_preset_on_capture`, Task 6 `add_split_reaches_the_rig_preset_on_capture`.
4. **A preset or scene switch keeping the OLD split** (the rig-nav port merge treats every `is_routing()` block as a chain port, and the split is routing) — the new preset's split and the new scene's path values must win. Fixed by Part 1 Task 5; pinned end to end here by Task 10 `switching_preset_puts_the_new_presets_split_in_place_of_the_old_one` / `switching_scene_applies_the_scene_bypass_to_a_block_inside_a_path`. A model swap inside a path must keep the active scene on the live block: Task 10b `a_model_swap_inside_a_path_keeps_the_active_scene_on_the_live_block`.
5. **The endpoint checklist silently reset** by the chain editor's Save (rename / E/S change), by a scene switch, or by save + reopen. Pinned by Task 7 `the_chain_editor_save_keeps_the_checklist`, `the_checklist_survives_a_scene_switch`, `the_checklist_is_captured_into_the_rig_input`.

## Known gaps this part does not close (hand to the owning part)

- ~~Model swap of a block inside a path (`project/src/rig_model_swap.rs:37-77`)~~ — **closed by Part 1 Task 7** (`write_back_model_swaps` recurses; pinned by `a_model_swap_inside_a_path_keeps_every_scene`).
- ~~Live re-resolve after that swap (`application/src/local_dispatcher_model_swap_rig.rs:27-48`)~~ — **closed by Task 10b** below (both lookups go through Part 1's recursive `find_block_mut`).
- **Rule source of truth.** `split_rules::ensure_split_rules` re-states spec §1.1. If Part 1's `split_block_methods.rs` exposes a block-list rule check, make `ensure_split_rules` delegate to it (keeping this part's error texts, which the tests pin) so the command layer and `rig_validate` cannot drift.
- **Flat-index addressing** (`SelectChainBlock`, MIDI block nav, `SaveChainInputEndpoints { block_index }`) still sees only the top level; Part 6 (Tasks 2–3) moves the GUI to id-based addressing. The MIDI block cursor inside a path stays open (see the plan README, "Open items").

---

## File Structure

Line counts are as of the recon (branch head before Part 1). "new" = created by this part.

| File | Responsibility (header sentence) | Lines | Tasks |
|---|---|---|---|
| `crates/application/src/block_path.rs` | addresses blocks inside a chain's split paths | new | 1, 2, 3, 4 |
| `crates/application/src/split_rules.rs` | says whether a chain's block list obeys the split rules | new | 3 |
| `crates/application/src/local_dispatcher_block_draft.rs` | commits a block-list edit only when the result obeys the split rules | new | 3 |
| `crates/application/src/local_dispatcher_split.rs` | handles the split lifecycle commands | new | 6 |
| `crates/application/src/local_dispatcher_chain_endpoints.rs` | handles the chain endpoint checklist command | new | 7 |
| `crates/application/src/command/split.rs` | names the split lifecycle commands | new | 6 |
| `crates/application/src/validate_split.rs` | resolves the bus layout a split hands to the block after it | new | 9 |
| `crates/project/src/endpoint_prune.rs` | drops the checklist refs a rig input's bindings no longer offer | new | 7b |
| `crates/application/src/local_dispatcher_access.rs` | borrows a chain out of the dispatcher state (unchanged) | 59 | 1 |
| `crates/project/src/block/param_writer.rs` | writes one parameter value into a block (unchanged) | 128 | 1 |
| `crates/application/src/query_block_params.rs` | reports the parameters one block exposes (unchanged) | 30 | 2 |
| `crates/application/src/query_ids.rs` | lists the ids a transport needs to address the project (unchanged) | 34 | 2 |
| `crates/application/src/query.rs` | routes the read-only project introspection surface (router) | 34 | 2 |
| `crates/application/src/local_dispatcher_block_edit.rs` | handles the block edit commands (unchanged) | 71 | 3, 5 |
| `crates/application/src/local_dispatcher_block_lifecycle.rs` | handles the block lifecycle commands (unchanged) | 148 | 4 |
| `crates/application/src/command/block.rs` | names the block-scoped commands (unchanged) | 128 | 4, 5 |
| `crates/application/src/command/chain.rs` | names the chain-scoped commands (unchanged) | 226 | 7 |
| `crates/application/src/command.rs` | names every state change any controller can ask for (unchanged) | 128 | 6 |
| `crates/application/src/local_dispatcher_trait.rs` | routes an incoming command to the handler that owns it (unchanged) | 438 | 6, 7 |
| `crates/application/src/local_dispatcher_chain_crud.rs` | handles the chain crud commands (unchanged) | 216 | 8 |
| `crates/application/src/local_dispatcher_chain_save.rs` | handles the chain save commands (unchanged) | 244 | 7, 8 |
| `crates/application/src/local_dispatcher_chain_io.rs` | handles the chain io commands (unchanged) | 150 | 8 |
| `crates/application/src/validate.rs` | says whether a project is internally consistent (unchanged) | 203 | 9 |
| `crates/application/src/local_dispatcher_rig.rs` | handles the rig commands (unchanged; Task 7b calls the prune in `handle_capture_rig_edits`, Task 10 adds a test mount — the ports-only predicate is Part 1 Task 5) | 190 | 7b, 10 |
| `crates/application/src/local_dispatcher_model_swap_rig.rs` | mirrors a block model swap on a rig chain into the attached rig (unchanged) | 50 | 10b |
| `crates/application/tests/issue_328_model_swap_in_path.rs` | tests | new | 10b |
| `crates/project/src/rig_sync.rs` | captures the projected chains back into the rig (not edited here: Part 1 Task 10 already captures `disabled_endpoints`; Task 7 only pins it) | 104 | — |
| `crates/application/src/lib.rs` | routes the application crate's public surface (router) | 160 | 1, 3, 6, 7, 8 |
| `crates/adapter-gui/src/block_choose_type_callback.rs` | (unchanged; `path: None` only) | 422 | 4 |
| `crates/adapter-gui/src/block_editor_persist.rs` | (unchanged; `path: None` only) | 343 | 4 |
| `crates/adapter-gui/src/block_reorder.rs` | (unchanged; `path: None` only) | 99 | 5 |
| `crates/adapter-gui/src/compact_chain_block_delete.rs` | (unchanged; `path: None` only) | 234 | 5 |
| `crates/application/src/split_tests_fixtures.rs` | test fixtures | new | 1, 3, 4 |
| `crates/application/src/ld_split_path_tests.rs` | tests | new | 1, 3, 4, 5 |
| `crates/application/src/query_split_tests.rs` | tests | new | 2 |
| `crates/application/src/local_dispatcher_split_tests.rs` | tests | new | 6 |
| `crates/application/src/local_dispatcher_chain_endpoints_tests.rs` | tests | new | 7 |
| `crates/application/src/ld_split_chain_doors_tests.rs` | tests | new | 8 |
| `crates/application/src/validate_tests_split.rs` | tests | new | 9 |
| `crates/application/src/validate_tests.rs` | test orchestrator | 16 | 9 |
| `crates/application/src/local_dispatcher_rig_split_tests.rs` | tests | new | 10 |
| `crates/application/tests/issue_328_block_path_wire.rs` | tests | new | 4, 5 |
| `crates/application/tests/issue_14_command_grouping_preserves_surface.rs` | tests | 247 | 6, 7 |
| `crates/adapter-mcp/src/tools_tests.rs` | tests | 162 | 4, 6, 7 |
| `crates/application/src/ld_block2_tests.rs`, `ld_insert_tests.rs`, `tests/scene_output_preservation_exhaustive.rs`, adapter-gui `*_tests.rs` listed in Tasks 4–5 | tests (`path: None` only) | — | 4, 5 |
| `docs/mcp.md` | docs | 252 | 1–8 |
| `docs/midi.md` | docs | 316 | 4, 5 |

---

### Task 1: Block commands reach blocks inside a path; the split's knobs are block parameters

**Files:**
- Create: `crates/application/src/block_path.rs`
- Create: `crates/application/src/split_tests_fixtures.rs` (test fixtures)
- Create: `crates/application/src/ld_split_path_tests.rs` (tests)
- Modify: `crates/application/src/local_dispatcher_access.rs:15-39` (`with_block`)
- Modify: `crates/project/src/block/param_writer.rs:12-14` (doc) and `:113-122` (`params_mut`)
- Modify: `crates/application/src/lib.rs:14` (after `pub mod block_factory;`) and end of file
- Modify: `docs/mcp.md:66` (after the `RuntimeControl` paragraph)

**Interfaces:**
- Consumes (Part 1): `AudioBlockKind::Split(SplitBlock { end, params, a, b })`, `SplitEnd::{Mix, Y}`, `default_split_params()`, `SPLIT_MODE`, `MIX_PAN_A`, `MIX_MASTER_SUM`.
- Produces:
  - `crate::block_path::find_block_mut<'a>(blocks: &'a mut [AudioBlock], id: &BlockId) -> Option<&'a mut AudioBlock>`
  - fixtures (`#[cfg(test)]`, crate-visible): `CHAIN: &str = "chain_0"`, `split(id: &str, end: SplitEnd, a: Vec<AudioBlock>, b: Vec<AudioBlock>) -> AudioBlock`, `mix_chain() -> Vec<AudioBlock>` (`[pre, split_0 → Mix { a: [a_0], b: [b_0] }, post]`), `project_with(blocks: Vec<AudioBlock>) -> Rc<RefCell<Project>>`, `split_of(project: &Project) -> SplitBlock`.
  - `LocalDispatcher::with_block` keeps its signature; it now finds path blocks.

- [ ] **Step 0: Preflight — Part 1's types are where this plan imports them from**

Run:
```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && grep -n "pub struct SplitBlock\|pub enum SplitEnd\|derive" crates/project/src/block/split_block.rs && grep -n "pub enum PathSide\|pub struct PathRef\|derive" crates/project/src/block/path_ref.rs && grep -n "pub use path_ref::{PathRef, PathSide}" crates/project/src/block/mod.rs && grep -n "pub fn default_split_params\|pub const SPLIT_MODE\|pub const MIX_PAN_A\|pub const MIX_MASTER_SUM" crates/project/src/block/split_params.rs && grep -n "pub struct EndpointDisables\|pub enum EndpointNode\|pub struct EndpointRef\|derive" crates/project/src/endpoint_disables.rs && grep -n "pub mod split_block\|pub mod split_params" crates/project/src/block/mod.rs && grep -n "pub mod endpoint_disables" crates/project/src/lib.rs
```
Expected: every item found; `PathRef`, `PathSide`, `SplitEnd`, `EndpointNode` derive `schemars::JsonSchema`; `EndpointDisables` derives `Default`. If a type lives in another module, change only the `use` paths of this plan to match — never the type names.

- [ ] **Step 1: Write the fixtures and the failing tests**

Create `crates/application/src/split_tests_fixtures.rs`:
```rust
//! Shared fixtures for the #328 split command tests.
//!
//! One chain (`CHAIN`) whose blocks each test sets. Later tasks add a
//! one-input rig for the tests that follow an edit into `project.yaml`.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::BlockId;
use project::block::split_block::{SplitBlock, SplitEnd};
use project::block::split_params::default_split_params;
use project::block::{AudioBlock, AudioBlockKind};
use project::project::Project;

use crate::local_dispatcher_tests::{make_core_block, make_project};

/// The chain every non-rig split test edits.
pub(crate) const CHAIN: &str = "chain_0";

/// A split block carrying the default knobs of spec §1.2.
pub(crate) fn split(id: &str, end: SplitEnd, a: Vec<AudioBlock>, b: Vec<AudioBlock>) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end,
            params: default_split_params(),
            a,
            b,
        }),
    }
}

/// `[pre, split_0 → Mix { a: [a_0], b: [b_0] }, post]`.
pub(crate) fn mix_chain() -> Vec<AudioBlock> {
    vec![
        make_core_block("pre", true),
        split(
            "split_0",
            SplitEnd::Mix,
            vec![make_core_block("a_0", true)],
            vec![make_core_block("b_0", true)],
        ),
        make_core_block("post", true),
    ]
}

/// A project whose only chain, `CHAIN`, holds exactly `blocks`.
pub(crate) fn project_with(blocks: Vec<AudioBlock>) -> Rc<RefCell<Project>> {
    let project = make_project(CHAIN, make_core_block("seed", true));
    project.borrow_mut().chains[0].blocks = blocks;
    project
}

/// The split of the project's first chain (panics when it has none).
pub(crate) fn split_of(project: &Project) -> SplitBlock {
    project.chains[0]
        .blocks
        .iter()
        .find_map(|b| match &b.kind {
            AudioBlockKind::Split(split) => Some(split.clone()),
            _ => None,
        })
        .expect("the first chain holds a split")
}
```

Create `crates/application/src/ld_split_path_tests.rs`:
```rust
//! #328 — block commands address blocks inside a split's paths.
//!
//! Every test drives the dispatcher the way a transport does. Fixtures live in
//! `split_tests_fixtures.rs`.

use project::block::split_params::{MIX_MASTER_SUM, MIX_PAN_A, SPLIT_MODE};

use crate::local_dispatcher_tests::*;
use crate::split_tests_fixtures::*;

#[test]
fn set_block_parameter_number_reaches_a_block_inside_path_b() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::Block(BlockCommand::SetBlockParameterNumber {
            chain: ChainId(CHAIN.into()),
            block: BlockId("b_0".into()),
            path: "gain".into(),
            value: 0.25,
        }))
        .expect("a block inside path B is addressable by id");

    let split = split_of(&project.borrow());
    let AudioBlockKind::Core(core) = &split.b[0].kind else {
        panic!("b_0 is a core block");
    };
    assert_eq!(core.params.get_f32("gain"), Some(0.25));
}

#[test]
fn toggle_block_enabled_reaches_a_block_inside_path_a() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::Block(BlockCommand::ToggleBlockEnabled {
            chain: ChainId(CHAIN.into()),
            block: BlockId("a_0".into()),
        }))
        .expect("a block inside path A is addressable by id");

    assert!(
        !split_of(&project.borrow()).a[0].enabled,
        "a_0 was switched off"
    );
}

#[test]
fn set_block_parameter_number_writes_a_split_mixer_knob() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::Block(BlockCommand::SetBlockParameterNumber {
            chain: ChainId(CHAIN.into()),
            block: BlockId("split_0".into()),
            path: MIX_PAN_A.into(),
            value: -50.0,
        }))
        .expect("the split's knobs are ordinary block parameters");

    assert_eq!(
        split_of(&project.borrow()).params.get_f32(MIX_PAN_A),
        Some(-50.0)
    );
}

#[test]
fn select_block_parameter_option_switches_the_split_mode() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::Block(BlockCommand::SelectBlockParameterOption {
            chain: ChainId(CHAIN.into()),
            block: BlockId("split_0".into()),
            path: SPLIT_MODE.into(),
            value: "dual_mono".into(),
            index: 1,
        }))
        .expect("split_mode is an option parameter of the split");

    assert_eq!(
        split_of(&project.borrow()).params.get_string(SPLIT_MODE),
        Some("dual_mono")
    );
}

#[test]
fn set_block_parameter_bool_turns_on_the_master_sum() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::Block(BlockCommand::SetBlockParameterBool {
            chain: ChainId(CHAIN.into()),
            block: BlockId("split_0".into()),
            path: MIX_MASTER_SUM.into(),
            value: true,
        }))
        .expect("mix_master_sum is a bool parameter of the split");

    assert_eq!(
        split_of(&project.borrow()).params.get_bool(MIX_MASTER_SUM),
        Some(true)
    );
}
```

In `crates/application/src/lib.rs`, append at the end of the file:
```rust

#[cfg(test)]
#[path = "split_tests_fixtures.rs"]
mod split_tests_fixtures;

#[cfg(test)]
#[path = "ld_split_path_tests.rs"]
mod ld_split_path;
```

- [ ] **Step 2: Run the tests, see them fail**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 ld_split_path`
Expected: 5 FAILED. The two path tests panic with `a block inside path B is addressable by id: block not found: BlockId("b_0")` (and `…path A…: block not found: BlockId("a_0")`); the three knob tests panic with `…: block kind 'split' does not carry an editable ParameterSet`. Paste the FAILED lines into the task log.

- [ ] **Step 3: Implement the path-aware lookup**

Create `crates/application/src/block_path.rs`:
```rust
//! Responsibility: addresses blocks inside a chain's split paths.
//!
//! #328: a chain's blocks are a tree one level deep — the top-level list and
//! the two paths (`a`, `b`) of its one split (spec §1.1 keeps a split out of a
//! path). Every command and read that finds, takes or places a block goes
//! through here, so a block inside a path is reachable exactly like a
//! top-level one.

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind};

/// The block with `id`, at the top level or inside a split path.
pub fn find_block_mut<'a>(
    blocks: &'a mut [AudioBlock],
    id: &BlockId,
) -> Option<&'a mut AudioBlock> {
    blocks.iter_mut().find_map(|block| {
        if block.id == *id {
            return Some(block);
        }
        match &mut block.kind {
            AudioBlockKind::Split(split) => split
                .a
                .iter_mut()
                .chain(split.b.iter_mut())
                .find(|inner| inner.id == *id),
            _ => None,
        }
    })
}
```

In `crates/application/src/lib.rs`, after `pub mod block_factory;` add:
```rust
mod block_path;
```

In `crates/application/src/local_dispatcher_access.rs`, add after `use crate::local_dispatcher::LocalDispatcher;`:
```rust
use crate::block_path::find_block_mut;
```
and replace the `with_block` doc + body (lines 15-39) with:
```rust
    /// Borrow the project mutably, locate `chain` then `block`, and run `f`
    /// against the located block. Centralises the chain-not-found /
    /// block-not-found lookup that every block-scoped arm performed inline.
    ///
    /// `pub(crate)` so the per-feature `handle_*` modules
    /// (`local_dispatcher_block_*`, `local_dispatcher_chain_*`) can share it.
    ///
    /// #328: the block is found at the top level or inside a split path
    /// (`block_path::find_block_mut`), so every block-scoped command reaches
    /// a path block by its id. The error strings are unchanged.
    pub(crate) fn with_block<R>(
        &self,
        chain: &ChainId,
        block: &BlockId,
        f: impl FnOnce(&mut project::block::AudioBlock) -> Result<R>,
    ) -> Result<R> {
        let mut proj = self.project.borrow_mut();
        let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == *chain) else {
            return Err(anyhow::anyhow!("chain not found: {:?}", chain));
        };
        let Some(target_block) = find_block_mut(&mut target_chain.blocks, block) else {
            return Err(anyhow::anyhow!("block not found: {:?}", block));
        };
        f(target_block)
    }
```

- [ ] **Step 4: Run, the path tests pass and the knob tests still fail**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 ld_split_path`
Expected: `set_block_parameter_number_reaches_a_block_inside_path_b` and `toggle_block_enabled_reaches_a_block_inside_path_a` PASS; the three knob tests FAIL with `block kind 'split' does not carry an editable ParameterSet` (if Part 1 already added the `Split` arm to `params_mut`, they PASS here — record that and skip Step 5).

- [ ] **Step 5: Make the split's knobs writable**

In `crates/project/src/block/param_writer.rs`, replace lines 12-14:
```rust
//! Only `Core` and `Nam` block kinds carry a `ParameterSet`; the other kinds
//! (`Input`, `Output`, `Insert`, `Select`) do not expose editable parameters
//! through these commands.
```
with:
```rust
//! `Core`, `Nam` and `Split` (#328: the split and mixer knobs of spec §1.2)
//! carry a `ParameterSet`; `Input`, `Output`, `Insert` and `Select` do not
//! expose editable parameters through these commands.
```
and in `params_mut` (lines 113-122) add the `Split` arm before `other =>`:
```rust
fn params_mut(block: &mut AudioBlock) -> Result<&mut block_core::param::ParameterSet> {
    match &mut block.kind {
        AudioBlockKind::Core(core) => Ok(&mut core.params),
        AudioBlockKind::Nam(nam) => Ok(&mut nam.params),
        // #328: the split's own knobs ride the same commands, so MIDI mapping,
        // scenes and MCP reach them with no new command.
        AudioBlockKind::Split(split) => Ok(&mut split.params),
        other => Err(anyhow!(
            "block kind '{}' does not carry an editable ParameterSet",
            other.label()
        )),
    }
}
```

- [ ] **Step 6: Run, all pass**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 ld_split_path && nice -n 19 cargo test -p project -j 2 param_writer`
Expected: 5 passed in `ld_split_path`; every `param_writer` test still passes.

- [ ] **Step 7: Document it in `docs/mcp.md`**

In `docs/mcp.md`, replace:
```
  sidecars, which only the GUI's own Save did before. See
  `docs/architecture.md` → "Write bus: `RuntimeControl`".
```
with:
```
  sidecars, which only the GUI's own Save did before. See
  `docs/architecture.md` → "Write bus: `RuntimeControl`".

  Split chains (#328): a chain may hold one split, with a path `a` and a
  path `b` running side by side. Every tool that finds a block by id
  (`set_block_parameter_*`, `toggle_block_enabled`,
  `replace_block_model`, …) also reaches the blocks inside a path. The
  split's own knobs (`split_mode`, `level_to_a`, `mix_pan_b`,
  `mix_master_sum`, …) are ordinary parameters of the split's block id.
```

- [ ] **Step 8: Commit and push**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && rustfmt --edition 2021 crates/application/src/block_path.rs crates/application/src/split_tests_fixtures.rs crates/application/src/ld_split_path_tests.rs crates/application/src/local_dispatcher_access.rs crates/project/src/block/param_writer.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/application/src/block_path.rs crates/application/src/split_tests_fixtures.rs crates/application/src/ld_split_path_tests.rs crates/application/src/local_dispatcher_access.rs crates/application/src/lib.rs crates/project/src/block/param_writer.rs docs/mcp.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): block commands reach blocks inside a split path; split knobs are block parameters"
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch origin
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status -sb
```
If `status` shows `behind`, run `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase origin feature/issue-328` and re-run Step 6. Then:
```bash
# CLAUDE.md push gate: fmt clean, every `test result: ok.`, no build warning/error line, no `✗ FAIL`. Push only if this line exits 0.
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 2 / Task 1 pushed ($(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD)): block commands reach path blocks; split knobs via SetBlockParameter*. Files: application block_path.rs, local_dispatcher_access.rs; project param_writer.rs; docs/mcp.md. cargo test -p application ld_split_path: 5 passed."
```

---

### Task 2: Reads reach blocks inside a path (MCP read parity)

**Files:**
- Modify: `crates/application/src/block_path.rs` (add `find_block`)
- Modify: `crates/application/src/query_block_params.rs:21-25`
- Modify: `crates/application/src/query_ids.rs:1-34`
- Modify: `crates/application/src/query.rs` (attach the test module)
- Create: `crates/application/src/query_split_tests.rs` (tests)
- Modify: `docs/mcp.md:70` and `:127-130`

**Interfaces:**
- Consumes: Task 1 fixtures `project_with`, `mix_chain`, `split`, `CHAIN`; `crate::block_factory::build_default_block(BlockId, &str, &str) -> Result<AudioBlock>`.
- Produces: `crate::block_path::find_block<'a>(blocks: &'a [AudioBlock], id: &BlockId) -> Option<&'a AudioBlock>`. `list_ids` prints each path block as `    path <a|b>  block <id>  <label>  <enabled|disabled>` under its split.

- [ ] **Step 1: Write the failing tests**

Create `crates/application/src/query_split_tests.rs`:
```rust
//! #328 — the read side reaches the blocks inside a split's paths, so an MCP
//! client (`openrig://ids`, the block-params resource) sees what the GUI shows.

use domain::ids::{BlockId, ChainId};
use project::block::split_block::SplitEnd;

use crate::block_factory::build_default_block;
use crate::query::{get_block_params, list_ids};
use crate::split_tests_fixtures::{mix_chain, project_with, split, CHAIN};

#[test]
fn get_block_params_reaches_a_block_inside_a_split_path() {
    let fuzz = build_default_block(BlockId("a_fuzz".into()), "gain", "fuzz_ge")
        .expect("fuzz_ge is a shipped gain model");
    let project = project_with(vec![split("split_0", SplitEnd::Mix, vec![fuzz], vec![])]);

    let json = get_block_params(
        &project.borrow(),
        &ChainId(CHAIN.into()),
        &BlockId("a_fuzz".into()),
    )
    .expect("a block inside path A is readable by id");

    assert!(json.starts_with("{\"params\":"), "{json}");
}

#[test]
fn list_ids_lists_the_blocks_inside_each_split_path() {
    let project = project_with(mix_chain());

    let out = list_ids(&project.borrow());

    assert!(out.contains("  block split_0  split  enabled"), "{out}");
    assert!(out.contains("    path a  block a_0  core  enabled"), "{out}");
    assert!(out.contains("    path b  block b_0  core  enabled"), "{out}");
}
```

In `crates/application/src/query.rs`, after the `plugin_params_tests` module (line 26) add:
```rust

#[cfg(test)]
#[path = "query_split_tests.rs"]
mod split_tests;
```

- [ ] **Step 2: Run, see them fail**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 query::split_tests`
Expected: 2 FAILED — `a block inside path A is readable by id: "block not found in chain chain_0: a_fuzz"`, and the `list_ids` assertion printing an output with no `path a` row.

- [ ] **Step 3: Implement**

Append to `crates/application/src/block_path.rs`:
```rust

/// Read-only twin of [`find_block_mut`].
pub fn find_block<'a>(blocks: &'a [AudioBlock], id: &BlockId) -> Option<&'a AudioBlock> {
    blocks.iter().find_map(|block| {
        if block.id == *id {
            return Some(block);
        }
        match &block.kind {
            AudioBlockKind::Split(split) => split
                .a
                .iter()
                .chain(&split.b)
                .find(|inner| inner.id == *id),
            _ => None,
        }
    })
}
```

In `crates/application/src/query_block_params.rs`, add to the doc comment (after line 10, `/// block / schema mismatch → `Err`.`):
```rust
/// #328: a block inside a split path is found by its id like any other.
```
and replace lines 21-25:
```rust
    let block_ref = chain_ref
        .blocks
        .iter()
        .find(|b| b.id == *block)
        .ok_or_else(|| format!("block not found in chain {}: {}", chain.0, block.0))?;
```
with:
```rust
    let block_ref = crate::block_path::find_block(&chain_ref.blocks, block)
        .ok_or_else(|| format!("block not found in chain {}: {}", chain.0, block.0))?;
```

Replace the whole body of `crates/application/src/query_ids.rs` with:
```rust
//! Responsibility: lists the ids a transport needs to address the project.

use project::block::AudioBlockKind;
use project::project::Project;
use std::fmt::Write;

/// Human-readable, copy-paste-ready listing of every chain and block with
/// its full ID, instrument/kind, and enabled state — the values that go
/// into `midi-map.yaml` `chain:` / `block:`. #328: the blocks inside a
/// split's paths are listed under the split, one `path a` / `path b` row
/// each, so they can be addressed too.
pub fn list_ids(project: &Project) -> String {
    let mut out = String::new();
    let name = project.name.as_deref().unwrap_or("(unnamed)");
    let _ = writeln!(out, "project: {name}");
    if project.chains.is_empty() {
        out.push_str("(no chains)\n");
        return out;
    }
    for chain in &project.chains {
        let state = if chain.enabled { "enabled" } else { "disabled" };
        let _ = writeln!(
            out,
            "chain {}  instrument={}  {}",
            chain.id.0, chain.instrument, state
        );
        if chain.blocks.is_empty() {
            out.push_str("  (no blocks)\n");
        }
        for b in &chain.blocks {
            let _ = writeln!(
                out,
                "  block {}  {}  {}",
                b.id.0,
                b.kind.label(),
                block_state(b.enabled)
            );
            if let AudioBlockKind::Split(split) = &b.kind {
                for (side, lane) in [("a", &split.a), ("b", &split.b)] {
                    for p in lane {
                        let _ = writeln!(
                            out,
                            "    path {side}  block {}  {}  {}",
                            p.id.0,
                            p.kind.label(),
                            block_state(p.enabled)
                        );
                    }
                }
            }
        }
    }
    let _ = writeln!(out, "(chains: {})", project.chains.len());
    out
}

fn block_state(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}
```

- [ ] **Step 4: Run, all pass (incl. the existing ids/params tests)**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 query`
Expected: all `query::*` tests pass, including `lists_full_ids_for_chains_and_blocks` and `marks_disabled_block_and_empty_chain` unchanged.

- [ ] **Step 5: Document it**

In `docs/mcp.md`, replace:
```
  - `openrig://ids` — chain/block IDs (for `midi-map.yaml`).
```
with:
```
  - `openrig://ids` — chain/block IDs (for `midi-map.yaml`). The blocks
    inside a split (#328) are listed under it as `path a` / `path b` rows.
```
and replace:
```
    (JSON, wrapped under a `params` envelope). Unknown chain / block
    → error from the bridge.
```
with:
```
    (JSON, wrapped under a `params` envelope). A block inside a split
    path (#328) is addressed by its id like any other. Unknown chain /
    block → error from the bridge.
```

- [ ] **Step 6: Commit and push**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && rustfmt --edition 2021 crates/application/src/block_path.rs crates/application/src/query_block_params.rs crates/application/src/query_ids.rs crates/application/src/query.rs crates/application/src/query_split_tests.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/application/src/block_path.rs crates/application/src/query_block_params.rs crates/application/src/query_ids.rs crates/application/src/query.rs crates/application/src/query_split_tests.rs docs/mcp.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): openrig://ids and block params reach blocks inside a split path"
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch origin
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status -sb
```
If behind: `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase origin feature/issue-328` and re-run Step 4. Then:
```bash
# CLAUDE.md push gate: fmt clean, every `test result: ok.`, no build warning/error line, no `✗ FAIL`. Push only if this line exits 0.
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 2 / Task 2 pushed ($(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD)): openrig://ids + block-params read path blocks. Files: block_path.rs, query_block_params.rs, query_ids.rs, docs/mcp.md. cargo test -p application query: green."
```

---

### Task 3: Structural edits are rule-checked drafts — RemoveBlock and OverwriteBlock

**Files:**
- Create: `crates/application/src/split_rules.rs`
- Create: `crates/application/src/local_dispatcher_block_draft.rs`
- Modify: `crates/application/src/block_path.rs` (add `remove_block`)
- Modify: `crates/application/src/local_dispatcher_block_edit.rs:1-38`
- Modify: `crates/application/src/lib.rs` (two `mod` lines)
- Modify: `crates/application/src/split_tests_fixtures.rs` (add `ids`)
- Test: `crates/application/src/ld_split_path_tests.rs`
- Modify: `docs/mcp.md`

**Interfaces:**
- Consumes: Task 1 `find_block_mut`, fixtures.
- Produces:
  - `crate::split_rules::ensure_split_rules(blocks: &[AudioBlock]) -> anyhow::Result<()>` — errors: `"a chain holds at most one split"`, `"split '<id>' path <A|B>: a '<label>' block cannot sit inside a path"`, `"nothing may follow a Y split: block '<id>' sits after split '<id>'"`.
  - `LocalDispatcher::edit_chain_blocks<R>(&self, chain: &ChainId, edit: impl FnOnce(&mut Vec<AudioBlock>) -> anyhow::Result<R>) -> anyhow::Result<R>` (`pub(crate)`).
  - `crate::block_path::remove_block(blocks: &mut Vec<AudioBlock>, id: &BlockId) -> Option<AudioBlock>`.
  - fixture `ids(blocks: &[AudioBlock]) -> Vec<String>`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/application/src/split_tests_fixtures.rs`:
```rust

/// The ids of `blocks`, in order.
pub(crate) fn ids(blocks: &[AudioBlock]) -> Vec<String> {
    blocks.iter().map(|b| b.id.0.clone()).collect()
}
```

Append to `crates/application/src/ld_split_path_tests.rs`:
```rust

#[test]
fn remove_block_takes_a_block_out_of_path_a() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatcher
        .dispatch(Command::Block(BlockCommand::RemoveBlock {
            chain: ChainId(CHAIN.into()),
            block: BlockId("a_0".into()),
        }))
        .expect("a block inside path A can be removed");

    assert!(split_of(&project.borrow()).a.is_empty(), "path A is empty");
    assert_eq!(
        ids(&project.borrow().chains[0].blocks),
        vec!["pre", "split_0", "post"]
    );
    assert_eq!(
        events,
        vec![Event::BlockRemoved {
            chain: ChainId(CHAIN.into()),
            block: BlockId("a_0".into()),
        }]
    );
}

#[test]
fn remove_block_refuses_the_split_and_leaves_the_chain_untouched() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatcher
        .dispatch(Command::Block(BlockCommand::RemoveBlock {
            chain: ChainId(CHAIN.into()),
            block: BlockId("split_0".into()),
        }))
        .expect_err("RemoveBlock must not drop both paths");

    assert!(err.to_string().contains("RemoveSplit"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

#[test]
fn overwrite_block_refuses_to_put_an_input_port_inside_a_path() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();
    let port = AudioBlock {
        id: BlockId("ignored".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
        }),
    };

    let err = dispatcher
        .dispatch(Command::Block(BlockCommand::OverwriteBlock {
            chain: ChainId(CHAIN.into()),
            block: BlockId("b_0".into()),
            replacement: port,
        }))
        .expect_err("an input port cannot sit inside a path");

    assert!(err.to_string().contains("path B"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

/// Characterization pin (green since Task 1): an ordinary overwrite inside a
/// path keeps working once OverwriteBlock goes through the rule-checked draft.
#[test]
fn overwrite_block_replaces_a_block_inside_path_b() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::Block(BlockCommand::OverwriteBlock {
            chain: ChainId(CHAIN.into()),
            block: BlockId("b_0".into()),
            replacement: make_core_block("ignored", false),
        }))
        .expect("a core block may replace a core block inside a path");

    let split = split_of(&project.borrow());
    assert_eq!(split.b[0].id.0, "b_0", "the original id is kept");
    assert!(!split.b[0].enabled, "the replacement's state landed");
}
```

- [ ] **Step 2: Run, see them fail**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 ld_split_path`
Expected FAILED:
- `remove_block_takes_a_block_out_of_path_a` — `a block inside path A can be removed: block not found: BlockId("a_0")`.
- `remove_block_refuses_the_split_and_leaves_the_chain_untouched` — `RemoveBlock must not drop both paths: [BlockRemoved { … }]`.
- `overwrite_block_refuses_to_put_an_input_port_inside_a_path` — `an input port cannot sit inside a path: [BlockReplaced { … }]`.
- `overwrite_block_replaces_a_block_inside_path_b` PASSES (characterization pin).

- [ ] **Step 3: Implement the rules, the draft helper, `remove_block`, and route the two handlers through them**

Create `crates/application/src/split_rules.rs`:
```rust
//! Responsibility: says whether a chain's block list obeys the split rules.
//!
//! #328 (spec §1.1): a chain holds at most one split; a path holds only
//! processing blocks (never a split, select, input, output or insert); a Y
//! split is the chain's last processing block. Every command that reshapes a
//! chain's blocks checks its RESULT here before committing it, so no door —
//! MCP, a MIDI map, the GUI — can leave a project that fails to reopen.

use anyhow::{bail, Result};
use project::block::split_block::SplitEnd;
use project::block::{AudioBlock, AudioBlockKind};

pub fn ensure_split_rules(blocks: &[AudioBlock]) -> Result<()> {
    let mut splits = blocks
        .iter()
        .enumerate()
        .filter_map(|(at, block)| match &block.kind {
            AudioBlockKind::Split(split) => Some((at, block, split)),
            _ => None,
        });
    let Some((at, block, split)) = splits.next() else {
        return Ok(());
    };
    if splits.next().is_some() {
        bail!("a chain holds at most one split");
    }
    for (side, lane) in [("A", &split.a), ("B", &split.b)] {
        if let Some(bad) = lane.iter().find(|b| !path_accepts(&b.kind)) {
            bail!(
                "split '{}' path {side}: a '{}' block cannot sit inside a path",
                block.id.0,
                bad.kind.label()
            );
        }
    }
    if matches!(split.end, SplitEnd::Y) {
        if let Some(after) = blocks[at + 1..].iter().find(|b| is_processing(&b.kind)) {
            bail!(
                "nothing may follow a Y split: block '{}' sits after split '{}'",
                after.id.0,
                block.id.0
            );
        }
    }
    Ok(())
}

/// Only processing blocks sit inside a path. `Select` stays out so nesting is
/// one level deep (spec §1.1).
fn path_accepts(kind: &AudioBlockKind) -> bool {
    matches!(kind, AudioBlockKind::Nam(_) | AudioBlockKind::Core(_))
}

/// A port does no audio work, so a Y split may still be followed by the
/// chain's own head/tail I/O blocks (legacy per-block chains carry them).
fn is_processing(kind: &AudioBlockKind) -> bool {
    !matches!(kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_))
}
```

Create `crates/application/src/local_dispatcher_block_draft.rs`:
```rust
//! Responsibility: commits a block-list edit only when the result obeys the split rules.
//!
//! #328: the edit runs on a COPY of the chain's blocks. A refused edit — a
//! missing block, a bad path, or a result that breaks a split rule — leaves
//! the project exactly as it was, whatever the edit had already moved in the
//! copy. The copy is made on the dispatcher thread, never the audio thread.

use anyhow::Result;
use domain::ids::ChainId;
use project::block::AudioBlock;

use crate::local_dispatcher::LocalDispatcher;
use crate::split_rules::ensure_split_rules;

impl LocalDispatcher {
    pub(crate) fn edit_chain_blocks<R>(
        &self,
        chain: &ChainId,
        edit: impl FnOnce(&mut Vec<AudioBlock>) -> Result<R>,
    ) -> Result<R> {
        self.with_chain(chain, |c| {
            let mut draft = c.blocks.clone();
            let out = edit(&mut draft)?;
            ensure_split_rules(&draft)?;
            c.blocks = draft;
            Ok(out)
        })
    }
}
```

Append to `crates/application/src/block_path.rs`:
```rust

/// Take the block with `id` out of the top level or out of a split path.
pub fn remove_block(blocks: &mut Vec<AudioBlock>, id: &BlockId) -> Option<AudioBlock> {
    if let Some(at) = blocks.iter().position(|b| b.id == *id) {
        return Some(blocks.remove(at));
    }
    blocks.iter_mut().find_map(|block| match &mut block.kind {
        AudioBlockKind::Split(split) => {
            take(&mut split.a, id).or_else(|| take(&mut split.b, id))
        }
        _ => None,
    })
}

fn take(lane: &mut Vec<AudioBlock>, id: &BlockId) -> Option<AudioBlock> {
    let at = lane.iter().position(|b| b.id == *id)?;
    Some(lane.remove(at))
}
```

In `crates/application/src/lib.rs`, after `mod local_dispatcher_attach;` add:
```rust
mod local_dispatcher_block_draft;
```
and after `pub mod snapshot;` add:
```rust
mod split_rules;
```

In `crates/application/src/local_dispatcher_block_edit.rs`, replace lines 1-38 (header, imports, the `OverwriteBlock` and `RemoveBlock` arms) with:
```rust
//! Responsibility: handles the block edit commands.
//! Block-edit handler (file-per-feature; #436 dispatcher split). #328: every
//! structural edit runs on a rule-checked draft (`edit_chain_blocks`), so a
//! refused edit leaves the chain untouched.

use anyhow::Result;

use project::block::AudioBlockKind;

use crate::block_path::{find_block_mut, remove_block};
use crate::command::{BlockCommand, Command};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Block-edit commands: overwrite/remove/move/insert-config.
    pub(crate) fn handle_block_edit(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::Block(BlockCommand::OverwriteBlock {
                chain,
                block,
                mut replacement,
            }) => {
                self.edit_chain_blocks(&chain, |blocks| {
                    let Some(b) = find_block_mut(blocks, &block) else {
                        return Err(anyhow::anyhow!("block not found: {:?}", block));
                    };
                    // Preserve the original block id; replace kind and enabled.
                    replacement.id = block.clone();
                    *b = replacement;
                    Ok(())
                })?;
                Ok(vec![Event::BlockReplaced { chain, block }])
            }
            Command::Block(BlockCommand::RemoveBlock { chain, block }) => {
                self.edit_chain_blocks(&chain, |blocks| {
                    let Some(removed) = remove_block(blocks, &block) else {
                        return Err(anyhow::anyhow!("block not found: {:?}", block));
                    };
                    if matches!(removed.kind, AudioBlockKind::Split(_)) {
                        return Err(anyhow::anyhow!(
                            "block {:?} is a split: remove it with RemoveSplit, which keeps \
                             path A (RemoveBlock would drop both paths)",
                            block
                        ));
                    }
                    Ok(())
                })?;
                Ok(vec![Event::BlockRemoved { chain, block }])
            }
```
(The `MoveBlock` and `SaveInsertBlock` arms below stay as they are until Task 5.)

- [ ] **Step 4: Run, all pass (incl. the existing block tests)**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 ld_`
Expected: every `ld_split_path::*` passes (9 tests), and the existing `ld_block2::remove_block_*` / `overwrite_block_*` tests pass unchanged.

- [ ] **Step 5: Document it**

In `docs/mcp.md`, replace:
```
  split's own knobs (`split_mode`, `level_to_a`, `mix_pan_b`,
  `mix_master_sum`, …) are ordinary parameters of the split's block id.
```
with:
```
  split's own knobs (`split_mode`, `level_to_a`, `mix_pan_b`,
  `mix_master_sum`, …) are ordinary parameters of the split's block id.

  An edit that would break a split rule — a second split; a split,
  select, input, output or insert block inside a path; a processing
  block after a Y split — is refused and leaves the chain exactly as it
  was. `remove_block` refuses the split itself, because removing it that
  way would drop both paths.
```

- [ ] **Step 6: Commit and push**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && rustfmt --edition 2021 crates/application/src/split_rules.rs crates/application/src/local_dispatcher_block_draft.rs crates/application/src/block_path.rs crates/application/src/local_dispatcher_block_edit.rs crates/application/src/split_tests_fixtures.rs crates/application/src/ld_split_path_tests.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/application/src/split_rules.rs crates/application/src/local_dispatcher_block_draft.rs crates/application/src/block_path.rs crates/application/src/local_dispatcher_block_edit.rs crates/application/src/lib.rs crates/application/src/split_tests_fixtures.rs crates/application/src/ld_split_path_tests.rs docs/mcp.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): RemoveBlock and OverwriteBlock run as split-rule-checked drafts"
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch origin
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status -sb
```
If behind: `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase origin feature/issue-328` and re-run Step 4. Then:
```bash
# CLAUDE.md push gate: fmt clean, every `test result: ok.`, no build warning/error line, no `✗ FAIL`. Push only if this line exits 0.
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 2 / Task 3 pushed ($(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD)): split rules + rule-checked draft; RemoveBlock/OverwriteBlock reach path blocks, RemoveBlock refuses the split. Files: split_rules.rs, local_dispatcher_block_draft.rs, block_path.rs, local_dispatcher_block_edit.rs, docs/mcp.md. cargo test -p application ld_: green."
```

---

### Task 4: AddBlock and InsertPrebuiltBlock take a split path

**Files:**
- Modify: `crates/application/src/command/block.rs:1-11` (imports) and `:76-93` (`AddBlock`, `InsertPrebuiltBlock`)
- Modify: `crates/application/src/local_dispatcher_block_lifecycle.rs:1-12` and `:97-138`
- Modify: `crates/application/src/block_path.rs` (add `insert_block`, `lane_mut`)
- Modify: `crates/application/src/split_tests_fixtures.rs` (rig + JSON fixtures)
- Test: `crates/application/src/ld_split_path_tests.rs`, create `crates/application/tests/issue_328_block_path_wire.rs`, `crates/adapter-mcp/src/tools_tests.rs`
- Modify (compile fix, `path: None` after the listed `position…,` line): production `crates/adapter-gui/src/block_choose_type_callback.rs:176`, `:243`; `crates/adapter-gui/src/block_editor_persist.rs:317`. Tests: `crates/adapter-gui/src/scene_param_persistence_tests.rs:80`; `crates/adapter-gui/src/issue_966_compact_scene_refresh_tests.rs:72`; `crates/adapter-gui/src/issue_690_nam_gate_persistence_tests.rs:114`; `crates/adapter-gui/src/project_admin_nam_tests.rs:30, :89, :145, :203, :386, :452`; `crates/adapter-gui/src/project_admin_persistence_tests.rs:427, :455, :492, :501, :549`; `crates/application/tests/scene_output_preservation_exhaustive.rs:239`; `crates/application/src/ld_block2_tests.rs:317, :349, :375, :395`; `crates/application/src/ld_insert_tests.rs:19, :44, :59`. (Line numbers as of the recon; if Part 1 shifted them, list the sites with `grep -rn "BlockCommand::AddBlock {\|BlockCommand::InsertPrebuiltBlock {" crates --include='*.rs'` — every struct literal gets the field, each edited individually.)
- Modify: `docs/mcp.md`, `docs/midi.md:56, :61, :267, :268, :299-301`

**Interfaces:**
- Consumes: `PathRef { split: BlockId, side: PathSide }`, `PathSide::{A, B}` (serialized `"a"` / `"b"`), `edit_chain_blocks`, `ensure_split_rules`.
- Produces:
  - `BlockCommand::AddBlock { chain, kind, model_id, position, path: Option<PathRef> }`, `BlockCommand::InsertPrebuiltBlock { chain, block, position, path: Option<PathRef> }` — `#[serde(default, skip_serializing_if = "Option::is_none")]` on `path`.
  - `crate::block_path::insert_block(blocks: &mut Vec<AudioBlock>, path: Option<&PathRef>, position: usize, block: AudioBlock) -> anyhow::Result<()>` — errors `"split not found: <id>"`, `"block <id> is not a split"`.
  - `AddBlock` ids are `BlockId::generate_for_chain(&chain)` (`"<chain>:block:<uuid>"`).
  - fixtures: `y_chain()`, `dispatch_json(dispatcher: &LocalDispatcher, variant: &str, args: serde_json::Value) -> anyhow::Result<Vec<Event>>`, `RIG_CHAIN = "rig:in"`, `rig_with_presets(presets: Vec<(&str, Vec<AudioBlock>)>) -> RigProject`, `rig_session_from(rig: RigProject) -> (Rc<RefCell<RigProject>>, Rc<RefCell<Project>>, LocalDispatcher)`, `preset_split(rig: &RigProject, name: &str) -> SplitBlock`.

- [ ] **Step 1: Add the fixtures**

Replace the `use` block at the top of `crates/application/src/split_tests_fixtures.rs` with:
```rust
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use domain::ids::BlockId;
use project::block::split_block::{SplitBlock, SplitEnd};
use project::block::split_params::default_split_params;
use project::block::{AudioBlock, AudioBlockKind};
use project::endpoint_disables::EndpointDisables;
use project::project::Project;
use project::rig::{RigInput, RigPreset, RigProject};

use crate::command_schema::command_from_variant;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::local_dispatcher_tests::{make_core_block, make_project};
```
and append:
```rust

/// `[pre, split_0 → Y { a: [a_0], b: [b_0] }]` — the split is last, as Y requires.
pub(crate) fn y_chain() -> Vec<AudioBlock> {
    vec![
        make_core_block("pre", true),
        split(
            "split_0",
            SplitEnd::Y,
            vec![make_core_block("a_0", true)],
            vec![make_core_block("b_0", true)],
        ),
    ]
}

/// Dispatch a command given as its wire form — the exact road an MCP tool
/// call or a `midi-map.yaml` line takes (`command_from_variant`).
pub(crate) fn dispatch_json(
    dispatcher: &LocalDispatcher,
    variant: &str,
    args: serde_json::Value,
) -> anyhow::Result<Vec<Event>> {
    dispatcher.dispatch(command_from_variant(variant, args)?)
}

/// The projected chain of the rig built by [`rig_with_presets`].
pub(crate) const RIG_CHAIN: &str = "rig:in";

/// A one-input rig (`in`, bound to the `io-main` E/S, so no I/O blocks are
/// synthesized) whose bank holds `presets` at positions 1, 2, … in order.
pub(crate) fn rig_with_presets(presets: Vec<(&str, Vec<AudioBlock>)>) -> RigProject {
    let mut bank = BTreeMap::new();
    let mut pool = BTreeMap::new();
    for (slot, (name, blocks)) in presets.into_iter().enumerate() {
        bank.insert(slot + 1, name.to_string());
        pool.insert(
            name.to_string(),
            RigPreset::from_legacy_blocks(blocks, 100.0),
        );
    }
    let input = RigInput {
        label: None,
        bank,
        active_preset: 1,
        active_scene: 1,
        routing: Vec::new(),
        instrument: "electric_guitar".to_string(),
        io: String::new(),
        endpoint: String::new(),
        io_binding_ids: vec!["io-main".to_string()],
        loopers: Vec::new(),
        disabled_endpoints: EndpointDisables::default(),
    };
    RigProject {
        name: None,
        inputs: BTreeMap::from([("in".to_string(), input)]),
        outputs: BTreeMap::new(),
        presets: pool,
        midi: None,
        chain_order: Vec::new(),
    }
}

/// The session a rig project opens into: the rig, its projected chains, and a
/// dispatcher attached to both.
pub(crate) fn rig_session_from(
    rig: RigProject,
) -> (Rc<RefCell<RigProject>>, Rc<RefCell<Project>>, LocalDispatcher) {
    let rig = Rc::new(RefCell::new(rig));
    let project = Rc::new(RefCell::new(
        engine::rig_runtime::rig_to_legacy_project(&rig.borrow(), &BTreeSet::new()),
    ));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    (rig, project, dispatcher)
}

/// The split stored in the rig preset `name` (panics when it has none).
pub(crate) fn preset_split(rig: &RigProject, name: &str) -> SplitBlock {
    rig.presets[name]
        .blocks
        .iter()
        .find_map(|b| match &b.kind {
            AudioBlockKind::Split(split) => Some(split.clone()),
            _ => None,
        })
        .expect("the preset holds a split")
}
```

- [ ] **Step 2: Write the failing tests**

Append to `crates/application/src/ld_split_path_tests.rs` (add `use serde_json::json;` and `use project::block::split_block::SplitEnd;` to its imports):
```rust

fn added_id(events: &[Event]) -> BlockId {
    events
        .iter()
        .find_map(|e| match e {
            Event::BlockAdded { block, .. } => Some(block.clone()),
            _ => None,
        })
        .expect("the command answers BlockAdded")
}

#[test]
fn add_block_with_a_path_lands_inside_that_path() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatch_json(
        &dispatcher,
        "AddBlock",
        json!({
            "chain": CHAIN, "kind": "gain", "model_id": "fuzz_ge", "position": 0,
            "path": { "split": "split_0", "side": "a" }
        }),
    )
    .expect("AddBlock into path A");

    let split = split_of(&project.borrow());
    assert_eq!(split.a.len(), 2, "path A = [new, a_0]");
    assert_eq!(split.a[1].id.0, "a_0");
    assert_eq!(added_id(&events), split.a[0].id);
    assert_eq!(
        ids(&project.borrow().chains[0].blocks),
        vec!["pre", "split_0", "post"],
        "the top level is untouched"
    );
}

/// Characterization pin (green before and after): a path-less AddBlock is a
/// top-level add, exactly as every pre-#328 payload expects.
#[test]
fn add_block_without_a_path_still_lands_at_the_top_level() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "AddBlock",
        json!({ "chain": CHAIN, "kind": "gain", "model_id": "fuzz_ge", "position": 0 }),
    )
    .expect("a top-level AddBlock");

    let blocks = project.borrow().chains[0].blocks.clone();
    assert_eq!(blocks.len(), 4);
    assert_eq!(ids(&blocks[1..]), vec!["pre", "split_0", "post"]);
    assert_eq!(split_of(&project.borrow()).a.len(), 1, "path A is untouched");
}

#[test]
fn add_block_refuses_an_input_port_inside_a_path() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatch_json(
        &dispatcher,
        "AddBlock",
        json!({
            "chain": CHAIN, "kind": "input", "model_id": "standard", "position": 0,
            "path": { "split": "split_0", "side": "b" }
        }),
    )
    .expect_err("an input port cannot sit inside a path");

    assert!(err.to_string().contains("path B"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

#[test]
fn add_block_refuses_a_processing_block_after_a_y_split() {
    let project = project_with(y_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatch_json(
        &dispatcher,
        "AddBlock",
        json!({ "chain": CHAIN, "kind": "gain", "model_id": "fuzz_ge", "position": 2 }),
    )
    .expect_err("nothing may follow a Y split");

    assert!(err.to_string().contains("Y split"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

#[test]
fn add_block_never_reuses_an_id_after_a_removal() {
    let project = project_with(vec![make_core_block("blk_0", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let add = || {
        dispatch_json(
            &dispatcher,
            "AddBlock",
            json!({ "chain": CHAIN, "kind": "gain", "model_id": "fuzz_ge", "position": 99 }),
        )
        .expect("AddBlock")
    };

    let first = added_id(&add());
    dispatcher
        .dispatch(Command::Block(BlockCommand::RemoveBlock {
            chain: ChainId(CHAIN.into()),
            block: BlockId("blk_0".into()),
        }))
        .expect("remove blk_0");
    let second = added_id(&add());

    assert_ne!(first, second, "a new block must never reuse an id in the chain");
    assert!(second.0.starts_with("chain_0:block:"), "{}", second.0);
}

#[test]
fn insert_prebuilt_block_with_a_path_lands_inside_path_b() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let block = serde_json::to_value(make_core_block("pre_b", true)).expect("block serializes");

    dispatch_json(
        &dispatcher,
        "InsertPrebuiltBlock",
        json!({
            "chain": CHAIN, "block": block, "position": 5,
            "path": { "split": "split_0", "side": "b" }
        }),
    )
    .expect("InsertPrebuiltBlock into path B");

    assert_eq!(ids(&split_of(&project.borrow()).b), vec!["b_0", "pre_b"]);
    assert_eq!(project.borrow().chains[0].blocks.len(), 3);
}

#[test]
fn insert_prebuilt_block_refuses_a_second_split() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();
    let second = serde_json::to_value(split("split_1", SplitEnd::Mix, vec![], vec![]))
        .expect("split serializes");

    let err = dispatch_json(
        &dispatcher,
        "InsertPrebuiltBlock",
        json!({ "chain": CHAIN, "block": second, "position": 0 }),
    )
    .expect_err("a chain holds one split");

    assert!(err.to_string().contains("at most one split"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

#[test]
fn add_block_into_a_path_reaches_the_rig_preset_on_capture() {
    let (rig, _project, dispatcher) = rig_session_from(rig_with_presets(vec![(
        "p1",
        vec![
            make_core_block("A", true),
            split("S", SplitEnd::Mix, vec![], vec![]),
        ],
    )]));

    dispatch_json(
        &dispatcher,
        "AddBlock",
        json!({
            "chain": RIG_CHAIN, "kind": "gain", "model_id": "fuzz_ge", "position": 0,
            "path": { "split": "S", "side": "a" }
        }),
    )
    .expect("AddBlock into path A of the rig chain");
    dispatcher
        .dispatch(Command::Project(ProjectCommand::CaptureRigEdits))
        .expect("capture");

    assert_eq!(
        preset_split(&rig.borrow(), "p1").a.len(),
        1,
        "the block added into path A must reach project.yaml"
    );
}
```

Create `crates/application/tests/issue_328_block_path_wire.rs`:
```rust
//! #328 — the optional split `path` on `AddBlock` / `InsertPrebuiltBlock` /
//! `MoveBlock` is part of the wire format MCP, MIDI maps and gRPC speak.
//!
//! Two kinds of pin: a payload written before #328 (no `path`) parses and
//! serializes back byte-identical, and a payload WITH a path keeps it.

use application::command::Command;
use serde_json::{json, Value};

fn round_trip(wire: &str) -> Value {
    let parsed: Command = serde_json::from_str(wire).expect("wire format parses");
    serde_json::to_value(&parsed).expect("Command serializes")
}

/// Characterization pin (green before and after).
#[test]
fn add_block_without_path_serializes_exactly_as_before() {
    let wire = r#"{"AddBlock":{"chain":"c1","kind":"gain","model_id":"fuzz_ge","position":0}}"#;
    let parsed: Command = serde_json::from_str(wire).expect("a pre-#328 AddBlock parses");
    assert_eq!(serde_json::to_string(&parsed).expect("serializes"), wire);
}

#[test]
fn add_block_keeps_the_split_path_it_was_sent() {
    let out = round_trip(
        r#"{"AddBlock":{"chain":"c1","kind":"gain","model_id":"fuzz_ge","position":0,"path":{"split":"s1","side":"b"}}}"#,
    );
    assert_eq!(out["AddBlock"]["path"], json!({ "split": "s1", "side": "b" }));
}

#[test]
fn insert_prebuilt_block_keeps_the_split_path_it_was_sent() {
    let out = round_trip(
        r#"{"InsertPrebuiltBlock":{"chain":"c1","block":{"id":"b1","enabled":true,"kind":{"Core":{"effect_type":"gain","model":"fuzz_ge","params":{"values":{}}}}},"position":0,"path":{"split":"s1","side":"a"}}}"#,
    );
    assert_eq!(
        out["InsertPrebuiltBlock"]["path"],
        json!({ "split": "s1", "side": "a" })
    );
}
```

Append to `crates/adapter-mcp/src/tools_tests.rs`:
```rust

#[test]
fn add_block_tool_keeps_the_split_path_and_defaults_to_the_top_level() {
    let with_path = build_command(
        "add_block",
        serde_json::json!({
            "chain": "rig:in", "kind": "gain", "model_id": "fuzz_ge", "position": 0,
            "path": { "split": "s1", "side": "b" }
        }),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&with_path).unwrap()["AddBlock"]["path"],
        serde_json::json!({ "split": "s1", "side": "b" }),
        "#328: the split path an MCP client sends must reach the command"
    );

    let without = build_command(
        "add_block",
        serde_json::json!({ "chain": "rig:in", "kind": "gain", "model_id": "fuzz_ge", "position": 0 }),
    )
    .unwrap();
    assert!(
        serde_json::to_value(&without).unwrap()["AddBlock"]
            .get("path")
            .is_none(),
        "a path-less add_block stays a top-level add"
    );
}
```

- [ ] **Step 3: Run, see them fail**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 ld_split_path && nice -n 19 cargo test -p application -j 2 --test issue_328_block_path_wire && nice -n 19 cargo test -p adapter-mcp -j 2 add_block_tool`
Expected FAILED (the field does not exist yet, so serde drops `path`):
- `add_block_with_a_path_lands_inside_that_path` — `path A = [new, a_0]`: left `1`, right `2`.
- `add_block_refuses_an_input_port_inside_a_path` — `an input port cannot sit inside a path: [BlockAdded { … }]`.
- `add_block_refuses_a_processing_block_after_a_y_split` — `nothing may follow a Y split: [BlockAdded { … }]`.
- `add_block_never_reuses_an_id_after_a_removal` — `a new block must never reuse an id in the chain`: both `BlockId("chain_0:gain:1")`.
- `insert_prebuilt_block_with_a_path_lands_inside_path_b` — left `["b_0"]`, right `["b_0", "pre_b"]`.
- `insert_prebuilt_block_refuses_a_second_split` — `a chain holds one split: [BlockAdded { … }]`.
- `add_block_into_a_path_reaches_the_rig_preset_on_capture` — left `0`, right `1`.
- wire: `add_block_keeps_the_split_path_it_was_sent`, `insert_prebuilt_block_keeps_the_split_path_it_was_sent` — left `Null`.
- adapter-mcp: `#328: the split path an MCP client sends must reach the command` — left `Null`.
- PASS (characterization pins): `add_block_without_a_path_still_lands_at_the_top_level`, `add_block_without_path_serializes_exactly_as_before`.

- [ ] **Step 4: Add the `path` field to the two commands**

In `crates/application/src/command/block.rs`, after `use project::block::AudioBlock;` add:
```rust
use project::block::PathRef;
```
and replace lines 76-93 (`AddBlock` and `InsertPrebuiltBlock`) with:
```rust
    /// Insert a new block at `position` in the chain.
    ///
    /// #328: `path` names the split path the block goes into; `None` (the
    /// default, and what every payload written before #328 carries) is the
    /// chain's top level. The new block's id is `BlockId::generate_for_chain`.
    AddBlock {
        chain: ChainId,
        kind: String,
        model_id: String,
        position: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<PathRef>,
    },

    /// Insert a fully-constructed `AudioBlock` at `position` in the chain.
    ///
    /// Unlike `AddBlock`, the caller is responsible for building the block
    /// (including its kind and parameters). The block's `id` is preserved
    /// as-is — the caller must supply a unique id within the chain.
    ///
    /// #328: `path` as in `AddBlock`.
    InsertPrebuiltBlock {
        chain: ChainId,
        block: AudioBlock,
        position: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<PathRef>,
    },
```

- [ ] **Step 5: Add `path: None` at every construction site**

With the Edit tool, one site at a time, insert the line `path: None,` (same indentation as the line above it) directly after the `position…,` line of each struct literal listed under **Files** (3 production sites, 22 test sites: 1 + 1 + 1 + 6 + 5 + 1 + 4 + 3). Example — `crates/adapter-gui/src/block_choose_type_callback.rs:172-177` becomes:
```rust
                .dispatch(Command::Block(BlockCommand::AddBlock {
                    chain: chain_id.clone(),
                    kind: effect_type_str.to_string(),
                    model_id: block_core::constants::IO_PORT_MODEL.to_string(),
                    position: before_index,
                    path: None,
                }))
```
and `crates/adapter-gui/src/issue_690_nam_gate_persistence_tests.rs:111-115` becomes:
```rust
        .dispatch(Command::Block(BlockCommand::InsertPrebuiltBlock {
            chain: chain.clone(),
            block,
            position,
            path: None,
        }))
```
(The workspace does not compile again until Step 6 gives the two handler patterns their `path` binding; Step 7 checks both.)

- [ ] **Step 6: Route the handlers through `insert_block`**

Append to `crates/application/src/block_path.rs` and replace its `use` lines:
```rust
use anyhow::{anyhow, Result};
use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, PathRef, PathSide};
```
```rust

/// Put `block` at `position` (clamped to the end) of the top level when
/// `path` is `None`, else of the named split path.
pub fn insert_block(
    blocks: &mut Vec<AudioBlock>,
    path: Option<&PathRef>,
    position: usize,
    block: AudioBlock,
) -> Result<()> {
    let lane = lane_mut(blocks, path)?;
    let at = position.min(lane.len());
    lane.insert(at, block);
    Ok(())
}

fn lane_mut<'a>(
    blocks: &'a mut Vec<AudioBlock>,
    path: Option<&PathRef>,
) -> Result<&'a mut Vec<AudioBlock>> {
    let Some(path) = path else {
        return Ok(blocks);
    };
    let target = blocks
        .iter_mut()
        .find(|b| b.id == path.split)
        .ok_or_else(|| anyhow!("split not found: {:?}", path.split))?;
    match &mut target.kind {
        AudioBlockKind::Split(split) => Ok(match path.side {
            PathSide::A => &mut split.a,
            PathSide::B => &mut split.b,
        }),
        _ => Err(anyhow!("block {:?} is not a split", path.split)),
    }
}
```

In `crates/application/src/local_dispatcher_block_lifecycle.rs`, replace lines 1-3:
```rust
//! Responsibility: handles the block lifecycle commands.
//! Block-lifecycle handler (file-per-feature; #436 dispatcher split).
//! Behaviour byte-identical to the original inline arm — pure move.
```
with:
```rust
//! Responsibility: handles the block lifecycle commands.
//! Block-lifecycle handler (file-per-feature; #436 dispatcher split). #328:
//! additions land at the top level or inside a split path, through the
//! rule-checked `edit_chain_blocks`.
```
add after `use crate::block_factory::{build_default_block, resolve_effect_type_for_model};`:
```rust
use crate::block_path::insert_block;
```
and replace the `AddBlock` and `InsertPrebuiltBlock` arms (lines 97-138) with:
```rust
            Command::Block(BlockCommand::AddBlock {
                chain,
                kind,
                model_id,
                position,
                path,
            }) => {
                // #328: a fresh id every time. The old `{chain}:{kind}:{len}`
                // came back after a removal, and a block inside a split path
                // does not count toward the chain's top-level length at all.
                let new_block =
                    build_default_block(BlockId::generate_for_chain(&chain), &kind, &model_id)?;
                let new_block_id = new_block.id.clone();
                self.edit_chain_blocks(&chain, |blocks| {
                    insert_block(blocks, path.as_ref(), position, new_block)
                })?;
                Ok(vec![Event::BlockAdded {
                    chain,
                    block: new_block_id,
                }])
            }
            Command::Block(BlockCommand::InsertPrebuiltBlock {
                chain,
                block,
                position,
                path,
            }) => {
                let block_id = block.id.clone();
                self.edit_chain_blocks(&chain, |blocks| {
                    insert_block(blocks, path.as_ref(), position, block)
                })?;
                Ok(vec![Event::BlockAdded {
                    chain,
                    block: block_id,
                }])
            }
```

- [ ] **Step 7: Run, all pass**

First confirm no construction site was missed:
Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test --workspace --no-run -j 2 2>&1 | grep -E "^error|missing field|does not mention field" | head`
Expected: no output.

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 ld_ && nice -n 19 cargo test -p application -j 2 --test issue_328_block_path_wire && nice -n 19 cargo test -p application -j 2 --test scene_output_preservation_exhaustive && nice -n 19 cargo test -p adapter-mcp -j 2 && nice -n 19 cargo test -p adapter-gui -j 2 project_admin`
Expected: all green, including the existing `ld_block2::add_block_*` and `ld_insert::*` tests.

- [ ] **Step 8: Document it**

In `docs/mcp.md`, replace:
```
  was. `remove_block` refuses the split itself, because removing it that
  way would drop both paths.
```
with:
```
  was. `remove_block` refuses the split itself, because removing it that
  way would drop both paths.

  `add_block` and `insert_prebuilt_block` take an optional
  `path: { "split": "<split block id>", "side": "a" | "b" }` (not to be
  confused with the parameter `path` of `set_block_parameter_*`).
  Without it the block goes to the chain's top level, exactly as
  before. `add_block` names new blocks `<chain>:block:<uuid>`.
```

In `docs/midi.md`, replace:
```
| 22 | `AddBlock` | Add a block | `{ chain: id, kind: text, model_id: text, position: uint }` | **Note 83** |
```
with:
```
| 22 | `AddBlock` | Add a block | `{ chain: id, kind: text, model_id: text, position: uint, path?: object }` | **Note 83** |
```
replace:
```
| 27 | `InsertPrebuiltBlock` | Insert a pre-built block | `{ chain: id, block: object, position: uint }` | — GUI/MCP (structured object) |
```
with:
```
| 27 | `InsertPrebuiltBlock` | Insert a pre-built block | `{ chain: id, block: object, position: uint, path?: object }` | — GUI/MCP (structured object) |
```
replace:
```
| 8 | `AddBlock` | Add a block | `{ chain: id, kind: text, model_id: text, position: uint }` |
| 9 | `InsertPrebuiltBlock` | Insert a pre-built block | `{ chain: id, block: object, position: uint }` |
```
with:
```
| 8 | `AddBlock` | Add a block | `{ chain: id, kind: text, model_id: text, position: uint, path?: object }` |
| 9 | `InsertPrebuiltBlock` | Insert a pre-built block | `{ chain: id, block: object, position: uint, path?: object }` |
```
and replace:
```
`{ StepScene: int }` (relative, wraps).

That is **all 34 commands** (enum order).
```
with:
```
`{ StepScene: int }` (relative, wraps).

`path?` (#328) is optional: `{ split: id, side: a }` (or `side: b`)
puts the block into that path of the chain's split. Leave it out for the
chain's top level — every map written before #328 keeps working
unchanged.

That is **all 34 commands** (enum order).
```

- [ ] **Step 9: Commit and push**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && rustfmt --edition 2021 crates/application/src/command/block.rs crates/application/src/local_dispatcher_block_lifecycle.rs crates/application/src/block_path.rs crates/application/src/split_tests_fixtures.rs crates/application/src/ld_split_path_tests.rs crates/application/tests/issue_328_block_path_wire.rs crates/adapter-mcp/src/tools_tests.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/application/src/command/block.rs crates/application/src/local_dispatcher_block_lifecycle.rs crates/application/src/block_path.rs crates/application/src/split_tests_fixtures.rs crates/application/src/ld_split_path_tests.rs crates/application/tests/issue_328_block_path_wire.rs crates/adapter-mcp/src/tools_tests.rs crates/adapter-gui/src/block_choose_type_callback.rs crates/adapter-gui/src/block_editor_persist.rs crates/adapter-gui/src/scene_param_persistence_tests.rs crates/adapter-gui/src/issue_966_compact_scene_refresh_tests.rs crates/adapter-gui/src/issue_690_nam_gate_persistence_tests.rs crates/adapter-gui/src/project_admin_nam_tests.rs crates/adapter-gui/src/project_admin_persistence_tests.rs crates/application/tests/scene_output_preservation_exhaustive.rs crates/application/src/ld_block2_tests.rs crates/application/src/ld_insert_tests.rs docs/mcp.md docs/midi.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): AddBlock and InsertPrebuiltBlock take an optional split path"
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch origin
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status -sb
```
If behind: `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase origin feature/issue-328` and re-run Step 7. Then:
```bash
# CLAUDE.md push gate: fmt clean, every `test result: ok.`, no build warning/error line, no `✗ FAIL`. Push only if this line exits 0.
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 2 / Task 4 pushed ($(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD)): AddBlock/InsertPrebuiltBlock take path (serde default, wire unchanged without it); AddBlock ids via generate_for_chain. Files: command/block.rs, local_dispatcher_block_lifecycle.rs, block_path.rs, adapter-gui call sites (path: None), docs/mcp.md, docs/midi.md. Tests: application ld_ + issue_328_block_path_wire, adapter-mcp, adapter-gui project_admin green."
```

---

### Task 5: MoveBlock moves a block within, between and out of paths

**Files:**
- Modify: `crates/application/src/command/block.rs` (`MoveBlock`, lines ~121-126 after Task 4)
- Modify: `crates/application/src/local_dispatcher_block_edit.rs` (`MoveBlock` arm)
- Modify (compile fix, `path: None` after the `new_position…,` line): production `crates/adapter-gui/src/block_reorder.rs:83`, `crates/adapter-gui/src/compact_chain_block_delete.rs:219`; tests `crates/adapter-gui/src/project_admin_persistence_tests.rs:516`, `crates/application/src/ld_block2_tests.rs:230, :263, :288` (verify with `grep -rn "BlockCommand::MoveBlock {" crates --include='*.rs'`).
- Test: `crates/application/src/ld_split_path_tests.rs`, `crates/application/tests/issue_328_block_path_wire.rs`
- Modify: `docs/mcp.md`, `docs/midi.md:49, :271`

**Interfaces:**
- Consumes: `remove_block`, `insert_block`, `edit_chain_blocks`.
- Produces: `BlockCommand::MoveBlock { chain, block, new_position, path: Option<PathRef> }` — `path` is the DESTINATION (`None` = top level).

- [ ] **Step 1: Write the failing tests**

Append to `crates/application/src/ld_split_path_tests.rs`:
```rust

#[test]
fn move_block_drops_a_top_level_block_into_path_b() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "MoveBlock",
        json!({
            "chain": CHAIN, "block": "pre", "new_position": 0,
            "path": { "split": "split_0", "side": "b" }
        }),
    )
    .expect("move pre into path B");

    assert_eq!(
        ids(&project.borrow().chains[0].blocks),
        vec!["split_0", "post"]
    );
    assert_eq!(ids(&split_of(&project.borrow()).b), vec!["pre", "b_0"]);
}

#[test]
fn move_block_drags_a_block_from_path_a_to_path_b() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatch_json(
        &dispatcher,
        "MoveBlock",
        json!({
            "chain": CHAIN, "block": "a_0", "new_position": 1,
            "path": { "split": "split_0", "side": "b" }
        }),
    )
    .expect("drag a_0 across the split");

    let split = split_of(&project.borrow());
    assert!(split.a.is_empty(), "path A is empty");
    assert_eq!(ids(&split.b), vec!["b_0", "a_0"]);
    assert_eq!(
        events,
        vec![Event::ChainReloaded {
            chain: ChainId(CHAIN.into())
        }]
    );
}

#[test]
fn move_block_without_a_path_lifts_a_path_block_to_the_top_level() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "MoveBlock",
        json!({ "chain": CHAIN, "block": "b_0", "new_position": 0 }),
    )
    .expect("lift b_0 out of path B");

    assert_eq!(
        ids(&project.borrow().chains[0].blocks),
        vec!["b_0", "pre", "split_0", "post"]
    );
    assert!(split_of(&project.borrow()).b.is_empty(), "path B is empty");
}

#[test]
fn move_block_refuses_to_put_the_split_inside_its_own_path() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatch_json(
        &dispatcher,
        "MoveBlock",
        json!({
            "chain": CHAIN, "block": "split_0", "new_position": 0,
            "path": { "split": "split_0", "side": "a" }
        }),
    )
    .expect_err("a split cannot go inside its own path");

    assert!(err.to_string().contains("split not found"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

#[test]
fn move_block_refuses_a_processing_block_after_a_y_split() {
    let project = project_with(y_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatch_json(
        &dispatcher,
        "MoveBlock",
        json!({ "chain": CHAIN, "block": "pre", "new_position": 9 }),
    )
    .expect_err("nothing may follow a Y split");

    assert!(err.to_string().contains("Y split"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}
```

Append to `crates/application/tests/issue_328_block_path_wire.rs`:
```rust

/// Characterization pin (green before and after).
#[test]
fn move_block_without_path_serializes_exactly_as_before() {
    let wire = r#"{"MoveBlock":{"chain":"c1","block":"b1","new_position":2}}"#;
    let parsed: Command = serde_json::from_str(wire).expect("a pre-#328 MoveBlock parses");
    assert_eq!(serde_json::to_string(&parsed).expect("serializes"), wire);
}

#[test]
fn move_block_keeps_the_destination_path_it_was_sent() {
    let out = round_trip(
        r#"{"MoveBlock":{"chain":"c1","block":"b1","new_position":2,"path":{"split":"s1","side":"b"}}}"#,
    );
    assert_eq!(out["MoveBlock"]["path"], json!({ "split": "s1", "side": "b" }));
}
```

- [ ] **Step 2: Run, see them fail**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 ld_split_path::move_block && nice -n 19 cargo test -p application -j 2 --test issue_328_block_path_wire`
Expected FAILED:
- `move_block_drops_a_top_level_block_into_path_b` — left `["pre", "split_0", "post"]`, right `["split_0", "post"]` (the path was dropped, `pre` moved to top-level 0).
- `move_block_drags_a_block_from_path_a_to_path_b` — `drag a_0 across the split: block not found: BlockId("a_0")`.
- `move_block_without_a_path_lifts_a_path_block_to_the_top_level` — `…: block not found: BlockId("b_0")`.
- `move_block_refuses_to_put_the_split_inside_its_own_path` — `a split cannot go inside its own path: [ChainReloaded { … }]`.
- `move_block_refuses_a_processing_block_after_a_y_split` — `nothing may follow a Y split: [ChainReloaded { … }]`.
- `move_block_keeps_the_destination_path_it_was_sent` — left `Null`.
- `move_block_without_path_serializes_exactly_as_before` PASSES (pin).

- [ ] **Step 3: Add the `path` field and fix every construction site**

In `crates/application/src/command/block.rs`, replace the `MoveBlock` variant:
```rust
    /// Move a block to `new_position` within its chain.
    MoveBlock {
        chain: ChainId,
        block: BlockId,
        new_position: usize,
    },
```
with:
```rust
    /// Move a block to `new_position` within its chain.
    ///
    /// #328: `path` is the DESTINATION — a split path, or the chain's top
    /// level when `None` (the default). The block is found wherever it is, so
    /// a move can cross from one path to the other, or in and out of a path.
    MoveBlock {
        chain: ChainId,
        block: BlockId,
        new_position: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<PathRef>,
    },
```
Then, with the Edit tool, insert `path: None,` after the `new_position…,` line of each of the 6 sites listed under **Files**. Example — `crates/adapter-gui/src/block_reorder.rs:80-84` becomes:
```rust
        .dispatch(Command::Block(BlockCommand::MoveBlock {
            chain: chain_id.clone(),
            block: block_id,
            new_position: insert_at,
            path: None,
        }))
```

- [ ] **Step 4: Route `MoveBlock` through the draft**

In `crates/application/src/local_dispatcher_block_edit.rs`, change the import to:
```rust
use crate::block_path::{find_block_mut, insert_block, remove_block};
```
and replace the `MoveBlock` arm:
```rust
            Command::Block(BlockCommand::MoveBlock {
                chain,
                block,
                new_position,
            }) => {
                self.with_chain(&chain, |c| {
                    let Some(from_idx) = c.blocks.iter().position(|b| b.id == block) else {
                        return Err(anyhow::anyhow!("block not found: {:?}", block));
                    };
                    let moved = c.blocks.remove(from_idx);
                    let insert_at = new_position.min(c.blocks.len());
                    c.blocks.insert(insert_at, moved);
                    Ok(())
                })?;
                Ok(vec![Event::ChainReloaded { chain }])
            }
```
with:
```rust
            Command::Block(BlockCommand::MoveBlock {
                chain,
                block,
                new_position,
                path,
            }) => {
                self.edit_chain_blocks(&chain, |blocks| {
                    let Some(moved) = remove_block(blocks, &block) else {
                        return Err(anyhow::anyhow!("block not found: {:?}", block));
                    };
                    insert_block(blocks, path.as_ref(), new_position, moved)
                })?;
                Ok(vec![Event::ChainReloaded { chain }])
            }
```

- [ ] **Step 5: Run, all pass**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 ld_ && nice -n 19 cargo test -p application -j 2 --test issue_328_block_path_wire && nice -n 19 cargo test -p adapter-gui -j 2 project_admin && nice -n 19 cargo test --workspace --no-run -j 2 2>&1 | grep -E "^error|missing field|does not mention field" | head`
Expected: all green (incl. `ld_block2::move_block_*` unchanged); the last command prints nothing.

- [ ] **Step 6: Document it**

In `docs/mcp.md`, replace:
```
  before. `add_block` names new blocks `<chain>:block:<uuid>`.
```
with:
```
  before. `add_block` names new blocks `<chain>:block:<uuid>`.

  `move_block` takes the same optional `path` as its destination, so a
  block moves within a path, between path `a` and path `b`, or between a
  path and the top level (no `path`).
```

In `docs/midi.md`, replace:
```
| 15 | `MoveBlock` | Move a block to a position | `{ chain: id, block: id, new_position: uint }` | **Note 76** |
```
with:
```
| 15 | `MoveBlock` | Move a block to a position | `{ chain: id, block: id, new_position: uint, path?: object }` | **Note 76** |
```
replace:
```
| 12 | `MoveBlock` | Move a block to a position | `{ chain: id, block: id, new_position: uint }` |
```
with:
```
| 12 | `MoveBlock` | Move a block to a position | `{ chain: id, block: id, new_position: uint, path?: object }` |
```

- [ ] **Step 7: Commit and push**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && rustfmt --edition 2021 crates/application/src/command/block.rs crates/application/src/local_dispatcher_block_edit.rs crates/application/src/ld_split_path_tests.rs crates/application/tests/issue_328_block_path_wire.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/application/src/command/block.rs crates/application/src/local_dispatcher_block_edit.rs crates/application/src/ld_split_path_tests.rs crates/application/tests/issue_328_block_path_wire.rs crates/adapter-gui/src/block_reorder.rs crates/adapter-gui/src/compact_chain_block_delete.rs crates/adapter-gui/src/project_admin_persistence_tests.rs crates/application/src/ld_block2_tests.rs docs/mcp.md docs/midi.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): MoveBlock moves a block across split paths"
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch origin
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status -sb
```
If behind: `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase origin feature/issue-328` and re-run Step 5. Then:
```bash
# CLAUDE.md push gate: fmt clean, every `test result: ok.`, no build warning/error line, no `✗ FAIL`. Push only if this line exits 0.
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 2 / Task 5 pushed ($(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD)): MoveBlock path = destination; moves within/between/out of paths, rule-checked. Files: command/block.rs, local_dispatcher_block_edit.rs, adapter-gui call sites (path: None), docs. Tests: application ld_ + wire, adapter-gui project_admin green."
```

---

### Task 6: AddSplit, SetSplitEnd and RemoveSplit

**Files:**
- Create: `crates/application/src/command/split.rs`
- Create: `crates/application/src/local_dispatcher_split.rs`
- Create: `crates/application/src/local_dispatcher_split_tests.rs` (tests — attached from `lib.rs`, so they compile and fail BEFORE the handler exists)
- Modify: `crates/application/src/command.rs:22-44` (module + re-export) and `:63-77` (variant)
- Modify: `crates/application/src/local_dispatcher_trait.rs` (route, after `Command::Looper(_) => self.handle_looper(cmd),`)
- Modify: `crates/application/src/lib.rs` (after `mod local_dispatcher_selection;`, and the test module at the end)
- Test: `crates/application/tests/issue_14_command_grouping_preserves_surface.rs:60-61`, `crates/adapter-mcp/src/tools_tests.rs:64`
- Modify: `docs/mcp.md`

**Interfaces:**
- Consumes: `SplitBlock`, `SplitEnd` (`"mix"` / `"y"`), `default_split_params()`, `edit_chain_blocks`, fixtures (`dispatch_json`, `rig_*`, `ids`, `split_of`, `split`, `mix_chain`, `project_with`).
- Produces:
  - `Command::Split(SplitCommand)`; `SplitCommand::AddSplit { chain: ChainId, position: usize, end: SplitEnd }`, `SplitCommand::SetSplitEnd { chain: ChainId, split_id: BlockId, end: SplitEnd }`, `SplitCommand::RemoveSplit { chain: ChainId, split_id: BlockId }`.
  - Events: `AddSplit` → `[BlockAdded { chain, block: <new split id> }]`; `SetSplitEnd` / `RemoveSplit` → `[ChainReloaded { chain }]`.
  - MCP tools `add_split`, `set_split_end`, `remove_split`; `COMMAND_VARIANT_COUNT` = 102.

- [ ] **Step 1: Write the failing tests**

Create `crates/application/src/local_dispatcher_split_tests.rs`:
```rust
//! #328 — `AddSplit`, `SetSplitEnd`, `RemoveSplit`, driven through their wire
//! form (the road an MCP tool call takes).

use project::block::split_block::SplitEnd;
use project::block::split_params::default_split_params;
use serde_json::json;

use crate::local_dispatcher_tests::*;
use crate::split_tests_fixtures::*;

#[test]
fn add_split_inserts_an_empty_split_with_the_default_knobs() {
    let project = project_with(vec![make_core_block("pre", true), make_core_block("post", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatch_json(
        &dispatcher,
        "AddSplit",
        json!({ "chain": CHAIN, "position": 1, "end": "mix" }),
    )
    .expect("AddSplit on a chain without a split");

    let blocks = project.borrow().chains[0].blocks.clone();
    assert_eq!(blocks.len(), 3);
    let AudioBlockKind::Split(split) = &blocks[1].kind else {
        panic!("slot 1 holds the split, got '{}'", blocks[1].kind.label());
    };
    assert!(matches!(split.end, SplitEnd::Mix));
    assert!(split.a.is_empty() && split.b.is_empty(), "both paths start empty");
    assert_eq!(split.params, default_split_params());
    assert!(
        blocks[1].id.0.starts_with("chain_0:block:"),
        "{}",
        blocks[1].id.0
    );
    assert_eq!(
        events,
        vec![Event::BlockAdded {
            chain: ChainId(CHAIN.into()),
            block: blocks[1].id.clone(),
        }]
    );
}

#[test]
fn add_split_refuses_a_second_split() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatch_json(
        &dispatcher,
        "AddSplit",
        json!({ "chain": CHAIN, "position": 0, "end": "mix" }),
    )
    .expect_err("a chain holds one split");

    assert!(err.to_string().contains("at most one split"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

#[test]
fn add_split_y_refuses_a_block_after_its_position() {
    let project = project_with(vec![make_core_block("pre", true), make_core_block("post", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let err = dispatch_json(
        &dispatcher,
        "AddSplit",
        json!({ "chain": CHAIN, "position": 1, "end": "y" }),
    )
    .expect_err("post would follow the Y split");

    assert!(err.to_string().contains("Y split"), "{err}");
    assert_eq!(ids(&project.borrow().chains[0].blocks), vec!["pre", "post"]);
}

#[test]
fn add_split_y_at_the_end_is_accepted() {
    let project = project_with(vec![make_core_block("pre", true), make_core_block("post", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "AddSplit",
        json!({ "chain": CHAIN, "position": 2, "end": "y" }),
    )
    .expect("a Y split may be the last block");

    assert_eq!(project.borrow().chains[0].blocks.len(), 3);
    assert!(matches!(split_of(&project.borrow()).end, SplitEnd::Y));
}

#[test]
fn set_split_end_to_y_is_refused_while_a_block_follows_the_split() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let err = dispatch_json(
        &dispatcher,
        "SetSplitEnd",
        json!({ "chain": CHAIN, "split_id": "split_0", "end": "y" }),
    )
    .expect_err("post follows the split");

    assert!(err.to_string().contains("Y split"), "{err}");
    assert!(
        matches!(split_of(&project.borrow()).end, SplitEnd::Mix),
        "the split kept its end"
    );
}

#[test]
fn set_split_end_to_y_is_accepted_when_the_split_is_last() {
    let project = project_with(vec![
        make_core_block("pre", true),
        split("split_0", SplitEnd::Mix, vec![], vec![]),
    ]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatch_json(
        &dispatcher,
        "SetSplitEnd",
        json!({ "chain": CHAIN, "split_id": "split_0", "end": "y" }),
    )
    .expect("nothing follows the split");

    assert!(matches!(split_of(&project.borrow()).end, SplitEnd::Y));
    assert_eq!(
        events,
        vec![Event::ChainReloaded {
            chain: ChainId(CHAIN.into())
        }]
    );
}

#[test]
fn remove_split_keeps_path_a_in_its_place_and_drops_path_b() {
    let project = project_with(vec![
        make_core_block("pre", true),
        split(
            "split_0",
            SplitEnd::Mix,
            vec![make_core_block("a_0", true), make_core_block("a_1", true)],
            vec![make_core_block("b_0", true)],
        ),
        make_core_block("post", true),
    ]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "RemoveSplit",
        json!({ "chain": CHAIN, "split_id": "split_0" }),
    )
    .expect("RemoveSplit");

    assert_eq!(
        ids(&project.borrow().chains[0].blocks),
        vec!["pre", "a_0", "a_1", "post"]
    );
}

#[test]
fn remove_split_refuses_a_block_that_is_not_a_split() {
    let project = project_with(mix_chain());
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let before = project.borrow().chains[0].blocks.clone();

    let err = dispatch_json(
        &dispatcher,
        "RemoveSplit",
        json!({ "chain": CHAIN, "split_id": "pre" }),
    )
    .expect_err("pre is not a split");

    assert!(err.to_string().contains("is not a split"), "{err}");
    assert_eq!(project.borrow().chains[0].blocks, before);
}

#[test]
fn add_split_reaches_the_rig_preset_on_capture() {
    let (rig, _project, dispatcher) =
        rig_session_from(rig_with_presets(vec![("p1", vec![make_core_block("A", true)])]));

    let events = dispatch_json(
        &dispatcher,
        "AddSplit",
        json!({ "chain": RIG_CHAIN, "position": 1, "end": "mix" }),
    )
    .expect("AddSplit on the rig chain");
    let Some(Event::BlockAdded { block: split_id, .. }) = events.first() else {
        panic!("AddSplit answers BlockAdded, got {events:?}");
    };
    dispatcher
        .dispatch(Command::Project(ProjectCommand::CaptureRigEdits))
        .expect("capture");

    assert_eq!(
        ids(&rig.borrow().presets["p1"].blocks),
        vec!["A".to_string(), split_id.0.clone()],
        "the new split must reach project.yaml"
    );
}
```

In `crates/application/src/lib.rs`, append:
```rust

#[cfg(test)]
#[path = "local_dispatcher_split_tests.rs"]
mod local_dispatcher_split_tests;
```

In `crates/application/tests/issue_14_command_grouping_preserves_surface.rs`, replace:
```rust
    "SetChainLooperTransport",
    // ── The surface that existed before the split ─────────────────────────
```
with:
```rust
    "SetChainLooperTransport",
    // ── Added by #328 (the chain split) ───────────────────────────────────
    "AddSplit",
    "RemoveSplit",
    "SetSplitEnd",
    // ── The surface that existed before the split ─────────────────────────
```

In `crates/adapter-mcp/src/tools_tests.rs`, replace:
```rust
/// reshapes a take exactly as the waveform editor does.
const COMMAND_VARIANT_COUNT: usize = 99;
```
with:
```rust
/// reshapes a take exactly as the waveform editor does.
/// #328 bumped to 102 with `AddSplit`, `SetSplitEnd` and `RemoveSplit` —
/// the chain split (Split → Mix, Y → A/B) created, switched and removed
/// from any transport.
const COMMAND_VARIANT_COUNT: usize = 102;
```
and append:
```rust

#[test]
fn split_tools_build_their_commands() {
    for (tool, args, wire) in [
        (
            "add_split",
            serde_json::json!({ "chain": "rig:in", "position": 1, "end": "mix" }),
            serde_json::json!({ "AddSplit": { "chain": "rig:in", "position": 1, "end": "mix" } }),
        ),
        (
            "set_split_end",
            serde_json::json!({ "chain": "rig:in", "split_id": "s1", "end": "y" }),
            serde_json::json!({ "SetSplitEnd": { "chain": "rig:in", "split_id": "s1", "end": "y" } }),
        ),
        (
            "remove_split",
            serde_json::json!({ "chain": "rig:in", "split_id": "s1" }),
            serde_json::json!({ "RemoveSplit": { "chain": "rig:in", "split_id": "s1" } }),
        ),
    ] {
        let cmd = build_command(tool, args).unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert_eq!(serde_json::to_value(&cmd).unwrap(), wire, "{tool}");
    }
}
```

- [ ] **Step 2: Run, see them fail**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 local_dispatcher_split_tests ; nice -n 19 cargo test -p application -j 2 --test issue_14_command_grouping_preserves_surface ; nice -n 19 cargo test -p adapter-mcp -j 2`
Expected FAILED (the variants do not exist, so `command_from_variant` refuses them — the exact error an MCP client gets today):
- the `expect(…)` tests panic with `…: unknown command: AddSplit` / `…: unknown command: SetSplitEnd` / `…: unknown command: RemoveSplit`;
- the `expect_err` tests fail on their message assertion, printing `unknown command: …` instead of `at most one split` / `Y split` / `is not a split`;
- `grouping Command dropped MCP tools that clients already call: ["AddSplit", "RemoveSplit", "SetSplitEnd"]`;
- `parity_guard_every_command_variant_is_a_tool` left `99` right `102`;
- `split_tools_build_their_commands` panics `add_split: unknown tool: add_split`.

- [ ] **Step 3: Add the command, its handler and its route**

Create `crates/application/src/command/split.rs`:
```rust
//! Responsibility: names the split lifecycle commands.
//!
//! #328: a chain may hold one split (Split → Mix or Y → A/B, spec §1.1). These
//! commands create it, switch its end and remove it. Blocks go into its paths
//! through `AddBlock` / `InsertPrebuiltBlock` / `MoveBlock` with a `path`, and
//! its knobs are ordinary `SetBlockParameter*` writes on the split's block id.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use domain::ids::{BlockId, ChainId};
use project::block::split_block::SplitEnd;

/// Every state change that creates, reshapes or removes a chain's split.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum SplitCommand {
    /// Insert an empty split (no block in either path, the default knobs of
    /// spec §1.2) at `position` of the chain's top level. Refused when the
    /// chain already has a split, or when `end` is `y` and a processing block
    /// would follow it. Answers `BlockAdded` with the new split's id.
    AddSplit {
        chain: ChainId,
        position: usize,
        end: SplitEnd,
    },
    /// Switch the split between Split → Mix and Y → A/B. Switching to `y` is
    /// refused while a processing block follows the split.
    SetSplitEnd {
        chain: ChainId,
        split_id: BlockId,
        end: SplitEnd,
    },
    /// Remove the split: path A's blocks take its place and path B's blocks
    /// are dropped (the GUI asks first when path B is not empty).
    RemoveSplit { chain: ChainId, split_id: BlockId },
}
```

In `crates/application/src/command.rs`, after `pub mod settings;` add `pub mod split;`; after `pub use settings::SettingsCommand;` add `pub use split::SplitCommand;`; and replace:
```rust
    /// #323: per-chain loopers — membership, transport, params, endpoints and
    /// the linked preset (phase 2).
    Looper(LooperCommand),
}
```
with:
```rust
    /// #323: per-chain loopers — membership, transport, params, endpoints and
    /// the linked preset (phase 2).
    Looper(LooperCommand),
    /// #328: the chain's split — create it, switch Mix/Y, remove it.
    Split(SplitCommand),
}
```

Create `crates/application/src/local_dispatcher_split.rs`:
```rust
//! Responsibility: handles the split lifecycle commands.
//!
//! #328 (spec §3): `AddSplit`, `SetSplitEnd` and `RemoveSplit` reshape the
//! chain's top-level block list around its one split. Each runs through
//! `edit_chain_blocks`, so the split rules (one split, Y last) are checked on
//! the result and a refused command changes nothing.

use anyhow::{anyhow, Result};

use domain::ids::BlockId;
use project::block::split_block::{SplitBlock, SplitEnd};
use project::block::split_params::default_split_params;
use project::block::{AudioBlock, AudioBlockKind};

use crate::command::{Command, SplitCommand};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    pub(crate) fn handle_split(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::Split(split_cmd) = cmd else {
            unreachable!("handle_split received a non-split command: {cmd:?}");
        };
        match split_cmd {
            SplitCommand::AddSplit {
                chain,
                position,
                end,
            } => {
                let split_id = BlockId::generate_for_chain(&chain);
                let block = empty_split(split_id.clone(), end);
                self.edit_chain_blocks(&chain, |blocks| {
                    let at = position.min(blocks.len());
                    blocks.insert(at, block);
                    Ok(())
                })?;
                Ok(vec![Event::BlockAdded {
                    chain,
                    block: split_id,
                }])
            }
            SplitCommand::SetSplitEnd {
                chain,
                split_id,
                end,
            } => {
                self.edit_chain_blocks(&chain, |blocks| {
                    split_mut(blocks, &split_id)?.end = end;
                    Ok(())
                })?;
                Ok(vec![Event::ChainReloaded { chain }])
            }
            SplitCommand::RemoveSplit { chain, split_id } => {
                self.edit_chain_blocks(&chain, |blocks| {
                    let at = blocks
                        .iter()
                        .position(|b| b.id == split_id)
                        .ok_or_else(|| anyhow!("split not found: {:?}", split_id))?;
                    let AudioBlockKind::Split(removed) = blocks.remove(at).kind else {
                        return Err(anyhow!("block {:?} is not a split", split_id));
                    };
                    // Path A takes the split's place; path B goes with it.
                    let tail = blocks.split_off(at);
                    blocks.extend(removed.a);
                    blocks.extend(tail);
                    Ok(())
                })?;
                Ok(vec![Event::ChainReloaded { chain }])
            }
        }
    }
}

fn empty_split(id: BlockId, end: SplitEnd) -> AudioBlock {
    AudioBlock {
        id,
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end,
            params: default_split_params(),
            a: Vec::new(),
            b: Vec::new(),
        }),
    }
}

fn split_mut<'a>(blocks: &'a mut [AudioBlock], split_id: &BlockId) -> Result<&'a mut SplitBlock> {
    let block = blocks
        .iter_mut()
        .find(|b| b.id == *split_id)
        .ok_or_else(|| anyhow!("split not found: {:?}", split_id))?;
    match &mut block.kind {
        AudioBlockKind::Split(split) => Ok(split),
        _ => Err(anyhow!("block {:?} is not a split", split_id)),
    }
}
```

In `crates/application/src/lib.rs`, after `mod local_dispatcher_selection;` add:
```rust
mod local_dispatcher_split;
```

In `crates/application/src/local_dispatcher_trait.rs`, replace:
```rust
            Command::Looper(_) => self.handle_looper(cmd),
```
with:
```rust
            Command::Looper(_) => self.handle_looper(cmd),

            // #328: the chain's split — create, switch Mix/Y, remove.
            Command::Split(_) => self.handle_split(cmd),
```

- [ ] **Step 4: Run, all pass**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 local_dispatcher_split_tests && nice -n 19 cargo test -p application -j 2 --test issue_14_command_grouping_preserves_surface && nice -n 19 cargo test -p application -j 2 --test issue_489_command_schema_completeness && nice -n 19 cargo test -p adapter-mcp -j 2`
Expected: 9 passed in `local_dispatcher_split_tests`; the surface pin, the #489 schema completeness test and every adapter-mcp test pass (102 tools).

- [ ] **Step 5: Document it**

In `docs/mcp.md`, replace:
```
  block moves within a path, between path `a` and path `b`, or between a
  path and the top level (no `path`).
```
with:
```
  block moves within a path, between path `a` and path `b`, or between a
  path and the top level (no `path`).

  `add_split` (`{ chain, position, end: "mix" | "y" }`) inserts an empty
  split with the default knobs and answers `BlockAdded` with its id;
  `set_split_end` (`{ chain, split_id, end }`) switches it between
  Split → Mix and Y → A/B; `remove_split` (`{ chain, split_id }`) puts
  path `a`'s blocks in its place and drops path `b`'s.
```

- [ ] **Step 6: Commit and push**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && rustfmt --edition 2021 crates/application/src/command/split.rs crates/application/src/command.rs crates/application/src/local_dispatcher_split.rs crates/application/src/local_dispatcher_split_tests.rs crates/application/src/local_dispatcher_trait.rs crates/application/tests/issue_14_command_grouping_preserves_surface.rs crates/adapter-mcp/src/tools_tests.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/application/src/command/split.rs crates/application/src/command.rs crates/application/src/local_dispatcher_split.rs crates/application/src/local_dispatcher_split_tests.rs crates/application/src/local_dispatcher_trait.rs crates/application/src/lib.rs crates/application/tests/issue_14_command_grouping_preserves_surface.rs crates/adapter-mcp/src/tools_tests.rs docs/mcp.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): AddSplit, SetSplitEnd and RemoveSplit commands with MCP tools"
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch origin
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status -sb
```
If behind: `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase origin feature/issue-328` and re-run Step 4. Then:
```bash
# CLAUDE.md push gate: fmt clean, every `test result: ok.`, no build warning/error line, no `✗ FAIL`. Push only if this line exits 0.
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 2 / Task 6 pushed ($(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD)): SplitCommand (AddSplit/SetSplitEnd/RemoveSplit) + handler + MCP tools (102). Files: command/split.rs, command.rs, local_dispatcher_split.rs, local_dispatcher_trait.rs, docs/mcp.md. Tests: application local_dispatcher_split + surface/schema pins, adapter-mcp green."
```

---

### Task 7: SetChainEndpointEnabled — the endpoint checklist, and it survives

**Files:**
- Create: `crates/application/src/local_dispatcher_chain_endpoints.rs`
- Create: `crates/application/src/local_dispatcher_chain_endpoints_tests.rs` (tests — attached from `lib.rs`, so they compile and fail BEFORE the handler exists)
- Modify: `crates/application/src/command/chain.rs:5-9` (import) and after `SetChainIoBindings` (line ~157)
- Modify: `crates/application/src/local_dispatcher_trait.rs:73-79` (route)
- Modify: `crates/application/src/lib.rs` (after `mod local_dispatcher_chain_crud;`, and the test module at the end)
- NOT modified: `crates/project/src/rig_sync.rs` — Part 1 Task 10 already copies `chain.disabled_endpoints` into `RigInput.disabled_endpoints` in `sync_synthetic_into_rig`, and `rig_to_chains` copies it back; this task only pins that end to end.
- Modify: `crates/application/src/local_dispatcher_chain_save.rs:~86-95` (SaveChain upsert)
- Test: `crates/application/tests/issue_14_command_grouping_preserves_surface.rs`, `crates/adapter-mcp/src/tools_tests.rs`
- Modify: `docs/mcp.md`

**Interfaces:**
- Consumes: `EndpointDisables { inputs, outputs, path_a_outputs, path_b_outputs }` + `is_empty` / `is_enabled` / `set_enabled(&mut self, EndpointNode, EndpointRef, bool)` (Part 1 Task 8 — the single toggle rule; this part does not re-implement it), `EndpointRef { io, endpoint }`, `EndpointNode::{Input, Output, PathAOutput, PathBOutput}` (`"input"`, `"output"`, `"path_a_output"`, `"path_b_output"`), `Chain.disabled_endpoints`, `RigInput.disabled_endpoints`, Part 1 Task 10's capture (`sync_synthetic_into_rig`) and projection (`rig_to_chains`) of the checklist; fixtures.
- Produces: `ChainCommand::SetChainEndpointEnabled { chain: ChainId, node: EndpointNode, io: String, endpoint: String, enabled: bool }` → `[ChainReloaded { chain }, ProjectMutated]`. `SaveChain` (upsert) keeps the existing chain's `disabled_endpoints`. MCP tool `set_chain_endpoint_enabled`; `COMMAND_VARIANT_COUNT` = 103.

- [ ] **Step 1: Write the failing tests**

Create `crates/application/src/local_dispatcher_chain_endpoints_tests.rs`:
```rust
//! #328 — the endpoint checklist of the chain graph's I/O nodes: recorded per
//! node, carried into the rig, and kept by the commands that rewrite a chain.

use project::endpoint_disables::{EndpointNode, EndpointRef};
use project::rig::RigScene;
use serde_json::json;

use crate::command::RigNavKind;
use crate::local_dispatcher_tests::*;
use crate::split_tests_fixtures::*;

fn endpoint(io: &str, name: &str) -> EndpointRef {
    EndpointRef {
        io: io.to_string(),
        endpoint: name.to_string(),
    }
}

fn set_enabled(dispatcher: &LocalDispatcher, chain: &str, node: &str, name: &str, enabled: bool) {
    dispatch_json(
        dispatcher,
        "SetChainEndpointEnabled",
        json!({ "chain": chain, "node": node, "io": "io-main", "endpoint": name, "enabled": enabled }),
    )
    .expect("SetChainEndpointEnabled");
}

#[test]
fn unchecking_an_input_records_it_on_that_node_only() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatch_json(
        &dispatcher,
        "SetChainEndpointEnabled",
        json!({ "chain": CHAIN, "node": "input", "io": "io-main", "endpoint": "In 1", "enabled": false }),
    )
    .expect("uncheck In 1 on the input node");

    let disables = project.borrow().chains[0].disabled_endpoints.clone();
    assert_eq!(disables.inputs, vec![endpoint("io-main", "In 1")]);
    assert!(!disables.is_enabled(EndpointNode::Input, &endpoint("io-main", "In 1")));
    assert!(
        disables.is_enabled(EndpointNode::Output, &endpoint("io-main", "In 1")),
        "another node is untouched"
    );
    assert!(
        events
            .iter()
            .any(|e| e.chain() == Some(&ChainId(CHAIN.into()))),
        "the drain must re-sync exactly this chain: {events:?}"
    );
}

#[test]
fn checking_it_again_clears_it_and_a_repeated_uncheck_is_recorded_once() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    set_enabled(&dispatcher, CHAIN, "input", "In 1", false);
    set_enabled(&dispatcher, CHAIN, "input", "In 1", false);
    assert_eq!(
        project.borrow().chains[0].disabled_endpoints.inputs,
        vec![endpoint("io-main", "In 1")],
        "unchecked twice, listed once"
    );

    set_enabled(&dispatcher, CHAIN, "input", "In 1", true);
    assert!(
        project.borrow().chains[0].disabled_endpoints.is_empty(),
        "checked again: nothing is disabled"
    );
}

#[test]
fn a_path_b_output_lands_on_the_path_b_list_only() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    set_enabled(&dispatcher, CHAIN, "path_b_output", "Out 3", false);

    let disables = project.borrow().chains[0].disabled_endpoints.clone();
    assert_eq!(disables.path_b_outputs, vec![endpoint("io-main", "Out 3")]);
    assert!(disables.inputs.is_empty());
    assert!(disables.outputs.is_empty());
    assert!(disables.path_a_outputs.is_empty());
}

#[test]
fn an_unknown_chain_is_refused() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatch_json(
        &dispatcher,
        "SetChainEndpointEnabled",
        json!({ "chain": "nope", "node": "input", "io": "io-main", "endpoint": "In 1", "enabled": false }),
    );

    let err = result.expect_err("no chain 'nope'");
    assert!(err.to_string().contains("chain not found"), "{err}");
}

#[test]
fn the_checklist_is_captured_into_the_rig_input() {
    let (rig, _project, dispatcher) =
        rig_session_from(rig_with_presets(vec![("p1", vec![make_core_block("A", true)])]));

    set_enabled(&dispatcher, RIG_CHAIN, "output", "Out 1", false);
    dispatcher
        .dispatch(Command::Project(ProjectCommand::CaptureRigEdits))
        .expect("capture");

    assert_eq!(
        rig.borrow().inputs["in"].disabled_endpoints.outputs,
        vec![endpoint("io-main", "Out 1")],
        "project.yaml persists the checklist on the input"
    );
}

#[test]
fn the_checklist_survives_a_scene_switch() {
    let mut rig = rig_with_presets(vec![("p1", vec![make_core_block("A", true)])]);
    rig.presets
        .get_mut("p1")
        .expect("p1")
        .scenes
        .insert(2, RigScene::default());
    let (_rig, project, dispatcher) = rig_session_from(rig);

    set_enabled(&dispatcher, RIG_CHAIN, "output", "Out 1", false);
    dispatcher
        .dispatch(Command::Selection(SelectionCommand::ApplyRigNav {
            chain: ChainId(RIG_CHAIN.into()),
            kind: RigNavKind::Scene(2),
        }))
        .expect("switch to scene 2");

    assert_eq!(
        project.borrow().chains[0].disabled_endpoints.outputs,
        vec![endpoint("io-main", "Out 1")],
        "a scene switch re-projects the chain; the checklist must come back with it"
    );
}

#[test]
fn the_chain_editor_save_keeps_the_checklist() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    set_enabled(&dispatcher, CHAIN, "input", "In 1", false);

    let mut edited = project.borrow().chains[0].clone();
    edited.description = Some("Renamed".into());
    edited.disabled_endpoints = Default::default();
    dispatcher
        .dispatch(Command::Chain(ChainCommand::SaveChain { chain: edited }))
        .expect("SaveChain (rename)");

    let chain = project.borrow().chains[0].clone();
    assert_eq!(chain.description.as_deref(), Some("Renamed"));
    assert_eq!(
        chain.disabled_endpoints.inputs,
        vec![endpoint("io-main", "In 1")],
        "the editor's Save carries no checklist and must not reset it"
    );
}
```

In `crates/application/src/lib.rs`, append:
```rust

#[cfg(test)]
#[path = "local_dispatcher_chain_endpoints_tests.rs"]
mod local_dispatcher_chain_endpoints_tests;
```

In `crates/application/tests/issue_14_command_grouping_preserves_surface.rs`, replace:
```rust
    "SetSplitEnd",
    // ── The surface that existed before the split ─────────────────────────
```
with:
```rust
    "SetSplitEnd",
    "SetChainEndpointEnabled",
    // ── The surface that existed before the split ─────────────────────────
```

In `crates/adapter-mcp/src/tools_tests.rs`, replace:
```rust
/// from any transport.
const COMMAND_VARIANT_COUNT: usize = 102;
```
with:
```rust
/// from any transport.
/// #328 bumped to 103 with `SetChainEndpointEnabled` — the endpoint
/// checklist of the chain graph's input/output nodes.
const COMMAND_VARIANT_COUNT: usize = 103;
```
and append:
```rust

#[test]
fn set_chain_endpoint_enabled_tool_builds_its_command() {
    let args = serde_json::json!({
        "chain": "rig:in", "node": "path_a_output", "io": "io-main",
        "endpoint": "Out 1", "enabled": false
    });
    let cmd = build_command("set_chain_endpoint_enabled", args.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(&cmd).unwrap(),
        serde_json::json!({ "SetChainEndpointEnabled": args })
    );
}
```

- [ ] **Step 2: Run, see them fail**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 local_dispatcher_chain_endpoints_tests ; nice -n 19 cargo test -p application -j 2 --test issue_14_command_grouping_preserves_surface ; nice -n 19 cargo test -p adapter-mcp -j 2`
Expected FAILED:
- all 7 `local_dispatcher_chain_endpoints_tests` — `SetChainEndpointEnabled: unknown command: SetChainEndpointEnabled` (and `an_unknown_chain_is_refused` fails its assertion, printing `unknown command: …` instead of `chain not found`);
- `grouping Command dropped MCP tools that clients already call: ["SetChainEndpointEnabled"]`;
- `parity_guard_every_command_variant_is_a_tool` left `102` right `103`;
- `set_chain_endpoint_enabled_tool_builds_its_command` panics `unknown tool: set_chain_endpoint_enabled`.

- [ ] **Step 3: Add the command, its handler and its route**

In `crates/application/src/command/chain.rs`, after `use project::chain::{Chain, DiOutputRef};` add:
```rust
use project::endpoint_disables::EndpointNode;
```
and after the `SetChainIoBindings { … },` variant add:
```rust

    /// #328: check or uncheck one endpoint of the chain's E/S on one node of
    /// the chain graph — the input node, the output node, or a Y split's path
    /// A / path B output node. The E/S itself is never edited: an unchecked
    /// endpoint stays listed and only stops feeding (or being fed by) that
    /// node. Unknown endpoints are ignored at runtime.
    SetChainEndpointEnabled {
        chain: ChainId,
        node: EndpointNode,
        io: String,
        endpoint: String,
        enabled: bool,
    },
```

Create `crates/application/src/local_dispatcher_chain_endpoints.rs`:
```rust
//! Responsibility: handles the chain endpoint checklist command.
//!
//! #328 (spec §1.3, §5.3): the graph's input and output nodes list every
//! endpoint of the chain's E/S, checked by default. Unchecking one records it
//! in the chain's `disabled_endpoints` for that node only. The rig capture
//! (`sync_synthetic_into_rig`) carries the list into the chain's `RigInput`,
//! which is what `project.yaml` persists.

use anyhow::Result;

use project::endpoint_disables::EndpointRef;

use crate::command::{ChainCommand, Command};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    pub(crate) fn handle_chain_endpoint_enabled(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::Chain(ChainCommand::SetChainEndpointEnabled {
            chain,
            node,
            io,
            endpoint,
            enabled,
        }) = cmd
        else {
            unreachable!("handle_chain_endpoint_enabled received {cmd:?}");
        };
        self.with_chain(&chain, |c| {
            // Part 1 Task 8 owns the toggle rule (idempotent, per node).
            c.disabled_endpoints
                .set_enabled(node, EndpointRef { io, endpoint }, enabled);
            Ok(())
        })?;
        // A routing change: the MCP/MIDI drain re-syncs exactly the chain an
        // event names (`runtime_sync_policy`), never a neighbour.
        Ok(vec![Event::ChainReloaded { chain }, Event::ProjectMutated])
    }
}
```

In `crates/application/src/lib.rs`, after `mod local_dispatcher_chain_crud;` add:
```rust
mod local_dispatcher_chain_endpoints;
```

In `crates/application/src/local_dispatcher_trait.rs`, replace:
```rust
                | ChainCommand::SetChainIoBindings { .. },
            ) => self.handle_chain_crud(cmd),
```
with:
```rust
                | ChainCommand::SetChainIoBindings { .. },
            ) => self.handle_chain_crud(cmd),

            // #328: the endpoint checklist of the chain graph's I/O nodes.
            Command::Chain(ChainCommand::SetChainEndpointEnabled { .. }) => {
                self.handle_chain_endpoint_enabled(cmd)
            }
```

- [ ] **Step 4: Run — the command tests pass, the editor-save test still fails**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 local_dispatcher_chain_endpoints_tests`
Expected: the four command tests PASS. `the_checklist_is_captured_into_the_rig_input` and `the_checklist_survives_a_scene_switch` also PASS here — characterization pins of Part 1 Task 10 (`sync_synthetic_into_rig` captures the checklist, `rig_to_chains` projects it back); if either FAILS, Part 1 Task 10 is missing and this part STOPS (do not re-add the capture here). FAILED:
- `the_chain_editor_save_keeps_the_checklist` — left `[]` (SaveChain replaced the chain whole).

- [ ] **Step 5: Keep the checklist across the editor's Save**

In `crates/application/src/local_dispatcher_chain_save.rs`, replace:
```rust
                    let keep_loopers = std::mem::take(&mut existing.loopers);
                    *existing = chain;
                    existing.enabled = keep_enabled;
                    existing.loopers = keep_loopers;
```
with:
```rust
                    let keep_loopers = std::mem::take(&mut existing.loopers);
                    // #328: the endpoint checklist has its own command; the
                    // editor's Save carries no checklist, so a rename or an
                    // E/S change must not reset it.
                    let keep_disabled = std::mem::take(&mut existing.disabled_endpoints);
                    *existing = chain;
                    existing.enabled = keep_enabled;
                    existing.loopers = keep_loopers;
                    existing.disabled_endpoints = keep_disabled;
```

- [ ] **Step 6: Run, all pass**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 local_dispatcher_chain_endpoints_tests && nice -n 19 cargo test -p application -j 2 ld_savechain && nice -n 19 cargo test -p project -j 2 rig_sync && nice -n 19 cargo test -p application -j 2 --test issue_14_command_grouping_preserves_surface && nice -n 19 cargo test -p application -j 2 --test issue_489_command_schema_completeness && nice -n 19 cargo test -p adapter-mcp -j 2`
Expected: 7 passed in `local_dispatcher_chain_endpoints_tests`; the existing SaveChain and rig_sync tests pass; the surface/schema pins pass; adapter-mcp green (103 tools).

- [ ] **Step 7: Document it**

In `docs/mcp.md`, replace:
```
  Split → Mix and Y → A/B; `remove_split` (`{ chain, split_id }`) puts
  path `a`'s blocks in its place and drops path `b`'s.
```
with:
```
  Split → Mix and Y → A/B; `remove_split` (`{ chain, split_id }`) puts
  path `a`'s blocks in its place and drops path `b`'s.

  `set_chain_endpoint_enabled` (`{ chain, node: "input" | "output" |
  "path_a_output" | "path_b_output", io, endpoint, enabled }`) checks
  or unchecks one endpoint of the chain's E/S on one node of the chain
  graph. The E/S itself is not edited: the unchecked endpoint stays
  listed, is saved with the chain's input in `project.yaml`, and
  survives preset/scene switches and the chain editor's Save.
```

- [ ] **Step 8: Commit and push**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && rustfmt --edition 2021 crates/application/src/command/chain.rs crates/application/src/local_dispatcher_chain_endpoints.rs crates/application/src/local_dispatcher_chain_endpoints_tests.rs crates/application/src/local_dispatcher_trait.rs crates/application/src/local_dispatcher_chain_save.rs crates/application/tests/issue_14_command_grouping_preserves_surface.rs crates/adapter-mcp/src/tools_tests.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/application/src/command/chain.rs crates/application/src/local_dispatcher_chain_endpoints.rs crates/application/src/local_dispatcher_chain_endpoints_tests.rs crates/application/src/local_dispatcher_trait.rs crates/application/src/local_dispatcher_chain_save.rs crates/application/src/lib.rs crates/application/tests/issue_14_command_grouping_preserves_surface.rs crates/adapter-mcp/src/tools_tests.rs docs/mcp.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): SetChainEndpointEnabled records the endpoint checklist and it survives capture, scene switch and editor save"
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch origin
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status -sb
```
If behind: `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase origin feature/issue-328` and re-run Step 6. Then:
```bash
# CLAUDE.md push gate: fmt clean, every `test result: ok.`, no build warning/error line, no `✗ FAIL`. Push only if this line exits 0.
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 2 / Task 7 pushed ($(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD)): SetChainEndpointEnabled + MCP tool (103), toggling through EndpointDisables::set_enabled; the checklist survives rig capture and scene switch (Part 1 Task 10, pinned here); SaveChain keeps the checklist. Files: command/chain.rs, local_dispatcher_chain_endpoints.rs, local_dispatcher_trait.rs, local_dispatcher_chain_save.rs, docs/mcp.md. Tests green."
```

---

### Task 7b: A checklist ref the E/S no longer offers is dropped on save

Spec §1.3: "Unknown refs (the endpoint was removed from the E/S) are ignored at runtime and dropped on the next save." Part 1 Task 11 covers "ignored at runtime" (an unknown ref matches no resolved port) and Part 1 Task 8 ships `EndpointDisables::retain_known` + `endpoint_candidates`, handing the save-path call to this part. Nothing called it: a ref to a deleted endpoint (or a binding removed from the chain's E/S) stayed in `project.yaml` forever. The save path is `CaptureRigEdits` (`local_dispatcher_project.rs:241` dispatches it before writing), and the dispatcher holds the E/S registry (`LocalDispatcher::io_bindings`, `local_dispatcher.rs:139`).

**Files:**
- Create: `crates/project/src/endpoint_prune.rs`
- Modify: `crates/project/src/lib.rs` (add `pub mod endpoint_prune;` after Part 1's `pub mod endpoint_disables;`)
- Modify: `crates/application/src/local_dispatcher_rig.rs:16` (import), `:113` (`handle_capture_rig_edits`)
- Test: append to `crates/application/src/local_dispatcher_chain_endpoints_tests.rs` (Task 7)
- Docs: `docs/audio-config.md` (Part 1's `### Endpoint checklist (issue #328)` section)

**Interfaces:**
- Consumes: Part 1 `project::endpoint_candidates::endpoint_candidates(&[String], &[IoBinding]) -> (Vec<EndpointRef>, Vec<EndpointRef>)`, `EndpointDisables::retain_known(&mut self, inputs: &[EndpointRef], outputs: &[EndpointRef])`, `RigInput.{io_binding_ids, disabled_endpoints}`; Task 7 `SetChainEndpointEnabled`, test helpers `set_enabled`, `endpoint`; fixtures `rig_with_presets`, `rig_session_from`, `RIG_CHAIN`; `LocalDispatcher::attach_io_bindings(Rc<RefCell<Vec<IoBinding>>>)`.
- Produces: `project::endpoint_prune::prune_stale_endpoint_disables(rig: &mut RigProject, registry: &[IoBinding])`; `CaptureRigEdits` prunes after the capture when a registry is attached (no registry → nothing pruned).

- [ ] **Step 1: Write the failing test**

Append to `crates/application/src/local_dispatcher_chain_endpoints_tests.rs` (add `use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};` to its imports; `Rc`, `RefCell` and `DeviceId` come from `crate::local_dispatcher_tests::*`):
```rust

/// The E/S registry of the fixtures' `io-main` binding: In 1, In 2 → Out 1.
fn io_main_registry() -> Rc<RefCell<Vec<IoBinding>>> {
    let ep = |name: &str, ch: usize| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    };
    Rc::new(RefCell::new(vec![IoBinding {
        id: "io-main".into(),
        name: "MAIN".into(),
        inputs: vec![ep("In 1", 0), ep("In 2", 1)],
        outputs: vec![ep("Out 1", 0)],
    }]))
}

#[test]
fn the_save_drops_a_ref_to_an_endpoint_the_io_no_longer_offers() {
    let (rig, _project, dispatcher) =
        rig_session_from(rig_with_presets(vec![("p1", vec![make_core_block("A", true)])]));
    dispatcher.attach_io_bindings(io_main_registry());

    set_enabled(&dispatcher, RIG_CHAIN, "input", "In 1", false);
    // "Gone" was removed from the E/S after the user unchecked it.
    set_enabled(&dispatcher, RIG_CHAIN, "input", "Gone", false);
    dispatcher
        .dispatch(Command::Project(ProjectCommand::CaptureRigEdits))
        .expect("capture (the save path)");

    assert_eq!(
        rig.borrow().inputs["in"].disabled_endpoints.inputs,
        vec![endpoint("io-main", "In 1")],
        "spec §1.3: an unknown ref is dropped on the next save; a known one is kept"
    );
}

#[test]
fn without_a_registry_the_save_prunes_nothing() {
    let (rig, _project, dispatcher) =
        rig_session_from(rig_with_presets(vec![("p1", vec![make_core_block("A", true)])]));

    set_enabled(&dispatcher, RIG_CHAIN, "input", "Gone", false);
    dispatcher
        .dispatch(Command::Project(ProjectCommand::CaptureRigEdits))
        .expect("capture");

    assert_eq!(
        rig.borrow().inputs["in"].disabled_endpoints.inputs,
        vec![endpoint("io-main", "Gone")],
        "no registry attached: nothing is known, so nothing may be dropped"
    );
}
```

- [ ] **Step 2: Run — behavioural RED**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 local_dispatcher_chain_endpoints_tests`
Expected: 1 FAILED — `the_save_drops_a_ref_to_an_endpoint_the_io_no_longer_offers`: `spec §1.3: an unknown ref is dropped on the next save; a known one is kept`, left `[EndpointRef { io: "io-main", endpoint: "In 1" }, EndpointRef { io: "io-main", endpoint: "Gone" }]`, right `[EndpointRef { io: "io-main", endpoint: "In 1" }]`. `without_a_registry_the_save_prunes_nothing` PASSES (characterization pin of the no-registry guard). The Task 7 tests stay green.

- [ ] **Step 3: Prune on capture**

Create `crates/project/src/endpoint_prune.rs`:
```rust
//! Responsibility: drops the checklist refs a rig input's bindings no longer offer.
//!
//! #328 (spec §1.3): a ref to an endpoint the E/S no longer offers is ignored
//! at runtime and dropped on the next save. The save captures the chains into
//! the rig, then this runs over the rig it writes.

use domain::io_binding::IoBinding;

use crate::endpoint_candidates::endpoint_candidates;
use crate::rig::RigProject;

/// Keep, on every rig input, only the unchecked refs its own bindings still
/// offer in `registry`.
pub fn prune_stale_endpoint_disables(rig: &mut RigProject, registry: &[IoBinding]) {
    for input in rig.inputs.values_mut() {
        let (inputs, outputs) = endpoint_candidates(&input.io_binding_ids, registry);
        input.disabled_endpoints.retain_known(&inputs, &outputs);
    }
}
```

In `crates/project/src/lib.rs` add `pub mod endpoint_prune;` right after `pub mod endpoint_disables;` (Part 1 Task 8).

In `crates/application/src/local_dispatcher_rig.rs`, replace:
```rust
use project::rig_sync::sync_synthetic_into_rig;
```
with:
```rust
use project::endpoint_prune::prune_stale_endpoint_disables;
use project::rig_sync::sync_synthetic_into_rig;
```
and replace:
```rust
        sync_synthetic_into_rig(&mut rig.borrow_mut(), &self.project.borrow());
        Ok(vec![Event::ProjectMutated])
```
with:
```rust
        sync_synthetic_into_rig(&mut rig.borrow_mut(), &self.project.borrow());
        // #328 (spec §1.3): the save drops checklist refs to endpoints the
        // E/S no longer offers. With no registry attached nothing is known,
        // so nothing is dropped.
        if let Some(registry) = self.io_bindings.borrow().clone() {
            prune_stale_endpoint_disables(&mut rig.borrow_mut(), &registry.borrow());
        }
        Ok(vec![Event::ProjectMutated])
```

- [ ] **Step 4: Run — GREEN**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 local_dispatcher_chain_endpoints_tests && nice -n 19 cargo test -p application -j 2 local_dispatcher_rig && nice -n 19 cargo test -p project -j 2 --test issue_328_endpoint_disables`
Expected: 9 passed in `local_dispatcher_chain_endpoints_tests`; the rig suites and Part 1's `issue_328_endpoint_disables` unchanged and green.

- [ ] **Step 5: Document it**

In `docs/audio-config.md` (Part 1's `### Endpoint checklist (issue #328)` section), replace:
```
A ref to an endpoint the E/S no longer offers matches nothing and is ignored; `EndpointDisables::retain_known` (fed by `endpoint_candidates`) prunes it.
```
with:
```
A ref to an endpoint the E/S no longer offers matches nothing and is ignored; the next save (`CaptureRigEdits`) drops it from `project.yaml` (`EndpointDisables::retain_known`, fed by `endpoint_candidates` of the chain's own bindings).
```

- [ ] **Step 6: Commit and push**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && rustfmt --edition 2021 crates/project/src/endpoint_prune.rs crates/application/src/local_dispatcher_rig.rs crates/application/src/local_dispatcher_chain_endpoints_tests.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/project/src/endpoint_prune.rs crates/project/src/lib.rs crates/application/src/local_dispatcher_rig.rs crates/application/src/local_dispatcher_chain_endpoints_tests.rs docs/audio-config.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): the save drops checklist refs to endpoints the E/S no longer offers"
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch origin
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status -sb
```
If behind: `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase origin feature/issue-328` and re-run Step 4. Then:
```bash
# CLAUDE.md push gate: fmt clean, every `test result: ok.`, no build warning/error line, no `✗ FAIL`. Push only if this line exits 0.
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 2 / Task 7b pushed ($(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD)): CaptureRigEdits prunes checklist refs the E/S no longer offers (spec §1.3). Files: project endpoint_prune.rs + lib.rs, application local_dispatcher_rig.rs, docs/audio-config.md. Tests green."
```

---

### Task 8: Whole-chain commands refuse a block list that breaks the split rules

**Files:**
- Modify: `crates/application/src/local_dispatcher_chain_crud.rs:5-9` (import), `:16` (`AddChain`), `:40` (`ConfigureChain`)
- Modify: `crates/application/src/local_dispatcher_chain_save.rs:5-9` (import), `:16-17` (`SaveChain`)
- Modify: `crates/application/src/local_dispatcher_chain_io.rs:5-9` (import), `:139-141` (`LoadChainPreset`)
- Create: `crates/application/src/ld_split_chain_doors_tests.rs` (tests)
- Modify: `crates/application/src/lib.rs` (test module)
- Modify: `docs/mcp.md`

**Interfaces:**
- Consumes: `ensure_split_rules`, fixtures.
- Produces: `AddChain`, `ConfigureChain`, `SaveChain` and `LoadChainPreset` return the split-rule error before any mutation (project and rig untouched).

- [ ] **Step 1: Write the failing tests**

Create `crates/application/src/ld_split_chain_doors_tests.rs`:
```rust
//! #328 — the commands that take a whole block list (a chain, a preset) are
//! doors too: a list that breaks a split rule is refused before anything —
//! project or rig — takes it.

use project::block::split_block::SplitEnd;
use project::block::SelectBlock;

use crate::local_dispatcher_tests::*;
use crate::split_tests_fixtures::*;

fn chain_with(project: &Rc<RefCell<Project>>, blocks: Vec<AudioBlock>) -> Chain {
    let mut chain = project.borrow().chains[0].clone();
    chain.blocks = blocks;
    chain
}

#[test]
fn add_chain_refuses_a_chain_with_two_splits() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let mut chain = chain_with(
        &project,
        vec![
            split("s1", SplitEnd::Mix, vec![], vec![]),
            split("s2", SplitEnd::Mix, vec![], vec![]),
        ],
    );
    chain.id = ChainId("chain_new".into());
    chain.enabled = false;

    let err = dispatcher
        .dispatch(Command::Chain(ChainCommand::AddChain { chain }))
        .expect_err("two splits in one chain");

    assert!(err.to_string().contains("at most one split"), "{err}");
    assert_eq!(project.borrow().chains.len(), 1, "nothing was added");
}

#[test]
fn configure_chain_refuses_a_select_inside_a_path() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let select = AudioBlock {
        id: BlockId("sel".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("opt".into()),
            options: vec![make_core_block("opt", true)],
        }),
    };
    let chain = chain_with(&project, vec![split("s1", SplitEnd::Mix, vec![select], vec![])]);

    let err = dispatcher
        .dispatch(Command::Chain(ChainCommand::ConfigureChain { chain }))
        .expect_err("a select cannot sit inside a path");

    assert!(err.to_string().contains("path A"), "{err}");
    assert_eq!(ids(&project.borrow().chains[0].blocks), vec!["amp"]);
}

#[test]
fn save_chain_refuses_a_block_after_a_y_split() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let chain = chain_with(
        &project,
        vec![
            split("s1", SplitEnd::Y, vec![], vec![]),
            make_core_block("late", true),
        ],
    );

    let err = dispatcher
        .dispatch(Command::Chain(ChainCommand::SaveChain { chain }))
        .expect_err("nothing may follow a Y split");

    assert!(err.to_string().contains("Y split"), "{err}");
    assert_eq!(ids(&project.borrow().chains[0].blocks), vec!["amp"]);
}

#[test]
fn load_chain_preset_refuses_a_block_after_a_y_split() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let err = dispatcher
        .dispatch(Command::Chain(ChainCommand::LoadChainPreset {
            chain: ChainId(CHAIN.into()),
            preset_instrument: "electric_guitar".into(),
            preset_blocks: vec![
                split("s1", SplitEnd::Y, vec![], vec![]),
                make_core_block("late", true),
            ],
        }))
        .expect_err("the preset puts a block after its Y split");

    assert!(err.to_string().contains("Y split"), "{err}");
    assert_eq!(ids(&project.borrow().chains[0].blocks), vec!["amp"]);
}

/// Characterization pin (green before and after): a preset whose Y split is
/// its last block loads.
#[test]
fn load_chain_preset_accepts_a_y_split_as_its_last_block() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::Chain(ChainCommand::LoadChainPreset {
            chain: ChainId(CHAIN.into()),
            preset_instrument: "electric_guitar".into(),
            preset_blocks: vec![
                make_core_block("pre", true),
                split("s1", SplitEnd::Y, vec![], vec![]),
            ],
        }))
        .expect("a Y split may end a preset");

    assert_eq!(ids(&project.borrow().chains[0].blocks), vec!["pre", "s1"]);
}
```

In `crates/application/src/lib.rs`, append:
```rust

#[cfg(test)]
#[path = "ld_split_chain_doors_tests.rs"]
mod ld_split_chain_doors;
```

- [ ] **Step 2: Run, see them fail**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 ld_split_chain_doors`
Expected FAILED: `two splits in one chain: [ChainAdded { … }, ProjectMutated]`, `a select cannot sit inside a path: [ChainConfigured { … }, ProjectMutated]`, `nothing may follow a Y split: [ChainSaved { … }, ProjectMutated]`, `the preset puts a block after its Y split: [ChainPresetLoaded { … }]`. `load_chain_preset_accepts_a_y_split_as_its_last_block` PASSES (pin).

- [ ] **Step 3: Guard the four doors**

In `crates/application/src/local_dispatcher_chain_crud.rs`, after `use crate::local_dispatcher::LocalDispatcher;` add `use crate::split_rules::ensure_split_rules;`, then replace:
```rust
            Command::Chain(ChainCommand::AddChain { mut chain }) => {
                // #833: a chain that arrives already enabled must not land on a
```
with:
```rust
            Command::Chain(ChainCommand::AddChain { mut chain }) => {
                // #328: a chain arrives whole — its split must obey the rules
                // before the project or the rig takes it.
                ensure_split_rules(&chain.blocks)?;
                // #833: a chain that arrives already enabled must not land on a
```
and replace:
```rust
            Command::Chain(ChainCommand::ConfigureChain { chain }) => {
                let chain_id = chain.id.clone();
```
with:
```rust
            Command::Chain(ChainCommand::ConfigureChain { chain }) => {
                ensure_split_rules(&chain.blocks)?;
                let chain_id = chain.id.clone();
```

In `crates/application/src/local_dispatcher_chain_save.rs`, after `use crate::local_dispatcher::LocalDispatcher;` add `use crate::split_rules::ensure_split_rules;`, then replace:
```rust
            Command::Chain(ChainCommand::SaveChain { mut chain }) => {
                // Detect upsert vs. create *before* mutating the project.
```
with:
```rust
            Command::Chain(ChainCommand::SaveChain { mut chain }) => {
                // #328: the saved chain's split must obey the rules.
                ensure_split_rules(&chain.blocks)?;
                // Detect upsert vs. create *before* mutating the project.
```

In `crates/application/src/local_dispatcher_chain_io.rs`, after `use crate::local_dispatcher::LocalDispatcher;` add `use crate::split_rules::ensure_split_rules;`, then replace:
```rust
                    merged.extend(outputs);
                    c.blocks = merged;
                    Ok(())
```
with:
```rust
                    merged.extend(outputs);
                    // #328: the loaded preset's split must still obey the
                    // rules once the chain's own ports sit around it.
                    ensure_split_rules(&merged)?;
                    c.blocks = merged;
                    Ok(())
```

- [ ] **Step 4: Run, all pass**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 ld_`
Expected: `ld_split_chain_doors::*` 5 passed; every existing `ld_chain`, `ld_savechain`, `ld_preset` test still passes.

- [ ] **Step 5: Document it**

In `docs/mcp.md`, replace:
```
  was. `remove_block` refuses the split itself, because removing it that
  way would drop both paths.
```
with:
```
  was. `remove_block` refuses the split itself, because removing it that
  way would drop both paths. `add_chain`, `configure_chain`, `save_chain`
  and `load_chain_preset` refuse a block list that breaks the same rules.
```

- [ ] **Step 6: Commit and push**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && rustfmt --edition 2021 crates/application/src/local_dispatcher_chain_crud.rs crates/application/src/local_dispatcher_chain_save.rs crates/application/src/local_dispatcher_chain_io.rs crates/application/src/ld_split_chain_doors_tests.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/application/src/local_dispatcher_chain_crud.rs crates/application/src/local_dispatcher_chain_save.rs crates/application/src/local_dispatcher_chain_io.rs crates/application/src/ld_split_chain_doors_tests.rs crates/application/src/lib.rs docs/mcp.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): whole-chain commands refuse a block list that breaks the split rules"
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch origin
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status -sb
```
If behind: `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase origin feature/issue-328` and re-run Step 4. Then:
```bash
# CLAUDE.md push gate: fmt clean, every `test result: ok.`, no build warning/error line, no `✗ FAIL`. Push only if this line exits 0.
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 2 / Task 8 pushed ($(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD)): AddChain/ConfigureChain/SaveChain/LoadChainPreset refuse split-rule violations. Files: local_dispatcher_chain_crud.rs, _chain_save.rs, _chain_io.rs, docs/mcp.md. cargo test -p application ld_: green."
```

---

### Task 9: `validate_project` walks both paths of a split

**Files:**
- Create: `crates/application/src/validate_split.rs`
- Modify: `crates/application/src/validate.rs:8` (module declaration), `:137-139` (the `validate_params` call at the top of `resolve_block_output_layout`) and the `AudioBlockKind::Split(..)` arm Part 1 added to it (find it with `grep -n "AudioBlockKind::Split" crates/application/src/validate.rs`)
- Create: `crates/application/src/validate_tests_split.rs` (tests)
- Modify: `crates/application/src/validate_tests.rs` (submodule)

**Interfaces:**
- Consumes: `SplitBlock`, `SPLIT_MODE` (value `"dual_mono"` = Mode II), the private `validate::resolve_block_output_layout(chain, block, input_layout) -> Result<AudioChannelLayout>`.
- Produces: `validate::split_layout::resolve_split_output_layout(chain: &Chain, block: &AudioBlock, split: &SplitBlock, input_layout: AudioChannelLayout) -> anyhow::Result<AudioChannelLayout>` (`pub(super)`); errors name the path: `"split '<id>' path <A|B>: <inner error>"`; the split hands `Stereo` to the next block (invariant #5).

- [ ] **Step 1: Write the failing tests**

Create `crates/application/src/validate_tests_split.rs`:
```rust
//! #328 — `validate_project` walks both paths of a split and names the path
//! an invalid block sits in.

use super::helpers::*;
use project::block::split_block::{SplitBlock, SplitEnd};
use project::block::split_params::default_split_params;

fn unknown_delay(id: &str, enabled: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: "nonexistent_model".to_string(),
            params: ParameterSet::default(),
        }),
    }
}

fn stereo_delay(id: &str) -> AudioBlock {
    let model = block_delay::supported_models()
        .iter()
        .copied()
        .find(|model| {
            project::block::schema_for_block_model("delay", model)
                .map(|s| {
                    s.audio_mode
                        .output_layout(block_core::AudioChannelLayout::Stereo)
                        .is_some()
                })
                .unwrap_or(false)
        })
        .expect("block-delay exposes a stereo-capable model");
    let schema =
        project::block::schema_for_block_model("delay", model).expect("delay schema exists");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("delay defaults normalize");
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: model.to_string(),
            params,
        }),
    }
}

fn chain_with_split(a: Vec<AudioBlock>, b: Vec<AudioBlock>) -> Chain {
    test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            AudioBlock {
                id: BlockId("block:split".to_string()),
                enabled: true,
                kind: AudioBlockKind::Split(SplitBlock {
                    end: SplitEnd::Mix,
                    params: default_split_params(),
                    a,
                    b,
                }),
            },
            test_output_block("dev-out", vec![0, 1]),
        ],
    )
}

#[test]
fn validate_project_names_the_split_path_that_holds_an_invalid_block() {
    let project = test_project(vec![chain_with_split(
        vec![],
        vec![unknown_delay("block:bad", true)],
    )]);

    let message = validate_project(&project)
        .expect_err("path B holds an unknown model")
        .to_string();

    assert!(message.contains("path B"), "{message}");
    assert!(message.contains("block:bad"), "{message}");
}

/// Characterization pin: a disabled block inside a path is skipped, like a
/// disabled top-level block.
#[test]
fn validate_project_skips_a_disabled_block_inside_a_path() {
    let project = test_project(vec![chain_with_split(
        vec![unknown_delay("block:off", false)],
        vec![],
    )]);
    assert!(validate_project(&project).is_ok());
}

/// Characterization pin: a split whose paths hold valid blocks validates.
#[test]
fn validate_project_accepts_a_split_with_valid_paths() {
    let project = test_project(vec![chain_with_split(
        vec![stereo_delay("block:delay_a")],
        vec![stereo_delay("block:delay_b")],
    )]);
    assert!(validate_project(&project).is_ok());
}
```

In `crates/application/src/validate_tests.rs`, append:
```rust

#[path = "validate_tests_split.rs"]
mod split_paths;
```

- [ ] **Step 2: Run, see it fail**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 validate::tests::split_paths`
Expected: `validate_project_names_the_split_path_that_holds_an_invalid_block` FAILED — `path B holds an unknown model: ()` (Part 1's placeholder arm does not look inside the paths). Two other red shapes are also correct and mean the same thing: if Part 1's `AudioBlock::validate_params` recurses into the paths (the `Select` precedent at `project/src/block/audio_block_methods.rs:26-32`), the error surfaces from the `validate_params` call at `validate.rs:137-139` as `block 'block:split': …` and the assertion fails with a message that has no `path B`; if Part 1's arm panics, the failure is that panic. The two pins PASS.

- [ ] **Step 3: Implement the walk**

Create `crates/application/src/validate_split.rs`:
```rust
//! Responsibility: resolves the bus layout a split hands to the block after it.
//!
//! #328 (spec §3): each path is walked from the layout the split feeds it —
//! Mode I (`same`) passes the incoming bus, Mode II (`dual_mono`) feeds each
//! path a dual-mono stereo bus — and every enabled block in it must accept
//! what it gets. The mixer (Split → Mix) and each Y output take either layout
//! from a path and emit the stereo bus (invariant #5).

use anyhow::{anyhow, Result};
use block_core::AudioChannelLayout;
use project::block::split_block::SplitBlock;
use project::block::split_params::SPLIT_MODE;
use project::block::AudioBlock;
use project::chain::Chain;

use super::resolve_block_output_layout;

pub(super) fn resolve_split_output_layout(
    chain: &Chain,
    block: &AudioBlock,
    split: &SplitBlock,
    input_layout: AudioChannelLayout,
) -> Result<AudioChannelLayout> {
    let path_input = if split.params.get_string(SPLIT_MODE) == Some("dual_mono") {
        AudioChannelLayout::Stereo
    } else {
        input_layout
    };
    for (side, lane) in [("A", &split.a), ("B", &split.b)] {
        let mut layout = path_input;
        for path_block in lane.iter().filter(|b| b.enabled) {
            layout = resolve_block_output_layout(chain, path_block, layout)
                .map_err(|e| anyhow!("split '{}' path {side}: {e}", block.id.0))?;
        }
    }
    // The split's own knobs last, so a bad block inside a path is reported
    // with its path even when `validate_params` also walks the paths.
    block
        .validate_params()
        .map_err(|error| anyhow!("block '{}': {}", block.id.0, error))?;
    Ok(AudioChannelLayout::Stereo)
}
```

In `crates/application/src/validate.rs`, after `use std::collections::HashMap;` add:
```rust

#[path = "validate_split.rs"]
mod split_layout;
```
replace lines 137-139 (the first statement of `resolve_block_output_layout`):
```rust
    block
        .validate_params()
        .map_err(|error| anyhow!("block '{}': {}", block.id.0, error))?;
```
with:
```rust
    // #328: a split validates its own knobs after walking its paths
    // (`split_layout`), so an invalid block inside a path is named with the
    // path it sits in instead of surfacing here as a bare split error.
    if !matches!(block.kind, AudioBlockKind::Split(_)) {
        block
            .validate_params()
            .map_err(|error| anyhow!("block '{}': {}", block.id.0, error))?;
    }
```
and replace the `AudioBlockKind::Split(..)` arm of `resolve_block_output_layout` (whatever body Part 1 gave it) with:
```rust
        // #328: walk both paths; the split hands the stereo bus onward.
        AudioBlockKind::Split(split_block) => {
            split_layout::resolve_split_output_layout(chain, block, split_block, input_layout)
        }
```

- [ ] **Step 4: Run, all pass**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 validate`
Expected: the 3 split tests pass; every existing `validate::tests::main` / `unit` test passes unchanged.

- [ ] **Step 5: Commit and push**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && rustfmt --edition 2021 crates/application/src/validate.rs crates/application/src/validate_split.rs crates/application/src/validate_tests.rs crates/application/src/validate_tests_split.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/application/src/validate.rs crates/application/src/validate_split.rs crates/application/src/validate_tests.rs crates/application/src/validate_tests_split.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): validate_project walks both split paths and names the failing one"
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch origin
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status -sb
```
If behind: `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase origin feature/issue-328` and re-run Step 4. Then:
```bash
# CLAUDE.md push gate: fmt clean, every `test result: ok.`, no build warning/error line, no `✗ FAIL`. Push only if this line exits 0.
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 2 / Task 9 pushed ($(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD)): validate_project walks path A/B (Mode II feeds stereo), errors name the path. Files: validate.rs, validate_split.rs. cargo test -p application validate: green."
```

---

### Task 10: A preset or scene switch puts the new preset's split in place (end-to-end pin of Part 1 Task 5)

**Ownership note (completeness review):** the production fix — `merge_preserved_ports` keeps only `Input`/`Output`/`Insert` in their slots, so a `Split` travels with the preset — is **Part 1 Task 5** (`crates/application/src/local_dispatcher_rig.rs:160-164`, pinned there by the pure `merge_preserved_ports` tests `switching_to_a_preset_brings_its_own_split` / `switching_to_a_preset_without_a_split_drops_the_old_one`). This task no longer edits that predicate: it adds the two END-TO-END pins below (the real `ApplyRigNav` flow through the dispatcher, plus Part 1 Task 6's `apply_scene` recursion inside a path). They pass on arrival — characterization pins, labelled as such. If either FAILS, Part 1 Task 5 or Task 6 is missing: STOP and report; do not patch `merge_preserved_ports` or `apply_scene` here.

**Files:**
- Modify: `crates/application/src/local_dispatcher_rig.rs` (end of file: test module mount only)
- Create: `crates/application/src/local_dispatcher_rig_split_tests.rs` (tests)

**Interfaces:**
- Consumes: `engine::rig_runtime::switch_and_project_input` (unchanged), fixtures `rig_with_presets`, `rig_session_from`, `split`, `split_of`, `ids`, `RIG_CHAIN`; Part 1 Task 5 (`merge_preserved_ports` ports-only predicate) and Task 6 (`apply_scene` recursion).
- Produces: nothing new in production — two end-to-end pins.

- [ ] **Step 1: Write the characterization pins**

Create `crates/application/src/local_dispatcher_rig_split_tests.rs`:
```rust
//! #328 — a split is PRESET content: switching preset or scene must bring the
//! new preset's split (its paths, its knobs, its scene values), never keep the
//! one the chain had. The port merge used to treat every `is_routing()` block
//! as a chain port, and a split is routing.

use std::collections::BTreeMap;

use project::block::split_block::SplitEnd;
use project::rig::RigScene;

use crate::command::RigNavKind;
use crate::local_dispatcher_tests::*;
use crate::split_tests_fixtures::*;

#[test]
fn switching_preset_puts_the_new_presets_split_in_place_of_the_old_one() {
    let (_rig, project, dispatcher) = rig_session_from(rig_with_presets(vec![
        (
            "p1",
            vec![
                make_core_block("A", true),
                split("S1", SplitEnd::Mix, vec![make_core_block("X", true)], vec![]),
            ],
        ),
        (
            "p2",
            vec![
                make_core_block("B", true),
                split("S2", SplitEnd::Mix, vec![make_core_block("Y", true)], vec![]),
            ],
        ),
    ]));

    dispatcher
        .dispatch(Command::Selection(SelectionCommand::ApplyRigNav {
            chain: ChainId(RIG_CHAIN.into()),
            // Preset position 1 → the 2nd bank key → p2.
            kind: RigNavKind::Preset(1),
        }))
        .expect("switch to p2");

    let project = project.borrow();
    assert_eq!(
        ids(&project.chains[0].blocks),
        vec!["B", "S2"],
        "p2's own split, not p1's"
    );
    assert_eq!(ids(&split_of(&project).a), vec!["Y"]);
}

#[test]
fn switching_scene_applies_the_scene_bypass_to_a_block_inside_a_path() {
    let mut rig = rig_with_presets(vec![(
        "p1",
        vec![
            make_core_block("A", true),
            split("S1", SplitEnd::Mix, vec![make_core_block("X", true)], vec![]),
        ],
    )]);
    let preset = rig.presets.get_mut("p1").expect("p1");
    preset.scenes.insert(1, RigScene::default());
    preset.scenes.insert(
        2,
        RigScene {
            label: None,
            bypass: BTreeMap::from([("X".to_string(), true)]),
            params: BTreeMap::new(),
            volume: None,
        },
    );
    let (_rig, project, dispatcher) = rig_session_from(rig);

    dispatcher
        .dispatch(Command::Selection(SelectionCommand::ApplyRigNav {
            chain: ChainId(RIG_CHAIN.into()),
            kind: RigNavKind::Scene(2),
        }))
        .expect("switch to scene 2");

    assert!(
        !split_of(&project.borrow()).a[0].enabled,
        "scene 2 bypasses X inside path A"
    );
}
```

In `crates/application/src/local_dispatcher_rig.rs`, append at the end of the file:
```rust

#[cfg(test)]
#[path = "local_dispatcher_rig_split_tests.rs"]
mod split_tests;
```

- [ ] **Step 2: Run the pins (expected PASS — characterization)**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 local_dispatcher_rig && nice -n 19 cargo test -p application -j 2 issue_85`
Expected: both split pins PASS (Part 1 Task 5 + Task 6 already landed); every existing rig-nav and #85 port-position test passes unchanged. A FAILED pin (`p2's own split, not p1's` — left `["B", "S1"]`; or `scene 2 bypasses X inside path A`) means Part 1 Task 5/6 is missing — STOP and report.

- [ ] **Step 3: Commit and push**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && rustfmt --edition 2021 crates/application/src/local_dispatcher_rig.rs crates/application/src/local_dispatcher_rig_split_tests.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/application/src/local_dispatcher_rig.rs crates/application/src/local_dispatcher_rig_split_tests.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): pin end to end that preset and scene switches bring the new preset's split"
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch origin
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status -sb
```
If behind: `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase origin feature/issue-328` and re-run Step 2. Then:
```bash
# CLAUDE.md push gate: fmt clean, every `test result: ok.`, no build warning/error line, no `✗ FAIL`. Push only if this line exits 0.
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 2 / Task 10 pushed ($(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD)): end-to-end pins that a preset/scene switch brings the new preset's split (fix is Part 1 Task 5). Files: local_dispatcher_rig.rs (test mount), local_dispatcher_rig_split_tests.rs. cargo test -p application local_dispatcher_rig issue_85: green."
```

---

### Task 10b: A model swap inside a path keeps the active scene on the live block

Closes the "Live re-resolve after that swap" known gap. After Task 1, `ReplaceBlockModel` reaches a block inside a path (`with_block` recurses), and Part 1 Task 7 makes the rig write-back (`write_back_model_swaps`) keep every scene. But `mirror_model_swap_into_rig` (`crates/application/src/local_dispatcher_model_swap_rig.rs:27-48`) looks the block up with two top-level `.find(|b| b.id == *block)`: for a path block both miss, the function returns early, and the live amp keeps the bare defaults `build_default_block` gave it instead of what the active scene resolves (#986 inside a path — the amp in path B changes model on scene 2 and loses scene 2's `input`/`output`).

**Files:**
- Modify: `crates/application/src/local_dispatcher_model_swap_rig.rs:8-9` (import), `:27-48` (the two lookups)
- Test: `crates/application/tests/issue_328_model_swap_in_path.rs` (new; reuses the #986 fixture `crates/application/tests/fixtures/issue_986_scened_preset.yaml`)

**Interfaces:**
- Consumes: Part 1 `project::block::{find_block_mut, AudioBlock, AudioBlockKind, SplitBlock, SplitEnd}` (`find_block_mut(&mut [AudioBlock], &str) -> Option<&mut AudioBlock>`, recursive into paths), Part 1 Task 6/7 (`apply_scene` and `write_back_model_swaps` recurse), Task 1 (`with_block` recurses), `infra_yaml::load_rig_project_file`, `engine::rig_runtime::rig_to_legacy_project`.
- Produces: `mirror_model_swap_into_rig` re-resolves a block anywhere in the chain, paths included. Signature unchanged.

- [ ] **Step 1: Write the failing test**

Create `crates/application/tests/issue_328_model_swap_in_path.rs`:
```rust
//! #328 — #986 inside a split path: changing the model of the amp that sits
//! in path B, while scene 2 is active, must leave the LIVE amp sounding like
//! scene 2 resolves it (its surviving `input`/`output` overrides applied), not
//! like the new model's bare defaults.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;

use domain::ids::{BlockId, ChainId};
use project::block::{find_block_mut, AudioBlock, AudioBlockKind, SplitBlock, SplitEnd};
use project::rig::RigProject;

use application::command::{BlockCommand, Command};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;

const INPUT: &str = "input-7";
const CHAIN: &str = "rig:input-7";
const PRESET: &str = "anal-dig";
const AMP: &str = "rig:input-7:block:amp";
const NEW_MODEL: &str = "chime";

/// The #986 fixture with its amp moved into path B of a Split → Mix, scene 2
/// (the scene that overrides the amp) active.
fn rig_with_the_amp_in_path_b() -> RigProject {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/issue_986_scened_preset.yaml");
    let mut rig = infra_yaml::load_rig_project_file(&path).expect("load the #986 fixture rig");
    let preset = rig.presets.get_mut(PRESET).expect("fixture preset");
    let at = preset
        .blocks
        .iter()
        .position(|b| b.id.0 == AMP)
        .expect("the fixture amp");
    let amp = preset.blocks.remove(at);
    preset.blocks.insert(
        at,
        AudioBlock {
            id: BlockId("rig:input-7:block:split".into()),
            enabled: true,
            kind: AudioBlockKind::Split(SplitBlock {
                b: vec![amp],
                ..SplitBlock::new(SplitEnd::Mix)
            }),
        },
    );
    rig.inputs.get_mut(INPUT).expect("input-7").active_scene = 2;
    rig
}

#[test]
fn a_model_swap_inside_a_path_keeps_the_active_scene_on_the_live_block() {
    let rig = rig_with_the_amp_in_path_b();
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig,
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::new(RefCell::new(rig)));

    dispatcher
        .dispatch(Command::Block(BlockCommand::ReplaceBlockModel {
            chain: ChainId(CHAIN.into()),
            block: BlockId(AMP.into()),
            model_id: NEW_MODEL.into(),
        }))
        .expect("ReplaceBlockModel reaches the amp inside path B");

    let mut project = project.borrow_mut();
    let chain = project
        .chains
        .iter_mut()
        .find(|c| c.id.0 == CHAIN)
        .expect("the rig chain");
    let live = find_block_mut(&mut chain.blocks, AMP).expect("the amp is still in path B");
    let AudioBlockKind::Core(core) = &live.kind else {
        panic!("the amp is a core block");
    };
    assert_eq!(core.model, NEW_MODEL, "the live amp took the new model");
    assert_eq!(
        core.params.get_f32("input"),
        Some(67.0),
        "scene 2's surviving `input` override reaches the live amp"
    );
    assert_eq!(
        core.params.get_f32("output"),
        Some(41.0),
        "scene 2's surviving `output` override reaches the live amp"
    );
}
```

- [ ] **Step 2: Run it — behavioural RED**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 --test issue_328_model_swap_in_path`
Expected: 1 FAILED — `scene 2's surviving `input` override reaches the live amp`, `left: Some(<chime's default input>)` (the value `build_default_block` gave the live amp), `right: Some(67.0)`. The `model` assertion passes (Task 1's recursive `with_block` did the swap); the re-resolve found nothing at the top level and returned early. Paste the FAILED line.

- [ ] **Step 3: Re-resolve the block wherever it sits**

In `crates/application/src/local_dispatcher_model_swap_rig.rs`, replace:
```rust
use domain::ids::{BlockId, ChainId};
use project::rig_sync::sync_synthetic_into_rig;
```
with:
```rust
use domain::ids::{BlockId, ChainId};
use project::block::find_block_mut;
use project::rig_sync::sync_synthetic_into_rig;
```
and replace:
```rust
        let resolved = {
            let rig = rig.borrow();
            rig.inputs.get(input).and_then(|ri| {
                let preset = rig.presets.get(ri.bank.get(&ri.active_preset)?)?;
                preset
                    .apply_scene(ri.active_scene)
                    .into_iter()
                    .find(|b| b.id == *block)
            })
        };
        let Some(resolved) = resolved else {
            return;
        };
        let mut project = self.project.borrow_mut();
        if let Some(live) = project
            .chains
            .iter_mut()
            .find(|c| c.id == *chain)
            .and_then(|c| c.blocks.iter_mut().find(|b| b.id == *block))
        {
            live.kind = resolved.kind;
        }
```
with:
```rust
        // #328: the swapped block may sit inside a split path, on both sides.
        let resolved = {
            let rig = rig.borrow();
            rig.inputs.get(input).and_then(|ri| {
                let preset = rig.presets.get(ri.bank.get(&ri.active_preset)?)?;
                let mut scene_blocks = preset.apply_scene(ri.active_scene);
                find_block_mut(&mut scene_blocks, &block.0).cloned()
            })
        };
        let Some(resolved) = resolved else {
            return;
        };
        let mut project = self.project.borrow_mut();
        if let Some(live) = project
            .chains
            .iter_mut()
            .find(|c| c.id == *chain)
            .and_then(|c| find_block_mut(&mut c.blocks, &block.0))
        {
            live.kind = resolved.kind;
        }
```

- [ ] **Step 4: Run — GREEN, #986 unchanged**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p application -j 2 --test issue_328_model_swap_in_path && nice -n 19 cargo test -p application -j 2 --test issue_986_model_change_keeps_scenes`
Expected: 1 passed; the six #986 tests pass unchanged (a top-level block is found by the same recursive lookup).

- [ ] **Step 5: Commit and push**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && rustfmt --edition 2021 crates/application/src/local_dispatcher_model_swap_rig.rs crates/application/tests/issue_328_model_swap_in_path.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/application/src/local_dispatcher_model_swap_rig.rs crates/application/tests/issue_328_model_swap_in_path.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): a model swap inside a split path keeps the active scene on the live block"
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch origin
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status -sb
```
If behind: `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase origin feature/issue-328` and re-run Step 4. Then:
```bash
# CLAUDE.md push gate: fmt clean, every `test result: ok.`, no build warning/error line, no `✗ FAIL`. Push only if this line exits 0.
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && ! (nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)") && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 2 / Task 10b pushed ($(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD)): a model swap inside a split path re-resolves the live block through the active scene (#986 inside a path). Files: local_dispatcher_model_swap_rig.rs, tests/issue_328_model_swap_in_path.rs. cargo test -p application issue_328_model_swap_in_path issue_986: green."
```

---

### Task 11: Part 2 verification gate

**Files:** none new — this task only proves the part is shippable.

**Interfaces:** Consumes everything above. Produces the green gate.

- [ ] **Step 1: Whole workspace tests**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test --workspace -j 2 2>&1 | tee target/part2-tests.log | grep -E "^test result|FAILED|panicked" | tail -40`
Expected: every `test result: ok.`; no `FAILED`. Then:
Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && grep "ignored" target/part2-tests.log | grep -v "0 ignored"`
Expected: no output (no `#[ignore]`).

- [ ] **Step 2: Zero-warning build and clean formatting**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^warning|^error" | head`
Expected: no output.
Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && cargo fmt --all -- --check`
Expected: no output, exit 0 (the `release → main` gate fails on the `fmt` metric otherwise, CLAUDE.md).

- [ ] **Step 3: Static checks on the whole repo**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates 2>&1 | grep -E "FAIL|block_path|split_rules|local_dispatcher_block_draft|local_dispatcher_split|local_dispatcher_chain_endpoints|endpoint_prune|split\.rs|validate_split" | head -40`
Expected: every new file shows `✓ OK` for its Responsibility header and size; no `✗ FAIL` anywhere.

- [ ] **Step 4: Docs cover every new surface**

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && for t in add_split set_split_end remove_split set_chain_endpoint_enabled move_block insert_prebuilt_block 'path a'; do grep -q "$t" docs/mcp.md || echo "docs/mcp.md is missing: $t"; done; grep -c 'path?' docs/midi.md`
Expected: no `missing` line; the count is 7 (six table rows plus the note).

- [ ] **Step 5: Push and report**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch origin
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status -sb
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 2 (commands + MCP) complete at $(git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 rev-parse --short HEAD): cargo test --workspace green (0 ignored), cargo build --workspace zero warnings, VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates clean. MCP surface: 103 tools (add_split, set_split_end, remove_split, set_chain_endpoint_enabled; path on add_block/insert_prebuilt_block/move_block)."
```
If `status -sb` showed `behind`, first `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 pull --rebase origin feature/issue-328` and repeat Steps 1–3 before pushing.
