# #328 Part 1: Model and persistence — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Contract notes (read first — later parts depend on these):**

- `EndpointRef` follows the shared contract exactly: Part 1 defines `pub struct EndpointRef { pub io: String, pub endpoint: String }` in `crates/project/src/endpoint_disables.rs` (path `project::endpoint_disables::EndpointRef`). The looper's `EndpointRef { binding_id, endpoint }` (`crates/project/src/endpoint_ref.rs:20-24`, #323, re-exported as `project::chain::EndpointRef`) is a DIFFERENT type and is not touched. Parts 2, 4 and 6 already build `EndpointRef { io, endpoint }` from `project::endpoint_disables`; wherever both types are in scope, the checklist one is spelled by its full path. `SetChainEndpointEnabled { .., io, endpoint, .. }` builds `EndpointRef { io, endpoint }`.
- The existing commands name the chain id field **`chain: ChainId`** (`crates/application/src/command/block.rs:77-82`), not `chain_id`. Part 1 adds no command; the commands part must use `chain`.
- Commit messages: `.claude/skills/openrig-code-quality/SKILL.md` (Naming OpenRig) says "Commits in English, no `Co-Authored-By` trailers". That project rule outranks the script-computed trailer, so the commit steps below carry no trailer.

**Goal:** Give the project model a chain split (`AudioBlockKind::Split`) with its knobs, rules, recursive walkers and persistence, plus the per-input endpoint checklist state (`EndpointDisables`) that filters which endpoints a chain resolves.

**Architecture:** The split is a new `AudioBlockKind` variant that nests two `Vec<AudioBlock>` paths, following the `Select` precedent. Every model walker (lookup, descriptors, scenes, write-back, model swap, load-time disable) goes through two new helpers (`block_walk`, `block_params`) so paths are reached in one place. The checklist lives on `RigInput`, is projected onto `Chain`, and is applied inside `resolve_chain_ports`, the single discovery point every engine/cpal consumer already uses.

**Tech Stack:** Rust 2021, serde + serde_yaml 0.9, schemars 0.8, `cargo test`. Crates touched: `project`, `infra-yaml`, `engine`, `application`, `adapter-gui`, `infra-cpal`.

**Spec:** `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328/docs/superpowers/specs/2026-09-28-issue-328-chain-split-graph-design.md` — covers §1.1, §1.2, §1.3, §1.4, §2, the model/YAML rows of §7, the `blocks-catalog` / project-format / `audio-config` rows of §8, and the volume/file-cap rows of §6.

**Depends on:** nothing (Part 1 is the base). The commands part (spec §3), the engine part (spec §4) and the GUI part (spec §5) depend on this part.

## Global Constraints

- Work ONLY in `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328` (branch `feature/issue-328`). Every command runs from that folder (`cd` into it first — the shell cwd resets between calls). Git is always `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 …`, never bare `git`, never `git worktree`, never `git add -A`.
- Builds and tests: `nice -n 19 cargo … -j 2`, one cargo process at a time.
- Production code is edited only with the Edit/Write tools (no `sed`/scripts — CLAUDE.md: "Proibido script regex/sed pra migrar conteúdo — análise caso a caso").
- File caps: "Cap de linhas (`.rs` 600, `.slint` 500) é só o alarme de fumaça — o limite real é a responsabilidade". Test files have no cap.
- "todo arquivo de produção DECLARA sua responsabilidade no cabeçalho — `//! Responsibility: <uma frase>` em `.rs`"; `scripts/validate.sh` rejects a declaration containing ` and `, ` e `, `,`, `;`, `/`, `&` or `+`.
- Tests live in `*_tests.rs` or `crates/*/tests/*.rs`; no inline `mod tests { }` in production files; `#[ignore]` is forbidden.
- "Zero alocação, lock, syscall ou I/O no audio thread. Sem exceção." Part 1 changes no audio-thread code; the only engine production edits run at build/projection time.
- "Isolation entre streams — cada `InputBlock` é um runtime paralelo TOTALMENTE isolado. Sem buffer/lock/route/tap compartilhado." Spec §6: "The mix is DSP inside one segment."
- "Volume por stream IMUTÁVEL … Se `volume_invariants_tests.rs` quebra, a fonte está errada, não o teste." Spec §6: "Chains without a split must produce bit-identical output; `volume_invariants_tests.rs` stays unchanged." `crates/engine/src/volume_invariants_tests.rs` is never edited.
- "TDD red-first OBRIGATÓRIO — proibido implementar/alterar produção sem um teste que falhou ANTES." Each task shows the behavioral red (the assertion line), not only a compile error.
- "Zero warnings (`cargo build` limpo)."
- "Conteúdo de repo sempre em inglês." (code, comments, docs, commits, issue comments).
- "Fix de Linux/Orange Pi/JACK fica atrás de `cfg` guards." Part 1 adds no platform fix; the struct-literal sweeps (Tasks 9, 10) must also reach code behind `cfg(all(target_os = "linux", feature = "jack"))`, which the macOS compiler does not see — each sweep ends with a grep check.
- Push gate (CLAUDE.md, before EVERY push), from the solver folder:
  ```bash
  nice -n 19 cargo fmt --all -- --check
  nice -n 19 cargo test --workspace -j 2
  nice -n 19 cargo build --workspace -j 2
  VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
  ```
  Expected: fmt prints nothing; every suite `ok`, `0 failed`; the build prints no line starting with `warning`; validate ends with 0 errors. A fmt hunk is applied with the Edit tool.
- After every push: `gh issue comment 328 --repo jpfaria/OpenRig --body "<hash> — <files> — push gate green"`.

## Review Focus

1. **Switching preset on a chain that has a split keeps the OLD preset's split.** `is_routing()` becomes true for `Split` (spec §1.1), and `merge_preserved_ports` keeps every routing block from the current chain. Expected: the new preset's split (or none) replaces it. Pinned in Task 5 (`switching_to_a_preset_brings_its_own_split`).
2. **Swapping the amp model inside a path wipes or bakes the scenes.** Without recursion the whole split is treated as one swapped block, so scene 2's values of the OTHER path become every scene's base. Expected: #986 behaviour inside paths. Pinned in Task 7 (`a_model_swap_inside_a_path_keeps_every_scene`).
3. **A knob, bypass or mixer knob edited inside a split is lost on save+reload.** Expected: float edits become the active scene's override, non-float split knobs go to the preset base (#690). Pinned in Task 7 (`a_knob_turned_inside_a_path_is_kept_in_the_active_scene`, `a_mixer_knob_is_a_scene_override_while_the_split_mode_is_preset_wide`).
4. **Opening a preset whose path holds a block this machine cannot load drops the WHOLE split (both amps).** Expected: only that block is dropped, like a top-level block. Pinned in Task 3 (`a_path_block_this_machine_cannot_load_drops_only_that_block`).
5. **Two chains share one E/S, each with the other's inputs unchecked, and the rig refuses the second one.** The rig-side `tap_conflict` reads `io_binding_ids` directly while the chain-side detectors resolve through `resolve_chain_ports` — they would disagree (#924 class). Expected: both chains enabled, all detectors agree. Pinned in Task 11 (`complementary_unchecked_inputs_share_one_binding_without_a_tap_conflict`).
6. **Renaming a chain in the chain editor re-checks every endpoint the user unchecked.** The mechanical sweep writes `disabled_endpoints: Default::default()` into `chain_from_draft`'s edit-mode literal, the exact #826 regression shape (loops and DI output were dropped the same way). Expected: the editor hands back what it does not edit. Pinned in Task 10 (`editing_a_chain_keeps_its_endpoint_checklists`).

---

## File Structure

| File | Action | Responsibility (header) | Lines now |
|---|---|---|---|
| `crates/project/src/block/split_params.rs` | Create (T1) | declares the knob schema of a split block | — |
| `crates/project/src/block/split_block.rs` | Create (T2) | describes the data a split block holds | — |
| `crates/project/src/block/path_ref.rs` | Create (T2) | addresses one path of a chain's split | — |
| `crates/project/src/block/block_params.rs` | Create (T2) | exposes the parameter set a block kind carries | — |
| `crates/project/src/block/split_block_methods.rs` | Create (T4) | states the structural rules a chain's split obeys | — |
| `crates/project/src/block/block_walk.rs` | Create (T6) | visits every block of a list through the paths of its split | — |
| `crates/project/src/format_version.rs` | Create (T3) | picks the on-disk format version a document needs | — |
| `crates/project/src/endpoint_disables.rs` | Create (T8) | records which endpoints of a chain's own bindings each graph node leaves out | — |
| `crates/project/src/endpoint_candidates.rs` | Create (T8) | lists the endpoints a chain's own bindings offer | — |
| `crates/infra-yaml/src/block_yaml_split.rs` | Create (T3) | maps a split block document onto the split block it describes | — |
| `crates/project/src/block/mod.rs` | Modify (T1,T2,T4,T6) | routes the block model's public surface | 44 |
| `crates/project/src/block/types.rs` | Modify (T2) | describes the data a chain block holds | 153 |
| `crates/project/src/block/audio_block_methods.rs` | Modify (T2,T4) | answers what a block declares about itself | 93 |
| `crates/project/src/block/select_block_methods.rs` | Modify (T4) | answers which option a select block is on | 60 |
| `crates/project/src/block/port_duplication.rs` | Modify (T2) | decides whether a port merely repeats the chain's own binding | 30 |
| `crates/project/src/block/param_writer.rs` | Modify (T2) | writes one parameter value into a block | 128 |
| `crates/project/src/lib.rs` | Modify (T3,T8) | routes the project crate's public surface | 47 |
| `crates/project/src/project.rs` | Modify (T6) | describes the legacy chain-based project | 108 |
| `crates/project/src/project_disable_unavailable.rs` | Modify (T2,T6) | disables the blocks this machine cannot resolve | 61 |
| `crates/project/src/rig.rs` | Modify (T3,T6,T9) | describes a rig with its per-input preset banks | 278 |
| `crates/project/src/rig_validate.rs` | Modify (T2,T4) | says whether a rig is internally consistent | 102 |
| `crates/project/src/rig_write_back.rs` | Modify (T7) | writes a chain's edited state back into the rig it came from | 220 |
| `crates/project/src/rig_model_swap.rs` | Modify (T7) | writes a same-slot block model swap into the rig preset it came from | 98 |
| `crates/project/src/rig_sync.rs` | Modify (T10) | captures the projected chains back into the rig | 104 |
| `crates/project/src/migrate.rs` | Modify (T9,T10) | migrates a legacy chain-based project into a rig | 143 |
| `crates/project/src/chain.rs` | Modify (T10) | describes one chain of blocks | 121 |
| `crates/project/src/binding_discovery.rs` | Modify (T11) | resolves which ports a chain gets from the bindings it selected | 234 |
| `crates/infra-yaml/src/block_yaml.rs` | Modify (T2,T3) | maps a block document onto the audio block it describes | 501 |
| `crates/infra-yaml/src/block_yaml_load.rs` | Modify (T3) | reads one audio block out of its YAML value | 194 |
| `crates/infra-yaml/src/lib.rs` | Modify (T3) | routes the YAML crate's public surface | 69 |
| `crates/infra-yaml/src/project_file.rs` | Modify (T3) | maps the `project.yaml` document onto the rig it describes | 145 |
| `crates/infra-yaml/src/preset_yaml.rs` | Modify (T3) | maps a chain preset file onto the blocks it carries | 138 |
| `crates/infra-yaml/src/chain_yaml.rs` | Modify (T10) | maps a chain document onto the chain it describes | 114 |
| `crates/engine/src/rig_projection.rs` | Modify (T10) | projects a rig into the chains the engine runs | 207 |
| `crates/engine/src/rig_tap_conflict.rs` | Modify (T11) | says when two rig inputs would fight over the same tap | 56 |
| `crates/engine/src/runtime_block_builders.rs` | Modify (T2, minimal arm) | (unchanged header) | 520 |
| `crates/application/src/validate.rs` | Modify (T2, minimal arm) | says whether a project is internally consistent | 203 |
| `crates/application/src/local_dispatcher_rig.rs` | Modify (T5) | handles the rig commands | 190 |
| `crates/adapter-gui/src/select_chain_block_callback.rs` | Modify (T2, final arm) | (unchanged header) | 413 |
| `crates/adapter-gui/src/chain_editor.rs` | Modify (T10: edit mode keeps the checklists, #826 class) | holds the chain editor draft | 136 |
| ~46 `RigInput { … }` literal sites (T9), ~340 `Chain { … }` literal sites in ~273 files (T10) | Modify (sweep) | add the new field only | — |
| Tests (all new): `crates/project/tests/issue_328_split_params.rs`, `issue_328_split_block.rs`, `issue_328_split_rules.rs`, `issue_328_split_walkers.rs`, `issue_328_split_write_back.rs`, `issue_328_endpoint_disables.rs`, `issue_328_endpoint_disables_capture.rs`, `issue_328_endpoint_discovery.rs`; `crates/infra-yaml/tests/issue_328_split_yaml.rs`, `issue_328_endpoint_disables_yaml.rs`; `crates/engine/tests/issue_328_endpoint_disables.rs`; `crates/application/src/issue_328_split_preset_switch_tests.rs`; one test added to `crates/infra-cpal/src/io_topology_tests.rs` and one to `crates/adapter-gui/src/chain_editor_tests.rs` | Create/Modify | — | — |
| `docs/blocks-catalog.md` (T1,T4), `docs/projects/project-openrig-format.md` (T3,T4,T7,T9), `docs/audio-config.md` (T11) | Modify | — | 154 / 244 / 1023 |

**Exhaustive `match` on `AudioBlockKind` (grep of every `Kind::Insert(` site, production and tests) — each gets a `Split` arm in Task 2 (Task 3 for the YAML ones):**

1. `crates/project/src/block/types.rs:67` `label()` — final.
2. `crates/project/src/block/types.rs:93` `model_identity()` — final.
3. `crates/project/src/block/audio_block_methods.rs:16` `validate_params` — final (T4 adds the structure check).
4. `crates/project/src/block/audio_block_methods.rs:40` `parameter_descriptors` — final.
5. `crates/project/src/block/audio_block_methods.rs:62` `audio_descriptors` — final.
6. `crates/project/src/block/audio_block_methods.rs:80` `model_ref` — final.
7. `crates/project/src/block/port_duplication.rs:19` — final.
8. `crates/project/src/project_disable_unavailable.rs:50` `block_model_is_available` — final.
9. `crates/project/src/rig_validate.rs:67` — final.
10. `crates/application/src/validate.rs:141` `resolve_block_output_layout` — **minimal; replaced by Part 2 (commands, spec §3 "validate.rs walks both paths"; part-2 Task 9 replaces this arm).**
11. `crates/engine/src/runtime_block_builders.rs:296` `build_block_runtime_node` — **minimal (pass-through); replaced by Part 3 (engine, Split → Mix, spec §4.1 `RuntimeProcessor::Split`; its Task 14 replaces this arm).**
12. `crates/adapter-gui/src/select_chain_block_callback.rs:178` — final (a split is not a port).
13. `crates/infra-yaml/src/block_yaml.rs:353` `from_audio_block` — interim error in T2, real mapping in T3.
14. `crates/infra-yaml/src/block_yaml_load.rs:92` `extract_core_block_fields` (on `AudioBlockYaml`) — T3.

`param_writer::params_mut` (`crates/project/src/block/param_writer.rs:113`) has a wildcard but must reach `Split` (spec §1.2) — T2. No test file matches exhaustively (the only test match, `crates/adapter-gui/src/issue_881_insert_editor_tests.rs:145`, has `other =>`).

**Spec §1.4 walkers deliberately left top-level:** `Chain::input_blocks/insert_blocks/output_blocks/first_input/last_output/has_io` (`crates/project/src/chain.rs:50-112`), the `duplicates_chain_binding` retains (`crates/engine/src/rig_projection.rs:81-83`, `crates/engine/src/rig_runtime_normalize.rs:30-32`) and the mid-port loop of `resolve_chain_ports` (`crates/project/src/binding_discovery.rs:83-112`). They look for `Input`/`Output`/`Insert` blocks, which spec §1.1 forbids inside a path and Task 4 rejects (`validate_params`, `RigProject::validate`); recursing would find nothing and would make their flat `chain.blocks` indices meaningless. Their `Split` handling is the `port_duplication` arm (a split never duplicates a binding, so the retains keep it). `resolve_chain_ports` does change — for the checklists (Task 11).

---

### Task 1: Split and mixer knob schema

**Files:**
- Create: `crates/project/src/block/split_params.rs`
- Modify: `crates/project/src/block/mod.rs:26` (add `pub mod split_params;` after `pub mod select_block_methods;`)
- Modify: `docs/blocks-catalog.md` (new section before the line `## Backends de áudio`, and one table row after the `**Input** / **Output** / **Insert**` row)
- Test: `crates/project/tests/issue_328_split_params.rs`

**Interfaces:**
- Consumes: `project::param::{bool_parameter, enum_parameter, float_parameter, BlockParameterDescriptor, MaterializeContext, ModelParameterSchema, ParameterSet, ParameterSpec, ParameterUnit}` (re-exports of `block_core::param`), `block_core::ModelAudioMode`.
- Produces (module `project::block::split_params`):
  - `pub const SPLIT_MODE: &str = "split_mode"; LEVEL_TO_A = "level_to_a"; LEVEL_TO_B = "level_to_b"; BALANCE_A = "balance_a"; BALANCE_B = "balance_b"; MIX_LEVEL_A = "mix_level_a"; MIX_LEVEL_B = "mix_level_b"; MIX_PAN_A = "mix_pan_a"; MIX_PAN_B = "mix_pan_b"; MIX_B_POLARITY = "mix_b_polarity"; MIX_MASTER = "mix_master"; MIX_MASTER_SUM = "mix_master_sum";`
  - `pub const SPLIT_MODE_SAME = "same"; SPLIT_MODE_DUAL_MONO = "dual_mono"; POLARITY_NORMAL = "normal"; POLARITY_INVERT = "invert"; SPLIT_SCHEMA_ID = "split";`
  - `pub fn split_param_specs() -> Vec<ParameterSpec>` (spec order; group `"split"` for the first five, `"mixer"` for the other seven)
  - `pub fn default_split_params() -> ParameterSet`
  - `pub fn normalize_split_params(params: ParameterSet) -> Result<ParameterSet, String>` (lenient: unknown keys kept, missing filled, out-of-range rejected)
  - `pub fn split_param_descriptors(block_id: &BlockId, params: &ParameterSet) -> Result<Vec<BlockParameterDescriptor>, String>`

- [ ] **Step 1: Write the failing test**

Create `crates/project/tests/issue_328_split_params.rs`:

```rust
//! #328 — the split and mixer knobs (spec §1.2): keys, Ampero defaults and
//! ranges. Every knob lives in `SplitBlock.params` and goes through the
//! ordinary parameter pipeline, so this schema is what the split editor, MIDI
//! mapping, scenes and MCP all read.

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::split_params::{
    default_split_params, normalize_split_params, split_param_descriptors, split_param_specs,
    BALANCE_A, BALANCE_B, LEVEL_TO_A, LEVEL_TO_B, MIX_B_POLARITY, MIX_LEVEL_A, MIX_LEVEL_B,
    MIX_MASTER, MIX_MASTER_SUM, MIX_PAN_A, MIX_PAN_B, SPLIT_MODE,
};
use project::param::{ParameterDomain, ParameterSet};

#[test]
fn defaults_are_the_ampero_defaults() {
    let params = default_split_params();
    let expected = [
        (SPLIT_MODE, ParameterValue::String("same".into())),
        (LEVEL_TO_A, ParameterValue::Float(100.0)),
        (LEVEL_TO_B, ParameterValue::Float(100.0)),
        (BALANCE_A, ParameterValue::Float(0.0)),
        (BALANCE_B, ParameterValue::Float(0.0)),
        (MIX_LEVEL_A, ParameterValue::Float(100.0)),
        (MIX_LEVEL_B, ParameterValue::Float(100.0)),
        (MIX_PAN_A, ParameterValue::Float(0.0)),
        (MIX_PAN_B, ParameterValue::Float(0.0)),
        (MIX_B_POLARITY, ParameterValue::String("normal".into())),
        (MIX_MASTER, ParameterValue::Float(50.0)),
        (MIX_MASTER_SUM, ParameterValue::Bool(false)),
    ];
    for (key, value) in &expected {
        assert_eq!(params.get(key), Some(value), "default of {key}");
    }
    assert_eq!(
        params.values.len(),
        expected.len(),
        "no knob beyond the spec table"
    );
}

#[test]
fn every_knob_declares_the_range_of_the_spec_table() {
    let specs = split_param_specs();
    let domain = |key: &str| {
        specs
            .iter()
            .find(|s| s.path == key)
            .map(|s| s.domain.clone())
            .unwrap_or_else(|| panic!("no spec for {key}"))
    };
    for key in [LEVEL_TO_A, LEVEL_TO_B, MIX_LEVEL_A, MIX_LEVEL_B, MIX_MASTER] {
        assert!(
            matches!(domain(key), ParameterDomain::FloatRange { min, max, .. } if min == 0.0 && max == 100.0),
            "{key} is 0–100"
        );
    }
    for key in [BALANCE_A, BALANCE_B, MIX_PAN_A, MIX_PAN_B] {
        assert!(
            matches!(domain(key), ParameterDomain::FloatRange { min, max, .. } if min == -50.0 && max == 50.0),
            "{key} is −50…+50"
        );
    }
    let options = |key: &str| match domain(key) {
        ParameterDomain::Enum { options } => options.into_iter().map(|o| o.value).collect::<Vec<_>>(),
        other => panic!("{key} is an enum, got {other:?}"),
    };
    assert_eq!(options(SPLIT_MODE), vec!["same", "dual_mono"]);
    assert_eq!(options(MIX_B_POLARITY), vec!["normal", "invert"]);
    assert_eq!(domain(MIX_MASTER_SUM), ParameterDomain::Bool);
}

#[test]
fn normalize_fills_the_missing_knobs_and_keeps_the_set_ones() {
    let mut partial = ParameterSet::default();
    partial.insert(MIX_PAN_A, ParameterValue::Float(-50.0));
    let normalized = normalize_split_params(partial).expect("a partial set normalizes");
    assert_eq!(normalized.get_f32(MIX_PAN_A), Some(-50.0));
    assert_eq!(
        normalized.get_f32(MIX_MASTER),
        Some(50.0),
        "a missing knob takes its default"
    );
}

#[test]
fn normalize_rejects_a_knob_out_of_its_range() {
    let mut params = default_split_params();
    params.insert(MIX_MASTER, ParameterValue::Float(150.0));
    let err = normalize_split_params(params).expect_err("150 is outside 0–100");
    assert!(err.contains(MIX_MASTER), "the error names the knob, got: {err}");
}

#[test]
fn descriptors_address_each_knob_on_the_split_block() {
    let id = BlockId("chain:x:block:split".into());
    let descriptors =
        split_param_descriptors(&id, &default_split_params()).expect("defaults describe");
    let paths: Vec<&str> = descriptors.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            SPLIT_MODE, LEVEL_TO_A, LEVEL_TO_B, BALANCE_A, BALANCE_B, MIX_LEVEL_A, MIX_LEVEL_B,
            MIX_PAN_A, MIX_PAN_B, MIX_B_POLARITY, MIX_MASTER, MIX_MASTER_SUM,
        ]
    );
    assert_eq!(descriptors[0].id.0, "chain:x:block:split::split_mode");
    let groups: Vec<Option<&str>> = descriptors.iter().map(|d| d.group.as_deref()).collect();
    assert_eq!(&groups[..5], &[Some("split"); 5], "the split editor's knobs");
    assert_eq!(&groups[5..], &[Some("mixer"); 7], "the mixer editor's knobs");
}
```

- [ ] **Step 2: Add the compile skeleton**

Create `crates/project/src/block/split_params.rs` with the constants and empty bodies, and add `pub mod split_params;` to `crates/project/src/block/mod.rs` after line 26 (`pub mod select_block_methods;`):

```rust
//! Responsibility: declares the knob schema of a split block.

use domain::ids::BlockId;

use crate::param::{BlockParameterDescriptor, ParameterSet, ParameterSpec};

pub const SPLIT_MODE: &str = "split_mode";
pub const LEVEL_TO_A: &str = "level_to_a";
pub const LEVEL_TO_B: &str = "level_to_b";
pub const BALANCE_A: &str = "balance_a";
pub const BALANCE_B: &str = "balance_b";
pub const MIX_LEVEL_A: &str = "mix_level_a";
pub const MIX_LEVEL_B: &str = "mix_level_b";
pub const MIX_PAN_A: &str = "mix_pan_a";
pub const MIX_PAN_B: &str = "mix_pan_b";
pub const MIX_B_POLARITY: &str = "mix_b_polarity";
pub const MIX_MASTER: &str = "mix_master";
pub const MIX_MASTER_SUM: &str = "mix_master_sum";

pub fn split_param_specs() -> Vec<ParameterSpec> {
    Vec::new()
}

pub fn default_split_params() -> ParameterSet {
    ParameterSet::default()
}

pub fn normalize_split_params(params: ParameterSet) -> Result<ParameterSet, String> {
    Ok(params)
}

pub fn split_param_descriptors(
    _block_id: &BlockId,
    _params: &ParameterSet,
) -> Result<Vec<BlockParameterDescriptor>, String> {
    Ok(Vec::new())
}
```

- [ ] **Step 3: Run the test — behavioral RED**

Run: `nice -n 19 cargo test -p project --test issue_328_split_params -j 2`
Expected: 5 FAILED, e.g.
`defaults_are_the_ampero_defaults … panicked … assertion `left == right` failed: default of split_mode` / `left: None` / `right: Some(String("same"))`,
`every_knob_declares_the_range_of_the_spec_table … panicked … no spec for level_to_a`,
`normalize_rejects_a_knob_out_of_its_range … panicked … 150 is outside 0–100`.

- [ ] **Step 4: Implement the schema**

Replace the whole `crates/project/src/block/split_params.rs`:

```rust
//! Responsibility: declares the knob schema of a split block.
//!
//! #328 (spec §1.2): the knobs of the Ampero II split node and mixer node.
//! They live in `SplitBlock.params` and are edited through the ordinary
//! `SetBlockParameter*` commands, so MIDI mapping, scenes and MCP reach them
//! with no split-specific path. Defaults are the Ampero defaults. A split is
//! not a catalog effect type: this schema never reaches the model registry.

use block_core::ModelAudioMode;
use domain::ids::BlockId;
use domain::value_objects::ParameterValue;

use crate::param::{
    bool_parameter, enum_parameter, float_parameter, BlockParameterDescriptor,
    MaterializeContext, ModelParameterSchema, ParameterSet, ParameterSpec, ParameterUnit,
};

pub const SPLIT_MODE: &str = "split_mode";
pub const LEVEL_TO_A: &str = "level_to_a";
pub const LEVEL_TO_B: &str = "level_to_b";
pub const BALANCE_A: &str = "balance_a";
pub const BALANCE_B: &str = "balance_b";
pub const MIX_LEVEL_A: &str = "mix_level_a";
pub const MIX_LEVEL_B: &str = "mix_level_b";
pub const MIX_PAN_A: &str = "mix_pan_a";
pub const MIX_PAN_B: &str = "mix_pan_b";
pub const MIX_B_POLARITY: &str = "mix_b_polarity";
pub const MIX_MASTER: &str = "mix_master";
pub const MIX_MASTER_SUM: &str = "mix_master_sum";

/// `split_mode`: Ampero Mode I — both paths get the bus.
pub const SPLIT_MODE_SAME: &str = "same";
/// `split_mode`: Ampero Mode II — each path gets a dual-mono feed picked by its balance.
pub const SPLIT_MODE_DUAL_MONO: &str = "dual_mono";
pub const POLARITY_NORMAL: &str = "normal";
pub const POLARITY_INVERT: &str = "invert";
/// Effect type and model name the split's descriptors and errors carry.
pub const SPLIT_SCHEMA_ID: &str = "split";

const SPLIT_GROUP: &str = "split";
const MIXER_GROUP: &str = "mixer";

/// Every split and mixer knob, in the spec table's order. The split editor
/// shows the `split` group, the mixer editor the `mixer` group.
pub fn split_param_specs() -> Vec<ParameterSpec> {
    let split = Some(SPLIT_GROUP);
    let mixer = Some(MIXER_GROUP);
    let percent = |path, label, group| {
        float_parameter(path, label, group, Some(100.0), 0.0, 100.0, 1.0, ParameterUnit::Percent)
    };
    let side = |path, label, group| {
        float_parameter(path, label, group, Some(0.0), -50.0, 50.0, 1.0, ParameterUnit::None)
    };
    vec![
        enum_parameter(
            SPLIT_MODE,
            "Mode",
            split,
            Some(SPLIT_MODE_SAME),
            &[(SPLIT_MODE_SAME, "Same"), (SPLIT_MODE_DUAL_MONO, "Dual mono")],
        ),
        percent(LEVEL_TO_A, "Level to A", split),
        percent(LEVEL_TO_B, "Level to B", split),
        side(BALANCE_A, "Balance A", split),
        side(BALANCE_B, "Balance B", split),
        percent(MIX_LEVEL_A, "Level A", mixer),
        percent(MIX_LEVEL_B, "Level B", mixer),
        side(MIX_PAN_A, "Pan A", mixer),
        side(MIX_PAN_B, "Pan B", mixer),
        enum_parameter(
            MIX_B_POLARITY,
            "B polarity",
            mixer,
            Some(POLARITY_NORMAL),
            &[(POLARITY_NORMAL, "Normal"), (POLARITY_INVERT, "Invert")],
        ),
        float_parameter(
            MIX_MASTER,
            "Master",
            mixer,
            Some(50.0),
            0.0,
            100.0,
            1.0,
            ParameterUnit::Percent,
        ),
        bool_parameter(MIX_MASTER_SUM, "Master sum", mixer, Some(false)),
    ]
}

/// The knobs of a freshly added split: every spec's default.
pub fn default_split_params() -> ParameterSet {
    let mut params = ParameterSet::default();
    for spec in split_param_specs() {
        if let Some(value) = spec.default_value {
            params.insert(spec.path, value);
        }
    }
    params
}

/// Fill the missing knobs with their defaults and reject a value outside its
/// range. Lenient on unknown keys (kept with a warning), like a saved model's
/// params, so a knob added by a newer build does not make this one refuse the
/// file.
pub fn normalize_split_params(params: ParameterSet) -> Result<ParameterSet, String> {
    params.normalized_against(&split_schema())
}

/// The split's knobs as descriptors addressed on `block_id`
/// (`<block_id>::<knob>`), the shape every block editor and MCP read.
pub fn split_param_descriptors(
    block_id: &BlockId,
    params: &ParameterSet,
) -> Result<Vec<BlockParameterDescriptor>, String> {
    let schema = split_schema();
    let normalized = params.normalized_against(&schema)?;
    let ctx = MaterializeContext {
        block_id,
        effect_type: SPLIT_SCHEMA_ID,
        model: SPLIT_SCHEMA_ID,
        audio_mode: schema.audio_mode,
    };
    Ok(schema
        .parameters
        .iter()
        .map(|spec| {
            let current_value = normalized
                .get(&spec.path)
                .cloned()
                .or_else(|| spec.default_value.clone())
                .unwrap_or(ParameterValue::Null);
            spec.materialize(&ctx, current_value)
        })
        .collect())
}

fn split_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: SPLIT_SCHEMA_ID.to_string(),
        model: SPLIT_SCHEMA_ID.to_string(),
        display_name: "Split".to_string(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: split_param_specs(),
    }
}
```

- [ ] **Step 5: Run the test — GREEN**

Run: `nice -n 19 cargo test -p project --test issue_328_split_params -j 2`
Expected: `test result: ok. 5 passed; 0 failed`.

- [ ] **Step 6: Document the knobs**

In `docs/blocks-catalog.md`, after the row `| **Input** / **Output** / **Insert** | I/O | — | standard, standard, external_loop |` add:

```markdown
| **Split** | Chain split: Split → Mix or Y → A/B (#328) | — | split |
```

and immediately before the line `## Backends de áudio` add:

```markdown
## Chain split (#328)

A `Split` block splits the chain into two paths, A and B, like the Ampero II split node. Blocks before it are shared by both paths.

- **Split → Mix** (`end: mix`): path A ∥ path B → mixer → the rest of the chain. Main use: amp A hard left, amp B hard right.
- **Y → A/B** (`end: y`): path A → its outputs, path B → its outputs. No mixer.

Knobs live in `SplitBlock.params` (keys in `project::block::split_params`) and are edited with the ordinary `SetBlockParameter*` commands. Defaults are the Ampero defaults. The split editor shows the `split` group, the mixer editor the `mixer` group.

| Key | Range | Default | Meaning |
|---|---|---|---|
| `split_mode` | `same` \| `dual_mono` | `same` | Ampero Mode I / Mode II |
| `level_to_a`, `level_to_b` | 0–100 | 100 | Linear gain into each path (`x/100`) |
| `balance_a`, `balance_b` | −50…+50 | 0 | Mode II only. −50 = L only, 0 = (L+R)/2, +50 = R only; the path gets dual mono `[s, s]` |
| `mix_level_a`, `mix_level_b` | 0–100 | 100 | Mix only. Linear gain of each path into the mixer |
| `mix_pan_a`, `mix_pan_b` | −50…+50 | 0 | Mix only. Balance law: centre = unity on both sides; the opposite side falls linearly to 0 at ±50 |
| `mix_b_polarity` | `normal` \| `invert` | `normal` | Mix only. Multiplies path B by −1 |
| `mix_master` | 0–100 | 50 | Mix only. Output gain `x/100`; at the default two identical paths sum to unity |
| `mix_master_sum` | bool | false | Mix only. Output becomes dual mono `L = R = (L+R)/2` |
```

- [ ] **Step 7: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/project/src/block/split_params.rs crates/project/src/block/mod.rs crates/project/tests/issue_328_split_params.rs docs/blocks-catalog.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): declare the split and mixer knob schema"
```

- [ ] **Step 8: Push gate, push, comment**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 1 T1 pushed (<hash>): split knob schema — crates/project/src/block/split_params.rs, docs/blocks-catalog.md. Push gate green."
```

