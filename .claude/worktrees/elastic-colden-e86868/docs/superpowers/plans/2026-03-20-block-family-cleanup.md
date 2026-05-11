# Block Family Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the remaining `block` and `track` vocabulary from the codebase, rename the workspace crates to the `block-*` family, rename support crates to `asset-runtime` and `audio-ir`, and replace model dispatch branch soup with registry-based lookups.

**Architecture:** The refactor has two parts. First, rename the workspace and public types so the codebase consistently uses `chain`/`block` language and no longer exposes `block-*` crates or `BlockProcessor`. Second, change multi-model block families to resolve models through explicit registries instead of centralized `if/else` dispatch in `lib.rs`, reducing coupling and making new model additions local to registries/modules.

**Tech Stack:** Rust workspace crates, Cargo workspace metadata, Slint GUI, YAML project/preset files, registry-based model dispatch.

---

### Task 1: Rename workspace crates and support crates

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/Cargo.toml`
- Modify: all `Cargo.toml` files under renamed crates and all dependent crates
- Rename directories under `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/`
- Remove: empty legacy directories `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/state`, `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/preset`, `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/ports`

- [ ] Rename `crates/block-*` directories to `crates/block-*`
- [ ] Rename `crates/block-core` to `crates/block-core`
- [ ] Rename `crates/asset-runtime` to `crates/asset-runtime`
- [ ] Rename `crates/ir` to `crates/audio-ir`
- [ ] Update workspace members and every path dependency
- [ ] Update package names in the renamed crates
- [ ] Remove empty dead directories not referenced by the workspace
- [ ] Run: `cargo check -p project -p engine -p adapter-console -p adapter-gui`

### Task 2: Rename shared public block types

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-core/src/lib.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-core/src/param.rs`
- Modify: all Rust sources importing `block_core` or `BlockProcessor`

- [ ] Rename `BlockProcessor` to `BlockProcessor`
- [ ] Rename `block_core` imports to `block_core`
- [ ] Replace remaining public `block_*`/`Block*` references in code and tests
- [ ] Run: `rg -n "block-|BlockProcessor|block_core|Block\\b|blocks\\b|chains\\b|Chain\\b" /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates -g '!**/target/**'`
- [ ] Run: `cargo check -p project -p engine -p adapter-console -p adapter-gui`

### Task 3: Replace centralized model dispatch with registries

**Files:**
- Modify: family crates currently multiplexing multiple models:
  - `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-amp-head/src/lib.rs`
  - `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-amp-combo/src/lib.rs`
  - `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-cab/src/lib.rs`
  - `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-delay/src/lib.rs`
- Create when needed:
  - `registry.rs` files in those crates

- [ ] Introduce registry-based model lookup per multi-model family
- [ ] Move model listing knowledge out of `lib.rs` into dedicated registries
- [ ] Make `lib.rs` use registry lookup instead of chained `if/else`
- [ ] Keep public schema/build/validate/asset-summary behavior intact
- [ ] Run: `cargo test -p block-amp-head -p block-amp-combo -p block-cab -p block-delay`

### Task 4: Align project, engine, YAML, console, and GUI with renamed crates

**Files:**
- Modify:
  - `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/project/src/block.rs`
  - `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/project/src/catalog.rs`
  - `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/engine/src/runtime.rs`
  - `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/infra-yaml/src/lib.rs`
  - `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/application/src/validate.rs`
  - `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-console/src/main.rs`
  - `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/lib.rs`
  - `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/ui_state.rs`

- [ ] Update imports and type names to `block-*`, `block_core`, `asset_runtime`, and `audio_ir`
- [ ] Remove remaining internal `block`/`track` leftovers from tests and helper names that are part of public/workspace semantics
- [ ] Ensure YAML loading/saving still targets `chains` and `blocks`
- [ ] Run: `cargo check -p project -p domain -p infra-yaml -p application -p engine -p infra-cpal -p adapter-console -p adapter-gui`

### Task 5: Final verification and commit

**Files:**
- Review blockd diff only

- [ ] Run: `cargo test -p project -p domain -p infra-yaml -p application -p engine -p infra-cpal -p adapter-console -p adapter-gui`
- [ ] Run: `cargo clippy -p project -p domain -p infra-yaml -p application -p engine -p infra-cpal -p adapter-console -p adapter-gui --all-targets -- -D warnings`
- [ ] Run: `rg -n "block-|BlockProcessor|block_core|Block\\b|blocks\\b|chains\\b|Chain\\b" /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig /Users/joao.faria/.openrig -g '!**/target/**'`
- [ ] Review: `git status --short`
- [ ] Commit with a message focused on workspace cleanup and block-family rename
