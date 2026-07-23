# Unify Block Editor Add/Edit Wiring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the detached block editor use ONE wiring for both adding and editing a block, so a newly added block shows parameter tabs exactly like an edited one (issue #815).

**Architecture:** Today the detached editor has two fully duplicated wirings: EDIT builds a fresh `BlockEditorWindow` via `block_editor_window_setup::create_and_wire` (tabs + tab-select + insert/overwrite save all wired), while ADD reuses a single persistent `BlockEditorWindow` created at startup and driven by a parallel, older set of wirings that never learned the #780 parameter tabs. We make `create_and_wire` handle the "new block" case (no `block_index` yet) and route ADD through it, then retire the persistent window and its duplicated wirings.

**Tech Stack:** Rust, Slint, `i-slint-backend-testing` for headless window tests.

## Global Constraints

- Zero warnings (`cargo build` clean).
- TDD red-first: no production change without a test that failed first; watch the assertion fail.
- Repo content in English (code, comments, commits, docs). Chat/reasoning pt-BR.
- Agent works ONLY under `.solvers/issue-815/`; never the main folder.
- No `#[ignore]`. Behavior gate is `cargo test --workspace --lib`.
- The save path already handles insert-vs-edit by `draft.block_index` (`Some` → `OverwriteBlock`, `None` → `InsertPrebuiltBlock`) in `persist_block_editor_draft` — do NOT duplicate that logic.
- Stream isolation / audio invariants untouched — this is GUI wiring only.

---

## File Structure

Phase 1 (fix the bug + unify onto `create_and_wire`):
- `crates/adapter-gui/src/block_editor_window_setup.rs` — `create_and_wire` gains new-block mode.
- `crates/adapter-gui/src/select_chain_block_callback.rs` — EDIT call site adapts to the new ctx shape.
- `crates/adapter-gui/src/block_choose_type_callback.rs` — ADD (detached branch) routes through `create_and_wire`.
- `crates/adapter-gui/src/issue_815_add_block_tabs_tests.rs` — new inline test module (behavioral + source-presence).
- `crates/adapter-gui/src/lib.rs` — register the test module under `#[cfg(test)]`.

Phase 2 (retire the persistent window + duplicated wirings):
- `crates/adapter-gui/src/desktop_app.rs`, `desktop_app_block_models.rs`, `helpers.rs`, and the persistent-window wiring files (`block_editor_window_wiring.rs`, `block_parameter_wiring.rs`, `block_model_search_wiring.rs`, `block_picker_wiring.rs`, `block_insert_callbacks.rs`, `block_drawer_save_delete_wiring.rs`, `block_drawer_close_wiring.rs`, `block_delete_wiring.rs`, `back_to_launcher_wiring.rs`).

---

## Task 1: `create_and_wire` accepts an optional `block_index` (mechanical, behavior-preserving)

**Files:**
- Modify: `crates/adapter-gui/src/block_editor_window_setup.rs` (ctx struct + body)
- Modify: `crates/adapter-gui/src/select_chain_block_callback.rs:346-378` (the one EDIT call site)

**Interfaces:**
- Produces: `BlockEditorWindowSetupCtx` with `block_index: Option<usize>`, `before_index: usize`, `block_id: Option<domain::ids::BlockId>` (was `block_index: usize`, `block_id: BlockId`).

- [ ] **Step 1: Change the ctx fields.** In `block_editor_window_setup.rs` change
  `pub block_index: usize,` → `pub block_index: Option<usize>,`, add `pub before_index: usize,`,
  and `pub block_id: domain::ids::BlockId,` → `pub block_id: Option<domain::ids::BlockId>,`.

- [ ] **Step 2: Adapt the body, KEEPING behavior identical for the edit case.** In `create_and_wire`:
  - `win_draft`: `block_index: block_index` (already `Option`), `before_index: before_index`.
  - Stream timer + `block_id` logging: guard with `if let Some(block_id) = &block_id { ... }` and only start the utility timer when `block_index.is_some()`.
  - Leave `win.set_block_drawer_edit_mode(true);` UNCHANGED for now (Task 2 makes it conditional). This keeps the refactor behavior-preserving.
  - The `block_editor_window_lifecycle::wire` / `block_editor_window_params::wire` ctxs take `block_index`/`before_index` — pass `block_index.unwrap_or(before_index)` where a bare `usize` is still required (lifecycle `chain_index`/`block_index` fields), preserving today's values for the edit path.