---

### Task 2: `AudioBlockKind::Split` exists

**Files:**
- Create: `crates/project/src/block/split_block.rs`, `crates/project/src/block/path_ref.rs`, `crates/project/src/block/block_params.rs`
- Modify: `crates/project/src/block/mod.rs:13-37` (modules + re-exports; line 37 after Task 1's insert)
- Modify: `crates/project/src/block/types.rs:53-103` (variant, `label`, `is_routing`, `model_identity`)
- Modify: `crates/project/src/block/audio_block_methods.rs:16-91`
- Modify: `crates/project/src/block/param_writer.rs:12-14,111-122`
- Modify: `crates/project/src/block/port_duplication.rs:24-27`
- Modify: `crates/project/src/project_disable_unavailable.rs:56-59`
- Modify: `crates/project/src/rig_validate.rs:70-73`
- Modify (minimal, Part 3 Task 14 replaces): `crates/engine/src/runtime_block_builders.rs:321-324`
- Modify (minimal, Part 2 Task 9 replaces): `crates/application/src/validate.rs:187-190`
- Modify (final): `crates/adapter-gui/src/select_chain_block_callback.rs:181-184`
- Modify (interim, Task 3 replaces): `crates/infra-yaml/src/block_yaml.rs:494-499`
- Test: `crates/project/tests/issue_328_split_block.rs`

**Interfaces:**
- Consumes: Task 1 `split_params::{default_split_params, normalize_split_params, split_param_descriptors}`.
- Produces:
  - `project::block::SplitBlock { pub end: SplitEnd, pub params: ParameterSet, pub a: Vec<AudioBlock>, pub b: Vec<AudioBlock> }` (derives `Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema`), `SplitBlock::new(end: SplitEnd) -> SplitBlock`
  - `project::block::SplitEnd { Mix, Y }` (`#[serde(rename_all = "snake_case")]`, `Copy, Eq`), `SplitEnd::as_str(self) -> &'static str` (`"mix"`, `"y"`)
  - `project::block::PathSide { A, B }` (`#[serde(rename_all = "snake_case")]`), `project::block::PathRef { pub split: BlockId, pub side: PathSide }`
  - `AudioBlockKind::Split(SplitBlock)`; `label() == "split"`; `is_routing() == true`; `model_identity()` = `split:<end>|a[<id>=<identity>,…]|b[…]`
  - `project::block::{block_params(&AudioBlockKind) -> Option<&ParameterSet>, block_params_mut(&mut AudioBlockKind) -> Option<&mut ParameterSet>}` (Core, Nam, Split)

- [ ] **Step 1: Write the failing test**

Create `crates/project/tests/issue_328_split_block.rs`:

```rust
//! #328 — `AudioBlockKind::Split` (spec §1.1): the split sits in a chain's
//! block list like any block, carries two paths, and its knobs go through the
//! ordinary parameter pipeline.

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::param_writer::{set_parameter_number, set_parameter_option};
use project::block::split_params::{MIX_PAN_A, SPLIT_MODE};
use project::block::{
    schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, SplitBlock, SplitEnd,
};
use project::param::ParameterSet;

fn delay(id: &str) -> AudioBlock {
    let model = block_delay::supported_models()
        .first()
        .expect("a native delay model")
        .to_string();
    let schema = schema_for_block_model("delay", &model).expect("delay schema");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("delay defaults");
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model,
            params,
        }),
    }
}

fn split(end: SplitEnd, a: Vec<AudioBlock>, b: Vec<AudioBlock>) -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            a,
            b,
            ..SplitBlock::new(end)
        }),
    }
}

fn split_of(block: &AudioBlock) -> &SplitBlock {
    match &block.kind {
        AudioBlockKind::Split(s) => s,
        other => panic!("expected a split, got {}", other.label()),
    }
}

#[test]
fn a_split_reports_its_kind_and_rebuilds_on_toggle() {
    let block = split(SplitEnd::Mix, vec![], vec![]);
    assert_eq!(block.kind.label(), "split");
    assert!(
        block.kind.is_routing(),
        "switching a split changes segments — a rebuild, never the in-place fade"
    );
    assert!(block.model_ref().is_none(), "a split has no single model");
}

#[test]
fn identity_follows_the_paths_structure_but_not_their_knobs() {
    let base = split(SplitEnd::Mix, vec![delay("amp-a")], vec![delay("amp-b")]);
    let identity = |b: &AudioBlock| b.kind.model_identity();

    let mut knob = base.clone();
    if let AudioBlockKind::Split(s) = &mut knob.kind {
        s.a[0].enabled = false;
        s.params.insert(MIX_PAN_A, ParameterValue::Float(-50.0));
    }
    assert_eq!(
        identity(&knob),
        identity(&base),
        "a knob or a bypass is scene state, not structure"
    );

    let added = split(
        SplitEnd::Mix,
        vec![delay("amp-a"), delay("comp")],
        vec![delay("amp-b")],
    );
    assert_ne!(identity(&added), identity(&base), "a block added inside path A is structural");

    let moved = split(SplitEnd::Mix, vec![], vec![delay("amp-a"), delay("amp-b")]);
    assert_ne!(identity(&moved), identity(&base), "a block moved from A to B is structural");

    let other_end = split(SplitEnd::Y, vec![delay("amp-a")], vec![delay("amp-b")]);
    assert_ne!(identity(&other_end), identity(&base), "Mix → Y is structural");

    let mut swapped = base.clone();
    if let AudioBlockKind::Split(s) = &mut swapped.kind {
        if let AudioBlockKind::Core(core) = &mut s.b[0].kind {
            core.model = "another_model".into();
        }
    }
    assert_ne!(
        identity(&swapped),
        identity(&base),
        "a model swap inside a path changes the path's identity"
    );
}

#[test]
fn the_parameters_of_a_split_are_its_knobs() {
    let block = split(SplitEnd::Mix, vec![delay("amp-a")], vec![]);
    let descriptors = block.parameter_descriptors().expect("describe");
    assert_eq!(descriptors.len(), 12, "the twelve split and mixer knobs");
    assert_eq!(descriptors[0].id.0, "split::split_mode");
}

#[test]
fn the_audio_of_a_split_is_the_audio_of_its_path_blocks() {
    let block = split(SplitEnd::Mix, vec![delay("amp-a")], vec![delay("amp-b")]);
    let ids: Vec<String> = block
        .audio_descriptors()
        .expect("describe")
        .into_iter()
        .map(|d| d.block_id.0)
        .collect();
    assert_eq!(ids, vec!["amp-a", "amp-b"]);
}

#[test]
fn validate_params_checks_the_knobs_and_the_path_blocks() {
    assert!(split(SplitEnd::Mix, vec![delay("amp-a")], vec![delay("amp-b")])
        .validate_params()
        .is_ok());
    let mut out_of_range = split(SplitEnd::Mix, vec![], vec![]);
    if let AudioBlockKind::Split(s) = &mut out_of_range.kind {
        s.params.insert(MIX_PAN_A, ParameterValue::Float(80.0));
    }
    let err = out_of_range
        .validate_params()
        .expect_err("80 is outside −50…+50");
    assert!(err.contains(MIX_PAN_A), "the error names the knob, got: {err}");
}

#[test]
fn the_knobs_are_edited_through_the_ordinary_parameter_writers() {
    let mut block = split(SplitEnd::Mix, vec![], vec![]);
    set_parameter_number(&mut block, MIX_PAN_A, -50.0).expect("a split knob is writable");
    set_parameter_option(&mut block, SPLIT_MODE, "dual_mono").expect("an option knob too");
    let s = split_of(&block);
    assert_eq!(s.params.get_f32(MIX_PAN_A), Some(-50.0));
    assert_eq!(s.params.get_string(SPLIT_MODE), Some("dual_mono"));
}
```

- [ ] **Step 2: Add the compile skeleton (types + every exhaustive arm, behaviour left out)**

Create `crates/project/src/block/split_block.rs`:

```rust
//! Responsibility: describes the data a split block holds.
//!
//! #328 (spec §1.1): a chain split. Blocks before it in the chain are shared
//! by both paths. With `Mix` the blocks after it are shared again after the
//! mixer; with `Y` nothing but the chain's own ports may follow it and each
//! path ends at its own outputs. The split and mixer knobs live in `params`
//! (see [`super::split_params`]).

use serde::{Deserialize, Serialize};

use super::split_params::default_split_params;
use super::types::AudioBlock;
use crate::param::ParameterSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SplitBlock {
    pub end: SplitEnd,
    pub params: ParameterSet,
    pub a: Vec<AudioBlock>,
    pub b: Vec<AudioBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SplitEnd {
    /// Split → Mix: both paths meet at the mixer and the chain continues.
    Mix,
    /// Y → A/B: each path ends at its own output node.
    Y,
}

impl SplitEnd {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mix => "mix",
            Self::Y => "y",
        }
    }
}

impl SplitBlock {
    /// An empty split with the Ampero default knobs.
    pub fn new(end: SplitEnd) -> Self {
        Self {
            end,
            params: default_split_params(),
            a: Vec::new(),
            b: Vec::new(),
        }
    }
}
```

Create `crates/project/src/block/path_ref.rs`:

```rust
//! Responsibility: addresses one path of a chain's split.
//!
//! #328 (spec §3): the commands that add, insert or move a block take an
//! optional `PathRef`; `None` means the chain's top-level block list.

use domain::ids::BlockId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PathSide {
    A,
    B,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PathRef {
    pub split: BlockId,
    pub side: PathSide,
}
```

Create `crates/project/src/block/block_params.rs`:

```rust
//! Responsibility: exposes the parameter set a block kind carries.

use super::types::AudioBlockKind;
use crate::param::ParameterSet;

/// The knobs of a block: a model's parameters for `Core` and `Nam`, the split
/// and mixer knobs for `Split` (#328). Ports, inserts and selects carry none —
/// a select's knobs live on its options. Exhaustive on purpose: a new kind
/// must decide here whether scenes, write-back and the parameter commands
/// reach it.
pub fn block_params(kind: &AudioBlockKind) -> Option<&ParameterSet> {
    match kind {
        AudioBlockKind::Core(core) => Some(&core.params),
        AudioBlockKind::Nam(nam) => Some(&nam.params),
        AudioBlockKind::Split(split) => Some(&split.params),
        AudioBlockKind::Select(_)
        | AudioBlockKind::Input(_)
        | AudioBlockKind::Output(_)
        | AudioBlockKind::Insert(_) => None,
    }
}

/// Mutable twin of [`block_params`].
pub fn block_params_mut(kind: &mut AudioBlockKind) -> Option<&mut ParameterSet> {
    match kind {
        AudioBlockKind::Core(core) => Some(&mut core.params),
        AudioBlockKind::Nam(nam) => Some(&mut nam.params),
        AudioBlockKind::Split(split) => Some(&mut split.params),
        AudioBlockKind::Select(_)
        | AudioBlockKind::Input(_)
        | AudioBlockKind::Output(_)
        | AudioBlockKind::Insert(_) => None,
    }
}
```

In `crates/project/src/block/mod.rs` replace lines 13-37 (the `pub mod` list through the closing `};` of `pub use types::{…}` — one line longer than on the base branch because Task 1 added `pub mod split_params;`) with:

```rust
pub mod audio_block_methods;
pub mod block_params;
pub mod core_block_methods;
mod disk_audio_mode;
pub mod dispatch;
mod grid_schema;
mod ir_schema;
mod lv2_bundle_ports;
mod lv2_schema;
pub mod manifest_labels;
pub mod methods;
mod nam_schema;
pub mod param_writer;
pub mod path_ref;
pub mod port_duplication;
pub mod select_block_methods;
pub mod split_block;
pub mod split_params;
pub mod types;
pub mod vst3_model_id;
pub mod vst3_schema;

pub use block_params::{block_params, block_params_mut};
pub use dispatch::{build_audio_block_kind, normalize_block_params, schema_for_block_model};
pub use path_ref::{PathRef, PathSide};
pub use port_duplication::duplicates_chain_binding;
pub use split_block::{SplitBlock, SplitEnd};
pub use types::{
    AudioBlock, AudioBlockKind, BlockAudioDescriptor, BlockModelRef, CoreBlock, InputBlock,
    InsertBlock, NamBlock, OutputBlock, SelectBlock,
};
```

In `crates/project/src/block/types.rs`: add `use super::split_block::SplitBlock;` after line 11 (`use block_core::ModelAudioMode;`); add `Split(SplitBlock),` after `Insert(InsertBlock),` (line 60); in `label()` add `Self::Split(_) => "split",` after line 74; in `model_identity()` add after line 100:

```rust
            Self::Split(b) => format!("split:{}", b.end.as_str()),
```

In `crates/project/src/block/audio_block_methods.rs` extend the three "no knobs" arms (lines 33, 52, 73) from `AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_) => {` to

```rust
            AudioBlockKind::Input(_)
            | AudioBlockKind::Output(_)
            | AudioBlockKind::Insert(_)
            | AudioBlockKind::Split(_) => {
```

and the `model_ref` `None` arm (lines 87-90) to `AudioBlockKind::Select(_) | AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_) | AudioBlockKind::Split(_) => None,`.

In `crates/project/src/block/port_duplication.rs` lines 24-27 add `| AudioBlockKind::Split(_)` after `| AudioBlockKind::Insert(_)` (a split carries no binding — final).

In `crates/project/src/project_disable_unavailable.rs` lines 56-59 add `| AudioBlockKind::Split(_)` to the `=> true` arm (composite kind — final).

In `crates/project/src/rig_validate.rs` lines 70-73 add `| AudioBlockKind::Split(_)` to the `=> continue` arm (not a port — final).

In `crates/engine/src/runtime_block_builders.rs` after line 324 (the closing `}` of the Input/Output/Insert arm) add:

```rust
        // #328: minimal arm — the engine part (spec §4.1) builds
        // `RuntimeProcessor::Split` here. Until then a split passes the bus
        // through untouched.
        AudioBlockKind::Split(_) => bypass_runtime_node(block, input_layout, content_mono),
```

In `crates/application/src/validate.rs` after line 190 (the closing `}` of the Input/Output/Insert arm) add:

```rust
        // #328: minimal arm — the commands part (spec §3) walks both paths and
        // the layout each one hands the mixer. The split's knobs and its path
        // blocks are already checked by `validate_params` above, and the mixer
        // always hands the rest of the chain a stereo bus.
        AudioBlockKind::Split(_) => Ok(AudioChannelLayout::Stereo),
```

In `crates/adapter-gui/src/select_chain_block_callback.rs` lines 181-184 add `| AudioBlockKind::Split(_)` after `| AudioBlockKind::Insert(_)` in the `=> None` arm (a split is not a port; the GUI part opens the split editor from its graph node — final).

In `crates/infra-yaml/src/block_yaml.rs` after line 498 (the end of the `AudioBlockKind::Insert` arm of `from_audio_block`) add the interim arm that Task 3 replaces:

```rust
            AudioBlockKind::Split(_) => Err(anyhow!(
                "block '{}': writing a split to a chain preset is not supported by this build",
                block.id.0
            )),
```

- [ ] **Step 3: Run the test — behavioral RED**

Run: `nice -n 19 cargo test -p project --test issue_328_split_block -j 2`
Expected: 6 FAILED:
- `a_split_reports_its_kind_and_rebuilds_on_toggle` — `switching a split changes segments — a rebuild, never the in-place fade`
- `identity_follows_the_paths_structure_but_not_their_knobs` — `assertion `left != right` failed: a block added inside path A is structural`
- `the_parameters_of_a_split_are_its_knobs` — `left: 0` / `right: 12`
- `the_audio_of_a_split_is_the_audio_of_its_path_blocks` — `left: []` / `right: ["amp-a", "amp-b"]`
- `validate_params_checks_the_knobs_and_the_path_blocks` — `80 is outside −50…+50`
- `the_knobs_are_edited_through_the_ordinary_parameter_writers` — `a split knob is writable: block kind 'split' does not carry an editable ParameterSet`

- [ ] **Step 4: Implement the behaviour**

In `crates/project/src/block/types.rs`, replace the `is_routing` doc and body (lines 78-85):

```rust
    /// Whether this block is ROUTING rather than an in-place processor: a mid
    /// `Input`/`Output` port or an `Insert` (the runtime builds no node for
    /// them — `runtime_segments` splits the chain on the enabled ones), or a
    /// `Split` (#328: its end and paths decide the chain's segments). Enabling
    /// or disabling one is a topology change that only a rebuild can apply,
    /// never the in-place block fade (#85/#881).
    pub fn is_routing(&self) -> bool {
        matches!(
            self,
            Self::Input(_) | Self::Output(_) | Self::Insert(_) | Self::Split(_)
        )
    }
```

replace the skeleton split arm of `model_identity()` with:

```rust
            // #328: a split's structure is its end plus, per path, each block's
            // id and model identity — so adding, removing, moving or swapping a
            // block inside a path is structural, while a knob or bypass is not.
            Self::Split(b) => format!(
                "split:{}|a[{}]|b[{}]",
                b.end.as_str(),
                path_identity(&b.a),
                path_identity(&b.b)
            ),
```

and add after the `impl AudioBlockKind` block (after line 103):

```rust
/// #328: one split path's structure — each block's id and model identity, in order.
fn path_identity(blocks: &[AudioBlock]) -> String {
    blocks
        .iter()
        .map(|b| format!("{}={}", b.id.0, b.kind.model_identity()))
        .collect::<Vec<_>>()
        .join(",")
}
```

In `crates/project/src/block/audio_block_methods.rs`: add `use super::split_params::{normalize_split_params, split_param_descriptors};` after line 9; remove `| AudioBlockKind::Split(_)` from the three skeleton arms again and add these arms before each of them:

`validate_params` (before the Input/Output/Insert arm):

```rust
            AudioBlockKind::Split(split) => {
                normalize_split_params(split.params.clone())?;
                for block in split.a.iter().chain(&split.b) {
                    block.validate_params()?;
                }
                Ok(())
            }
```

`parameter_descriptors`:

```rust
            // #328: a split's own parameters are its split and mixer knobs;
            // its path blocks describe themselves.
            AudioBlockKind::Split(split) => split_param_descriptors(&self.id, &split.params),
```

`audio_descriptors`:

```rust
            AudioBlockKind::Split(split) => {
                let mut descriptors = Vec::new();
                for block in split.a.iter().chain(&split.b) {
                    descriptors.extend(block.audio_descriptors()?);
                }
                Ok(descriptors)
            }
```

In `crates/project/src/block/param_writer.rs`: replace lines 12-14 with

```rust
//! Only `Core`, `Nam` and `Split` (#328: the split and mixer knobs) carry a
//! `ParameterSet` (see [`super::block_params`]); `Input`, `Output`, `Insert`
//! and `Select` expose no editable parameters through these commands.
```

replace `use super::types::{AudioBlock, AudioBlockKind};` (line 19) with `use super::block_params::block_params_mut;` and `use super::types::AudioBlock;`, and replace `params_mut` (lines 111-122) with:

```rust
/// Return a mutable reference to the `ParameterSet` of `block`, or an error
/// if the block kind does not carry one.
fn params_mut(block: &mut AudioBlock) -> Result<&mut block_core::param::ParameterSet> {
    let label = block.kind.label();
    block_params_mut(&mut block.kind).ok_or_else(|| {
        anyhow!(
            "block kind '{}' does not carry an editable ParameterSet",
            label
        )
    })
}
```

- [ ] **Step 5: Run the test — GREEN**

Run: `nice -n 19 cargo test -p project --test issue_328_split_block -j 2`
Expected: `test result: ok. 6 passed; 0 failed`.
Then `nice -n 19 cargo test -p project -j 2` — every existing project suite still passes (`param_writer_tests` keep their error message).

- [ ] **Step 6: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/project/src/block/split_block.rs crates/project/src/block/path_ref.rs crates/project/src/block/block_params.rs crates/project/src/block/mod.rs crates/project/src/block/types.rs crates/project/src/block/audio_block_methods.rs crates/project/src/block/param_writer.rs crates/project/src/block/port_duplication.rs crates/project/src/project_disable_unavailable.rs crates/project/src/rig_validate.rs crates/engine/src/runtime_block_builders.rs crates/application/src/validate.rs crates/adapter-gui/src/select_chain_block_callback.rs crates/infra-yaml/src/block_yaml.rs crates/project/tests/issue_328_split_block.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): add the Split block kind with its paths and knobs"
```

- [ ] **Step 7: Push gate, push, comment**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 1 T2 pushed (<hash>): AudioBlockKind::Split + SplitBlock/SplitEnd/PathRef/PathSide + block_params; minimal arms in application/validate.rs (commands part) and engine/runtime_block_builders.rs (engine part). Push gate green."
```

