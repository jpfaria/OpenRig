# Chain/Block Rename Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename the project domain from `track/stage` to `chain/block` everywhere, with no compatibility aliases, including runtime, YAML, console, GUI, and local project files under `~/.openrig/`.

**Architecture:** Keep runtime behavior unchanged while renaming the public contract and internal types in one pass. Rename the domain model first, then propagate it into YAML repositories, runtime/controller code, and GUI state/models/callbacks so the whole workspace speaks one vocabulary.

**Tech Stack:** Rust, Serde YAML, Slint, Cargo check/test/clippy

---

### Task 1: Rename project-domain types and IDs

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/domain/src/ids.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/project/src/lib.rs`
- Create: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/project/src/chain.rs`
- Delete: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/project/src/track.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/project/src/project.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/project/src/block.rs`

- [ ] Rename `TrackId` to `ChainId` and change generated prefixes from `track:` to `chain:`.
- [ ] Rename `generate_for_track` to `generate_for_chain`.
- [ ] Rename `Track` to `Chain`, `TrackOutputMixdown` to `ChainOutputMixdown`, and `tracks` collections to `chains`.
- [ ] Remove any `serde` aliases for `stages`.
- [ ] Run: `cargo check -p project -p domain`

### Task 2: Rename YAML contract and repository mapping

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/infra-yaml/src/lib.rs`

- [ ] Rename YAML structs and field mappings from `tracks` to `chains`.
- [ ] Rename loader/saver helpers from `TrackYaml` to `ChainYaml`.
- [ ] Rename `stages` to `blocks` in project and preset YAML mappings with no alias fallback.
- [ ] Update unit tests and fixture strings to the new names.
- [ ] Run: `cargo test -p infra-yaml`

### Task 3: Propagate rename through validation, runtime, and console

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/application/src/validate.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/engine/src/runtime.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-console/src/main.rs`

- [ ] Rename imports, types, helper names, and error messages from track/stage to chain/block.
- [ ] Rename runtime state handles and update helpers to `chain`.
- [ ] Keep behavior intact while switching all identifiers/messages.
- [ ] Run: `cargo check -p application -p engine -p adapter-console`

### Task 4: Rename GUI state, models, callbacks, and page copy

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/lib.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/ui_state.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/models.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/pages/project_tracks.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/pages/track_editor.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/app-window.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/desktop_main.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/touch_main.slint`

- [ ] Rename UI models from `Track*`/`Stage*` to `Chain*`/`Block*`.
- [ ] Rename callbacks, state, and event handlers to the new vocabulary.
- [ ] Update visible text strings from `track`/`stage` to `chain`/`block`.
- [ ] Preserve current UI behavior while renaming the contract.
- [ ] Run: `cargo check -p adapter-gui`

### Task 5: Rewrite project YAMLs and user YAMLs

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/project.yaml`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/presets/example.yaml`
- Modify: `/Users/joao.faria/.openrig/project.yaml`
- Modify: `/Users/joao.faria/.openrig/project-test.yaml`
- Modify any preset YAML in `/Users/joao.faria/.openrig/presets/*.yaml` that still uses `stages`

- [ ] Rename `tracks` to `chains`.
- [ ] Rename `stages` to `blocks`.
- [ ] Keep semantic content unchanged.
- [ ] Verify by loading through repository tests or console build.

### Task 6: Full verification

**Files:**
- No code changes expected

- [ ] Run: `cargo check`
- [ ] Run: `cargo test -p project -p infra-yaml -p application -p engine -p adapter-console`
- [ ] Run: `cargo clippy -p project -p infra-yaml -p application -p engine -p adapter-console --all-targets -- -D warnings`
- [ ] Run: `cargo check -p adapter-gui`