- [ ] **Step 3: Update the EDIT call site** (`select_chain_block_callback.rs`): pass `block_index: Some(bi)`, `before_index: bi`, `block_id: Some(block_id_for_editor)`.

- [ ] **Step 4: Build + existing tests green** (proves behavior-preserving):

Run: `cargo test -p adapter-gui --lib`
Expected: PASS, zero warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/adapter-gui/src/block_editor_window_setup.rs crates/adapter-gui/src/select_chain_block_callback.rs
git commit -m "refactor(#815): create_and_wire takes Option<block_index> (edit path unchanged)"
```

---

## Task 2: New-block mode drives edit-mode + confirm label (RED first)

**Files:**
- Create: `crates/adapter-gui/src/issue_815_add_block_tabs_tests.rs`
- Modify: `crates/adapter-gui/src/lib.rs` (register test module)
- Modify: `crates/adapter-gui/src/block_editor_window_setup.rs`

**Interfaces:**
- Consumes: `BlockEditorWindowSetupCtx` from Task 1.

- [ ] **Step 1: Write the failing test.** New inline module. It builds a minimal ctx with an empty in-memory session and calls `create_and_wire` in new-block mode for the 8-band EQ (`filter` / `eq_eight_band_parametric`, which has 8 param groups). Assert new-block behavior.

```rust
//! #815 — a block ADDED to the chain must open the same tabbed editor as an
//! edited block. The ADD flow now goes through `create_and_wire` in
//! "new-block" mode (`block_index: None`): edit-mode off, "Adicionar" confirm
//! label, and the #780 parameter tabs populated exactly like the edit path.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Model, VecModel};

use crate::block_editor_window_setup::{create_and_wire, BlockEditorWindowSetupCtx};
use crate::project_ops::create_new_project_session;
use crate::state::{BlockEditorData, ProjectSession};

fn empty_session() -> Rc<RefCell<Option<ProjectSession>>> {
    let tmp = tempfile::TempDir::new().unwrap();
    let session = create_new_project_session(&tmp.path().join("config.yaml"));
    // keep tmp alive for the session's lifetime in-test
    std::mem::forget(tmp);
    Rc::new(RefCell::new(Some(session)))
}

fn new_block_ctx() -> BlockEditorWindowSetupCtx {
    let seeded = application::block_factory::default_params_for_model(
        "filter",
        "eq_eight_band_parametric",
    )
    .unwrap_or_default();
    BlockEditorWindowSetupCtx {
        chain_index: 0,
        block_index: None,
        before_index: 0,
        instrument: "electric_guitar".to_string(),
        effect_type: "filter".to_string(),
        model_id: "eq_eight_band_parametric".to_string(),
        enabled: true,
        editor_data: BlockEditorData {
            effect_type: "filter".to_string(),
            model_id: "eq_eight_band_parametric".to_string(),
            params: seeded,
            enabled: true,
            is_select: false,
            select_options: Vec::new(),
            selected_select_option_block_id: None,
        },
        block_id: None,
        project_session: empty_session(),
        project_chains: Rc::new(VecModel::default()),
        project_runtime: Rc::new(RefCell::new(None)),
        saved_project_snapshot: Rc::new(RefCell::new(None)),
        project_dirty: Rc::new(RefCell::new(false)),
        input_chain_devices: Rc::new(RefCell::new(Vec::new())),
        output_chain_devices: Rc::new(RefCell::new(Vec::new())),
        selected_block: Rc::new(RefCell::new(None)),
        open_block_windows: Rc::new(RefCell::new(Vec::new())),
        plugin_info_window: Rc::new(RefCell::new(None)),
        auto_save: false,
    }
}

