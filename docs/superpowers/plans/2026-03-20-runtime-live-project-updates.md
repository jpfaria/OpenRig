# Runtime Live Project Updates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow project, track, and stage changes in memory to affect the running runtime without restarting the process, preserving unaffected stage state.

**Architecture:** Introduce a per-track runtime controller with mutable stage nodes keyed by stable `BlockId`. Stage edits rebuild only the affected node when necessary and preserve existing nodes whenever their identity, layout, and configuration remain valid. Track/device changes rebuild only the affected track streams instead of restarting the full project.

**Tech Stack:** Rust, CPAL, existing `engine` runtime graph, existing `project` model, existing GUI controller logic.

---

### Task 1: Stabilize runtime identities

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/domain/src/ids.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/infra-yaml/src/lib.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/lib.rs`

- [ ] Add helpers for generating stable `TrackId`/`BlockId` values.
- [ ] Stop reassigning block ids on reorder in memory.
- [ ] Keep loaded/generated ids stable during the app session.

### Task 2: Make track runtime mutable

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/engine/src/runtime.rs`
- Test: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/engine/src/runtime.rs`

- [ ] Split track runtime state from project-wide graph building.
- [ ] Store routing metadata inside `TrackRuntimeState` instead of capturing immutable `Track` in stream callbacks.
- [ ] Introduce runtime stage nodes keyed by `BlockId`.
- [ ] Reuse unchanged stage nodes when rebuilding a track chain.
- [ ] Rebuild only changed stage nodes when params/model change.
- [ ] Preserve existing nodes when stages are reordered and still layout-compatible.

### Task 3: Make streams replaceable per track

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/infra-cpal/src/lib.rs`
- Test: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/infra-cpal/src/lib.rs`

- [ ] Split stream creation into per-track handles.
- [ ] Introduce a runtime controller that owns per-track streams.
- [ ] Support add/remove/replace of a single track without stopping the whole project.

### Task 4: Wire incremental updates from the desktop adapter

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/lib.rs`

- [ ] Replace full project stop/start on stage edits with runtime track updates.
- [ ] Route stage param/model/order/add/remove actions to the live runtime controller.
- [ ] Route track audio config edits to per-track rebuilds.

### Task 5: Verify behavior

**Files:**
- Test: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/engine/src/runtime.rs`
- Test: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/infra-cpal/src/lib.rs`

- [ ] Add tests for parameter updates preserving unaffected stage instances.
- [ ] Add tests for reorder preserving stage identity.
- [ ] Add tests for replacing one track without rebuilding all tracks.
- [ ] Run `cargo test` on impacted non-UI crates.
- [ ] Run `cargo check` on impacted crates.
- [ ] Run `cargo clippy` with `-D warnings` on impacted non-UI crates.
