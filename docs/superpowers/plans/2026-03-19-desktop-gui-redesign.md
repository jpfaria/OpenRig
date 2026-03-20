# Desktop GUI Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the OpenRig desktop GUI into a professional multi-track guitar rig interface while preserving the current project workflow and preparing shared primitives for the future touch UI.

**Architecture:** Keep the existing screen-routing model in `adapter-gui`, but replace the current neutral card layouts with a track-centric desktop shell, denser project cards, interactive stage-chain widgets, and a stronger visual token system. Extract UI metadata and stage presentation rules into focused helpers so the Slint layer stays mostly declarative and so non-visual behavior can be covered by unit tests before visual changes land.

**Tech Stack:** Rust, Slint, existing `surrealism-ui` primitives, Cargo unit tests, manual runtime verification with `cargo run -p adapter-gui`

---

## File map

### Existing files to modify

- `docs/gui/README.md`
  Purpose: canonical redesign handoff document shared with future agents.
- `crates/adapter-gui/ui/theme.slint`
  Purpose: global color, spacing, typography, and elevation tokens for the new desktop look.
- `crates/adapter-gui/ui/models.slint`
  Purpose: UI-facing data models that feed track cards and stage chains.
- `crates/adapter-gui/ui/widgets/widgets.slint`
  Purpose: export surface for shared GUI primitives.
- `crates/adapter-gui/ui/widgets/action_button.slint`
  Purpose: current action controls that need visual overhaul and more expressive states.
- `crates/adapter-gui/ui/widgets/device_row.slint`
  Purpose: denser routing/device rows aligned with the new visual system.
- `crates/adapter-gui/ui/pages/project_launcher.slint`
  Purpose: launcher redesign.
- `crates/adapter-gui/ui/pages/project_tracks.slint`
  Purpose: core project overview and track/stage interaction redesign.
- `crates/adapter-gui/ui/pages/project_settings.slint`
  Purpose: redesigned project setup screen.
- `crates/adapter-gui/ui/pages/track_editor.slint`
  Purpose: redesigned track editor screen.
- `crates/adapter-gui/ui/desktop_main.slint`
  Purpose: desktop layout shell if new framing or composition is needed.
- `crates/adapter-gui/ui/app-window.slint`
  Purpose: page wiring and shared properties if new interaction state is introduced.
- `crates/adapter-gui/src/lib.rs`
  Purpose: UI state, model mapping, callbacks, and new desktop-only interaction state for stage editing.

### New files likely needed

- `crates/adapter-gui/src/ui_state.rs`
  Purpose: focused Rust helpers for transforming domain tracks/blocks into richer GUI models and for stage editor state.
- `crates/adapter-gui/ui/widgets/stage_chain.slint`
  Purpose: reusable stage chip, insertion affordance, quick editor shell, and chain layout primitives.
- `crates/adapter-gui/ui/widgets/track_card.slint`
  Purpose: reusable desktop track card with stage chain and track actions.
- `crates/adapter-gui/ui/widgets/surface_primitives.slint`
  Purpose: shared panels, pills, status chips, and section headers if the redesign outgrows current widgets.

## Validation strategy

There is no existing `adapter-gui` test coverage today, so phase 1 must establish the minimum safe loop:

- add Rust unit tests for extracted UI metadata/state helpers before implementation
- keep Slint-facing view code declarative and as thin as practical
- use `cargo test -p adapter-gui`
- use `cargo run -p adapter-gui` for manual runtime verification after each visual slice

## Task 1: Establish testable UI metadata and desktop interaction state

**Files:**
- Create: `crates/adapter-gui/src/ui_state.rs`
- Modify: `crates/adapter-gui/src/lib.rs`
- Modify: `crates/adapter-gui/ui/models.slint`
- Test: `crates/adapter-gui/src/ui_state.rs`

- [ ] **Step 1: Write the failing tests for stage presentation metadata**

Add unit tests covering:
- block kind to icon/stage-family mapping
- enabled/disabled presentation state
- insertion slot bookkeeping for between-stage actions
- project track summary strings or labels that the redesigned cards depend on

Run: `cargo test -p adapter-gui ui_state -- --nocapture`
Expected: FAIL because the helper module and tests do not exist yet.

- [ ] **Step 2: Create minimal `ui_state.rs` with test-only helper surface**

Define focused structs and helpers for:
- stage presentation metadata
- track card metadata
- lightweight stage quick-editor selection state

Keep the first implementation minimal and only enough to satisfy the tests.

- [ ] **Step 3: Run tests to verify GREEN**

Run: `cargo test -p adapter-gui ui_state -- --nocapture`
Expected: PASS for the new unit tests.

- [ ] **Step 4: Wire the new helper module into `lib.rs`**

Replace ad-hoc UI mapping logic in `lib.rs` with calls into the new helper layer, without changing visible behavior yet.

- [ ] **Step 5: Run the full crate test suite**

Run: `cargo test -p adapter-gui`
Expected: PASS

## Task 2: Rebuild the launcher into a product-grade desktop entry screen