#[test]
fn adding_a_block_opens_the_tabbed_editor_in_add_mode() {
    i_slint_backend_testing::init_no_event_loop();
    let weak = {
        let w = crate::AppWindow::new().unwrap();
        w.as_weak()
    };
    let (win, _timer) = create_and_wire(weak, new_block_ctx()).unwrap();

    // The #780 parameter tabs must be built for a NEW block, just like edit.
    assert!(
        win.get_block_parameter_groups().row_count() > 1,
        "a newly added 8-band EQ must show its parameter tabs"
    );
    // New block => add mode, not edit mode (no delete, confirm = Adicionar).
    assert!(
        !win.get_block_drawer_edit_mode(),
        "adding a block must NOT be in edit mode"
    );
}
```

- [ ] **Step 2: Register the module.** In `lib.rs`, next to the other `#[cfg(test)] mod *_tests;` lines add:

```rust
#[cfg(test)]
mod issue_815_add_block_tabs_tests;
```

- [ ] **Step 3: Run test to verify it fails behaviorally.**

Run: `cargo test -p adapter-gui --lib adding_a_block_opens_the_tabbed_editor_in_add_mode -- --nocapture`
Expected: FAIL on the `edit_mode` assertion (currently `create_and_wire` hardcodes edit-mode on). The tabs assertion already passes — that is the point: `create_and_wire` already builds tabs; only add-mode is missing.

- [ ] **Step 4: Make it pass.** In `block_editor_window_setup.rs` replace the hardcoded
  `win.set_block_drawer_edit_mode(true);` with add/edit-aware state:

```rust
let is_edit = block_index.is_some();
win.set_block_drawer_edit_mode(is_edit);
win.set_block_drawer_confirm_label(
    if is_edit { rust_i18n::t!("btn-save") } else { rust_i18n::t!("btn-add") }
        .as_ref()
        .into(),
);
```

- [ ] **Step 5: Run test to verify it passes + whole lib green.**

Run: `cargo test -p adapter-gui --lib`
Expected: PASS, zero warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/adapter-gui/src/block_editor_window_setup.rs crates/adapter-gui/src/issue_815_add_block_tabs_tests.rs crates/adapter-gui/src/lib.rs
git commit -m "feat(#815): create_and_wire builds an add-mode tabbed editor for new blocks"
```

---

## Task 3: Route the ADD (detached) flow through `create_and_wire`

**Files:**
- Modify: `crates/adapter-gui/src/block_choose_type_callback.rs` (detached branch + ctx)
- Modify: `crates/adapter-gui/src/desktop_app.rs` (pass the extra deps into the choose-type ctx)
- Modify: `crates/adapter-gui/src/issue_815_add_block_tabs_tests.rs` (add source-presence pin)

**Interfaces:**
- Consumes: `create_and_wire` new-block mode from Task 2.

- [ ] **Step 1: Write the failing source-presence test** (pins the routing; the behavioral end-to-end needs a fully wired `AppWindow`, which the repo keeps out of tests, so we pin the wiring by source — the `no_native_dialogs.rs` convention):

```rust
#[test]
fn add_flow_uses_create_and_wire_not_the_persistent_window() {
    let src = include_str!("block_choose_type_callback.rs");
    assert!(
        src.contains("create_and_wire"),
        "the ADD detached branch must build the editor via create_and_wire"
    );
    assert!(
        !src.contains("sync_block_editor_window"),
        "the ADD flow must NOT sync the old persistent window anymore"
    );
}
```

- [ ] **Step 2: Run it to verify it fails.**

Run: `cargo test -p adapter-gui --lib add_flow_uses_create_and_wire_not_the_persistent_window`
Expected: FAIL (`block_choose_type_callback.rs` still calls `sync_block_editor_window` and never `create_and_wire`).

- [ ] **Step 3: Rewire the detached branch.** In `block_choose_type_callback.rs`, replace the
  `else { ... sync_block_editor_window ... show_child_window(block_editor_window) }` block (the non-inline path, ~lines 270-278) with a `create_and_wire` call mirroring the EDIT site: build `BlockEditorData` from the seeded `new_params`' source model, `block_index: None`, `before_index: draft.before_index`, register the returned window in `open_block_windows` (sentinel `block_index: usize::MAX`) so it stays alive, and `show_child_window`. Expand `BlockChooseTypeCallbackCtx` with the deps `create_and_wire` needs (`project_runtime`, `saved_project_snapshot`, `project_dirty`, `input_chain_devices`, `output_chain_devices`, `selected_block`, `open_block_windows`, `plugin_info_window`) and thread them from `desktop_app.rs`. Remove the now-unused `sync_block_editor_window` import.

- [ ] **Step 4: Run test + whole lib green.**

Run: `cargo test -p adapter-gui --lib`
Expected: PASS, zero warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/adapter-gui/src/block_choose_type_callback.rs crates/adapter-gui/src/desktop_app.rs crates/adapter-gui/src/issue_815_add_block_tabs_tests.rs
git commit -m "feat(#815): route the ADD detached flow through create_and_wire (tabs on add)"
```