---

### Task 3: Persist the split (`!Split`, `type: split`, version 2 only with a split)

**Files:**
- Create: `crates/project/src/format_version.rs`, `crates/infra-yaml/src/block_yaml_split.rs`
- Modify: `crates/project/src/lib.rs:27` (add `pub mod format_version;` after `pub mod endpoint_ref;`)
- Modify: `crates/project/src/rig.rs:152-157` (doc of the two consts)
- Modify: `crates/project/src/block/path_ref.rs` (add `PathSide::as_str`)
- Modify: `crates/infra-yaml/src/block_yaml.rs:9-12,215-216,230,301,494-499` (import, variant, `pub(crate)`, load arm, save arm)
- Modify: `crates/infra-yaml/src/block_yaml_load.rs:190-193` (unreachable arm)
- Modify: `crates/infra-yaml/src/lib.rs:15` (add `mod block_yaml_split;` after `mod block_yaml_save;`)
- Modify: `crates/infra-yaml/src/project_file.rs:11,33-58`
- Modify: `crates/infra-yaml/src/preset_yaml.rs:34-41,122`
- Modify: `docs/projects/project-openrig-format.md` (model table, new section, versioning section)
- Test: `crates/infra-yaml/tests/issue_328_split_yaml.rs`

**Interfaces:**
- Consumes: Task 2 `SplitBlock`, `SplitEnd`, `PathSide`; Task 1 `split_params::normalize_split_params`.
- Produces:
  - `project::format_version::{SPLIT_FORMAT_VERSION: u32 = 2, MAX_READABLE_FORMAT_VERSION: u32 = 2, blocks_need_split_format(&[AudioBlock]) -> bool, project_format_version(&RigProject) -> u32, preset_format_version(&[AudioBlock]) -> u32}`
  - `PathSide::as_str(self) -> &'static str` (`"a"`, `"b"`)
  - YAML: `type: split` with `enabled`, `end`, `params`, `a`, `b`; path block ids `<split id>::a:<i>` / `<split id>::b:<i>`.

- [ ] **Step 1: Write the failing test**

Create `crates/infra-yaml/tests/issue_328_split_yaml.rs`:

```rust
//! #328 — persistence of a chain split (spec §2).
//!
//! `project.yaml` carries the split through the derive (`kind: !Split`).
//! Chain presets and legacy project files carry it as `type: split` with
//! positional path blocks, loaded as `<split>::a:<i>` / `<split>::b:<i>`. A
//! document that holds a split is `version: 2`; a split-free one stays at
//! `version: 1`, so an older build keeps opening it.

use std::collections::BTreeMap;
use std::fs;

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use infra_yaml::{
    load_chain_preset_file, parse_rig_project, save_chain_preset_file, serialize_rig_project,
    ChainBlocksPreset, YamlProjectRepository,
};
use project::block::split_params::{MIX_PAN_A, MIX_PAN_B};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, SplitBlock, SplitEnd};
use project::param::ParameterSet;
use project::rig::{RigInput, RigPreset, RigProject};
use tempfile::tempdir;

fn delay_model() -> String {
    block_delay::supported_models()
        .first()
        .expect("a native delay model")
        .to_string()
}

fn delay(id: &str) -> AudioBlock {
    let model = delay_model();
    let schema = project::block::schema_for_block_model("delay", &model).expect("delay schema");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("delay defaults");
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model,
            params,
        }),
    }
}

/// Amp A panned hard left, amp B hard right — the main use of Split → Mix.
fn dual_amp_split(id: &str) -> AudioBlock {
    let mut split = SplitBlock::new(SplitEnd::Mix);
    split.a = vec![delay(&format!("{id}::a:0"))];
    split.b = vec![delay(&format!("{id}::b:0"))];
    split.params.insert(MIX_PAN_A, ParameterValue::Float(-50.0));
    split.params.insert(MIX_PAN_B, ParameterValue::Float(50.0));
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Split(split),
    }
}

fn rig_with(blocks: Vec<AudioBlock>) -> RigProject {
    RigProject {
        name: Some("Split".into()),
        inputs: BTreeMap::from([(
            "g".to_string(),
            RigInput {
                label: None,
                bank: BTreeMap::from([(1, "p".to_string())]),
                active_preset: 1,
                active_scene: 1,
                routing: Vec::new(),
                instrument: "electric_guitar".into(),
                io: String::new(),
                endpoint: String::new(),
                io_binding_ids: Vec::new(),
                loopers: Vec::new(),
            },
        )]),
        outputs: BTreeMap::new(),
        presets: BTreeMap::from([("p".to_string(), RigPreset::from_legacy_blocks(blocks, 100.0))]),
        midi: None,
        chain_order: Vec::new(),
    }
}

fn split_of(block: &AudioBlock) -> &SplitBlock {
    match &block.kind {
        AudioBlockKind::Split(s) => s,
        other => panic!("expected a split, got {}", other.label()),
    }
}

#[test]
fn a_project_with_a_split_round_trips_and_is_stamped_version_2() {
    let rig = rig_with(vec![delay("pre"), dual_amp_split("split"), delay("post")]);
    let yaml = serialize_rig_project(&rig).expect("serialize");
    assert!(yaml.contains("!Split"), "the split is a tagged kind, got:\n{yaml}");
    assert!(
        yaml.contains("version: 2\n"),
        "a file holding a split is version 2, got:\n{yaml}"
    );
    let back = parse_rig_project(&yaml).expect("a version 2 file loads in this build");
    assert_eq!(back, rig, "every path block and knob survives the round trip");
}

#[test]
fn a_split_free_project_stays_version_1() {
    let yaml = serialize_rig_project(&rig_with(vec![delay("pre")])).expect("serialize");
    assert!(
        yaml.contains("version: 1\n") && !yaml.contains("version: 2"),
        "no split, no bump — older builds keep opening it, got:\n{yaml}"
    );
}

#[test]
fn this_build_reads_version_2_and_refuses_version_3() {
    let v1 = serialize_rig_project(&rig_with(vec![delay("pre")])).expect("serialize");
    parse_rig_project(&v1.replacen("version: 1", "version: 2", 1))
        .expect("version 2 is readable");
    let err = parse_rig_project(&v1.replacen("version: 1", "version: 3", 1))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("newer") && err.contains("max 2"),
        "a newer file is refused cleanly, got: {err}"
    );
}

#[test]
fn a_chain_preset_writes_type_split_with_positional_path_blocks() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("dual.yaml");
    let preset = ChainBlocksPreset {
        id: "dual".into(),
        name: Some("Dual".into()),
        volume: 100.0,
        instrument: "electric_guitar".into(),
        blocks: vec![dual_amp_split("whatever")],
    };
    save_chain_preset_file(&path, &preset).expect("save");
    let raw = fs::read_to_string(&path).expect("read back");
    assert!(
        raw.contains("type: split") && raw.contains("version: 2"),
        "got:\n{raw}"
    );

    let loaded = load_chain_preset_file(&path).expect("load");
    let split = split_of(&loaded.blocks[0]);
    assert_eq!(loaded.blocks[0].id.0, "preset:dual:block:0");
    assert_eq!(split.a[0].id.0, "preset:dual:block:0::a:0");
    assert_eq!(split.b[0].id.0, "preset:dual:block:0::b:0");
    assert_eq!(split.end, SplitEnd::Mix);
    assert_eq!(split.params.get_f32(MIX_PAN_A), Some(-50.0));
    assert_eq!(split.params.get_f32(MIX_PAN_B), Some(50.0));
}

#[test]
fn a_path_block_this_machine_cannot_load_drops_only_that_block() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("partial.yaml");
    let model = delay_model();
    // No `version:` key on purpose: this pins the per-block drop inside a
    // path, not the version gate (which `this_build_reads_version_2…` and
    // `a_chain_preset_writes_type_split…` pin). With `version: 2` the red
    // before Step 4 would be the "newer version" refusal, not the drop.
    fs::write(
        &path,
        format!(
            "id: partial\nblocks:\n  - type: split\n    end: mix\n    a:\n      - type: delay\n        model: {model}\n      - type: delay\n        model: no_such_delay_328\n    b:\n      - type: delay\n        model: {model}\n"
        ),
    )
    .expect("write");

    let loaded = load_chain_preset_file(&path).expect("load");
    assert_eq!(loaded.blocks.len(), 1, "the split itself survives");
    let split = split_of(&loaded.blocks[0]);
    let a_ids: Vec<&str> = split.a.iter().map(|b| b.id.0.as_str()).collect();
    assert_eq!(
        a_ids,
        vec!["preset:partial:block:0::a:0"],
        "only the block this machine cannot load is dropped"
    );
    assert_eq!(split.b.len(), 1, "path B is untouched");
}

#[test]
fn a_legacy_project_file_reads_type_split() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("legacy.yaml");
    let model = delay_model();
    fs::write(
        &path,
        format!(
            "chains:\n  - instrument: electric_guitar\n    blocks:\n      - type: split\n        end: y\n        a:\n          - type: delay\n            model: {model}\n        b: []\n"
        ),
    )
    .expect("write");
    let project = YamlProjectRepository { path }
        .load_current_project()
        .expect("legacy load");
    let split = split_of(&project.chains[0].blocks[0]);
    assert_eq!(split.end, SplitEnd::Y);
    assert_eq!(split.a[0].id.0, "chain:0:block:0::a:0");
}
```

- [ ] **Step 2: Add the compile skeleton (version helpers that still answer 1, wired into the readers/writers)**

Create `crates/project/src/format_version.rs`:

```rust
//! Responsibility: picks the on-disk format version a document needs.

use crate::block::AudioBlock;
use crate::rig::{RigProject, PRESET_FORMAT_VERSION, PROJECT_FORMAT_VERSION};

pub const SPLIT_FORMAT_VERSION: u32 = 2;
pub const MAX_READABLE_FORMAT_VERSION: u32 = PROJECT_FORMAT_VERSION;

pub fn blocks_need_split_format(_blocks: &[AudioBlock]) -> bool {
    false
}

pub fn project_format_version(_rig: &RigProject) -> u32 {
    PROJECT_FORMAT_VERSION
}

pub fn preset_format_version(_blocks: &[AudioBlock]) -> u32 {
    PRESET_FORMAT_VERSION
}
```

Add `pub mod format_version;` to `crates/project/src/lib.rs` after line 27 (`pub mod endpoint_ref;`).

In `crates/infra-yaml/src/project_file.rs` replace line 11 with

```rust
use project::format_version::{project_format_version, MAX_READABLE_FORMAT_VERSION};
use project::rig::RigProject;
```

replace lines 36-42 with

```rust
    if file.version > MAX_READABLE_FORMAT_VERSION {
        return Err(anyhow!(
            "project.yaml version {} is newer than this build supports \
             (max {MAX_READABLE_FORMAT_VERSION}); please upgrade OpenRig",
            file.version
        ));
    }
```

and replace lines 50-58 with

```rust
/// Serialize a [`RigProject`] back to a `project.yaml` YAML string,
/// stamping the format version its content needs (#328: `2` only when a
/// preset holds a split, so a split-free project stays readable by builds
/// that predate the split).
pub fn serialize_rig_project(project: &RigProject) -> Result<String> {
    let file = RigProjectFile {
        version: project_format_version(project),
        project: project.clone(),
    };
    serde_yaml::to_string(&file).context("failed to serialize project.yaml")
}
```

In `crates/infra-yaml/src/preset_yaml.rs` replace lines 34-42 with

```rust
    if dto.version > project::format_version::MAX_READABLE_FORMAT_VERSION {
        anyhow::bail!(
            "preset {:?} version {} is newer than this build supports (max {}); \
             please upgrade OpenRig",
            path,
            dto.version,
            project::format_version::MAX_READABLE_FORMAT_VERSION
        );
    }
```

and line 122 with `version: project::format_version::preset_format_version(&preset.blocks),`.

- [ ] **Step 3: Run the test — behavioral RED**

Run: `nice -n 19 cargo test -p infra-yaml --test issue_328_split_yaml -j 2`
Expected: 5 FAILED, 1 passed (`a_split_free_project_stays_version_1`):
- `a_project_with_a_split_round_trips_and_is_stamped_version_2` — `a file holding a split is version 2, got: version: 1 …`
- `this_build_reads_version_2_and_refuses_version_3` — `version 2 is readable: project.yaml version 2 is newer than this build supports (max 1); please upgrade OpenRig`
- `a_chain_preset_writes_type_split_with_positional_path_blocks` — `save: block 'whatever': writing a split to a chain preset is not supported by this build`
- `a_path_block_this_machine_cannot_load_drops_only_that_block` — `the split itself survives` / `left: 0` / `right: 1` (the unknown `type: split` is dropped whole by `load_audio_block_value`)
- `a_legacy_project_file_reads_type_split` — `index out of bounds: the len is 0 but the index is 0`

- [ ] **Step 4: Implement**

Replace the whole `crates/project/src/format_version.rs`:

```rust
//! Responsibility: picks the on-disk format version a document needs.
//!
//! #328 (spec §2): a split is the first shape an older build cannot read. A
//! document that holds one is stamped `version: 2`, so an older build refuses
//! it with its "newer version" error instead of failing inside serde; a
//! split-free document stays at `version: 1`, so older builds keep opening it.

use crate::block::{AudioBlock, AudioBlockKind};
use crate::rig::{RigProject, PRESET_FORMAT_VERSION, PROJECT_FORMAT_VERSION};

/// The first format version that can hold a `Split` block.
pub const SPLIT_FORMAT_VERSION: u32 = 2;
/// The newest format version this build reads, for projects and presets.
pub const MAX_READABLE_FORMAT_VERSION: u32 = SPLIT_FORMAT_VERSION;

/// Whether a block list needs the split format. A split only ever sits at the
/// top of a list (a path never holds one), so the top level is enough.
pub fn blocks_need_split_format(blocks: &[AudioBlock]) -> bool {
    blocks
        .iter()
        .any(|block| matches!(block.kind, AudioBlockKind::Split(_)))
}

/// The version a `project.yaml` document is written with.
pub fn project_format_version(rig: &RigProject) -> u32 {
    if rig
        .presets
        .values()
        .any(|preset| blocks_need_split_format(&preset.blocks))
    {
        SPLIT_FORMAT_VERSION
    } else {
        PROJECT_FORMAT_VERSION
    }
}

/// The version a chain preset file with `blocks` is written with.
pub fn preset_format_version(blocks: &[AudioBlock]) -> u32 {
    if blocks_need_split_format(blocks) {
        SPLIT_FORMAT_VERSION
    } else {
        PRESET_FORMAT_VERSION
    }
}
```

In `crates/project/src/rig.rs` replace lines 152-157 with:

```rust
/// The version a document WITHOUT a split is written with (#328: a document
/// that holds a split is written with `format_version::SPLIT_FORMAT_VERSION`;
/// the loader refuses anything above `format_version::MAX_READABLE_FORMAT_VERSION`).
/// Bumped only when the YAML schema changes in a way that needs a staged
/// upgrade.
pub const PROJECT_FORMAT_VERSION: u32 = 1;
/// See [`PROJECT_FORMAT_VERSION`]; the standalone preset file schema.
pub const PRESET_FORMAT_VERSION: u32 = 1;
```

In `crates/project/src/block/path_ref.rs` add after the `PathSide` enum:

```rust
impl PathSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A => "a",
            Self::B => "b",
        }
    }
}
```

Create `crates/infra-yaml/src/block_yaml_split.rs`:

```rust
//! Responsibility: maps a split block document onto the split block it describes.
//!
//! #328 (spec §2): a chain preset or legacy project writes a split as
//! `type: split` with `end`, `params`, `a` and `b`. Path blocks carry no id on
//! disk; they load as `<split>::a:<i>` / `<split>::b:<i>`, the Select id
//! scheme. A path block this machine cannot load is dropped with a warning,
//! exactly like a top-level block, and the rest of the split is kept.

use anyhow::{Context, Result};
use domain::ids::BlockId;
use project::block::split_params::normalize_split_params;
use project::block::{AudioBlock, AudioBlockKind, PathSide, SplitBlock, SplitEnd};
use serde_yaml::Value;

use crate::block_yaml::AudioBlockYaml;
use crate::{flatten_parameter_set, parameter_set_to_yaml_value};

pub(crate) fn split_from_yaml(
    id: BlockId,
    enabled: bool,
    end: SplitEnd,
    params: Value,
    paths: [Vec<Value>; 2],
) -> Result<AudioBlock> {
    let params =
        normalize_split_params(flatten_parameter_set(params)?).map_err(anyhow::Error::msg)?;
    let [a, b] = paths;
    let a = load_path(&id, PathSide::A, a);
    let b = load_path(&id, PathSide::B, b);
    Ok(AudioBlock {
        id,
        enabled,
        kind: AudioBlockKind::Split(SplitBlock { end, params, a, b }),
    })
}

pub(crate) fn split_to_yaml(block: &AudioBlock, split: &SplitBlock) -> Result<AudioBlockYaml> {
    Ok(AudioBlockYaml::Split {
        enabled: block.enabled,
        end: split.end,
        params: parameter_set_to_yaml_value(&split.params),
        a: path_to_yaml(block, PathSide::A, &split.a)?,
        b: path_to_yaml(block, PathSide::B, &split.b)?,
    })
}

/// The id a path block loads with: `<split>::a:<index>`.
fn path_block_id(split: &BlockId, side: PathSide, index: usize) -> BlockId {
    BlockId(format!("{}::{}:{}", split.0, side.as_str(), index))
}

fn load_path(split: &BlockId, side: PathSide, values: Vec<Value>) -> Vec<AudioBlock> {
    values
        .into_iter()
        .enumerate()
        .filter_map(|(index, value)| {
            let id = path_block_id(split, side, index);
            let loaded = serde_yaml::from_value::<AudioBlockYaml>(value)
                .map_err(anyhow::Error::from)
                .and_then(|yaml| yaml.into_audio_block_with_id(id.clone()));
            match loaded {
                Ok(block) => Some(block),
                Err(error) => {
                    log::warn!("ignoring unsupported or invalid block at {}: {}", id.0, error);
                    None
                }
            }
        })
        .collect()
}

fn path_to_yaml(block: &AudioBlock, side: PathSide, blocks: &[AudioBlock]) -> Result<Vec<Value>> {
    blocks
        .iter()
        .enumerate()
        .map(|(index, nested)| {
            let yaml = AudioBlockYaml::from_audio_block(nested).with_context(|| {
                format!(
                    "failed to serialize path {} block {} of split '{}'",
                    side.as_str(),
                    index,
                    block.id.0
                )
            })?;
            Ok(serde_yaml::to_value(yaml)?)
        })
        .collect()
}
```

