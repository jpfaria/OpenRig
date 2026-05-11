# Block Drawer Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replaced the inline block editor with a floating right-side drawer that supports adding and editing blocks, including type/model selection and parameter editing.

**Architecture:** Keep the main chains screen visible and mount a single overlaid drawer on the right. Reuse the existing block insertion state, extend it into a unified block-editor state for add/edit, and drive parameter controls from the block schema so file/path parameters can use native file picking.

**Tech Stack:** Slint, Rust, existing OpenRig project/block schema system

---

### Task 1: Add block-editor state and pure helpers

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/ui_state.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/lib.rs`
- Test: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/ui_state.rs`

- [ ] Add pure helper types/functions for the drawer mode and step labels.
- [ ] Write failing tests for add/edit mode labeling and picker transitions.
- [ ] Implement minimal helper code until tests pass.

### Task 2: Add Slint models for the drawer

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/models.slint`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/app-window.slint`

- [ ] Add UI structs/properties for block editor mode, selected type/model, and parameter rows.
- [ ] Thread these properties through the existing app window.

### Task 3: Replace inline block editor with floating drawer

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/ui/pages/project_chains.slint`

- [ ] Remove the inline “Block selecionado” box.
- [ ] Add right-side floating drawer with add/edit variants.
- [ ] Keep type, model, parameters, bypass, delete, cancel, and OK in the same panel.

### Task 4: Wire Rust callbacks for add/edit/delete/bypass and schema-driven parameters

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/adapter-gui/src/lib.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/project/src/block.rs`

- [ ] Populate the drawer when clicking `+` or an existing block.
- [ ] Update models when type/model changes.
- [ ] Save parameters back into the block.
- [ ] Support native file picking for file/path parameters.

### Task 5: Update GUI handoff doc and verify

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/docs/gui/README.md`

- [ ] Record the new drawer-based block editing behavior.
- [ ] Run `cargo test -p adapter-gui`.