**Files:**
- Modify: `crates/adapter-gui/ui/theme.slint`
- Modify: `crates/adapter-gui/ui/pages/project_launcher.slint`
- Modify: `crates/adapter-gui/ui/desktop_main.slint`
- Optional Create: `crates/adapter-gui/ui/widgets/surface_primitives.slint`
- Test: `crates/adapter-gui/src/ui_state.rs`

- [ ] **Step 1: Write the failing tests for launcher-facing metadata if new helper output is needed**

If launcher cards or recent-project status chips require new Rust formatting helpers, add the test first.

Run: `cargo test -p adapter-gui launcher -- --nocapture`
Expected: FAIL if new helper behavior is introduced.

- [ ] **Step 2: Implement the desktop token refresh**

Update:
- palette
- spacing scale
- typography hierarchy
- panel contrast/elevation

Do this in `theme.slint` first so later screens consume shared tokens instead of hard-coded values.

- [ ] **Step 3: Implement the launcher redesign**

Build:
- stronger brand/context rail
- primary CTA hierarchy
- recent projects as the main visual object
- remove the current ambiguous white bottom bar

- [ ] **Step 4: Verify compile/runtime**

Run: `cargo run -p adapter-gui`
Expected: app launches and the launcher renders without Slint compile errors.

## Task 3: Rebuild the project overview into the multi-track rig workspace

**Files:**
- Modify: `crates/adapter-gui/ui/models.slint`
- Create: `crates/adapter-gui/ui/widgets/stage_chain.slint`
- Create: `crates/adapter-gui/ui/widgets/track_card.slint`
- Modify: `crates/adapter-gui/ui/widgets/widgets.slint`
- Modify: `crates/adapter-gui/ui/pages/project_tracks.slint`
- Modify: `crates/adapter-gui/src/lib.rs`
- Test: `crates/adapter-gui/src/ui_state.rs`

- [ ] **Step 1: Write the failing tests for stage-chain state**

Cover:
- selected stage quick-editor state
- stage insertion slot identity
- per-track stage list mapping with enabled state

Run: `cargo test -p adapter-gui stage_chain -- --nocapture`
Expected: FAIL because the new state is not implemented yet.

- [ ] **Step 2: Implement minimal Rust state to support the redesigned track view**

Add only the state required for:
- active stage selection in a track card
- inline quick editor visibility
- insertion target selection

- [ ] **Step 3: Implement shared stage-chain widgets**

Build reusable Slint components for:
- stage chips with real SVG icons
- enabled/disabled state styling
- hover/focus insertion affordance between stages
- quick inline editor shell for a clicked stage

- [ ] **Step 4: Implement the track card redesign**

Build denser cards that show:
- track identity
- routing summary
- track power/state
- stage chain
- track-level actions

- [ ] **Step 5: Integrate the redesigned project screen**

Replace the old list rendering in `project_tracks.slint` with the new workspace layout.

- [ ] **Step 6: Run tests and compile verification**

Run:
- `cargo test -p adapter-gui`
- `cargo run -p adapter-gui`

Expected:
- tests PASS
- app launches and the project screen renders

## Task 4: Rebuild project settings and track editor to match the new system

**Files:**
- Modify: `crates/adapter-gui/ui/widgets/device_row.slint`
- Modify: `crates/adapter-gui/ui/widgets/action_button.slint`
- Modify: `crates/adapter-gui/ui/pages/project_settings.slint`
- Modify: `crates/adapter-gui/ui/pages/track_editor.slint`
- Modify: `crates/adapter-gui/ui/theme.slint`

- [ ] **Step 1: Write the failing tests for any new helper formatting introduced by these screens**

Only add tests if new Rust-side view helpers are introduced. Keep this small.

- [ ] **Step 2: Redesign the device rows**

Make selected rows more prominent and compact non-selected rows.

- [ ] **Step 3: Redesign project settings**

Improve:
- density
- selected/unselected hierarchy
- section separation
- footer/status treatment

- [ ] **Step 4: Redesign track editor**

Improve:
- overall density
- two-column rhythm
- clearer action hierarchy
- consistency with the project-level visual system

- [ ] **Step 5: Verify compile/runtime**

Run: `cargo run -p adapter-gui`
Expected: app launches and these screens render without Slint errors.

## Task 5: Document, verify, and hand off

**Files:**
- Modify: `docs/gui/README.md`
- Modify: any touched implementation files

- [ ] **Step 1: Update GUI handoff documentation**

Record what is now concrete in the implementation, especially:
- new shared primitives
- actual screen behavior that differs from the draft
- any tradeoffs made for desktop-first delivery

- [ ] **Step 2: Run final verification**

Run:
- `cargo test -p adapter-gui`
- `cargo run -p adapter-gui`

Expected:
- tests PASS
- app launches successfully

- [ ] **Step 3: Inspect git diff for coherence**

Run: `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig diff -- crates/adapter-gui docs/gui`

Expected:
- only intended GUI/doc changes
- no accidental unrelated edits