In `crates/infra-yaml/src/lib.rs` add `mod block_yaml_split;` after line 15 (`mod block_yaml_save;`).

In `crates/infra-yaml/src/block_yaml.rs`:
- lines 9-12 import: add `SplitEnd` → `use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InsertBlock, NamBlock, OutputBlock, SelectBlock, SplitEnd};`
- after the `Insert { … }` variant (after line 215) add:

```rust
    /// #328: a chain split. Its path blocks carry no id on disk — they are
    /// positional and load as `<split>::a:<i>` / `<split>::b:<i>` (see
    /// `block_yaml_split`).
    Split {
        #[serde(default = "default_enabled")]
        enabled: bool,
        end: SplitEnd,
        #[serde(default)]
        params: Value,
        #[serde(default)]
        a: Vec<Value>,
        #[serde(default)]
        b: Vec<Value>,
    },
```

- line 230: `fn into_audio_block_with_id` → `pub(crate) fn into_audio_block_with_id`
- before the `other => {` arm (line 301) add:

```rust
            AudioBlockYaml::Split {
                enabled,
                end,
                params,
                a,
                b,
            } => crate::block_yaml_split::split_from_yaml(
                generated_id,
                enabled,
                end,
                params,
                [a, b],
            ),
```

- replace the interim `AudioBlockKind::Split(_) => Err(…)` arm of `from_audio_block` with:

```rust
            AudioBlockKind::Split(split) => crate::block_yaml_split::split_to_yaml(block, split),
```

In `crates/infra-yaml/src/block_yaml_load.rs` after the `AudioBlockYaml::Insert { .. } => { … }` arm (after line 192) add:

```rust
        AudioBlockYaml::Split { .. } => {
            unreachable!("Split handled before extract_core_block_fields")
        }
```

- [ ] **Step 5: Run the test — GREEN**

Run: `nice -n 19 cargo test -p infra-yaml -j 2`
Expected: `issue_328_split_yaml` 6 passed; `project_file_tests::serialize_writes_current_version`, `lib_misc_tests::preset_save_writes_version` and every other suite still pass (split-free docs stay at 1).

- [ ] **Step 6: Document the format**

In `docs/projects/project-openrig-format.md`:
- add to the Model table after the `presets.<name>` row:

```markdown
| `presets.<name>.blocks[].kind: !Split` | `SplitBlock` | #328 chain split: `{ end: mix \| y, params, a: [blocks], b: [blocks] }`. Path blocks are full `AudioBlock`s with their own ids. |
```

- add after the "### Model swap keeps the scenes (#986)" section:

```markdown
### Chain split (#328)

A preset may hold a `Split` block (`kind: !Split`). Blocks before it are shared by both paths; for `end: mix` the blocks after it are shared again after the mixer. `a` and `b` are the two paths. The split and mixer knobs live in `params` (see `docs/blocks-catalog.md` → Chain split).

Chain preset files and legacy project files write the split as `type: split` with `end`, `params`, `a` and `b`. Path blocks carry no id on disk and load as `<split id>::a:<i>` / `<split id>::b:<i>`. A path block this machine cannot load is dropped with a warning and the rest of the split is kept.
```

- in "## Format versioning + backward-compat (#450)" replace the paragraph that starts "Both `project.yaml` and standalone preset files carry an explicit" and ends "— currently `1`):" with:

```markdown
Both `project.yaml` and standalone preset files carry an explicit
top-level `version:`. A document is written with the lowest version that can
hold it: `project::rig::{PROJECT_FORMAT_VERSION, PRESET_FORMAT_VERSION}` (`1`),
or `project::format_version::SPLIT_FORMAT_VERSION` (`2`) when a preset holds a
`Split` (#328). This build reads up to `MAX_READABLE_FORMAT_VERSION` (`2`); an
older build refuses a version 2 file with its "newer than this build" error
instead of failing inside serde:
```

- [ ] **Step 7: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/project/src/format_version.rs crates/project/src/lib.rs crates/project/src/rig.rs crates/project/src/block/path_ref.rs crates/infra-yaml/src/block_yaml_split.rs crates/infra-yaml/src/block_yaml.rs crates/infra-yaml/src/block_yaml_load.rs crates/infra-yaml/src/lib.rs crates/infra-yaml/src/project_file.rs crates/infra-yaml/src/preset_yaml.rs crates/infra-yaml/tests/issue_328_split_yaml.rs docs/projects/project-openrig-format.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): persist the split, version 2 only when a document holds one"
```

- [ ] **Step 8: Push gate, push, comment**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 1 T3 pushed (<hash>): !Split / type: split persistence + format_version (v2 only with a split). Push gate green."
```

---

### Task 4: Split rules (one per chain, processing-only paths, Y ends the chain)

**Files:**
- Create: `crates/project/src/block/split_block_methods.rs`
- Modify: `crates/project/src/block/mod.rs` (add `pub mod split_block_methods;` + `pub use split_block_methods::{find_split, validate_split_layout};`)
- Modify: `crates/project/src/block/audio_block_methods.rs` (Split arm of `validate_params`)
- Modify: `crates/project/src/block/select_block_methods.rs:25-36`
- Modify: `crates/project/src/rig_validate.rs:9-21` (doc) and after line 86
- Modify: `docs/blocks-catalog.md` (Chain split section), `docs/projects/project-openrig-format.md` (Validation list)
- Test: `crates/project/tests/issue_328_split_rules.rs`

**Interfaces:**
- Consumes: Task 2 `SplitBlock`, `SplitEnd`, `AudioBlockKind::Split`.
- Produces:
  - `SplitBlock::validate_structure(&self) -> Result<(), String>` (path blocks may not be Split/Select/Input/Output/Insert; the error names the offending kind label)
  - `project::block::find_split(blocks: &[AudioBlock]) -> Option<(usize, &SplitBlock)>`
  - `project::block::validate_split_layout(blocks: &[AudioBlock]) -> Result<(), String>` ("at most one split"; after a Y split only `Input`/`Output` ports)
  - `RigProject::validate()` rejects a preset breaking either rule with `preset '<name>': …`
  - Hand-off: Part 2 (commands, Task 9) calls `validate_split_layout` per chain in `application/src/validate.rs`.

- [ ] **Step 1: Write the failing test**

Create `crates/project/tests/issue_328_split_rules.rs`:

```rust
//! #328 — the structural rules of a chain split (spec §1.1): at most one split
//! per chain, a path holds processing blocks only (no split, select or port,
//! so nesting stays one level deep), and a Y split ends the chain.

use std::collections::BTreeMap;

use domain::ids::BlockId;
use project::block::{
    find_split, schema_for_block_model, validate_split_layout, AudioBlock, AudioBlockKind,
    CoreBlock, InputBlock, InsertBlock, OutputBlock, SelectBlock, SplitBlock, SplitEnd,
};
use project::param::ParameterSet;
use project::rig::{RigInput, RigPreset, RigProject};

fn block(id: &str, kind: AudioBlockKind) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind,
    }
}

fn delay(id: &str) -> AudioBlock {
    let model = block_delay::supported_models()
        .first()
        .expect("a native delay model")
        .to_string();
    let schema = schema_for_block_model("delay", &model).expect("delay schema");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("delay defaults");
    block(
        id,
        AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model,
            params,
        }),
    )
}

fn input_port(id: &str) -> AudioBlock {
    block(
        id,
        AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "In".into(),
        }),
    )
}

fn output_port(id: &str) -> AudioBlock {
    block(
        id,
        AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "Out".into(),
        }),
    )
}

fn insert(id: &str) -> AudioBlock {
    block(
        id,
        AudioBlockKind::Insert(InsertBlock {
            model: "external_loop".into(),
            io: "fx".into(),
        }),
    )
}

fn select(id: &str) -> AudioBlock {
    block(
        id,
        AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId(format!("{id}::opt")),
            options: vec![delay(&format!("{id}::opt"))],
        }),
    )
}

fn split(id: &str, end: SplitEnd, a: Vec<AudioBlock>) -> AudioBlock {
    block(
        id,
        AudioBlockKind::Split(SplitBlock {
            a,
            ..SplitBlock::new(end)
        }),
    )
}

fn rig(blocks: Vec<AudioBlock>) -> RigProject {
    RigProject {
        name: None,
        inputs: BTreeMap::from([(
            "g".to_string(),
            RigInput {
                label: None,
                bank: BTreeMap::from([(1, "p".to_string())]),
                active_preset: 1,
                active_scene: 1,
                routing: Vec::new(),
                instrument: "electric_guitar".into(),
                io: String::new(),
                endpoint: String::new(),
                io_binding_ids: Vec::new(),
                loopers: Vec::new(),
            },
        )]),
        outputs: BTreeMap::new(),
        presets: BTreeMap::from([("p".to_string(), RigPreset::from_legacy_blocks(blocks, 100.0))]),
        midi: None,
        chain_order: Vec::new(),
    }
}

#[test]
fn a_path_holds_processing_blocks_only() {
    let forbidden = [
        (split("inner", SplitEnd::Mix, vec![]), "split"),
        (select("sel"), "select"),
        (input_port("in"), "input"),
        (output_port("out"), "output"),
        (insert("fx"), "insert"),
    ];
    for (bad, label) in forbidden {
        let outer = split("outer", SplitEnd::Mix, vec![delay("ok"), bad]);
        let AudioBlockKind::Split(s) = &outer.kind else {
            unreachable!()
        };
        let err = s
            .validate_structure()
            .expect_err("a forbidden block in a path");
        // `is a <label> block`, not just `<label>`: every message starts with
        // "split path block", so a bare `contains("split")` would always pass.
        assert!(
            err.contains(&format!("is a {label} block")),
            "the error names the {label} block, got: {err}"
        );
        assert!(
            outer.validate_params().is_err(),
            "validate_params enforces it too ({label})"
        );
    }
    let ok = split("s", SplitEnd::Mix, vec![delay("amp")]);
    let AudioBlockKind::Split(s) = &ok.kind else {
        unreachable!()
    };
    assert!(s.validate_structure().is_ok());
}

#[test]
fn a_chain_holds_at_most_one_split() {
    let blocks = vec![
        split("s1", SplitEnd::Mix, vec![]),
        delay("amp"),
        split("s2", SplitEnd::Mix, vec![]),
    ];
    let err = validate_split_layout(&blocks).expect_err("two splits");
    assert!(err.contains("at most one split"), "got: {err}");
}

#[test]
fn a_y_split_ends_the_chain() {
    let err = validate_split_layout(&[split("s", SplitEnd::Y, vec![]), delay("reverb")])
        .expect_err("a block after a Y split");
    assert!(err.contains("reverb"), "the error names the block that follows, got: {err}");
    assert!(
        validate_split_layout(&[delay("drive"), split("s", SplitEnd::Y, vec![]), output_port("tail")])
            .is_ok(),
        "only the chain's own ports may follow a Y split"
    );
    assert!(
        validate_split_layout(&[split("s", SplitEnd::Y, vec![]), insert("fx")]).is_err(),
        "an insert is in the signal path, it cannot follow a Y split"
    );
    assert!(
        validate_split_layout(&[split("s", SplitEnd::Mix, vec![]), delay("reverb")]).is_ok(),
        "a Mix split continues into shared blocks"
    );
}

#[test]
fn find_split_reports_the_split_and_where_it_sits() {
    let blocks = vec![delay("drive"), split("s", SplitEnd::Mix, vec![delay("amp")])];
    let (position, found) = find_split(&blocks).expect("the chain has a split");
    assert_eq!(position, 1);
    assert_eq!(found.a[0].id.0, "amp");
    assert!(find_split(&[delay("drive")]).is_none());
}

#[test]
fn a_select_option_cannot_be_a_split() {
    let select = SelectBlock {
        selected_block_id: BlockId("s".into()),
        options: vec![split("s", SplitEnd::Mix, vec![])],
    };
    let err = select.validate_structure().expect_err("a split option");
    assert!(err.contains("split"), "got: {err}");
}

#[test]
fn a_rig_refuses_a_preset_that_breaks_the_split_rules() {
    let err = rig(vec![
        split("s1", SplitEnd::Mix, vec![]),
        split("s2", SplitEnd::Mix, vec![]),
    ])
    .validate()
    .expect_err("two splits");
    assert!(
        err.contains("preset 'p'") && err.contains("at most one split"),
        "got: {err}"
    );
    let err = rig(vec![split("s", SplitEnd::Mix, vec![input_port("in")])])
        .validate()
        .expect_err("a port in a path");
    assert!(
        err.contains("preset 'p'") && err.contains("is a input block"),
        "got: {err}"
    );
    assert!(rig(vec![delay("drive"), split("s", SplitEnd::Y, vec![delay("amp")])])
        .validate()
        .is_ok());
}
```

- [ ] **Step 2: Add the compile skeleton**

Create `crates/project/src/block/split_block_methods.rs`:

```rust
//! Responsibility: states the structural rules a chain's split obeys.

use super::split_block::SplitBlock;
use super::types::AudioBlock;

impl SplitBlock {
    pub fn validate_structure(&self) -> Result<(), String> {
        Ok(())
    }
}

pub fn find_split(_blocks: &[AudioBlock]) -> Option<(usize, &SplitBlock)> {
    None
}

pub fn validate_split_layout(_blocks: &[AudioBlock]) -> Result<(), String> {
    Ok(())
}
```

In `crates/project/src/block/mod.rs` add `pub mod split_block_methods;` after `pub mod split_block;` and `pub use split_block_methods::{find_split, validate_split_layout};` after `pub use split_block::{SplitBlock, SplitEnd};`.

- [ ] **Step 3: Run the test — behavioral RED**

Run: `nice -n 19 cargo test -p project --test issue_328_split_rules -j 2`
Expected: 6 FAILED:
- `a_path_holds_processing_blocks_only` — `a forbidden block in a path: ()` (the `expect_err` on `Ok(())`)
- `a_chain_holds_at_most_one_split` — `two splits: ()`
- `a_y_split_ends_the_chain` — `a block after a Y split: ()`
- `find_split_reports_the_split_and_where_it_sits` — `the chain has a split`
- `a_select_option_cannot_be_a_split` — `got: select block option 's' does not expose a concrete model`
- `a_rig_refuses_a_preset_that_breaks_the_split_rules` — `two splits: ()`

- [ ] **Step 4: Implement the rules**

Replace the whole `crates/project/src/block/split_block_methods.rs`:

```rust
//! Responsibility: states the structural rules a chain's split obeys.
//!
//! #328 (spec §1.1): at most one split per chain; a path holds processing
//! blocks only — no split, select or port — so nesting stays one level deep;
//! and a Y split ends the chain (only the chain's own `Input`/`Output` ports
//! may follow it).

use super::split_block::{SplitBlock, SplitEnd};
use super::types::{AudioBlock, AudioBlockKind};

impl SplitBlock {
    /// The path rule: every block of `a` and `b` is a processing block.
    pub fn validate_structure(&self) -> Result<(), String> {
        for block in self.a.iter().chain(&self.b) {
            if matches!(
                block.kind,
                AudioBlockKind::Split(_)
                    | AudioBlockKind::Select(_)
                    | AudioBlockKind::Input(_)
                    | AudioBlockKind::Output(_)
                    | AudioBlockKind::Insert(_)
            ) {
                return Err(format!(
                    "split path block '{}' is a {} block; a path holds processing blocks only",
                    block.id.0,
                    block.kind.label()
                ));
            }
        }
        Ok(())
    }
}

/// The chain's split and its position in `blocks`, if it has one.
pub fn find_split(blocks: &[AudioBlock]) -> Option<(usize, &SplitBlock)> {
    blocks
        .iter()
        .enumerate()
        .find_map(|(position, block)| match &block.kind {
            AudioBlockKind::Split(split) => Some((position, split)),
            _ => None,
        })
}

/// The chain-level rules: at most one split, and nothing but the chain's own
/// `Input`/`Output` ports after a Y split.
pub fn validate_split_layout(blocks: &[AudioBlock]) -> Result<(), String> {
    let splits = blocks
        .iter()
        .filter(|block| matches!(block.kind, AudioBlockKind::Split(_)))
        .count();
    if splits > 1 {
        return Err(format!("a chain holds at most one split, found {splits}"));
    }
    let Some((position, split)) = find_split(blocks) else {
        return Ok(());
    };
    if split.end == SplitEnd::Y {
        let follower = blocks[position + 1..].iter().find(|block| {
            !matches!(
                block.kind,
                AudioBlockKind::Input(_) | AudioBlockKind::Output(_)
            )
        });
        if let Some(block) = follower {
            return Err(format!(
                "a Y split ends the chain, but block '{}' follows it",
                block.id.0
            ));
        }
    }
    Ok(())
}
```

In `crates/project/src/block/audio_block_methods.rs`, first line of the Split arm of `validate_params` becomes `split.validate_structure()?;`:

```rust
            AudioBlockKind::Split(split) => {
                split.validate_structure()?;
                normalize_split_params(split.params.clone())?;
                for block in split.a.iter().chain(&split.b) {
                    block.validate_params()?;
                }
                Ok(())
            }
```

In `crates/project/src/block/select_block_methods.rs` replace lines 25-36 with:

```rust
            if matches!(
                option.kind,
                AudioBlockKind::Select(_)
                    | AudioBlockKind::Split(_)
                    | AudioBlockKind::Input(_)
                    | AudioBlockKind::Output(_)
                    | AudioBlockKind::Insert(_)
            ) {
                return Err(
                    "select block options cannot be select, split, input, output, or insert blocks"
                        .to_string(),
                );
            }
```

In `crates/project/src/rig_validate.rs`: add to the rule list of the doc comment (after line 17) `/// 6. a preset breaking the split rules of #328 (one split, processing-only paths, a Y split last);` and after the `for block in &preset.blocks { … }` loop (after line 86) add:

```rust
            // #328 (spec §1.1): one split per preset, a Y split ends it, and a
            // path holds processing blocks only.
            crate::block::validate_split_layout(&preset.blocks)
                .map_err(|e| format!("preset '{name}': {e}"))?;
            if let Some((_, split)) = crate::block::find_split(&preset.blocks) {
                split
                    .validate_structure()
                    .map_err(|e| format!("preset '{name}': {e}"))?;
            }
```

- [ ] **Step 5: Run the test — GREEN**

Run: `nice -n 19 cargo test -p project --test issue_328_split_rules -j 2`
Expected: `test result: ok. 6 passed; 0 failed`. Then `nice -n 19 cargo test -p project -j 2` — all green (the Select error string changed; no existing test asserts it).

- [ ] **Step 6: Document the rules**

In `docs/blocks-catalog.md`, at the end of the "## Chain split (#328)" section add:

```markdown
Rules (`project::block::split_block_methods`, enforced by `validate_params` and `RigProject::validate`): at most one split per chain; a path holds processing blocks only — no split, select, input, output or insert, so nesting stays one level deep; a Y split ends the chain, only the chain's own `Input`/`Output` ports may follow it. A select option cannot be a split.
```

In `docs/projects/project-openrig-format.md`, add to the "## Validation" list after item 6:

```markdown
7. a preset breaking the split rules of #328: two splits, a split/select/input/output/insert inside a path, or anything but a port after a Y split.
```

- [ ] **Step 7: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/project/src/block/split_block_methods.rs crates/project/src/block/mod.rs crates/project/src/block/audio_block_methods.rs crates/project/src/block/select_block_methods.rs crates/project/src/rig_validate.rs crates/project/tests/issue_328_split_rules.rs docs/blocks-catalog.md docs/projects/project-openrig-format.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): enforce the split rules on blocks, selects and rigs"
```

- [ ] **Step 8: Push gate, push, comment**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 1 T4 pushed (<hash>): split rules (one per chain, processing-only paths, Y last) in validate_params + RigProject::validate. Push gate green."
```

---

### Task 5: A preset switch brings the new preset's split

**Files:**
- Modify: `crates/application/src/local_dispatcher_rig.rs:160-164` (the `is_port` predicate) and end of file (test mount)
- Test: `crates/application/src/issue_328_split_preset_switch_tests.rs`

**Interfaces:**
- Consumes: Task 2 `AudioBlockKind::Split`, `SplitBlock::new`, `is_routing() == true` for Split.
- Produces: `merge_preserved_ports` keeps only `Input`/`Output`/`Insert` in the chain's slots; a `Split` travels with the preset.

- [ ] **Step 1: Write the failing test**

Create `crates/application/src/issue_328_split_preset_switch_tests.rs`:

```rust
//! #328 — a split belongs to the PRESET, not to the chain.
//!
//! `merge_preserved_ports` keeps the chain's own ports (and inserts) in their
//! slots across a preset/scene switch. A split is routing too — switching it
//! needs a rebuild — but it is one of the preset's blocks: switching preset
//! must bring THAT preset's split (or none), never keep the old one.

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, OutputBlock, SplitBlock, SplitEnd};
use project::param::ParameterSet;

use super::merge_preserved_ports;

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

fn split(id: &str, a: Vec<AudioBlock>) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            a,
            ..SplitBlock::new(SplitEnd::Mix)
        }),
    }
}

fn aux_send(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "Send".into(),
        }),
    }
}

fn ids(blocks: &[AudioBlock]) -> Vec<&str> {
    blocks.iter().map(|b| b.id.0.as_str()).collect()
}

#[test]
fn switching_to_a_preset_brings_its_own_split() {
    let current = vec![
        effect("drive"),
        split("old-split", vec![effect("amp-old")]),
        aux_send("aux"),
    ];
    let rebuilt = vec![effect("comp"), split("new-split", vec![effect("amp-new")])];
    let merged = merge_preserved_ports(&current, rebuilt);
    assert_eq!(
        ids(&merged),
        vec!["comp", "new-split", "aux"],
        "#328: the new preset's split replaces the old one; the aux send keeps its slot"
    );
}

#[test]
fn switching_to_a_preset_without_a_split_drops_the_old_one() {
    let current = vec![split("old-split", vec![effect("amp-old")]), aux_send("aux")];
    let rebuilt = vec![effect("comp")];
    let merged = merge_preserved_ports(&current, rebuilt);
    assert_eq!(ids(&merged), vec!["comp", "aux"]);
}
```

Mount it at the end of `crates/application/src/local_dispatcher_rig.rs` (after line 190):

```rust

#[cfg(test)]
#[path = "issue_328_split_preset_switch_tests.rs"]
mod issue_328_split_preset_switch_tests;
```

- [ ] **Step 2: Run the test — behavioral RED**

Run: `nice -n 19 cargo test -p application issue_328_split_preset_switch -j 2`
Expected: 2 FAILED:
- `switching_to_a_preset_brings_its_own_split` — `left: ["comp", "old-split", "aux"]` / `right: ["comp", "new-split", "aux"]`
- `switching_to_a_preset_without_a_split_drops_the_old_one` — `left: ["old-split", "aux", "comp"]` / `right: ["comp", "aux"]`

- [ ] **Step 3: Implement**

In `crates/application/src/local_dispatcher_rig.rs` replace lines 160-164 with:

```rust
    // #881: an `Insert` is routing too — it splits the chain at its own
    // position — so it keeps its slot exactly like a port. Consuming that slot
    // for the next rebuilt effect dropped the loop and shifted every block
    // after it up by one. #328: a `Split` is routing as well (a rebuild, never
    // the in-place fade) but it is the PRESET's block — switching presets must
    // bring the new preset's split, never keep the old one in the chain's slot.
    let is_port = |b: &AudioBlock| {
        matches!(
            b.kind,
            AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_)
        )
    };
```