---

## Task 4: Retire the persistent block-editor window and its duplicated wirings

> Mechanical cleanup once ADD no longer shows the persistent window. Do it in
> one commit but verify after each file that the crate still builds.

**Files:**
- Modify: `crates/adapter-gui/src/desktop_app.rs` — remove the persistent `block_editor_window` creation (`desktop_app.rs:232`) and every `wire(...)` call that took it.
- Modify: `crates/adapter-gui/src/desktop_app_block_models.rs` — drop the persistent-window seeding.
- Modify: `crates/adapter-gui/src/helpers.rs` — delete `sync_block_editor_window`.
- Remove the persistent-window wiring bodies (or the `&BlockEditorWindow` parameter and its uses) in: `block_editor_window_wiring.rs`, `block_parameter_wiring.rs`, `block_model_search_wiring.rs`, `block_picker_wiring.rs`, `block_insert_callbacks.rs`, `block_drawer_save_delete_wiring.rs`, `block_drawer_close_wiring.rs`, `block_delete_wiring.rs`, `back_to_launcher_wiring.rs`.

- [ ] **Step 1: Write the failing guard test** (pins that the persistent window is gone):

```rust
#[test]
fn no_persistent_block_editor_window_remains() {
    let src = include_str!("desktop_app.rs");
    let occurrences = src.matches("BlockEditorWindow::new").count();
    assert_eq!(
        occurrences, 0,
        "the persistent BlockEditorWindow must be retired; editors are built per-block via create_and_wire"
    );
}
```

- [ ] **Step 2: Run it to verify it fails.**

Run: `cargo test -p adapter-gui --lib no_persistent_block_editor_window_remains`
Expected: FAIL (`desktop_app.rs` still creates the persistent window).

- [ ] **Step 3: Remove the persistent window + dead wirings.** Delete the creation and each dead `wire(&block_editor_window, ...)` call; where a whole wiring file only existed to drive the persistent window, delete the file and its `mod` line; where a file wired BOTH the main window and the persistent one, drop only the persistent-window parameter and its uses. Keep deleting until `cargo build -p adapter-gui` is clean with zero warnings (unused imports/params guide you).

- [ ] **Step 4: Run guard test + whole lib green.**

Run: `cargo test -p adapter-gui --lib`
Expected: PASS, zero warnings.

- [ ] **Step 5: Commit**

```bash
git add -A crates/adapter-gui/src
git commit -m "refactor(#815): retire the persistent block-editor window and its duplicated wirings"
```

---

## Task 5: Docs

**Files:**
- Modify: `docs/architecture.md` (BlockEditorPanel / detached editor section) and/or `docs/screens.md` (Block Editor) — state that the detached block editor is built per-block via `create_and_wire` for BOTH add and edit, with #780 parameter tabs in both.

- [ ] **Step 1:** Update the relevant doc paragraph to describe the single add/edit editor path.
- [ ] **Step 2: Commit**

```bash
git add docs
git commit -m "docs(#815): document the unified add/edit block editor path"
```

---

## Self-Review

- **Spec coverage:** Bug (no tabs on add) → Task 2 + Task 3. "Same slint for add/edit" → Tasks 3-4 (one wiring). "Retire persistent window" → Task 4. Docs → Task 5.
- **Type consistency:** `block_index: Option<usize>`, `before_index: usize`, `block_id: Option<BlockId>` used consistently Task 1 → 3. `create_and_wire` return `(BlockEditorWindow, Option<Rc<Timer>>)` unchanged.
- **Placeholder scan:** none — every code step shows the code or the exact deletion criterion (build-clean).
- **Risk:** Task 4 is broad; its guard is "crate builds warning-free + full lib green + the source-presence pins from Tasks 3-4". Real-hardware battery not needed (no audio-thread change).
</content>
</invoke>
