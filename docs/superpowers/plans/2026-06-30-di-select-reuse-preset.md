# DI source select = the preset select (one reusable component) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The DI loop source picker becomes a real SELECT that is **identical to the chain preset select** — a field showing the current value + caret that opens a dropdown with a search box, a scrolling list, and the active row highlighted — by reusing ONE shared select component that the preset select also uses.

**Architecture:** Generalize the PROVEN `preset_select.slint` (field + search + dropdown, which the user already relies on daily) into a parameterized `Select` component. Re-point the preset bank to it FIRST — if presets keep working (existing preset tests + the user), the shared component is proven. The DI then uses the SAME component, so it looks and behaves exactly like the preset and is reliable by construction (same code path). Search filtering stays in Rust per-consumer (Slint has no string `contains`), each behind a small global mirroring `PresetPicker`.

**Tech Stack:** Slint 1.16 (`.slint`), Rust (adapter-gui wiring), `i-slint-backend-testing` for headless interaction tests, `tools/slint-render` for layout parity screenshots.

## Global Constraints

- Repo content (code, comments, commits, docs) in English; chat in pt-BR.
- Zero `cargo build` warnings.
- Any new visible `@tr("…")` string → add to `adapter-gui.pot` + all 9 `.po` locales same commit (i18n LAW).
- `.slint` ≤ 500 lines; one responsibility per file.
- Work only in `.solvers/issue-749`; stage explicit paths; push after each commit.
- **Keep the DI AUDIO fixes already on this branch** (`adopt_taps_from` carrying `di_loop`, per-output-stream arming) — they are the actual "DI plays again" fix and are correct. This plan only REDOES the select UI.
- **No trial-and-error:** every task ends GREEN (preset tests as the regression guard) or with a render the user approves. Nothing ships unverified.
- Selects/dropdowns must converge on ONE component (issue #754) — this plan delivers that component and migrates preset + DI onto it.

---

### Task 1: Throw away the ad-hoc DI select; park the DI control

**Files:**
- Delete: `crates/adapter-gui/ui/components/select.slint` (the ad-hoc one I invented)
- Modify: `crates/adapter-gui/ui/components/chain_di_loop_button.slint` (revert to the fone icon + header play only, NO source dropdown yet, so the branch compiles and the audio fixes stay testable)
- Modify: `crates/adapter-gui/ui/pages/chain_row.slint` (restore the DI button x if changed)
- Remove the temp probe in `crates/adapter-gui/src/chain_row_wiring.rs`
- Test: existing `tests/issue_749_di_loop_popup_interaction.rs` keeps passing (play button)

- [ ] **Step 1:** Delete `select.slint`; rewrite `chain_di_loop_button.slint` to render only the fone icon (no popup) + the header play/stop (keep the proven interaction test for play). Remove the `[#749-probe]` eprintln from `chain_row_wiring.rs`.
- [ ] **Step 2:** `cargo build -p adapter-gui` → zero warnings/errors.
- [ ] **Step 3:** `cargo test -p adapter-gui --test issue_749_di_loop_popup_interaction` → play button test GREEN.
- [ ] **Step 4:** Commit `chore(#749): discard the ad-hoc DI select; redo via the shared component`.

### Task 2: Create the shared `Select` by extracting preset_select's exact markup

**Files:**
- Create: `crates/adapter-gui/ui/components/select.slint`
- Reference (do NOT change yet): `crates/adapter-gui/ui/components/preset_select.slint`
- Render: standalone mock → `tools/slint-render`

**Interfaces — Produces:**
```
Select {
  in [SelectOption] options;     // SelectOption { key: string, label: string }
  in string active-key;          // highlights the matching row
  in string field-label;         // current value shown in the closed field
  in string placeholder;
  in string search-placeholder;
  in-out string search;
  in length field-width: 240px;
  in length popup-width: 380px;
  in length max-popup-height: 380px;
  callback picked(string /*key*/);
  callback query-changed(string);
  callback opened();
}
```
`SelectOption` added to `models.slint`.

- [ ] **Step 1:** Add `struct SelectOption { key: string, label: string }` to `models.slint`.
- [ ] **Step 2:** Create `select.slint` by COPYING preset_select.slint's field + search-header + Flickable-list + highlight + shadow markup VERBATIM, swapping `PresetPicker.*` for the `in`/`callback` parameters above and `PresetOption.slot`→`SelectOption.key`. Row click: `root.picked(opt.key); pop.close();` plus `scroll-event(_) => { reject }`. `close-policy: close-on-click-outside`.
- [ ] **Step 3:** Standalone mock `Window` instantiating `Select` with fake options + a selected key; build `tools/slint-render`; render the CLOSED field and (via a forced-open mock of the inner Rectangle) the OPEN dropdown.
- [ ] **Step 4:** Read both PNGs; confirm pixel-parity with a preset_select render (same field, same search box, same row height/highlight/shadow). Fix any drift.
- [ ] **Step 5:** `cargo build -p adapter-gui` zero warnings. Commit `feat(#749): shared Select component (extracted from preset_select)`.

### Task 3: Re-point the preset bank to `Select` (proves the shared component)

**Files:**
- Modify: `crates/adapter-gui/ui/components/preset_select.slint` → thin wrapper over `Select`
- Modify: `crates/adapter-gui/ui/preset_picker_globals.slint` (`options: [SelectOption]`, key = slot as string)
- Modify: `crates/adapter-gui/src/chain_preset_wiring.rs` (populate `SelectOption{ key: slot.to_string(), label }`; `picked` parses key→slot)
- Test (guard): `cargo test -p adapter-gui` preset wiring tests + `i18n_tests`

- [ ] **Step 1:** Change `PresetPicker.options` to `[SelectOption]`; update `chain_preset_wiring.rs` to build `SelectOption { key: slot.to_string(), label }` and parse the picked key back to the int slot.
- [ ] **Step 2:** Rewrite `preset_select.slint` as a `Select { … }` wrapper passing `PresetPicker.options`, `active-key = active-slot.to_string()`, `field-label = active preset label`, search bound to `PresetPicker.search`, `query-changed => PresetPicker.query-changed`, `opened => PresetPicker.open(labels)`, `picked(key) => root.picked(key.to-int())`.
- [ ] **Step 3:** `cargo test -p adapter-gui` → ALL preset tests + `i18n_tests` GREEN (regression guard).
- [ ] **Step 4:** Render the chain title preset select before/after → identical. **User checkpoint: confirm presets still open/search/select in the running app.**
- [ ] **Step 5:** Commit `refactor(#749): preset bank uses the shared Select`.

### Task 4: DI search backend (mirror PresetPicker)

**Files:**
- Create: `crates/adapter-gui/ui/di_source_picker_globals.slint` (`global DiSourcePicker` — `options: [SelectOption]`, `search`, `open([string])`, `query-changed(string)`)
- Modify: `crates/adapter-gui/src/chain_row_wiring.rs` (wire `DiSourcePicker.on_open` / `on_query_changed` → filter `di_loop_sources` with the SAME `filter_*` helper style as preset → publish `options`)
- Test: a unit test for the DI filter (substring, case-insensitive), like `filter_preset_names`

- [ ] **Step 1:** Write a failing unit test `di_source_filter_matches_substring_case_insensitive`.
- [ ] **Step 2:** Run it → FAIL.
- [ ] **Step 3:** Add the `DiSourcePicker` global + the filter fn + wire `on_open`/`on_query_changed`.
- [ ] **Step 4:** Run it → PASS; `cargo build` zero warnings.
- [ ] **Step 5:** Commit `feat(#749): DI source search backend (DiSourcePicker)`.

### Task 5: DI control uses `Select` (field + play), placed in the header

**Files:**
- Modify: `crates/adapter-gui/ui/components/chain_di_loop_button.slint` (a `Select` field showing the selected source + the header play/stop)
- Modify: `crates/adapter-gui/ui/pages/chain_row.slint` (placement + width for the DI select field)
- Render: header-context mock via `tools/slint-render`

- [ ] **Step 1:** `chain_di_loop_button.slint` = play/stop button (left, when a source is selected) + `Select { field-label = selected source label; placeholder; options = DiSourcePicker.options; active-key; search bound to DiSourcePicker.search; opened => DiSourcePicker.open(sources); query-changed => DiSourcePicker.query-changed; picked(key) => choose-file or di-loop-source-selected }`.
- [ ] **Step 2:** Place the DI select field in the chain header with a concrete x + width; render a header-context mock (title field + DI select + icon cluster) and confirm no overlap, fits min/max window width.
- [ ] **Step 3:** **User checkpoint: show the render; confirm it reads as a select identical to the preset (field + caret), correctly placed.** Adjust until approved.
- [ ] **Step 4:** If a placeholder string is added, update `.pot` + all 9 `.po`. `cargo build` zero warnings.
- [ ] **Step 5:** Commit `feat(#749): DI source select uses the shared Select (identical to preset)`.

### Task 6: Verify + finalize

**Files:**
- Modify: `crates/adapter-gui/tests/issue_749_di_loop_popup_interaction.rs` (assert the DI `Select` field trigger exists + the header play fires — the testable parts; the popup row click rides on the proven preset path)
- Docs: `docs/screens.md` (DI control description) same commit if behavior described

- [ ] **Step 1:** Extend the interaction test: the DI `Select` field is present, the header play/stop fires (already proven), the field opens (`opened` callback) on click.
- [ ] **Step 2:** `cargo test -p adapter-gui` (DI + preset + i18n) GREEN; `cargo test -p engine -p infra-cpal --lib` GREEN (audio fixes intact).
- [ ] **Step 3:** Push. **User validates in the running app: open DI → field+caret like preset → search → pick → field shows it → play.**
- [ ] **Step 4:** Remove the temp probe if still present; commit `test(#749): DI select interaction + docs`.

---

## Self-Review

- **Spec coverage:** identical-to-preset (Tasks 2,3,5 — shared component + parity renders + user checkpoints); one reusable component (#754, Tasks 2–5); throw away the ad-hoc select (Task 1); keep audio fixes (Global Constraints + Task 6 Step 2); no trial-and-error (preset tests guard Task 3, render checkpoints Tasks 2/5, interaction test Task 6).
- **Open decision for the user (Task 5):** exact placement/width of the DI select field in the cramped header — resolved at the Task 5 render checkpoint, not guessed.
- **Risk:** Task 3 re-points the working preset; the existing preset tests + the user checkpoint are the guard. If anything regresses, stop at Task 3 — do not proceed.