- [ ] **Step 4: Run the test — GREEN**

Run: `nice -n 19 cargo test -p application issue_328_split_preset_switch -j 2` then `nice -n 19 cargo test -p application issue_85_port_position -j 2`
Expected: 2 passed; the five `issue_85_port_position_tests` still pass.

- [ ] **Step 5: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/application/src/local_dispatcher_rig.rs crates/application/src/issue_328_split_preset_switch_tests.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): a preset switch brings the new preset's split"
```

- [ ] **Step 6: Push gate, push, comment**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 1 T5 pushed (<hash>): merge_preserved_ports keeps ports/inserts only; the split travels with the preset. Push gate green."
```

---

### Task 6: Model walkers reach the paths

**Files:**
- Create: `crates/project/src/block/block_walk.rs`
- Modify: `crates/project/src/block/mod.rs` (add `pub mod block_walk;` + `pub use block_walk::{find_block_mut, for_each_block_mut, walk_blocks};`)
- Modify: `crates/project/src/project.rs:5,26-34,92-104`
- Modify: `crates/project/src/rig.rs:11,232-261` (`apply_scene`)
- Modify: `crates/project/src/project_disable_unavailable.rs:18-40`
- Test: `crates/project/tests/issue_328_split_walkers.rs`

**Interfaces:**
- Consumes: Task 2 `block_params_mut`, `SplitBlock`.
- Produces:
  - `project::block::walk_blocks(blocks: &[AudioBlock]) -> Vec<&AudioBlock>` (split, then path A, then path B; select options not visited)
  - `project::block::for_each_block_mut(blocks: &mut [AudioBlock], f: &mut dyn FnMut(&mut AudioBlock))` (same order; `f` runs on a split before its paths)
  - `project::block::find_block_mut<'a>(blocks: &'a mut [AudioBlock], id: &str) -> Option<&'a mut AudioBlock>`
  - `Project::find_block`, `Project::parameter_descriptors`/`find_parameter_descriptor`, `RigPreset::apply_scene`, `disable_unavailable_blocks` reach path blocks and the split's knobs.

- [ ] **Step 1: Write the failing test**

Create `crates/project/tests/issue_328_split_walkers.rs`:

```rust
//! #328 — the model walkers of spec §1.4 reach the blocks inside a split's
//! paths: lookup by id, project-wide parameter descriptors, scene resolution
//! and the load-time "disable what this machine cannot build" pass (#606).

use std::collections::BTreeMap;

use domain::ids::{BlockId, ChainId, ParameterId};
use domain::value_objects::ParameterValue;
use project::block::split_params::MIX_PAN_A;
use project::block::{
    find_block_mut, for_each_block_mut, schema_for_block_model, walk_blocks, AudioBlock,
    AudioBlockKind, CoreBlock, SplitBlock, SplitEnd,
};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;
use project::project_disable_unavailable::disable_unavailable_blocks;
use project::rig::{RigPreset, RigScene};

fn core(id: &str, effect_type: &str, model: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.into(),
            model: model.into(),
            params: ParameterSet::default(),
        }),
    }
}

fn amp(id: &str, gain: f32) -> AudioBlock {
    let mut block = core(id, "amp", "m1");
    if let AudioBlockKind::Core(c) = &mut block.kind {
        c.params.insert("gain", ParameterValue::Float(gain));
    }
    block
}

fn split(a: Vec<AudioBlock>, b: Vec<AudioBlock>) -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            a,
            b,
            ..SplitBlock::new(SplitEnd::Mix)
        }),
    }
}

/// Paths read straight off the split, independent of the walkers under test.
fn paths(block: &AudioBlock) -> (&[AudioBlock], &[AudioBlock]) {
    match &block.kind {
        AudioBlockKind::Split(s) => (&s.a, &s.b),
        other => panic!("expected a split, got {}", other.label()),
    }
}

fn gain(block: &AudioBlock) -> Option<f32> {
    match &block.kind {
        AudioBlockKind::Core(c) => c.params.get_f32("gain"),
        _ => None,
    }
}

fn project_of(blocks: Vec<AudioBlock>) -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("c".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: Vec::new(),
            blocks,
            di_output: None,
            loopers: Vec::new(),
        }],
        midi: None,
    }
}

#[test]
fn the_walk_visits_the_split_then_path_a_then_path_b() {
    let blocks = vec![
        amp("drive", 0.5),
        split(vec![amp("a1", 0.5), amp("a2", 0.5)], vec![amp("b1", 0.5)]),
        amp("reverb", 0.5),
    ];
    let walked: Vec<&str> = walk_blocks(&blocks)
        .into_iter()
        .map(|b| b.id.0.as_str())
        .collect();
    assert_eq!(walked, vec!["drive", "split", "a1", "a2", "b1", "reverb"]);

    let mut visited = Vec::new();
    let mut blocks = blocks;
    for_each_block_mut(&mut blocks, &mut |b| visited.push(b.id.0.clone()));
    assert_eq!(visited, vec!["drive", "split", "a1", "a2", "b1", "reverb"]);
}

#[test]
fn a_path_block_is_found_by_id() {
    let mut blocks = vec![split(vec![amp("a1", 0.5)], vec![amp("b1", 0.5)])];
    find_block_mut(&mut blocks, "b1")
        .expect("path B's block is reachable")
        .enabled = false;
    assert!(!paths(&blocks[0]).1[0].enabled);
    assert!(
        project_of(blocks).find_block(&BlockId("a1".into())).is_some(),
        "Project::find_block reaches path A"
    );
}

#[test]
fn the_parameters_of_path_blocks_are_addressable_project_wide() {
    let model = block_delay::supported_models()
        .first()
        .expect("a native delay")
        .to_string();
    let schema = schema_for_block_model("delay", &model).expect("delay schema");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults");
    let echo = AudioBlock {
        id: BlockId("echo".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model,
            params,
        }),
    };
    let project = project_of(vec![split(vec![echo], vec![])]);
    let nested = ParameterId::for_block_path(&BlockId("echo".into()), &schema.parameters[0].path);
    assert!(
        project.find_parameter_descriptor(&nested).expect("describe").is_some(),
        "a path block's knob is found"
    );
    let knob = ParameterId::for_block_path(&BlockId("split".into()), MIX_PAN_A);
    assert!(
        project.find_parameter_descriptor(&knob).expect("describe").is_some(),
        "the mixer's knob is found"
    );
}

#[test]
fn a_scene_reaches_the_path_blocks_and_the_split_knobs() {
    let mut preset = RigPreset::from_legacy_blocks(
        vec![split(vec![amp("amp_a", 0.5)], vec![amp("amp_b", 0.5)])],
        100.0,
    );
    preset.scene_params = vec!["amp_a.gain".into(), "split.mix_pan_a".into()];
    preset.scenes = BTreeMap::from([(
        2,
        RigScene {
            label: None,
            bypass: BTreeMap::from([("amp_b".to_string(), true)]),
            params: BTreeMap::from([
                ("amp_a.gain".to_string(), 0.9),
                ("split.mix_pan_a".to_string(), -50.0),
            ]),
            volume: None,
        },
    )]);
    let blocks = preset.apply_scene(2);
    let (a, b) = paths(&blocks[0]);
    assert_eq!(gain(&a[0]), Some(0.9), "path A's knob takes the scene value");
    assert!(!b[0].enabled, "path B's block takes the scene bypass");
    let AudioBlockKind::Split(s) = &blocks[0].kind else {
        unreachable!()
    };
    assert_eq!(
        s.params.get_f32(MIX_PAN_A),
        Some(-50.0),
        "the mixer knob takes the scene value"
    );
}

#[test]
fn a_path_block_whose_model_is_missing_is_disabled_on_load() {
    let mut project = project_of(vec![split(
        vec![
            core("ts9", "gain", "ibanez_ts9"),
            core("missing", "gain", "nam_uninstalled_pedal_for_issue_328"),
        ],
        vec![],
    )]);
    let disabled = disable_unavailable_blocks(&mut project);
    assert_eq!(disabled, vec![BlockId("missing".into())]);
    let blocks = &project.chains[0].blocks;
    assert!(blocks[0].enabled, "the split itself stays on");
    let (a, _) = paths(&blocks[0]);
    assert!(a[0].enabled, "an available path block stays on");
    assert!(!a[1].enabled, "#606 inside a path: the unavailable block is switched off");
}
```

- [ ] **Step 2: Add the compile skeleton (top-level walkers only — no production caller changes yet)**

Create `crates/project/src/block/block_walk.rs`:

```rust
//! Responsibility: visits every block of a list through the paths of its split.

use super::types::AudioBlock;

pub fn walk_blocks(blocks: &[AudioBlock]) -> Vec<&AudioBlock> {
    blocks.iter().collect()
}

pub fn for_each_block_mut(blocks: &mut [AudioBlock], f: &mut dyn FnMut(&mut AudioBlock)) {
    for block in blocks {
        f(block);
    }
}

pub fn find_block_mut<'a>(blocks: &'a mut [AudioBlock], id: &str) -> Option<&'a mut AudioBlock> {
    blocks.iter_mut().find(|block| block.id.0 == id)
}
```

In `crates/project/src/block/mod.rs` add `pub mod block_walk;` after `pub mod block_params;` and `pub use block_walk::{find_block_mut, for_each_block_mut, walk_blocks};` after `pub use block_params::{block_params, block_params_mut};`.

