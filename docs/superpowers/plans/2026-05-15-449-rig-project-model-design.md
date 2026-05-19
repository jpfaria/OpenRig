# #449 — `project.openrig` model + parser — Implementation Plan

> Sub-issue de #436 (rig architecture). Escopo: modelo + parser + testes. Fora: engine, migração, UI, cenas.

**Goal:** `project.openrig` domain model (`RigProject`) + YAML parser/serializer with validation and deterministic round-trip, mapping 1:1 onto the existing `InputEntry`/`OutputEntry` model.

**Architecture:**
- Domain model + validation → `crates/project/src/rig.rs` (deps `serde`+`anyhow`, matching the crate). Reuses `InputEntry`/`OutputEntry`/`AudioBlock` — single source of truth.
- YAML file I/O → `crates/infra-yaml/src/rig_yaml.rs` (owns `serde_yaml`).
- Legacy `project::project::Project` untouched; migration is #450.

## File structure

| File | Responsibility |
|---|---|
| `crates/project/src/rig.rs` | `RigProject/RigInput/RigOutput/RigPreset` + `RigProject::validate()` |
| `crates/project/src/rig_tests.rs` | model + validation unit tests |
| `crates/project/src/lib.rs` | `pub mod rig;` |
| `crates/infra-yaml/src/rig_yaml.rs` | `parse_rig_project`, `serialize_rig_project`, `load_rig_project_file`, `save_rig_project_file` |
| `crates/infra-yaml/src/lib.rs` | re-export rig_yaml |
| `crates/infra-yaml/src/rig_yaml_tests.rs` | round-trip + file I/O tests |

## Validation rules (closed in #436)

1. every `bank` value must name a preset in `presets`;
2. each input's `active-preset` must be a key in its own `bank`;
3. each input's `active-scene` ∈ `1..=8`;
4. no preset may contain an `Input`/`Output` block (preset = processing-only);
5. per-input source channel conflicts — reuse `InputBlock::validate_channel_conflicts`;
6. every `routing` target must name an `outputs` entry.

## Tasks

- [x] Task 1 — model structs compile (`RigProject` et al.), `pub mod rig;`.
- [x] Task 2 — `RigProject::validate()` TDD (7 tests RED→GREEN).
- [ ] Task 3 — `infra-yaml` parser + deterministic round-trip TDD.
- [ ] Task 4 — docs `docs/projects/project-openrig-format.md` + `./scripts/qa.sh` green + push + issue comment.

## Notes

- Workspace `.solvers/issue-436` recreated as a clean **local git clone** (the `cp -cR` from gitflow dragged 534 untracked generated plugin `.rs` files from the user's dirty main tree that don't compile against committed develop; pristine clone = clean build).