`Project::parameter_descriptors`, `find_block_recursive`, `RigPreset::apply_scene` and `disable_unavailable_blocks` stay exactly as they are in this step, so the red below is the unchanged production code failing (routing them through the helpers here would already change `apply_scene` for a top-level split's knobs before any test saw it fail).

- [ ] **Step 3: Run the test — behavioral RED**

Run: `nice -n 19 cargo test -p project --test issue_328_split_walkers -j 2`
Expected: 5 FAILED:
- `the_walk_visits_the_split_then_path_a_then_path_b` — `left: ["drive", "split", "reverb"]` / `right: ["drive", "split", "a1", "a2", "b1", "reverb"]`
- `a_path_block_is_found_by_id` — `path B's block is reachable`
- `the_parameters_of_path_blocks_are_addressable_project_wide` — `a path block's knob is found`
- `a_scene_reaches_the_path_blocks_and_the_split_knobs` — `path A's knob takes the scene value` / `left: Some(0.5)` / `right: Some(0.9)`
- `a_path_block_whose_model_is_missing_is_disabled_on_load` — `left: []` / `right: [BlockId("missing")]`

- [ ] **Step 4: Implement the recursion and route the walkers through it**

Replace the whole `crates/project/src/block/block_walk.rs`:

```rust
//! Responsibility: visits every block of a list through the paths of its split.
//!
//! #328 (spec §1.4): every model walker — lookup, descriptors, scenes, edit
//! capture, model swap, load-time disable — reaches the blocks inside a split's
//! paths through these helpers, so the recursion lives in one place. Signal
//! order: a split, then its path A, then its path B. Select options are not
//! visited: a select is one slot whose inactive options are not in the signal
//! path.

use super::types::{AudioBlock, AudioBlockKind};

/// Every block of `blocks`, paths included, in signal order.
pub fn walk_blocks(blocks: &[AudioBlock]) -> Vec<&AudioBlock> {
    let mut walked = Vec::with_capacity(blocks.len());
    push_walk(blocks, &mut walked);
    walked
}

fn push_walk<'a>(blocks: &'a [AudioBlock], walked: &mut Vec<&'a AudioBlock>) {
    for block in blocks {
        walked.push(block);
        if let AudioBlockKind::Split(split) = &block.kind {
            push_walk(&split.a, walked);
            push_walk(&split.b, walked);
        }
    }
}

/// Mutable twin of [`walk_blocks`]: `f` runs on every block, on a split before
/// its paths.
pub fn for_each_block_mut(blocks: &mut [AudioBlock], f: &mut dyn FnMut(&mut AudioBlock)) {
    for block in blocks {
        f(block);
        if let AudioBlockKind::Split(split) = &mut block.kind {
            for_each_block_mut(&mut split.a, f);
            for_each_block_mut(&mut split.b, f);
        }
    }
}

/// The block with `id` anywhere in `blocks`, paths included.
pub fn find_block_mut<'a>(blocks: &'a mut [AudioBlock], id: &str) -> Option<&'a mut AudioBlock> {
    blocks
        .iter_mut()
        .find_map(|block| find_in_block_mut(block, id))
}

fn find_in_block_mut<'a>(block: &'a mut AudioBlock, id: &str) -> Option<&'a mut AudioBlock> {
    if block.id.0 == id {
        return Some(block);
    }
    match &mut block.kind {
        AudioBlockKind::Split(split) => {
            find_block_mut(&mut split.a, id).or_else(|| find_block_mut(&mut split.b, id))
        }
        _ => None,
    }
}
```

In `crates/project/src/project.rs` replace line 5 with `use crate::block::{walk_blocks, AudioBlock, BlockAudioDescriptor};` and lines 26-34 with:

```rust
    pub fn parameter_descriptors(&self) -> Result<Vec<BlockParameterDescriptor>, String> {
        let mut descriptors = Vec::new();
        for chain in &self.chains {
            // #328: a split's path blocks are addressable like top-level ones
            // (a split's own descriptors are only its knobs).
            for block in walk_blocks(&chain.blocks) {
                descriptors.extend(collect_block_parameter_descriptors(block)?);
            }
        }
        Ok(descriptors)
    }
```

(`block_audio_descriptors` stays top-level: a split's `audio_descriptors` already includes its paths.)

In `crates/project/src/rig.rs` replace line 11 with `use crate::block::{block_params_mut, for_each_block_mut, AudioBlock};` and lines 232-261 with:

```rust
    /// Resolve scene `idx` into concrete blocks: clone the base blocks, apply
    /// the scene's bypass (`enabled = !bypassed`) and override **only** the
    /// marked `scene_params` with the scene's values. Anything not marked is
    /// fixed by the preset (Helix Snapshot rule). #328: the blocks inside a
    /// split's paths and the split's own knobs are resolved exactly like a
    /// top-level block. Pure & deterministic.
    pub fn apply_scene(&self, idx: usize) -> Vec<AudioBlock> {
        let scene = self.scene_or_default(idx);
        let mut blocks = self.blocks.clone();
        for_each_block_mut(&mut blocks, &mut |block| {
            let bid = block.id.0.clone();
            if let Some(&bypassed) = scene.bypass.get(&bid) {
                block.enabled = !bypassed;
            }
            if let Some(params) = block_params_mut(&mut block.kind) {
                let prefix = format!("{bid}.");
                for key in &self.scene_params {
                    if let Some(param_id) = key.strip_prefix(&prefix) {
                        if let Some(&value) = scene.params.get(key) {
                            params.insert(param_id.to_string(), ParameterValue::Float(value));
                        }
                    }
                }
            }
        });
        blocks
    }
```

In `crates/project/src/project_disable_unavailable.rs` replace lines 18-40 with:

```rust
//! Routing-only kinds (Input/Output/Insert) and the composite Select and Split
//! kinds have no single resolvable model and are never touched here; #328: the
//! blocks inside a split's paths are checked like top-level blocks.

use crate::block::{for_each_block_mut, AudioBlockKind};
use crate::project::Project;
use domain::ids::BlockId;

/// Disable every currently-enabled block whose model is unavailable, paths
/// included. Returns the ids of the blocks that were flipped off
/// (already-disabled blocks are left as-is and not reported).
pub fn disable_unavailable_blocks(project: &mut Project) -> Vec<BlockId> {
    let mut disabled = Vec::new();
    for chain in &mut project.chains {
        for_each_block_mut(&mut chain.blocks, &mut |block| {
            if !block.enabled || block_model_is_available(&block.kind) {
                return;
            }
            block.enabled = false;
            disabled.push(block.id.clone());
        });
    }
    disabled
}
```

In `crates/project/src/project.rs` replace `find_block_recursive` (lines 92-104) with:

```rust
fn find_block_recursive<'a>(block: &'a AudioBlock, block_id: &BlockId) -> Option<&'a AudioBlock> {
    if block.id == *block_id {
        return Some(block);
    }
    match &block.kind {
        crate::block::AudioBlockKind::Select(select) => select
            .options
            .iter()
            .find_map(|option| find_block_recursive(option, block_id)),
        // #328: the blocks inside a split's paths.
        crate::block::AudioBlockKind::Split(split) => split
            .a
            .iter()
            .chain(&split.b)
            .find_map(|nested| find_block_recursive(nested, block_id)),
        _ => None,
    }
}
```

- [ ] **Step 5: Run the test — GREEN**

Run: `nice -n 19 cargo test -p project -j 2`
Expected: `issue_328_split_walkers` 5 passed; `rig_scene_tests`, `issue_606_disable_unavailable_blocks`, `project_tests` unchanged and green.

- [ ] **Step 6: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/project/src/block/block_walk.rs crates/project/src/block/mod.rs crates/project/src/project.rs crates/project/src/rig.rs crates/project/src/project_disable_unavailable.rs crates/project/tests/issue_328_split_walkers.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): model walkers reach the blocks inside a split's paths"
```

- [ ] **Step 7: Push gate, push, comment**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 1 T6 pushed (<hash>): block_walk + recursion in find_block, parameter descriptors, apply_scene, disable_unavailable. Push gate green."
```

---

### Task 7: Edit capture and model swap reach the paths

**Files:**
- Modify: `crates/project/src/rig_write_back.rs:5,39-44,53,75-79,113-127`
- Modify: `crates/project/src/rig_model_swap.rs` (whole body after line 9)
- Modify: `docs/projects/project-openrig-format.md` ("### Chain split (#328)" section)
- Test: `crates/project/tests/issue_328_split_write_back.rs`

**Interfaces:**
- Consumes: Task 6 `walk_blocks`, `for_each_block_mut`, `find_block_mut`; Task 2 `block_params`, `block_params_mut`, nested `model_identity`.
- Produces: `RigProject::write_back_processing_blocks` diffs path blocks and split knobs; `RigProject::write_back_model_swaps` handles a swap on a path block (#986) and leaves a path whose shape changed to the structural path; `replace_preset_blocks_if_structural` catches nested structural edits through `model_identity` (unchanged code).

- [ ] **Step 1: Write the failing test**

Create `crates/project/tests/issue_328_split_write_back.rs`:

```rust
//! #328 — edits made inside a split survive save + reload (spec §1.4): the
//! capture (`sync_synthetic_into_rig`) reaches the path blocks and the split's
//! knobs, a model swap on a path block keeps every scene (#986 inside paths),
//! and adding a block to a path is structural like on the top level.

use std::collections::BTreeMap;

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::split_params::{MIX_PAN_A, SPLIT_MODE};
use project::block::{
    find_block_mut, walk_blocks, AudioBlock, AudioBlockKind, CoreBlock, SplitBlock, SplitEnd,
};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;
use project::rig::{RigInput, RigPreset, RigProject, RigScene};
use project::rig_sync::sync_synthetic_into_rig;

fn amp(id: &str, model: &str, gain: f32) -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert("gain", ParameterValue::Float(gain));
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

fn dual_amp(a: Vec<AudioBlock>, b: Vec<AudioBlock>) -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            a,
            b,
            ..SplitBlock::new(SplitEnd::Mix)
        }),
    }
}

/// Input `g` plays preset `p` = one split, amp_a on A and amp_b on B (gain
/// 0.5 each). Scene 2 bypasses amp_b and pulls its gain to 0.1.
fn rig(active_scene: usize) -> RigProject {
    let mut preset = RigPreset::from_legacy_blocks(
        vec![dual_amp(
            vec![amp("amp_a", "m1", 0.5)],
            vec![amp("amp_b", "m1", 0.5)],
        )],
        100.0,
    );
    preset.scene_params = vec!["amp_b.gain".into()];
    preset.scenes = BTreeMap::from([
        (1, RigScene::default()),
        (
            2,
            RigScene {
                label: None,
                bypass: BTreeMap::from([("amp_b".to_string(), true)]),
                params: BTreeMap::from([("amp_b.gain".to_string(), 0.1)]),
                volume: None,
            },
        ),
    ]);
    RigProject {
        name: None,
        inputs: BTreeMap::from([(
            "g".to_string(),
            RigInput {
                label: None,
                bank: BTreeMap::from([(1, "p".to_string())]),
                active_preset: 1,
                active_scene,
                routing: Vec::new(),
                instrument: "electric_guitar".into(),
                io: String::new(),
                endpoint: String::new(),
                io_binding_ids: Vec::new(),
                loopers: Vec::new(),
            },
        )]),
        outputs: BTreeMap::new(),
        presets: BTreeMap::from([("p".to_string(), preset)]),
        midi: None,
        chain_order: Vec::new(),
    }
}

/// The projected chain the user edits (the active scene applied, then `edit`),
/// captured back into the rig the way a save does.
fn capture(rig: &mut RigProject, edit: impl FnOnce(&mut Vec<AudioBlock>)) {
    let scene = rig.inputs["g"].active_scene;
    let mut blocks = rig.presets["p"].apply_scene(scene);
    edit(&mut blocks);
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("rig:g".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: Vec::new(),
            blocks,
            di_output: None,
            loopers: Vec::new(),
        }],
        midi: None,
    };
    sync_synthetic_into_rig(rig, &project);
}

fn edited<'a>(blocks: &'a mut [AudioBlock], id: &str) -> &'a mut AudioBlock {
    find_block_mut(blocks, id).unwrap_or_else(|| panic!("no block {id}"))
}

fn block<'a>(blocks: &'a [AudioBlock], id: &str) -> &'a AudioBlock {
    walk_blocks(blocks)
        .into_iter()
        .find(|b| b.id.0 == id)
        .unwrap_or_else(|| panic!("no block {id}"))
}

fn gain(block: &AudioBlock) -> Option<f32> {
    match &block.kind {
        AudioBlockKind::Core(c) => c.params.get_f32("gain"),
        _ => None,
    }
}

#[test]
fn a_knob_turned_inside_a_path_is_kept_in_the_active_scene() {
    let mut rig = rig(1);
    capture(&mut rig, |blocks| {
        if let AudioBlockKind::Core(c) = &mut edited(blocks, "amp_a").kind {
            c.params.insert("gain", ParameterValue::Float(0.8));
        }
    });
    let preset = &rig.presets["p"];
    assert_eq!(
        preset.scenes[&1].params.get("amp_a.gain"),
        Some(&0.8),
        "the edit is the active scene's override"
    );
    assert!(preset.scene_params.contains(&"amp_a.gain".to_string()));
    assert_eq!(
        gain(block(&preset.apply_scene(2), "amp_a")),
        Some(0.5),
        "scene 2 keeps the preset base"
    );
}

#[test]
fn a_bypass_inside_a_path_is_kept_in_the_active_scene() {
    let mut rig = rig(1);
    capture(&mut rig, |blocks| edited(blocks, "amp_a").enabled = false);
    assert_eq!(rig.presets["p"].scenes[&1].bypass.get("amp_a"), Some(&true));
    assert!(
        block(&rig.presets["p"].apply_scene(2), "amp_a").enabled,
        "scene 2 is untouched"
    );
}

#[test]
fn a_mixer_knob_is_a_scene_override_while_the_split_mode_is_preset_wide() {
    let mut rig = rig(1);
    capture(&mut rig, |blocks| {
        if let AudioBlockKind::Split(s) = &mut edited(blocks, "split").kind {
            s.params.insert(MIX_PAN_A, ParameterValue::Float(-50.0));
            s.params
                .insert(SPLIT_MODE, ParameterValue::String("dual_mono".into()));
        }
    });
    let preset = &rig.presets["p"];
    assert_eq!(
        preset.scenes[&1].params.get("split.mix_pan_a"),
        Some(&-50.0),
        "a float knob is per scene"
    );
    let AudioBlockKind::Split(base) = &preset.blocks[0].kind else {
        panic!("block 0 is the split")
    };
    assert_eq!(
        base.params.get_string(SPLIT_MODE),
        Some("dual_mono"),
        "a non-float knob is preset-wide (#690)"
    );
}

#[test]
fn a_model_swap_inside_a_path_keeps_every_scene() {
    let mut rig = rig(2);
    capture(&mut rig, |blocks| {
        edited(blocks, "amp_a").kind = amp("amp_a", "m2", 0.3).kind;
    });
    let preset = &rig.presets["p"];
    assert_eq!(
        preset.scenes.len(),
        2,
        "#986: a model swap is not structural, inside a path too"
    );
    let scene_1 = preset.apply_scene(1);
    assert_eq!(
        gain(block(&scene_1, "amp_b")),
        Some(0.5),
        "scene 1 must not inherit scene 2's amp_b values"
    );
    assert!(block(&scene_1, "amp_b").enabled, "nor its bypass");
    let AudioBlockKind::Core(amp_a) = &block(&preset.blocks, "amp_a").kind else {
        panic!("amp_a is a core block")
    };
    assert_eq!(amp_a.model, "m2", "the swapped model is in the preset base");
}

#[test]
fn a_block_added_inside_a_path_is_a_structural_edit() {
    let mut rig = rig(1);
    capture(&mut rig, |blocks| {
        if let AudioBlockKind::Split(s) = &mut edited(blocks, "split").kind {
            s.a.push(amp("amp_c", "m1", 0.5));
        }
    });
    let preset = &rig.presets["p"];
    let AudioBlockKind::Split(split) = &preset.blocks[0].kind else {
        panic!("block 0 is the split")
    };
    let ids: Vec<&str> = split.a.iter().map(|b| b.id.0.as_str()).collect();
    assert_eq!(ids, vec!["amp_a", "amp_c"], "the new block is in the preset");
    assert!(
        preset.scenes.is_empty(),
        "adding a block is structural and resets the scenes, inside a path as on the top level"
    );
}
```

- [ ] **Step 2: Run the test — behavioral RED**

Run: `nice -n 19 cargo test -p project --test issue_328_split_write_back -j 2`
Expected: 5 FAILED:
- `a_knob_turned_inside_a_path_is_kept_in_the_active_scene` — `the edit is the active scene's override` / `left: None` / `right: Some(0.8)`
- `a_bypass_inside_a_path_is_kept_in_the_active_scene` — `left: None` / `right: Some(true)`
- `a_mixer_knob_is_a_scene_override_while_the_split_mode_is_preset_wide` — `a float knob is per scene` / `left: None` / `right: Some(-50.0)`
- `a_model_swap_inside_a_path_keeps_every_scene` — `scene 1 must not inherit scene 2's amp_b values` / `left: Some(0.1)` / `right: Some(0.5)`
- `a_block_added_inside_a_path_is_a_structural_edit` — `adding a block is structural and resets the scenes, inside a path as on the top level`

- [ ] **Step 3: Implement the capture**

In `crates/project/src/rig_write_back.rs`:
- line 5: `use crate::block::{block_params_mut, find_block_mut, walk_blocks, AudioBlock, AudioBlockKind};`
- lines 39-44:

```rust
        // Factory template, indexed by block id (immutable diff base). #328:
        // the blocks inside a split's paths are keyed the same way, so a knob
        // or bypass edited inside a path diffs exactly like a top-level one.
        let base: BTreeMap<String, AudioBlock> = walk_blocks(&preset.blocks)
            .into_iter()
            .map(|b| (b.id.0.clone(), b.clone()))
            .collect();
```

- line 53: `for edited in walk_blocks(&blocks) {`
- lines 75-79:

```rust
            let pair = match (&edited.kind, &base_blk.kind) {
                (AudioBlockKind::Core(e), AudioBlockKind::Core(b)) => Some((&e.params, &b.params)),
                (AudioBlockKind::Nam(e), AudioBlockKind::Nam(b)) => Some((&e.params, &b.params)),
                // #328: the split and mixer knobs diff like a model's params.
                (AudioBlockKind::Split(e), AudioBlockKind::Split(b)) => {
                    Some((&e.params, &b.params))
                }
                _ => None,
            };
```

- lines 113-127:

```rust
        for (bid, pid, val) in set_base_param {
            let params = find_block_mut(&mut preset.blocks, &bid)
                .and_then(|b| block_params_mut(&mut b.kind));
            if let Some(params) = params {
                params.insert(pid, val);
            }
        }
```

Replace `crates/project/src/rig_model_swap.rs` from line 10 to the end with:

```rust
//!
//! #328: the blocks inside a split's paths are slots too. A model swap on one
//! of them takes this path; a split whose paths changed shape (a block added,
//! removed or moved) is structural and is left to the structural write-back.

use crate::block::{block_params, for_each_block_mut, walk_blocks, AudioBlock, AudioBlockKind};
use crate::rig::RigProject;

/// The slot shape of a block list: every block id in walk order and, for a
/// split, its end and the length of each path — so moving a block from path A
/// to path B is a different shape even though the walk visits the same ids.
fn slot_layout(blocks: &[AudioBlock]) -> Vec<String> {
    walk_blocks(blocks)
        .into_iter()
        .map(|b| match &b.kind {
            AudioBlockKind::Split(split) => format!(
                "{}|{}|{}|{}",
                b.id.0,
                split.end.as_str(),
                split.a.len(),
                split.b.len()
            ),
            _ => b.id.0.clone(),
        })
        .collect()
}

impl RigProject {
    /// Write every same-id, same-position model swap in `blocks` (the chain's
    /// processing blocks, split paths included) into the input's active
    /// preset: the base block takes the new kind and keeps its own `enabled`,
    /// and the scene overrides and `scene-params` entries of that block whose
    /// parameter the new model lacks are removed. Everything else in the
    /// preset is left alone.
    ///
    /// Returns `blocks` ready for the per-scene diff: a swapped block is
    /// replaced by what the active scene resolves it to (keeping the live
    /// `enabled`), so the diff neither clears the active scene's surviving
    /// overrides nor bakes them into the base. When the slot layout differs
    /// (a structural edit) nothing is written and `blocks` comes back
    /// unchanged.
    pub fn write_back_model_swaps(
        &mut self,
        input: &str,
        blocks: Vec<AudioBlock>,
    ) -> Vec<AudioBlock> {
        let Some((preset_name, scene_idx)) = self.inputs.get(input).and_then(|ri| {
            ri.bank
                .get(&ri.active_preset)
                .cloned()
                .map(|n| (n, ri.active_scene))
        }) else {
            return blocks;
        };
        let Some(preset) = self.presets.get_mut(&preset_name) else {
            return blocks;
        };
        if slot_layout(&preset.blocks) != slot_layout(&blocks) {
            return blocks;
        }

        // Every slot whose model changed, with the kind it takes. A split's
        // identity carries its paths, so it differs whenever a path block was
        // swapped — the swap is written on that path block instead.
        let mut swapped: Vec<(String, AudioBlockKind)> = Vec::new();
        {
            let live = walk_blocks(&blocks);
            let mut slot = 0;
            for_each_block_mut(&mut preset.blocks, &mut |base| {
                let edited = live[slot];
                slot += 1;
                if matches!(base.kind, AudioBlockKind::Split(_))
                    || base.kind.model_identity() == edited.kind.model_identity()
                {
                    return;
                }
                base.kind = edited.kind.clone();
                swapped.push((base.id.0.clone(), edited.kind.clone()));
            });
        }
        if swapped.is_empty() {
            return blocks;
        }

        for (bid, kind) in &swapped {
            let prefix = format!("{bid}.");
            let kept = |key: &str| match key.strip_prefix(&prefix) {
                Some(param) => block_params(kind).is_some_and(|p| p.get(param).is_some()),
                None => true,
            };
            for scene in preset.scenes.values_mut() {
                scene.params.retain(|key, _| kept(key));
            }
            preset.scene_params.retain(|key| kept(key));
        }

        let resolved_blocks = preset.apply_scene(scene_idx);
        let resolved = walk_blocks(&resolved_blocks);
        let mut live = blocks;
        for_each_block_mut(&mut live, &mut |block| {
            if !swapped.iter().any(|(id, _)| *id == block.id.0) {
                return;
            }
            if let Some(scene_view) = resolved.iter().find(|b| b.id == block.id) {
                *block = AudioBlock {
                    enabled: block.enabled,
                    ..(*scene_view).clone()
                };
            }
        });
        live
    }
}
```

(Keep lines 1-9 — the header and the #986 paragraph — unchanged.)

- [ ] **Step 4: Run the test — GREEN**

Run: `nice -n 19 cargo test -p project -j 2` then `nice -n 19 cargo test -p application --test issue_986_model_change_keeps_scenes -j 2`
Expected: `issue_328_split_write_back` 5 passed; `rig_writeback_tests`, `rig_scene_tests`, `rig_sync_tests` and the #986 suite unchanged and green.

- [ ] **Step 5: Document**

In `docs/projects/project-openrig-format.md`, at the end of "### Chain split (#328)" add:

```markdown
Scenes, edit capture and model swaps reach the blocks inside the paths exactly like top-level blocks: a path block's scene keys are `<its id>.<param>`, the split's own knobs are `<split id>.<param>` (float knobs per scene, `split_mode` / `mix_b_polarity` / `mix_master_sum` preset-wide, #690). Swapping a path block's model keeps every scene (#986 applies inside paths); adding, removing or moving a block inside a path is structural and resets the scenes, like on the top level.
```

- [ ] **Step 6: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/project/src/rig_write_back.rs crates/project/src/rig_model_swap.rs crates/project/tests/issue_328_split_write_back.rs docs/projects/project-openrig-format.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): capture edits and model swaps made inside a split"
```

- [ ] **Step 7: Push gate, push, comment**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 1 T7 pushed (<hash>): write-back + model swap recurse into split paths (#986 inside paths, nested structural edit resets scenes). Push gate green."
```

---

### Task 8: `EndpointDisables` model

**Files:**
- Create: `crates/project/src/endpoint_disables.rs`, `crates/project/src/endpoint_candidates.rs`
- Modify: `crates/project/src/lib.rs` (add `pub mod endpoint_candidates;` and `pub mod endpoint_disables;` before `pub mod endpoint_ref;`)
- Test: `crates/project/tests/issue_328_endpoint_disables.rs`

**Interfaces:**
- Consumes: `domain::io_binding::IoBinding`.
- Produces:
  - `project::endpoint_disables::EndpointRef { pub io: String, pub endpoint: String }` (`Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema`) — the contract's checklist ref, a separate type from the looper's `project::endpoint_ref::EndpointRef { binding_id, endpoint }`
  - `project::endpoint_disables::EndpointNode { Input, Output, PathAOutput, PathBOutput }` (`#[serde(rename_all = "snake_case")]`, `Copy, Eq, JsonSchema`)
  - `project::endpoint_disables::EndpointDisables { pub inputs, pub outputs, pub path_a_outputs, pub path_b_outputs: Vec<EndpointRef> }` (`Default, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema`; empty lists not written)
  - methods: `is_empty(&self) -> bool`, `is_enabled(&self, node: EndpointNode, r: &EndpointRef) -> bool`, `set_enabled(&mut self, node: EndpointNode, r: EndpointRef, enabled: bool)`, `tail_output_enabled(&self, y_split: bool, r: &EndpointRef) -> bool`, `retain_known(&mut self, inputs: &[EndpointRef], outputs: &[EndpointRef])`
  - `project::endpoint_candidates::endpoint_candidates(io_binding_ids: &[String], registry: &[IoBinding]) -> (Vec<project::endpoint_disables::EndpointRef>, Vec<project::endpoint_disables::EndpointRef>)`
  - Hand-off: Part 2 (commands) calls `set_enabled` in `SetChainEndpointEnabled` and `retain_known` + `endpoint_candidates` on the save path ("dropped on the next save", spec §1.3); Part 6 (chain row) lists `endpoint_candidates` in the checklist — NOT `resolve_chain_ports`, which drops the unchecked endpoints after Task 11, so an unchecked row would vanish from its own checklist.

- [ ] **Step 1: Write the failing test**

Create `crates/project/tests/issue_328_endpoint_disables.rs`:

```rust
//! #328 — the graph's input/output checklists (spec §1.3). Every endpoint of
//! the chain's own E/S is checked by default; unchecking one leaves it out of
//! THAT node only, and it stays listed.

use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::endpoint_candidates::endpoint_candidates;
use project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef};

fn r(binding: &str, endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: binding.into(),
        endpoint: endpoint.into(),
    }
}

fn ep(name: &str) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: vec![0],
    }
}

fn binding(id: &str, inputs: &[&str], outputs: &[&str]) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.to_uppercase(),
        inputs: inputs.iter().map(|n| ep(n)).collect(),
        outputs: outputs.iter().map(|n| ep(n)).collect(),
    }
}

#[test]
fn every_endpoint_is_checked_by_default() {
    let d = EndpointDisables::default();
    assert!(d.is_empty());
    for node in [
        EndpointNode::Input,
        EndpointNode::Output,
        EndpointNode::PathAOutput,
        EndpointNode::PathBOutput,
    ] {
        assert!(d.is_enabled(node, &r("io", "x")), "{node:?} starts checked");
    }
}

#[test]
fn unchecking_leaves_the_endpoint_out_of_that_node_only() {
    let mut d = EndpointDisables::default();
    d.set_enabled(EndpointNode::PathAOutput, r("io", "out L"), false);
    assert!(!d.is_enabled(EndpointNode::PathAOutput, &r("io", "out L")));
    assert!(
        d.is_enabled(EndpointNode::PathBOutput, &r("io", "out L")),
        "path B's node still has it"
    );
    assert!(d.is_enabled(EndpointNode::Output, &r("io", "out L")));
    assert!(
        d.is_enabled(EndpointNode::PathAOutput, &r("other", "out L")),
        "the same name on another E/S is another endpoint"
    );
}

#[test]
fn rechecking_restores_the_default_and_unchecking_twice_records_once() {
    let mut d = EndpointDisables::default();
    d.set_enabled(EndpointNode::Input, r("io", "in 2"), false);
    d.set_enabled(EndpointNode::Input, r("io", "in 2"), false);
    assert_eq!(d.inputs, vec![r("io", "in 2")]);
    d.set_enabled(EndpointNode::Input, r("io", "in 2"), true);
    assert!(d.is_empty());
}

#[test]
fn a_y_tail_output_lives_while_either_path_keeps_it() {
    let mut d = EndpointDisables::default();
    d.set_enabled(EndpointNode::PathAOutput, r("io", "out L"), false);
    assert!(d.tail_output_enabled(true, &r("io", "out L")), "path B still feeds it");
    d.set_enabled(EndpointNode::PathBOutput, r("io", "out L"), false);
    assert!(!d.tail_output_enabled(true, &r("io", "out L")), "no path feeds it");
    assert!(
        d.tail_output_enabled(false, &r("io", "out L")),
        "a linear or Mix chain reads the chain output node"
    );
}

#[test]
fn endpoints_the_bindings_no_longer_offer_are_dropped() {
    let mut d = EndpointDisables::default();
    d.set_enabled(EndpointNode::Input, r("io", "in 2"), false);
    d.set_enabled(EndpointNode::Input, r("io", "gone"), false);
    d.set_enabled(EndpointNode::PathBOutput, r("io", "gone out"), false);
    d.set_enabled(EndpointNode::Output, r("io", "out L"), false);
    d.retain_known(&[r("io", "in 1"), r("io", "in 2")], &[r("io", "out L")]);
    assert_eq!(d.inputs, vec![r("io", "in 2")]);
    assert!(d.path_b_outputs.is_empty());
    assert_eq!(d.outputs, vec![r("io", "out L")]);
}

#[test]
fn the_checklist_writes_only_the_nodes_that_leave_something_out() {
    let mut d = EndpointDisables::default();
    d.set_enabled(EndpointNode::PathBOutput, r("io", "out R"), false);
    let yaml = serde_yaml::to_string(&d).expect("serialize");
    assert!(yaml.contains("path_b_outputs"), "got:\n{yaml}");
    assert!(!yaml.contains("inputs:"), "empty nodes are not written, got:\n{yaml}");
    assert_eq!(
        serde_yaml::from_str::<EndpointDisables>(&yaml).expect("deserialize"),
        d
    );
    assert_eq!(
        serde_yaml::to_string(&EndpointNode::PathAOutput)
            .expect("serialize node")
            .trim(),
        "path_a_output"
    );
}

#[test]
fn the_candidates_are_every_endpoint_of_the_selected_bindings() {
    let registry = vec![
        binding("io", &["in 1", "in 2"], &["out L", "out R"]),
        binding("fx", &["ret"], &["send"]),
    ];
    let (inputs, outputs) =
        endpoint_candidates(&["io".to_string(), "missing".to_string()], &registry);
    assert_eq!(inputs, vec![r("io", "in 1"), r("io", "in 2")]);
    assert_eq!(outputs, vec![r("io", "out L"), r("io", "out R")]);
}
```

- [ ] **Step 2: Add the compile skeleton**

Create `crates/project/src/endpoint_disables.rs`:

```rust
//! Responsibility: records which endpoints of a chain's own bindings each graph node leaves out.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EndpointRef {
    pub io: String,
    pub endpoint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EndpointNode {
    Input,
    Output,
    PathAOutput,
    PathBOutput,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EndpointDisables {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<EndpointRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<EndpointRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_a_outputs: Vec<EndpointRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_b_outputs: Vec<EndpointRef>,
}

impl EndpointDisables {
    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn is_enabled(&self, _node: EndpointNode, _r: &EndpointRef) -> bool {
        false
    }

    pub fn set_enabled(&mut self, _node: EndpointNode, _r: EndpointRef, _enabled: bool) {}

    pub fn tail_output_enabled(&self, _y_split: bool, _r: &EndpointRef) -> bool {
        false
    }

    pub fn retain_known(&mut self, _inputs: &[EndpointRef], _outputs: &[EndpointRef]) {}
}
```

Create `crates/project/src/endpoint_candidates.rs`:

```rust
//! Responsibility: lists the endpoints a chain's own bindings offer.

use domain::io_binding::IoBinding;

use crate::endpoint_disables::EndpointRef;

pub fn endpoint_candidates(
    _io_binding_ids: &[String],
    _registry: &[IoBinding],
) -> (Vec<EndpointRef>, Vec<EndpointRef>) {
    (Vec::new(), Vec::new())
}
```

Add `pub mod endpoint_candidates;` and `pub mod endpoint_disables;` to `crates/project/src/lib.rs` before `pub mod endpoint_ref;` (line 27).

- [ ] **Step 3: Run the test — behavioral RED**

Run: `nice -n 19 cargo test -p project --test issue_328_endpoint_disables -j 2`
Expected: 7 FAILED, e.g. `every_endpoint_is_checked_by_default` — `assertion failed: d.is_empty()`; `rechecking_restores…` — `left: []` / `right: [EndpointRef { io: "io", endpoint: "in 2" }]`; `the_candidates_are…` — `left: []` / `right: [EndpointRef { io: "io", endpoint: "in 1" }, …]`.

- [ ] **Step 4: Implement**

Replace the whole `crates/project/src/endpoint_disables.rs`:

```rust
//! Responsibility: records which endpoints of a chain's own bindings each graph node leaves out.
//!
//! #328 (spec §1.3): the input and output nodes of a chain's graph list every
//! endpoint of the chain's E/S bindings, checked by default. Unchecking one
//! leaves it out of THAT node only — it stays listed, and nothing is removed
//! from the E/S. This is chain configuration, not preset data (one preset is
//! reused by several inputs), so it lives on `RigInput` and on the projected
//! `Chain`.

use serde::{Deserialize, Serialize};

/// One endpoint of one of the chain's own E/S bindings, as a checklist row
/// addresses it: the binding id (`io`, the same field name as `InputBlock.io`)
/// plus the endpoint name — a name alone is not unique across bindings.
/// Not the looper's `crate::endpoint_ref::EndpointRef` (#323), which keeps its
/// own `binding_id` field; where both are in scope, spell this one
/// `project::endpoint_disables::EndpointRef`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EndpointRef {
    pub io: String,
    pub endpoint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EndpointNode {
    /// The chain's input node.
    Input,
    /// The chain's output node (linear chains and Split → Mix).
    Output,
    /// Y → A/B: path A's output node.
    PathAOutput,
    /// Y → A/B: path B's output node.
    PathBOutput,
}

/// The unchecked endpoints, per node. Empty — the default — keeps every
/// endpoint; empty nodes are not written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EndpointDisables {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<EndpointRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<EndpointRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_a_outputs: Vec<EndpointRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_b_outputs: Vec<EndpointRef>,
}

impl EndpointDisables {
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
            && self.outputs.is_empty()
            && self.path_a_outputs.is_empty()
            && self.path_b_outputs.is_empty()
    }

    /// Whether `node` keeps the endpoint `r` (checked).
    pub fn is_enabled(&self, node: EndpointNode, r: &EndpointRef) -> bool {
        !self.list(node).contains(r)
    }

    /// Check (`enabled`) or uncheck one endpoint on one node. Idempotent.
    pub fn set_enabled(&mut self, node: EndpointNode, r: EndpointRef, enabled: bool) {
        let list = self.list_mut(node);
        list.retain(|kept| *kept != r);
        if !enabled {
            list.push(r);
        }
    }

    /// Whether a chain TAIL output stays live. A Y → A/B chain has no chain
    /// output node: the output lives while either path's node keeps it.
    pub fn tail_output_enabled(&self, y_split: bool, r: &EndpointRef) -> bool {
        if y_split {
            self.is_enabled(EndpointNode::PathAOutput, r)
                || self.is_enabled(EndpointNode::PathBOutput, r)
        } else {
            self.is_enabled(EndpointNode::Output, r)
        }
    }

    /// Drop the refs to endpoints the chain's bindings no longer offer (spec
    /// §1.3: ignored at runtime, dropped on the next save). `inputs` and
    /// `outputs` are what [`crate::endpoint_candidates::endpoint_candidates`]
    /// lists for the chain.
    pub fn retain_known(&mut self, inputs: &[EndpointRef], outputs: &[EndpointRef]) {
        self.inputs.retain(|r| inputs.contains(r));
        self.outputs.retain(|r| outputs.contains(r));
        self.path_a_outputs.retain(|r| outputs.contains(r));
        self.path_b_outputs.retain(|r| outputs.contains(r));
    }

    fn list(&self, node: EndpointNode) -> &Vec<EndpointRef> {
        match node {
            EndpointNode::Input => &self.inputs,
            EndpointNode::Output => &self.outputs,
            EndpointNode::PathAOutput => &self.path_a_outputs,
            EndpointNode::PathBOutput => &self.path_b_outputs,
        }
    }

    fn list_mut(&mut self, node: EndpointNode) -> &mut Vec<EndpointRef> {
        match node {
            EndpointNode::Input => &mut self.inputs,
            EndpointNode::Output => &mut self.outputs,
            EndpointNode::PathAOutput => &mut self.path_a_outputs,
            EndpointNode::PathBOutput => &mut self.path_b_outputs,
        }
    }
}
```

Replace the whole `crates/project/src/endpoint_candidates.rs`:

```rust
//! Responsibility: lists the endpoints a chain's own bindings offer.
//!
//! #328 (spec §5.3): what the graph's input and output checklists show — every
//! input and every output endpoint of the E/S bindings the chain selects,
//! unchecked ones included — and what a stale checklist entry is pruned
//! against (`EndpointDisables::retain_known`).

use domain::io_binding::IoBinding;

use crate::endpoint_disables::EndpointRef;

/// `(inputs, outputs)` of the bindings in `io_binding_ids`, in selection order
/// then binding order. A selected binding missing from `registry` offers
/// nothing.
pub fn endpoint_candidates(
    io_binding_ids: &[String],
    registry: &[IoBinding],
) -> (Vec<EndpointRef>, Vec<EndpointRef>) {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for binding_id in io_binding_ids {
        let Some(binding) = registry.iter().find(|b| &b.id == binding_id) else {
            continue;
        };
        let refer = |name: &str| EndpointRef {
            io: binding.id.clone(),
            endpoint: name.to_string(),
        };
        inputs.extend(binding.inputs.iter().map(|ep| refer(&ep.name)));
        outputs.extend(binding.outputs.iter().map(|ep| refer(&ep.name)));
    }
    (inputs, outputs)
}
```

- [ ] **Step 5: Run the test — GREEN**

Run: `nice -n 19 cargo test -p project --test issue_328_endpoint_disables -j 2`
Expected: `test result: ok. 7 passed; 0 failed`.

- [ ] **Step 6: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/project/src/endpoint_disables.rs crates/project/src/endpoint_candidates.rs crates/project/src/lib.rs crates/project/tests/issue_328_endpoint_disables.rs
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): model the graph's endpoint checklists"
```

- [ ] **Step 7: Push gate, push, comment**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 1 T8 pushed (<hash>): EndpointRef { io, endpoint } + EndpointDisables/EndpointNode + endpoint_candidates. Push gate green."
```

---

### Task 9: `RigInput` carries the checklists (persisted, no version bump)

**Files:**
- Modify: `crates/project/src/rig.rs:12-13` (import) and after line 95 (field)
- Modify: `crates/project/src/migrate.rs:87` (`disabled_endpoints: Default::default(),` — Task 10 carries the chain's value)
- Modify (sweep): every other `RigInput { … }` literal (~46 sites; test files listed by the compiler) — add `disabled_endpoints: Default::default(),` after the `loopers:` line
- Modify: `docs/projects/project-openrig-format.md` (input fields)
- Test: `crates/infra-yaml/tests/issue_328_endpoint_disables_yaml.rs`

**Interfaces:**
- Consumes: Task 8 `EndpointDisables`, `EndpointNode`, `EndpointRef`.
- Produces: `RigInput.disabled_endpoints: EndpointDisables` with `#[serde(default, skip_serializing_if = "EndpointDisables::is_empty")]`, YAML key `disabled_endpoints`.

- [ ] **Step 1: Write the failing test**

Create `crates/infra-yaml/tests/issue_328_endpoint_disables_yaml.rs`:

```rust
//! #328 — the endpoint checklists are chain configuration (spec §1.3): they
//! live on the rig input in `project.yaml`, survive save + reload, need no
//! version bump, and a file written before them loads with every endpoint
//! checked.

use infra_yaml::{parse_rig_project, serialize_rig_project};
use project::endpoint_disables::{EndpointNode, EndpointRef};

const BOUND: &str = r#"
project:
  inputs:
    g:
      bank:
        1: p
      active-preset: 1
      io_binding_ids: [io]
  presets:
    p:
      blocks: []
"#;

fn r(endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: "io".into(),
        endpoint: endpoint.into(),
    }
}

#[test]
fn an_inputs_unchecked_endpoints_survive_save_and_reload() {
    let mut rig = parse_rig_project(BOUND).expect("a pre-#328 file loads");
    assert!(
        rig.inputs["g"].disabled_endpoints.is_empty(),
        "a file written before the checklist has every endpoint checked"
    );
    let untouched = serialize_rig_project(&rig).expect("serialize");
    assert!(
        !untouched.contains("disabled_endpoints"),
        "nothing unchecked, nothing written, got:\n{untouched}"
    );

    let input = rig.inputs.get_mut("g").expect("input g");
    input
        .disabled_endpoints
        .set_enabled(EndpointNode::Input, r("in 2"), false);
    input
        .disabled_endpoints
        .set_enabled(EndpointNode::PathBOutput, r("out L"), false);
    let yaml = serialize_rig_project(&rig).expect("serialize");
    assert!(yaml.contains("disabled_endpoints"), "the checklist is written, got:\n{yaml}");
    assert!(yaml.contains("version: 1\n"), "the checklist needs no version bump, got:\n{yaml}");

    let back = parse_rig_project(&yaml).expect("reload");
    assert_eq!(
        back.inputs["g"].disabled_endpoints, rig.inputs["g"].disabled_endpoints,
        "the checklist survives save and reload"
    );
}
```

- [ ] **Step 2: Add the field as a compile skeleton (not persisted yet) and sweep the literals**

In `crates/project/src/rig.rs` add `use crate::endpoint_disables::EndpointDisables;` to the imports (after the `use crate::block::…` line) and after line 95 (`pub loopers: …,`) add:

```rust
    /// #328: the endpoints of this input's own bindings that a node of its
    /// chain graph leaves out (the input/output checklists).
    #[serde(skip)]
    pub disabled_endpoints: EndpointDisables,
```

Sweep every `RigInput { … }` literal: run

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo build --workspace --all-targets --keep-going -j 2 2>&1 | grep -A1 'missing field `disabled_endpoints` in initializer of `[a-z_:]*RigInput`' | grep -- "-->"
```

(the pattern accepts `project::rig::RigInput` too, for a literal written with its full path) and, with the Edit tool, add `disabled_endpoints: Default::default(),` right after the `loopers: …,` line of each listed literal (production: `crates/project/src/migrate.rs:87`; the rest are tests, ~46 literals in ~37 files). Re-run until the grep prints nothing (a crate hidden behind a failing dependency only reports once that dependency builds). Then check, file by file, the code the macOS compiler does not see (`cfg(all(target_os = "linux", feature = "jack"))`, Windows-only) — a file with more `RigInput {` literals than `disabled_endpoints` lines is listed (the `--include` glob is quoted: the user's shell is zsh):

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && for f in $(grep -rlE 'RigInput \{' crates --include='*.rs'); do lits=$(grep -E 'RigInput \{' "$f" | grep -cvE '(struct|impl) '); set=$(grep -c 'disabled_endpoints' "$f"); [ "$set" -lt "$lits" ] && echo "$f: $lits literal(s), $set disabled_endpoints line(s)"; done
```

Expected: no output.

- [ ] **Step 3: Run the test — behavioral RED**

Run: `nice -n 19 cargo test -p infra-yaml --test issue_328_endpoint_disables_yaml -j 2`
Expected: FAILED — `the checklist is written, got: version: 1 project: …` (no `disabled_endpoints` key).

- [ ] **Step 4: Persist it**

Replace the `#[serde(skip)]` field in `crates/project/src/rig.rs` with:

```rust
    /// #328: the endpoints of this input's own bindings that a node of its
    /// chain graph leaves out (the input/output checklists). Chain
    /// configuration, not preset data. Empty — the default — keeps every
    /// endpoint, so pre-#328 files load unchanged and it needs no version bump.
    #[serde(default, skip_serializing_if = "EndpointDisables::is_empty")]
    pub disabled_endpoints: EndpointDisables,
```

- [ ] **Step 5: Run the test — GREEN**

Run: `nice -n 19 cargo test -p infra-yaml -j 2`
Expected: `issue_328_endpoint_disables_yaml` 1 passed; `project_file_tests` (round trip determinism) green.

- [ ] **Step 6: Document**

In `docs/projects/project-openrig-format.md`, add to the Model table after the `inputs.<name>.active-scene` row:

```markdown
| `inputs.<name>.disabled_endpoints` | `EndpointDisables` | #328 graph checklists: `{ inputs, outputs, path_a_outputs, path_b_outputs }`, each a list of `{ io, endpoint }` (binding id + endpoint name) left out of that node. Absent = every endpoint checked; no version bump. |
```

- [ ] **Step 7: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status --short
```

Add every path listed (explicitly, one `add` per path or a space-separated list — never `-A`), then:

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): persist the endpoint checklists on the rig input"
```

- [ ] **Step 8: Push gate, push, comment**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 1 T9 pushed (<hash>): RigInput.disabled_endpoints (serde default, no version bump) + literal sweep. Push gate green."
```

---

### Task 10: `Chain` carries the checklists (projection, capture, legacy YAML)

**Files:**
- Modify: `crates/project/src/chain.rs:6,44-45` (import + field after `loopers`)
- Modify: `crates/engine/src/rig_projection.rs:110` (copy from the rig input)
- Modify: `crates/project/src/rig_sync.rs:74` (capture back)
- Modify: `crates/project/src/migrate.rs:87` (carry the chain's value)
- Modify: `crates/infra-yaml/src/chain_yaml.rs:8,46,86,109` (legacy/dirty-fingerprint persistence)
- Modify: `crates/adapter-gui/src/chain_editor.rs:51-65` (edit mode carries the existing chain's checklists — the #826 rule "everything the editor does NOT edit comes back untouched")
- Modify (sweep): every other `Chain { … }` literal (~356 sites in ~280 files) — `disabled_endpoints: Default::default(),` after the `loopers:` line
- Test: `crates/engine/tests/issue_328_endpoint_disables.rs` (created here), `crates/project/tests/issue_328_endpoint_disables_capture.rs`, `crates/infra-yaml/tests/issue_328_endpoint_disables_yaml.rs` (appended), `crates/adapter-gui/src/chain_editor_tests.rs` (one test appended)

**Interfaces:**
- Consumes: Task 8/9.
- Produces: `project::chain::Chain.disabled_endpoints: EndpointDisables` (`#[serde(default, skip_serializing_if = "EndpointDisables::is_empty")]`); `rig_to_chains` copies it from `RigInput`; `sync_synthetic_into_rig` captures it back; `migrate_legacy_project` carries it; `serialize_project` / `YamlProjectRepository` round-trip it.

- [ ] **Step 1: Write the failing tests**

Create `crates/engine/tests/issue_328_endpoint_disables.rs`:

```rust
//! #328 — the endpoint checklists reach the engine (spec §1.3): the projected
//! chain carries its input's checklist, and the rig and chain conflict
//! detectors agree on it.

use std::collections::BTreeMap;

use engine::rig_runtime::rig_to_chains;
use project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef};
use project::rig::{RigInput, RigPreset, RigProject};

fn unchecked_input(endpoint: &str) -> EndpointDisables {
    let mut disables = EndpointDisables::default();
    disables.set_enabled(
        EndpointNode::Input,
        EndpointRef {
            io: "shared".into(),
            endpoint: endpoint.into(),
        },
        false,
    );
    disables
}

fn input(disabled_endpoints: EndpointDisables) -> RigInput {
    RigInput {
        label: None,
        bank: BTreeMap::from([(1, "p".to_string())]),
        active_preset: 1,
        active_scene: 1,
        routing: Vec::new(),
        instrument: "electric_guitar".into(),
        io: String::new(),
        endpoint: String::new(),
        io_binding_ids: vec!["shared".into()],
        loopers: Vec::new(),
        disabled_endpoints,
    }
}

fn rig(g1: EndpointDisables, g2: EndpointDisables) -> RigProject {
    RigProject {
        name: None,
        inputs: BTreeMap::from([("g1".to_string(), input(g1)), ("g2".to_string(), input(g2))]),
        outputs: BTreeMap::new(),
        presets: BTreeMap::from([(
            "p".to_string(),
            RigPreset::from_legacy_blocks(Vec::new(), 100.0),
        )]),
        midi: None,
        chain_order: Vec::new(),
    }
}

#[test]
fn the_projected_chain_carries_its_inputs_unchecked_endpoints() {
    let chains = rig_to_chains(&rig(unchecked_input("in 2"), EndpointDisables::default()));
    let g1 = chains
        .iter()
        .find(|c| c.id.0 == "rig:g1")
        .expect("g1 is projected");
    assert_eq!(
        g1.disabled_endpoints,
        unchecked_input("in 2"),
        "the synthetic chain carries the rig input's checklist"
    );
}
```

Create `crates/project/tests/issue_328_endpoint_disables_capture.rs`:

```rust
//! #328 — a checklist edit made on the projected chain reaches the rig (the
//! save path), and a new chain saved into the rig keeps its checklist.

use std::collections::BTreeMap;

use domain::ids::ChainId;
use project::chain::Chain;
use project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef};
use project::migrate::migrate_legacy_project;
use project::project::Project;
use project::rig::{RigInput, RigPreset, RigProject};
use project::rig_sync::sync_synthetic_into_rig;

fn unchecked() -> EndpointDisables {
    let mut disables = EndpointDisables::default();
    disables.set_enabled(
        EndpointNode::Output,
        EndpointRef {
            io: "io".into(),
            endpoint: "out R".into(),
        },
        false,
    );
    disables
}

fn chain(id: &str, disabled_endpoints: EndpointDisables) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("Guitar".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: Vec::new(),
        di_output: None,
        loopers: Vec::new(),
        disabled_endpoints,
    }
}

fn project_with(chains: Vec<Chain>) -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        chains,
        midi: None,
    }
}

#[test]
fn a_checklist_edit_on_the_projected_chain_is_captured_into_the_rig_input() {
    let mut rig = RigProject {
        name: None,
        inputs: BTreeMap::from([(
            "g".to_string(),
            RigInput {
                label: None,
                bank: BTreeMap::from([(1, "p".to_string())]),
                active_preset: 1,
                active_scene: 1,
                routing: Vec::new(),
                instrument: "electric_guitar".into(),
                io: String::new(),
                endpoint: String::new(),
                io_binding_ids: vec!["io".into()],
                loopers: Vec::new(),
                disabled_endpoints: EndpointDisables::default(),
            },
        )]),
        outputs: BTreeMap::new(),
        presets: BTreeMap::from([(
            "p".to_string(),
            RigPreset::from_legacy_blocks(Vec::new(), 100.0),
        )]),
        midi: None,
        chain_order: Vec::new(),
    };
    sync_synthetic_into_rig(&mut rig, &project_with(vec![chain("rig:g", unchecked())]));
    assert_eq!(
        rig.inputs["g"].disabled_endpoints,
        unchecked(),
        "the save path captures the checklist into project.yaml"
    );
}

#[test]
fn a_new_chain_saved_into_the_rig_keeps_its_checklist() {
    let rig = migrate_legacy_project(&project_with(vec![chain("chain:new", unchecked())]));
    assert_eq!(rig.inputs["input-1"].disabled_endpoints, unchecked());
}
```

Append to `crates/infra-yaml/tests/issue_328_endpoint_disables_yaml.rs`:

```rust
#[test]
fn a_checklist_edit_changes_the_legacy_serialization_the_dirty_check_compares() {
    use domain::ids::ChainId;
    use infra_yaml::{serialize_project, YamlProjectRepository};
    use project::chain::Chain;
    use project::endpoint_disables::EndpointDisables;
    use project::project::Project;

    let project_with = |disabled_endpoints: EndpointDisables| Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("rig:g".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["io".into()],
            blocks: Vec::new(),
            di_output: None,
            loopers: Vec::new(),
            disabled_endpoints,
        }],
        midi: None,
    };
    let mut unchecked = EndpointDisables::default();
    unchecked.set_enabled(EndpointNode::Input, r("in 2"), false);

    let before = serialize_project(&project_with(EndpointDisables::default())).expect("serialize");
    let after = serialize_project(&project_with(unchecked.clone())).expect("serialize");
    assert_ne!(
        before, after,
        "the dirty fingerprint must see a checklist edit, or Save answers 'no changes'"
    );

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("p.yaml");
    std::fs::write(&path, &after).expect("write");
    let loaded = YamlProjectRepository { path }
        .load_current_project()
        .expect("load");
    assert_eq!(loaded.chains[0].disabled_endpoints, unchecked);
}
```

Append to `crates/adapter-gui/src/chain_editor_tests.rs` (its `recorded_chain()` literal gets `disabled_endpoints: Default::default()` from the Step 2 sweep; `Chain`, `chain_draft_from_chain` and `chain_from_draft` are already imported at the top of that file):

```rust
// ── #328: the endpoint checklists are something the editor does NOT edit ────

/// The chain editor edits name, instrument and bindings. The graph's endpoint
/// checklists are chain configuration it never shows, so a rename must hand
/// them back untouched — the #826 rule, or renaming a chain re-checks every
/// endpoint the user left out (a second guitar's input suddenly plays).
#[test]
fn editing_a_chain_keeps_its_endpoint_checklists() {
    use project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef};

    let mut disabled_endpoints = EndpointDisables::default();
    disabled_endpoints.set_enabled(
        EndpointNode::Input,
        EndpointRef {
            io: "io-1".into(),
            endpoint: "in 2".into(),
        },
        false,
    );
    let existing = Chain {
        disabled_endpoints,
        ..recorded_chain()
    };
    let mut draft = chain_draft_from_chain(0, &existing);
    draft.name = "GUITARRA - TONES".into();

    let edited = chain_from_draft(&draft, Some(&existing));

    assert_eq!(
        edited.disabled_endpoints, existing.disabled_endpoints,
        "renaming a chain must not re-check the endpoints its graph left out"
    );
}
```

- [ ] **Step 2: Add the field as a compile skeleton and sweep the literals**

In `crates/project/src/chain.rs` add `use crate::endpoint_disables::EndpointDisables;` after line 6 (`use crate::block::{…};`) and, after the `pub loopers: Vec<LooperConfig>,` field (line 46 once the import is in), add:

```rust
    /// #328: the endpoints of the chain's own bindings that a node of its
    /// graph leaves out (the input/output checklists). Projected from the rig
    /// input by `rig_to_chains` and captured back by `sync_synthetic_into_rig`;
    /// empty keeps every endpoint.
    #[serde(default, skip_serializing_if = "EndpointDisables::is_empty")]
    pub disabled_endpoints: EndpointDisables,
```

Sweep every `Chain { … }` literal (including `crates/engine/src/rig_projection.rs:85-111`, `crates/infra-yaml/src/chain_yaml.rs:77-87`, `crates/adapter-gui/src/chain_editor.rs:51-65` and `crates/application/src/chain_factory.rs:110-120`, all with `Default::default()` for now — Step 4 replaces the rig projection, the chain YAML and the chain editor ones):

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo build --workspace --all-targets --keep-going -j 2 2>&1 | grep -A1 'missing field `disabled_endpoints` in initializer of `[a-z_:]*Chain`' | grep -- "-->"
```

Edit each listed site (Edit tool, one site at a time) adding `disabled_endpoints: Default::default(),` after its `loopers:` line; re-run until the grep prints nothing. Then catch, file by file, the literals the macOS compiler does not see (`cfg(all(target_os = "linux", feature = "jack"))` — e.g. `infra-cpal/src/controller_*_tests.rs`, `stream_builder_tests.rs`, `tests_regression.rs` — and Windows-only code). The check lists a file with more `Chain {` literals than `disabled_endpoints` lines; `struct`/`impl`/`for Chain {` and `-> Chain {` lines are not counted (~356 literals in ~280 files before the sweep):

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && for f in $(grep -rlE '(^|[^A-Za-z_])Chain \{' crates --include='*.rs'); do lits=$(grep -E '(^|[^A-Za-z_])Chain \{' "$f" | grep -cvE '(struct|impl|enum|trait) |for Chain \{|-> ([a-z_]+::)*Chain \{'); set=$(grep -c 'disabled_endpoints' "$f"); [ "$set" -lt "$lits" ] && echo "$f: $lits literal(s), $set disabled_endpoints line(s)"; done
```

Expected: no output (a line listed only because a doc comment spells `Chain {` is checked by eye and left alone).

- [ ] **Step 3: Run the tests — behavioral RED**

Run: `nice -n 19 cargo test -p engine --test issue_328_endpoint_disables -j 2`, then `nice -n 19 cargo test -p project --test issue_328_endpoint_disables_capture -j 2`, then `nice -n 19 cargo test -p infra-yaml --test issue_328_endpoint_disables_yaml -j 2`, then `nice -n 19 cargo test -p adapter-gui --lib editing_a_chain_keeps_its_endpoint_checklists -j 2`
Expected FAILs:
- `the_projected_chain_carries_its_inputs_unchecked_endpoints` — `the synthetic chain carries the rig input's checklist` / `left: EndpointDisables { inputs: [], … }` / `right: EndpointDisables { inputs: [EndpointRef { io: "shared", endpoint: "in 2" }], … }`
- `a_checklist_edit_on_the_projected_chain_is_captured_into_the_rig_input` — `the save path captures the checklist into project.yaml`
- `a_new_chain_saved_into_the_rig_keeps_its_checklist` — `left: EndpointDisables { … outputs: [] … }`
- `a_checklist_edit_changes_the_legacy_serialization_the_dirty_check_compares` — `the dirty fingerprint must see a checklist edit, or Save answers 'no changes'`
- `editing_a_chain_keeps_its_endpoint_checklists` — `renaming a chain must not re-check the endpoints its graph left out` / `left: EndpointDisables { inputs: [], … }`

- [ ] **Step 4: Implement the five hand-overs**

`crates/engine/src/rig_projection.rs` — in the `Chain { … }` literal replace the swept `disabled_endpoints: Default::default(),` with:

```rust
            // #328: the graph's checklists are the INPUT's configuration; the
            // chain carries them so `resolve_chain_ports` can apply them.
            disabled_endpoints: input.disabled_endpoints.clone(),
```

`crates/project/src/rig_sync.rs` — after line 74 (`rig_input.loopers = chain.loopers.clone();`) add:

```rust
            // #328: capture the graph's endpoint checklists (chain config,
            // not preset data) so they persist on the rig path.
            rig_input.disabled_endpoints = chain.disabled_endpoints.clone();
```

`crates/project/src/migrate.rs` — replace the swept `disabled_endpoints: Default::default(),` of the `RigInput` literal with `disabled_endpoints: chain.disabled_endpoints.clone(),`.

`crates/adapter-gui/src/chain_editor.rs` — in the edit-mode `Chain { … }` literal of `chain_from_draft` (lines 51-65), replace the #826 comment and the two lines under it plus the swept `disabled_endpoints: Default::default(),` with:

```rust
            // #826: the editor edits name / instrument / bindings. Everything
            // it does NOT edit comes back untouched — dropping these deleted
            // the chain's recorded loops (wavs left orphaned beside the
            // project) and its chosen DI output on every rename. #328: the
            // graph's endpoint checklists are one of those things.
            di_output: existing.di_output.clone(),
            loopers: existing.loopers.clone(),
            disabled_endpoints: existing.disabled_endpoints.clone(),
```

(the create-mode literal below it keeps `disabled_endpoints: Default::default(),` — a new chain starts with every endpoint checked).

`crates/infra-yaml/src/chain_yaml.rs` — add `use project::endpoint_disables::EndpointDisables;` after line 8; add to `ChainYaml` after the `loopers` field (after line 46, `loopers: Vec<LooperConfig>,`):

```rust
    /// #328: the graph's endpoint checklists. Written only when something is
    /// unchecked; kept here so the dirty fingerprint (`serialize_project`)
    /// sees a checklist edit.
    #[serde(default, skip_serializing_if = "EndpointDisables::is_empty")]
    disabled_endpoints: EndpointDisables,
```

in `into_chain` replace the swept `disabled_endpoints: Default::default(),` with `disabled_endpoints: self.disabled_endpoints,`; in `from_chain` add `disabled_endpoints: chain.disabled_endpoints.clone(),` after `loopers: chain.loopers.clone(),`.

- [ ] **Step 5: Run the tests — GREEN**

Run the four commands of Step 3.
Expected: all pass (`chain_editor_tests::editing_a_chain_keeps_what_the_editor_does_not_edit`, the #826 pin, stays green). Then `nice -n 19 cargo test -p engine -j 2` — `volume_invariants_tests`, golden and isolation suites green (no audio change).

- [ ] **Step 6: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status --short
```

Add every listed path explicitly (never `-A`), then:

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): project, capture and persist the checklists on the chain"
```

- [ ] **Step 7: Push gate, push, comment**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 1 T10 pushed (<hash>): Chain.disabled_endpoints (rig_to_chains copy, sync capture, migrate carry, ChainYaml, chain editor keeps it on rename) + literal sweep incl. jack-gated files. Push gate green."
```

---

### Task 11: Discovery honours the checklists

**Files:**
- Modify: `crates/project/src/binding_discovery.rs:13-16,45-80` (+ new private fn `ref_of`)
- Modify: `crates/engine/src/rig_tap_conflict.rs:1-34`
- Modify: `crates/engine/tests/issue_328_endpoint_disables.rs` (append the shared-E/S helpers and one test)
- Modify: `crates/infra-cpal/src/io_topology_tests.rs` (append one test)
- Modify: `docs/audio-config.md` (new section before `### Mid-chain ports (issue #85)`)
- Test: `crates/project/tests/issue_328_endpoint_discovery.rs`

**Interfaces:**
- Consumes: Task 4 `find_split`, `SplitEnd`; Task 8 `EndpointDisables::{is_enabled, tail_output_enabled}`; Task 10 `Chain.disabled_endpoints`, `RigInput.disabled_endpoints`.
- Produces: `resolve_chain_ports` drops head inputs unchecked on `Input`, and tail outputs unchecked on `Output` (linear/Mix) or on both `PathAOutput` and `PathBOutput` (Y); mid ports untouched. `tap_conflict` skips unchecked inputs. Every consumer of `resolve_chain_ports` (`resolve_chain_io`, `input_conflicting_chains`, `conflicting_input_channel`, `runtime_segments`, `stream_io_labels`, `endpoint_entry`, `di_output_resolve`, `chain_resolve_io_map`, `bound_io_signature`, looper selectors) follows.
- Hand-off (Part 4, engine Y → A/B; its Task 2 overlaps this task on `binding_discovery.rs` — reuse, do not re-filter): the per-output path SET of a Y chain (`SegmentPaths`) is built by the engine from `is_enabled(PathAOutput/PathBOutput, …)`; a chain that resolves NO input must build no segment (today `effective_inputs` falls back to device `""` channel 0 — `crates/engine/src/effective_endpoints.rs:112-122`).

- [ ] **Step 1: Write the failing tests**

Create `crates/project/tests/issue_328_endpoint_discovery.rs`:

```rust
//! #328 — `resolve_chain_ports` applies the graph's checklists (spec §1.3). An
//! endpoint unchecked on its node opens no port, so it opens no stream, builds
//! no segment and claims no capture tap. A Y chain has no chain output node:
//! a tail output lives while either path's output node keeps it. Mid ports
//! (#85) are not on the checklist.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::block::{AudioBlock, AudioBlockKind, OutputBlock, SplitBlock, SplitEnd};
use project::chain::Chain;
use project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef};

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
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![ep("in 1", 0), ep("in 2", 1)],
        outputs: vec![ep("out L", 0), ep("out R", 1)],
    }]
}

fn chain(blocks: Vec<AudioBlock>, unchecked: &[(EndpointNode, &str)]) -> Chain {
    let mut disabled_endpoints = EndpointDisables::default();
    for (node, endpoint) in unchecked {
        disabled_endpoints.set_enabled(
            *node,
            EndpointRef {
                io: "io".into(),
                endpoint: (*endpoint).into(),
            },
            false,
        );
    }
    Chain {
        id: ChainId("rig:g".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks,
        di_output: None,
        loopers: vec![],
        disabled_endpoints,
    }
}

fn split(end: SplitEnd) -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock::new(end)),
    }
}

fn names(chain: &Chain, direction: PortDirection) -> Vec<String> {
    resolve_chain_ports(chain, &registry())
        .into_iter()
        .filter(|p| p.direction == direction)
        .map(|p| p.endpoint.name)
        .collect()
}

#[test]
fn an_input_unchecked_on_the_input_node_opens_no_port() {
    let c = chain(vec![], &[(EndpointNode::Input, "in 2")]);
    assert_eq!(names(&c, PortDirection::Input), vec!["in 1"]);
    assert_eq!(
        names(&c, PortDirection::Output),
        vec!["out L", "out R"],
        "the output node is untouched"
    );
}

#[test]
fn an_output_unchecked_on_the_output_node_opens_no_port() {
    let c = chain(vec![], &[(EndpointNode::Output, "out R")]);
    assert_eq!(names(&c, PortDirection::Output), vec!["out L"]);
}

#[test]
fn a_mix_chain_ends_at_the_chain_output_node() {
    let c = chain(
        vec![split(SplitEnd::Mix)],
        &[(EndpointNode::PathAOutput, "out L"), (EndpointNode::Output, "out R")],
    );
    assert_eq!(
        names(&c, PortDirection::Output),
        vec!["out L"],
        "a Mix chain reads the chain output node, not the path nodes"
    );
}

#[test]
fn a_y_output_lives_while_either_path_keeps_it() {
    let c = chain(
        vec![split(SplitEnd::Y)],
        &[
            (EndpointNode::PathAOutput, "out L"),
            (EndpointNode::PathBOutput, "out L"),
            (EndpointNode::PathBOutput, "out R"),
            (EndpointNode::Output, "out R"),
        ],
    );
    assert_eq!(
        names(&c, PortDirection::Output),
        vec!["out R"],
        "out L is fed by no path; out R is still fed by path A (the chain output node does not exist on a Y chain)"
    );
}

#[test]
fn unchecking_every_input_leaves_the_chain_without_inputs() {
    let c = chain(
        vec![],
        &[(EndpointNode::Input, "in 1"), (EndpointNode::Input, "in 2")],
    );
    assert!(
        names(&c, PortDirection::Input).is_empty(),
        "no input port — never a fallback to another endpoint"
    );
}

#[test]
fn a_mid_port_is_not_on_the_checklist() {
    let aux = AudioBlock {
        id: BlockId("aux".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "io".into(),
            endpoint: "out R".into(),
        }),
    };
    let c = chain(vec![aux], &[(EndpointNode::Output, "out R")]);
    let outputs: Vec<(String, bool)> = resolve_chain_ports(&c, &registry())
        .into_iter()
        .filter(|p| p.direction == PortDirection::Output)
        .map(|p| (p.endpoint.name, p.from_block))
        .collect();
    assert_eq!(
        outputs,
        vec![("out L".to_string(), false), ("out R".to_string(), true)],
        "the tail out R is unchecked; the mid port on out R stays"
    );
}
```

Append to `crates/engine/tests/issue_328_endpoint_disables.rs` (created in Task 10):

```rust
use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};

fn ep(name: &str, channels: Vec<usize>, mode: ChannelMode) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("scarlett".into()),
        mode,
        channels,
    }
}

/// One E/S with two guitar inputs and one stereo output.
fn shared_binding() -> IoBinding {
    IoBinding {
        id: "shared".into(),
        name: "SHARED".into(),
        inputs: vec![
            ep("in 1", vec![0], ChannelMode::Mono),
            ep("in 2", vec![1], ChannelMode::Mono),
        ],
        outputs: vec![ep("out", vec![0, 1], ChannelMode::Stereo)],
    }
}

#[test]
fn complementary_unchecked_inputs_share_one_binding_without_a_tap_conflict() {
    use engine::rig_runtime::RigRuntime;
    use engine::runtime_endpoints::input_conflicting_chains;

    // g1 plays in 1 only, g2 plays in 2 only — one E/S, two guitars.
    let r = rig(unchecked_input("in 2"), unchecked_input("in 1"));
    let registry = vec![shared_binding()];

    let chains = rig_to_chains(&r);
    assert_eq!(
        input_conflicting_chains(chains.iter(), &registry),
        Vec::<domain::ids::ChainId>::new(),
        "chain side: an unchecked input claims no tap"
    );

    let rt = RigRuntime::build(r, 48_000.0, registry).expect("the rig builds");
    assert!(
        rt.is_enabled("g1") && rt.is_enabled("g2"),
        "rig side: g2 must not be refused a tap g1 does not hold (#924: the detectors agree)"
    );
}
```

Append to `crates/infra-cpal/src/io_topology_tests.rs`:

```rust
// ── #328: an endpoint unchecked on the chain graph is a re-bind ─────────────

/// Unchecking an input on a RUNNING chain must read as an I/O change, or the
/// live-edit path keeps the stream it no longer wants open.
#[test]
fn unchecking_an_input_endpoint_changes_the_bound_io_signature() {
    use domain::ids::ChainId;
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use project::chain::Chain;
    use project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef};

    let ep = |name: &str, ch: usize| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("scarlett".into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    };
    let registry = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![ep("in 1", 0), ep("in 2", 1)],
        outputs: vec![ep("out", 0)],
    }];
    let chain = |disabled_endpoints: EndpointDisables| Chain {
        id: ChainId("rig:g".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints,
    };
    let mut unchecked = EndpointDisables::default();
    unchecked.set_enabled(
        EndpointNode::Input,
        EndpointRef {
            io: "io".into(),
            endpoint: "in 2".into(),
        },
        false,
    );

    let (all_inputs, _) = super::bound_io_signature(&chain(EndpointDisables::default()), &registry);
    let (kept_inputs, _) = super::bound_io_signature(&chain(unchecked), &registry);
    assert_eq!(all_inputs.len(), 2);
    assert_eq!(
        kept_inputs,
        vec![(DeviceId("scarlett".into()), vec![0])],
        "#328: the unchecked input opens no stream, so a running chain must re-bind"
    );
}
```

- [ ] **Step 2: Run the tests — behavioral RED**

Run: `nice -n 19 cargo test -p project --test issue_328_endpoint_discovery -j 2`
Expected: 6 FAILED, e.g. `an_input_unchecked_on_the_input_node_opens_no_port` — `left: ["in 1", "in 2"]` / `right: ["in 1"]`; `a_mid_port_is_not_on_the_checklist` — `left: [("out L", false), ("out R", false), ("out R", true)]`; `a_y_output_lives_while_either_path_keeps_it` — `left: ["out L", "out R"]` / `right: ["out R"]`; `unchecking_every_input_leaves_the_chain_without_inputs` — `no input port — never a fallback to another endpoint`.
Run: `nice -n 19 cargo test -p engine --test issue_328_endpoint_disables -j 2`
Expected: `complementary_unchecked_inputs_share_one_binding_without_a_tap_conflict` FAILED — `chain side: an unchecked input claims no tap` / `left: [ChainId("rig:g2")]` / `right: []`.
Run: `nice -n 19 cargo test -p infra-cpal unchecking_an_input_endpoint -j 2`
Expected: FAILED — `left: [(DeviceId("scarlett"), [0]), (DeviceId("scarlett"), [1])]` / `right: [(DeviceId("scarlett"), [0])]`.

- [ ] **Step 3: Implement the discovery filter**

In `crates/project/src/binding_discovery.rs` replace lines 15-16 with:

```rust
use crate::block::{find_split, AudioBlockKind, SplitEnd};
use crate::chain::Chain;
use crate::endpoint_disables::EndpointNode;
```

(`EndpointRef` is not imported bare: this file already names the looper's `crate::chain::EndpointRef` in `resolve_input_segment` and friends, so the checklist ref is spelled `crate::endpoint_disables::EndpointRef` in `ref_of` below.)

replace lines 45-80 with:

```rust
/// Resolve every I/O port of `chain` against the binding `registry`.
///
/// - Head inputs / tail outputs come from the bindings the chain selects
///   (`io_binding_ids`) — these are never persisted in the chain.
/// - #328: the graph's checklists (`chain.disabled_endpoints`) leave out a
///   head input unchecked on the input node, and a tail output unchecked on
///   the chain output node — or, on a Y chain (no chain output node), on BOTH
///   path output nodes. A left-out endpoint opens no stream, builds no segment
///   and claims no tap.
/// - Mid `Input` / `Output` blocks resolve their `io`/`endpoint` reference;
///   they are not on the checklist.
///
/// Ports whose binding (or endpoint) is absent from the registry are skipped.
pub fn resolve_chain_ports(chain: &Chain, registry: &[IoBinding]) -> Vec<ChainPort> {
    let find = |id: &str| registry.iter().find(|b| b.id == id);
    let tail = chain.blocks.len();
    let disabled = &chain.disabled_endpoints;
    let y_split = find_split(&chain.blocks).is_some_and(|(_, split)| split.end == SplitEnd::Y);
    let mut ports = Vec::new();

    // Head inputs + tail outputs come from the bindings the chain selects.
    for binding_id in &chain.io_binding_ids {
        let Some(binding) = find(binding_id) else {
            continue; // selection references a binding not in the registry → skip
        };
        for ep in &binding.inputs {
            if !disabled.is_enabled(EndpointNode::Input, &ref_of(binding, ep)) {
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
            if !disabled.tail_output_enabled(y_split, &ref_of(binding, ep)) {
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
    }
```

(the mid-port loop from line 82 on stays unchanged) and add before the `resolve_input_segment` fn:

```rust
/// The checklist address of one endpoint of `binding`.
fn ref_of(binding: &IoBinding, ep: &IoEndpoint) -> crate::endpoint_disables::EndpointRef {
    crate::endpoint_disables::EndpointRef {
        io: binding.id.clone(),
        endpoint: ep.name.clone(),
    }
}
```

- [ ] **Step 4: Run — project and infra-cpal GREEN, engine rig side still RED**

Run: `nice -n 19 cargo test -p project --test issue_328_endpoint_discovery -j 2` → 6 passed.
Run: `nice -n 19 cargo test -p infra-cpal unchecking_an_input_endpoint -j 2` → passed.
Run: `nice -n 19 cargo test -p engine --test issue_328_endpoint_disables -j 2`
Expected: FAILED — `rig side: g2 must not be refused a tap g1 does not hold (#924: the detectors agree)` (the chain side now passes; `tap_conflict` still claims every input of the binding).

- [ ] **Step 5: Make the rig-side detector agree**

Replace lines 1-34 of `crates/engine/src/rig_tap_conflict.rs` with:

```rust
//! Responsibility: says when two rig inputs would fight over the same tap.

use domain::io_binding::IoBinding;
use project::endpoint_disables::{EndpointNode, EndpointRef};
use project::rig::{RigInput, RigProject};
use std::collections::BTreeSet;

/// The `(device, channel)` capture taps an input occupies. Two inputs are
/// in conflict iff their tap sets intersect — they would read the same
/// physical capture point, which two isolated runtimes must never share
/// (invariant #4).
pub(crate) fn input_taps(input: &RigInput, registry: &[IoBinding]) -> Vec<(String, usize)> {
    let mut taps = Vec::new();
    // Checklist selection: every input endpoint of every selected binding —
    // #328: except the ones the chain graph's input node leaves unchecked.
    // They open no stream, so they claim no tap; the chain-side detectors
    // skip them through `resolve_chain_ports` and all three must agree (#924).
    for binding_id in &input.io_binding_ids {
        let Some(binding) = registry.iter().find(|b| &b.id == binding_id) else {
            continue;
        };
        for ep in &binding.inputs {
            let endpoint = EndpointRef {
                io: binding.id.clone(),
                endpoint: ep.name.clone(),
            };
            if !input
                .disabled_endpoints
                .is_enabled(EndpointNode::Input, &endpoint)
            {
                continue;
            }
            for &ch in &ep.channels {
                taps.push((ep.device_id.0.clone(), ch));
            }
        }
    }
    // Single per-input binding reference (legacy-ish io/endpoint).
    if !input.io.is_empty() {
        if let Some(binding) = registry.iter().find(|b| b.id == input.io) {
            for ep in &binding.inputs {
                if input.endpoint.is_empty() || ep.name == input.endpoint {
                    for &ch in &ep.channels {
                        taps.push((ep.device_id.0.clone(), ch));
                    }
                }
            }
        }
    }
    taps
}
```

(`tap_conflict` below stays unchanged.)

- [ ] **Step 6: Run — GREEN**

Run: `nice -n 19 cargo test -p engine -j 2`
Expected: `issue_328_endpoint_disables` 2 passed; `issue_716_input_conflict`, `issue_833_input_channel_conflict`, `rig_runtime_tests` green.

- [ ] **Step 7: Document**

In `docs/audio-config.md`, immediately before the line `### Mid-chain ports (issue #85)` add:

```markdown
### Endpoint checklist (issue #328)

The input and output nodes of a chain's graph list every endpoint of the chain's own E/S bindings (`io_binding_ids`), checked by default. Unchecking one leaves that endpoint out of THAT node only: it stays listed, and nothing is removed from the E/S. It is chain configuration, not preset data — `RigInput.disabled_endpoints` in `project.yaml`, projected onto `Chain.disabled_endpoints` by `rig_to_chains` and captured back by `sync_synthetic_into_rig` (`project::endpoint_disables::EndpointDisables`: `inputs`, `outputs`, `path_a_outputs`, `path_b_outputs`, each a list of `{ io, endpoint }` — binding id plus endpoint name).

`resolve_chain_ports` applies it before anything else sees the chain's I/O, so an unchecked endpoint opens no stream, builds no segment and claims no capture tap:

- a head input is kept while the input node has it checked;
- a tail output is kept while the chain output node has it checked — or, on a Y → A/B chain (which has no chain output node), while either path's output node does;
- mid `Input`/`Output` ports (#85) are not on the checklist.

The input-conflict detectors agree on it (#924): the chain-side ones resolve through `resolve_chain_ports`, and the rig-side `tap_conflict` skips a `RigInput`'s unchecked inputs itself. Two chains can therefore share one E/S, each playing the inputs the other leaves out. A ref to an endpoint the E/S no longer offers matches nothing and is ignored; `EndpointDisables::retain_known` (fed by `endpoint_candidates`) prunes it. Unchecking every input or every output of a node leaves that node with no port.

Contract tests: `crates/project/tests/issue_328_endpoint_discovery.rs`, `crates/engine/tests/issue_328_endpoint_disables.rs`, `crates/infra-cpal/src/io_topology_tests.rs` (`unchecking_an_input_endpoint_changes_the_bound_io_signature`).
```

- [ ] **Step 8: Commit**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 add crates/project/src/binding_discovery.rs crates/engine/src/rig_tap_conflict.rs crates/project/tests/issue_328_endpoint_discovery.rs crates/engine/tests/issue_328_endpoint_disables.rs crates/infra-cpal/src/io_topology_tests.rs docs/audio-config.md
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 commit -m "feat(#328): resolve only the endpoints the chain graph keeps checked"
```

- [ ] **Step 9: Push gate, push, comment**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check && nice -n 19 cargo test --workspace -j 2 && nice -n 19 cargo build --workspace -j 2 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 push origin feature/issue-328
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 1 T11 pushed (<hash>): resolve_chain_ports + tap_conflict honour the checklists (Y tail = either path). Push gate green."
```

---

### Task 12: Part 1 verification

**Files:** none new (verification only; fix anything it finds with the Edit tool, red-first when it is behaviour).

**Interfaces:**
- Consumes: Tasks 1–11.
- Produces: a green, warning-free branch the commands, engine and GUI parts build on.

- [ ] **Step 1: Full workspace gate**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo fmt --all -- --check
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test --workspace -j 2 > target/issue-328-part-1-tests.log 2>&1; echo "cargo test exit: $?"; grep "test result:" target/issue-328-part-1-tests.log | grep -v " 0 failed; 0 ignored"
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^(warning|error)"
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
```

Expected: fmt silent; `cargo test exit: 0` and the `test result:` grep prints nothing (every suite `0 failed; 0 ignored` — `#[ignore]` is forbidden; the exit code is checked too, because a compile error prints no `test result:` line at all); the build grep prints nothing (no warning, no error); validate ends with 0 errors.

- [ ] **Step 2: No literal left behind, volume pin untouched**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && for f in $(grep -rlE 'RigInput \{' crates --include='*.rs'); do lits=$(grep -E 'RigInput \{' "$f" | grep -cvE '(struct|impl) '); set=$(grep -c 'disabled_endpoints' "$f"); [ "$set" -lt "$lits" ] && echo "$f: $lits literal(s), $set disabled_endpoints line(s)"; done
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && for f in $(grep -rlE '(^|[^A-Za-z_])Chain \{' crates --include='*.rs'); do lits=$(grep -E '(^|[^A-Za-z_])Chain \{' "$f" | grep -cvE '(struct|impl|enum|trait) |for Chain \{|-> ([a-z_]+::)*Chain \{'); set=$(grep -c 'disabled_endpoints' "$f"); [ "$set" -lt "$lits" ] && echo "$f: $lits literal(s), $set disabled_endpoints line(s)"; done
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 log --oneline --grep="#328" -- crates/engine/src/volume_invariants_tests.rs
```

Expected: all three print nothing (the `Chain` check may list a file only because a doc comment spells `Chain {` — check it by eye).

- [ ] **Step 3: Push and hand off**

```bash
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 fetch origin
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status -sb
```

If the remote moved: `git -C … pull --rebase origin feature/issue-328`, re-run Step 1, then `git -C … push origin feature/issue-328`.

```bash
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 1 (model + persistence) complete at <hash>. Hand-off: minimal Split arms in crates/application/src/validate.rs (Part 2 Task 9) and crates/engine/src/runtime_block_builders.rs (Part 3 Task 14); the checklist ref is project::endpoint_disables::EndpointRef { io, endpoint } (not the looper's { binding_id, endpoint }); resolve_chain_ports already drops unchecked endpoints, so the GUI checklist must list endpoint_candidates; save-path pruning = EndpointDisables::retain_known + endpoint_candidates (Part 2); a chain that resolves no input still falls back to device \"\" ch 0 in effective_inputs (Part 4). No user validation needed for this part (nothing audible or visible yet)."
```

---

## Hand-off to the later parts (not Part 1 work)

**Status after the completeness review (see `README.md` of this plan):** the overlaps below are resolved in the later parts — Part 2 Task 7 toggles through `EndpointDisables::set_enabled` and pins (not re-adds) the rig capture; Part 2 Task 7b calls `retain_known` + `endpoint_candidates` on the save path (`CaptureRigEdits`); Part 2 Task 10 pins this part's Task 5 end to end instead of re-editing `merge_preserved_ports`; Part 2 Task 10b makes `mirror_model_swap_into_rig` find path blocks; Part 4 Task 2 only verifies Task 11; Part 6 lists `endpoint_candidates`. `PathRef`/`PathSide` are imported as `project::block::{PathRef, PathSide}` everywhere.

- **Part 2 — commands (spec §3):** Task 9 replaces the `Split` arm of `crates/application/src/validate.rs:141` with the per-path walk; call `project::block::validate_split_layout` per chain in `validate_project` (its `ensure_split_rules` can delegate to it); `with_block`, `RemoveBlock`, `MoveBlock` lookup and `query_block_params::get_block_params` (`crates/application/src/query_block_params.rs:21-25`, top-level lookup today) go through `find_block_mut`/`walk_blocks`; `SetChainEndpointEnabled` can call `EndpointDisables::set_enabled(node, EndpointRef { io, endpoint }, enabled)` instead of re-implementing it; the save path prunes with `retain_known` + `endpoint_candidates`; `AddSplit` creates `SplitBlock::new(end)`. Already done by Part 1, so Part 2's reds that expect them missing turn green on arrival: `params_mut` reaches `Split` (Task 2), `sync_synthetic_into_rig` captures `disabled_endpoints` (Task 10), `write_back_model_swaps` recurses (Task 7). The chain id field is `chain`.
- **Part 3 — engine, Split → Mix (spec §4.1):** Task 14 replaces the pass-through `Split` arm of `crates/engine/src/runtime_block_builders.rs:296-326`; `crates/infra-cpal/src/controller_offthread_live_rebuild.rs:246-252` (`block_contains_vst3`) and `crates/engine/src/runtime_graph.rs:437` do not recurse into paths.
- **Part 4 — engine, Y → A/B (spec §4.2):** `crates/engine/src/effective_endpoints.rs:112-122` falls back to device `""` channel 0 when a chain resolves no input — with every input unchecked this must build no segment instead (spec §5.3); `crates/infra-cpal/src/io_topology.rs:108` puts a split's `enabled` flag in the stream-structure signature because `is_routing()` is true (a split toggle = stream rebuild; decide against #967's 2–3 s silence); the Y path set per output reads `EndpointDisables::is_enabled(PathAOutput | PathBOutput, …)`. **Overlap:** Part 4 Task 2 ("`resolve_chain_ports` applies the checklist", new `project::endpoint_feeds`) re-does Part 1 Task 11 on the same lines of `binding_discovery.rs`; whichever lands second must reuse the first one's filter instead of adding a second one.
- **Parts 5/6 — GUI (spec §5):** `crates/adapter-gui/src/compact_block_view.rs:238` shows a split as a routing row (label `split`) because `is_routing()` is true — the spec's compact "Split" chip; `chain_block_item.rs`, `block_editor.rs` fall to their `_` arms for a split; the checklist lists `endpoint_candidates(&chain.io_binding_ids, registry)` and checks `chain.disabled_endpoints.is_enabled(node, &r)`. **Conflict:** Part 6's `endpoint_rows` (`endpoint_checklist_items.rs`) lists rows through `resolve_chain_ports`, which after Part 1 Task 11 no longer returns an unchecked endpoint — the row the user just unchecked would vanish and could never be re-checked (spec §1 decision 9: "it stays listed"). It must list `endpoint_candidates` (binding names for the labels still come from the registry). The split/mixer editors read `split_param_specs()` groups `split` / `mixer`; the new labels ("Mode", "Level to A", …) need the nine translation files. README feature list (three languages) lands with the GUI part, when the feature is usable.
