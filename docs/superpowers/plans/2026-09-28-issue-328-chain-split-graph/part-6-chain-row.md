# #328 Part 6: Chain row integration, editors, checklist, docs — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every desktop chain row draws its chain as a graph (input → shared blocks → split → lane A / lane B → mixer or one output per lane → output); every gesture on a card or node dispatches the Part 2 `Command` it means; splits are added from the block picker; the split, mixer and endpoint-checklist editors are root-level overlays; touch and compact views show one Split chip.

**Architecture:** Pure Rust layers turn a `Chain` into node ids, endpoint rows and Part 5 stages, laid out and anchored by Part 5's own `linear_chain_layout` / `insert_anchors` (`chain_block_lists.rs`, `chain_graph_ids.rs`, `endpoint_checklist_items.rs`, `chain_graph_adapter.rs`), and turn Part 5's anchor ids (`stage:{i}` / `lane:{stage}:{lane}:{i}`) back into "position + split path" (`graph_anchor.rs`). A publisher (`chain_graph_models.rs`) turns them into Slint models stored on `ProjectChainItem` once per row rebuild — the 15 Hz meter tick clones the row and never rebuilds them. `ChainRow` hosts Part 5's `GraphView` through a new `ChainRowGraph` page. Gestures travel through two Slint globals (`ChainGraphBridge`, `ChainGraphOverlayState`) instead of being drilled through `app-window.slint`, and land in small Rust handlers that dispatch `BlockCommand` / `SplitCommand` / `ChainCommand` variants and resync the chain runtime (#614); the drag's live answers (`resolve-drop-anchor`, `node_dragged`) come from `chain_graph_drop.rs` / `chain_graph_drag.rs`.

**Tech Stack:** Rust 2021; Slint 1.16.1 with `i-slint-backend-testing` 1.16 (headless interaction tests); `tools/slint-render` (headless PNG); rust-i18n YAML (`crates/adapter-gui/locales/*.yml`) and gettext `.po` (`crates/adapter-gui/translations/*/LC_MESSAGES/adapter-gui.po`).

**Spec:** `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328/docs/superpowers/specs/2026-09-28-issue-328-chain-split-graph-design.md` — sections covered here: §5.1 (graph content: I/O nodes, split/mixer nodes, card parity, "+" affordances, drag, picker entries), §5.2 last two bullets (id-based addressing replacing `real_index`, the `ui_index_to_real_block_index` / `real_block_index_to_ui` mismatch, `chain_graph_adapter.rs`, meter-poll model reuse), §5.3 (endpoint checklist), §5.4 (where it lives: ChainRow, row height by lane count, `app-window.slint` split, compact/touch Split chip), §7 Slint bullet (interaction tests, three PNGs) and the "drop-target resolution" part of the GraphView-model bullet, §8 (docs: `blocks-catalog.md`, `audio-config.md`, `screens.md`, `docs/gui/graph-view.md`, README ×3, nine catalogs).

**Depends on** (names as the Part 1, 2 and 5 plans in this folder define them):
- **Part 1** (model): `AudioBlockKind::Split(SplitBlock)` (`label() == "split"`, `is_routing()` true), `project::block::{SplitBlock, SplitEnd, PathRef, PathSide}` (`SplitEnd`/`PathSide` derive `Copy`; `PathRef` derives `Clone, PartialEq, Eq, Debug`), `project::block::split_params::{…, default_split_params, split_param_specs}` (group `"split"` for the first five specs, `"mixer"` for the other seven), `project::endpoint_disables::{EndpointDisables, EndpointRef, EndpointNode}` (`EndpointNode: Copy`; `is_enabled(&self, EndpointNode, &EndpointRef) -> bool`), `project::endpoint_candidates::endpoint_candidates(&[String], &[IoBinding]) -> (Vec<EndpointRef>, Vec<EndpointRef>)`, `Chain.disabled_endpoints`. Part 1's `docs/blocks-catalog.md` "## Chain split (#328)" section (knob table) and `docs/audio-config.md` "### Endpoint checklist (issue #328)" section already exist; this part only appends the GUI sentences to them.
- **Part 2** (commands): `path: Option<PathRef>` on `BlockCommand::{AddBlock, InsertPrebuiltBlock, MoveBlock}`; a new sub-enum `Command::Split(SplitCommand)` with `SplitCommand::{AddSplit { chain, position, end }, SetSplitEnd { chain, split_id, end }, RemoveSplit { chain, split_id }}` (`crates/application/src/command/split.rs`, re-exported as `application::command::SplitCommand`); `ChainCommand::SetChainEndpointEnabled { chain, node, io, endpoint, enabled }`; recursive `ToggleBlockEnabled` / `RemoveBlock` / `MoveBlock` / `OverwriteBlock` / `SetBlockParameter*` lookup, including a `Split`'s own params.
- **Part 5** (GraphView): Rust `graph_view_model::{linear_chain_layout, insert_anchors, resolve_drop_anchor, AnchorSlot, GraphAnchor, NodeKind, BlockBlueprint, ChainStage, ParallelEnd, GraphNode, GraphEdge, GridMetrics, NodeCategory, default_palette}` (`BlockBlueprint::with_kind`, `GraphNode.kind: NodeKind`, `NodeKind::as_str`); the Slint types file `ui/components/graph_view_types.slint` (`GraphNode { id, label, category, fill, border, layout_x, layout_y, bypass, selected, kind, neighbor, block: ChainBlockItem }`, `GraphEdgeGeometry`, `GraphAnchor { id, layout_x, layout_y, always_visible }`), re-exported by `graph_view.slint`; `GraphView` properties `nodes`, `edges`, `anchors`, `zoom`, `node_width`/`node_height` (default 100 px), `show_grid`, `background_color`, `markers_visible`; callbacks `node_clicked(id)`, `node_dragged(id, x, y)` (the host writes the position back into its model so the card follows the pointer), `node_drag_ended`, `bypass-toggled(id)`, `remove-requested(id)`, `add-requested(anchor id)`, `node-dropped(node id, anchor id)`, `pure callback resolve-drop-anchor(node id, x, y) -> anchor id` (the host answers with `resolve_drop_anchor`; `""` = none). Anchor ids are `AnchorSlot::anchor_id()`: `"stage:{i}"` (before top-level stage `i`) and `"lane:{stage}:{lane}:{i}"` (before blueprint `i` of lane `lane` of the parallel stage `stage`). The block card already keeps `BlockChip` parity (type label, icon, LED, unavailable tint, MIDI markers) from `node.block`, and the canvas draws the hover tooltip. Routing icons `ui/assets/graph-split.svg`, `ui/assets/graph-mix.svg` (text-free, colorizable).
- Parts 3 and 4 (engine) are not called by this part's code; what the owner hears is covered by the final checklist.

**Contract notes (read before Task 1):**
- The existing command variants name the chain field `chain`, not `chain_id` (`crates/application/src/command/block.rs:17-127`), and Part 2 follows that: this part sends `Command::Split(SplitCommand::AddSplit { chain, position, end })`, `Command::Split(SplitCommand::RemoveSplit { chain, split_id })`, `Command::Chain(ChainCommand::SetChainEndpointEnabled { chain, node, io, endpoint, enabled })` — only the chain field differs from the contract, `split_id` stays as the contract names it.
- `MoveBlock`'s position field is `new_position` (`block.rs:112-116`); this part sends `MoveBlock { chain, block, new_position, path }` (`path` = destination list).
- `project::chain::EndpointRef { binding_id, endpoint }` already exists (`crates/project/src/endpoint_ref.rs:21-24`, used by the looper). The contract's `EndpointRef { io, endpoint }` lives in `project::endpoint_disables`; this part always spells it `project::endpoint_disables::EndpointRef`.
- The checklist rows come from Part 1's `endpoint_candidates`, never from `resolve_chain_ports`: after Part 1 Task 11 `resolve_chain_ports` drops unchecked endpoints, so an unchecked row would vanish from its own checklist (spec owner decision 9: "it stays listed").
- Slint import cycle: Part 5's `graph_view_types.slint` imports `ChainBlockItem` from `models.slint`, so `models.slint` cannot import `GraphNode` back. Task 7 first moves `ChainBlockItem` and `BlockParamSummaryEntry` (behaviour-preserving) into `ui/chain_block_item.slint`, re-exported by `models.slint`, and points Part 5's one import line at the new file.
- The endpoint checklist reuses `ChannelPicker` (`ui/components/channel_picker.slint`, #880 "the ONE channel picker"), not `Select` (`ui/components/select.slint`). `Select` is single-choice and opens a `PopupWindow` (`select.slint:98`), whose content does not receive clicks (#749/#761). The checklist needs inline multi-check rows, which is what `ChannelPicker` is.
- `SetSplitEnd` has no GUI entry in this part: spec §5.1 names no control for switching Mix ↔ Y. It stays reachable over MCP (Part 2).
- Selecting a card inside a split path opens its editor but does not move the dispatcher's MIDI block cursor: `SelectChainBlock` is index-based over the top level and the contract adds no id-based selection command (MCP count fixed at 103).

## Global Constraints

- Work only inside `S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328` (a real `git clone`, `.git` is a directory). Never run anything in the main folder. Shell state does not persist between tool calls: start every command with `S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328;`.
- Cargo always runs as `nice -n 19 cargo … -j 2`, one cargo process at a time.
- **One file, one responsibility.** Caps: `.rs` 600 lines, `.slint` 500 lines. Test files are the only size exception. Every production file starts with `//! Responsibility: <one sentence>` (`.rs`) or `// Responsibility: <one sentence>` (`.slint`), and the sentence has no "and", "e", ",", ";", "+", "&", "/", "also", "plus" (`scripts/validate.sh:94-108`).
- Zero allocation, lock, syscall or I/O on the audio thread. This part adds no audio-thread code.
- Stream isolation: N streams = N isolated pipelines; nothing in our code mixes, sums or selects runtimes of different streams. This part only dispatches commands; it never touches runtimes.
- `crates/engine/src/volume_invariants_tests.rs` is never edited. If it fails, the source is wrong.
- TDD red-first: every production change is preceded by a test that was run and seen failing with an assertion (paste the `FAILED`/`panicked` line into the step log). Where a new symbol makes the first run a compile error, the step then adds a neutral stub and runs again to show the behavioural red before implementing.
- Zero warnings in `cargo build --workspace` AND in `cargo test --workspace` (the CI quality gate lints every target). The pure modules of Tasks 3-6 have no production caller until Tasks 7-14; each is declared in `lib.rs` with `#[cfg_attr(not(test), expect(dead_code, reason = "#328 part 6: first production caller lands in a later task"))]` so every commit pushes warning-free (LEI Nº 2: commit = push). The `cfg_attr(not(test), …)` matters: in a test build the module's own tests use every item, so a bare `#[expect(dead_code)]` would be unfulfilled and warn under `cargo test`. When a later task gives every item of such a module a production caller, `cargo build` prints `this lint expectation is unfulfilled` — delete that attribute line in the same commit. Never `#[allow(dead_code)]`.
- Repo content in English (code, comments, docs, commits). Chat stays out of the repo.
- Linux/JACK-only code goes behind `cfg(all(target_os = "linux", feature = "jack"))`. This part adds none.
- Every new user-visible string goes through `@tr(...)` (Slint) or `t!(...)` (Rust) and is filled in all nine locales in the same commit (Appendix A). Glyphs are never icons: icons are SVG through `@image-url` + `colorize`.
- Every `.slint` layout step: invoke `claude-plugin:ux-ui` (this environment's `ui-ux-pro-max`), `slint:slint` and `slint-best-practices` before the first line — CLAUDE.md requires all three; if the session does not list `slint:slint`, say so in the task's issue comment instead of skipping silently — render with `tools/slint-render`, open the PNG, run the self-critique list of `openrig-code-quality` (hierarchy, semantic colour, empty/disabled/selected states, 8 px rhythm, ≥44 px targets) before calling it done. Every click, overlay and drag is proven with an `i-slint-backend-testing` interaction test before push.
- Docs change in the same commit as the behaviour they describe (spec §8).
- **Push gate** (CLAUDE.md "Antes de TODO push"), run in this order, all green, before every push:
  ```bash
  S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && cargo fmt --all -- --check
  S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test --workspace -j 2
  S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo build --workspace -j 2
  S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo build --workspace -j 2 2>&1 | grep -E "^warning" && echo "WARNINGS FOUND — fix before push"
  S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
  ```
  The first build must exit 0; the second (cached, cargo replays diagnostics) must print nothing. Then push: `git -C "$S" fetch && git -C "$S" push` (if the remote moved: `git -C "$S" pull --rebase`, re-run the gate, push). After each push: `gh issue comment 328 --body "<short hash> — <files> — push gate green"`.
- Commit messages: `feat(#328): <what>` in English, **no `Co-Authored-By` trailer**: `.claude/skills/openrig-code-quality/SKILL.md` ("Naming OpenRig") says "Commits in English, no `Co-Authored-By` trailers", and that project rule outranks the workflow's default trailer — the same decision Parts 1 and 5 took. Stage explicit paths only, never `git add -A`.

## Review Focus

1. **Index aliasing between a path and the top level.** A knob edit, a delete or an editor opened for block 0 of path A must never land on `chain.blocks[0]`. Pinned in Task 10 (`a_knob_edit_on_a_path_block_reaches_that_block_not_the_top_level_one`, `deleting_a_path_block_removes_it_from_its_path_only`, `inserting_into_path_a_puts_the_block_in_path_a`).
2. **Removing a split silently deletes path B.** Removing the split or mixer node when path B holds blocks must ask first and change nothing until confirmed. Pinned in Task 11 (`removing_a_split_with_blocks_in_path_b_asks_first`, `the_confirm_dialog_fires_the_split_removal`).
3. **A card left where the pointer let go.** A drop that is a no-op or that the dispatcher refuses must snap the card back to its slot. Pinned in Task 11 (`a_no_op_drop_snaps_the_card_back`, `a_refused_drop_snaps_the_card_back`).
4. **Selection lit on the wrong chip.** On a chain with a mid Input/Output port the highlight must land on the block the user selected, not one chip off. Pinned in Task 2 (`the_highlight_lands_on_the_chip_the_strip_draws_for_the_block`).
5. **A split offered where it cannot go.** The picker must not offer a split inside a path or on a chain that already has one, and must not offer `Y → A/B` where blocks follow (AddSplit refuses it). Pinned in Task 12 (`no_split_entry_inside_a_path`, `no_split_entry_when_the_chain_already_has_one`, `y_is_offered_only_at_the_end_of_the_chain`).

Also pinned (not in the top five): the meter tick keeps the graph models (Task 7, `a_meter_tick_keeps_the_graph_models`); an unchecked endpoint stays listed (Task 5, `an_unchecked_endpoint_stays_listed_disabled`; Task 15, `opening_the_input_node_lists_the_chains_inputs_and_a_toggle_dispatches`); unchecking the last output is allowed (Task 15, `unchecking_the_last_output_is_allowed`).

## File Structure

Line counts are at plan time (`wc -l`). "Part 5 file" = created by Part 5; its size is whatever Part 5 left.

| File | Status | Responsibility | Lines |
|---|---|---|---|
| `crates/adapter-gui/ui/app-window.slint` | modify | hosts the desktop application window | 497 (498 after Part 5's harness import at line 28) |
| `crates/adapter-gui/ui/components/root_modal_overlays.slint` | create | draws the window's root-level modal dialogs | — |
| `crates/adapter-gui/src/selection_highlight.rs` | modify | resolves what the chains screen highlights | 83 |
| `crates/adapter-gui/src/chain_endpoint_labels.rs` | modify | formats the endpoint text a chain row prints | 43 |
| `crates/adapter-gui/src/project_view.rs` | modify | builds what the chains screen shows | 19 |
| `crates/adapter-gui/src/block_picker_items.rs` | modify | builds the picker lists a block type or model is chosen from | 179 |
| `crates/adapter-gui/src/compact_chain_callbacks.rs` | modify | (unchanged responsibility) | 484 |
| `crates/adapter-gui/src/chain_block_lists.rs` | create | finds a block list of a chain by its split path | — |
| `crates/adapter-gui/src/chain_graph_ids.rs` | create | names every node of a chain graph | — |
| `crates/adapter-gui/src/graph_anchor.rs` | create | turns a graph anchor into a place in the chain | — |
| `crates/adapter-gui/src/endpoint_checklist_items.rs` | create | lists what a graph input or output node shows | — |
| `crates/adapter-gui/src/chain_graph_adapter.rs` | create | turns a chain into the graph its row draws | — |
| `crates/adapter-gui/src/chain_graph_models.rs` | create | publishes a chain graph as the row's Slint models | — |
| `crates/adapter-gui/src/project_chains_refresh.rs` | modify | rebuilds the chain rows the screen is bound to | 265 |
| `crates/adapter-gui/ui/models.slint` | modify | declares the data shapes the UI binds to (unchanged) | 467 |
| `crates/adapter-gui/ui/chain_block_item.slint` | create | declares the tile data one chain block shows | — |
| `crates/adapter-gui/ui/components/graph_view_types.slint` | modify (Part 5 file: one import line) | Part 5's | Part 5 |
| `crates/adapter-gui/ui/components/graph_view.slint` | modify (Part 5 file: two selection inputs) | renders a node canvas | Part 5 |
| `crates/adapter-gui/ui/components/graph_node_card.slint` | modify (Part 5 file: selection by id) | draws one node card of the graph view | Part 5 |
| `crates/adapter-gui/src/chain_graph_drop.rs` | create | answers which anchor a dragged graph card lands on | — |
| `crates/adapter-gui/src/chain_graph_drag.rs` | create | moves a dragged card within its row's published graph | — |
| `crates/adapter-gui/ui/components/chain_latency_badge.slint` | create | shows a chain's probed latency | — |
| `crates/adapter-gui/ui/pages/chain_row_blocks.slint` | modify | renders one chain card's pedal strip | 441 |
| `crates/adapter-gui/ui/pages/chain_row_graph.slint` | create | renders one chain card's signal graph | — |
| `crates/adapter-gui/ui/pages/chain_row.slint` | modify | composes one chain card from the pieces that draw it | 164 |
| `crates/adapter-gui/ui/chain_graph_globals.slint` | create | carries the chain graph row's gestures to Rust | — |
| `crates/adapter-gui/ui/chain_graph_overlay_globals.slint` | create | holds the chain graph overlays' state | — |
| `crates/adapter-gui/ui/components/chain_row_graph_test_harness.slint` | create | hosts chain graph rows for render checks | — |
| `crates/adapter-gui/src/desktop_app_init.rs` | modify | (unchanged) | 157 |
| `crates/adapter-gui/src/block_editor_draft.rs` | modify | holds what the block editor is editing | 69 |
| `crates/adapter-gui/src/block_window.rs` | modify | tracks one detached block editor window | 15 |
| `crates/adapter-gui/src/block_param_apply.rs` | modify | applies one edited block parameter to the project | 158 |
| `crates/adapter-gui/src/block_editor_persist.rs` | modify | persists the block editor's draft | 343 |
| `crates/adapter-gui/src/block_delete.rs` | modify | removes the block the editor is pointed at | 83 |
| `crates/adapter-gui/src/block_editor_window_delete.rs` | modify | (unchanged) | 271 |
| `crates/adapter-gui/src/block_editor_window_lifecycle.rs` | modify | (unchanged) | 458 |
| `crates/adapter-gui/src/block_parameter_extras.rs` | modify | (unchanged) | 322 |
| `crates/adapter-gui/src/block_editor_window_setup.rs` | modify | (unchanged) | 396 |
| `crates/adapter-gui/src/select_chain_block_callback.rs` | modify | handles the user tapping a block | 413 |
| `crates/adapter-gui/src/block_insert_callbacks.rs` | modify | handles the user inserting a block | 256 |
| `crates/adapter-gui/src/block_choose_type_callback.rs` | modify | (unchanged) | 422 |
| `crates/adapter-gui/src/graph_click.rs` | create | decides what a click on a graph node opens | — |
| `crates/adapter-gui/src/graph_gesture_actions.rs` | create | turns a chain graph gesture into the command it asks for | — |
| `crates/adapter-gui/src/chain_graph_wiring.rs` | create | connects the chain graph's gestures to their handlers | — |
| `crates/adapter-gui/src/split_picker_entries.rs` | create | offers the split entries of the add-block picker | — |
| `crates/adapter-gui/src/split_insert.rs` | create | adds a split to a chain from the add-block picker | — |
| `crates/adapter-gui/src/chain_block_item.rs` | modify | projects one block into the tile the chain row shows | 128 |
| `crates/adapter-gui/ui/components/effect_type_icon.slint` | modify | maps an effect type onto its icon | 59 |
| `crates/adapter-gui/src/block_editor_param_items.rs` | modify | builds the parameter rows the block editor renders | 247 |
| `crates/adapter-gui/src/split_editor_items.rs` | create | lists the knobs a split editor shows | — |
| `crates/adapter-gui/src/split_editor_wiring.rs` | create | drives the split editor overlay | — |
| `crates/adapter-gui/ui/components/split_editor_overlay.slint` | create | renders the split editor overlay | — |
| `crates/adapter-gui/ui/components/split_editor_test_harness.slint` | create | hosts the split editor overlay for a test | — |
| `crates/adapter-gui/ui/components/channel_picker.slint` | modify | renders the channel picker | 189 |
| `crates/adapter-gui/src/endpoint_toggle.rs` | create | switches one endpoint of a graph node on or off | — |
| `crates/adapter-gui/src/endpoint_checklist_wiring.rs` | create | drives the endpoint checklist overlay | — |
| `crates/adapter-gui/ui/components/endpoint_checklist_overlay.slint` | create | renders the endpoint checklist overlay | — |
| `crates/adapter-gui/ui/components/endpoint_checklist_test_harness.slint` | create | hosts the endpoint checklist overlay for a test | — |
| `crates/adapter-gui/src/desktop_app_block_wiring.rs` | modify | dispatches the block callback wirings at startup | 307 |
| `crates/adapter-gui/src/lib.rs` | modify | (module list) | 385 |
| `crates/adapter-gui/locales/*.yml` (9) | modify | rust-i18n catalogs | — |
| `crates/adapter-gui/translations/*/LC_MESSAGES/adapter-gui.po` (9) + `adapter-gui.pot` | modify | gettext catalogs | — |
| `docs/screens.md`, `docs/gui/graph-view.md`, `docs/blocks-catalog.md`, `docs/audio-config.md`, `README.md`, `README.pt-BR.md`, `README.es-ES.md` | modify | docs | 46 / 183 / 154 / 1023 / 266 ×3 |
| `docs/gui/assets/chain-row-graph-{linear,split-mix,y}.png` | create | render evidence used by `graph-view.md` | — |

Test files (no cap): `crates/adapter-gui/tests/issue_328_app_window_headroom.rs`, `crates/adapter-gui/tests/issue_328_chain_row_graph_source.rs`, `crates/adapter-gui/src/chain_graph_fixtures_tests.rs`, `chain_block_lists_tests.rs`, `chain_graph_ids_tests.rs`, `graph_anchor_tests.rs`, `endpoint_checklist_items_tests.rs`, `chain_graph_adapter_tests.rs`, `chain_graph_models_tests.rs`, `issue_328_graph_row_interaction_tests.rs`, `chain_graph_drop_tests.rs`, `chain_graph_drag_tests.rs`, `issue_328_path_block_editor_tests.rs`, `graph_click_tests.rs`, `graph_gesture_actions_tests.rs`, `split_picker_entries_tests.rs`, `issue_328_split_picker_tests.rs`, `chain_block_item_tests.rs`, `issue_328_split_chip_tests.rs`, `split_editor_items_tests.rs`, `issue_328_split_editor_interaction_tests.rs`, `endpoint_toggle_tests.rs`, `issue_328_endpoint_checklist_interaction_tests.rs` (all under `crates/adapter-gui/src/`), plus edits in `selection_highlight_tests.rs`, `lib_tests.rs`, `meter_wiring_poll_tests.rs`, `block_param_apply_tests.rs`, `block_delete_tests.rs`, `block_editor_param_items_tests.rs`, and the fixture literals listed in Task 10.

---

### Task 0: Preflight — confirm what Parts 1, 2 and 5 delivered

No production change, no commit. Stop and report if anything below is missing: this part does not start on a guess.

- [ ] **Step 1: The solver is a real clone on the branch, up to date**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; test -d "$S/.git" && git -C "$S" branch --show-current && git -C "$S" pull --ff-only
```
Expected: `feature/issue-328`, fast-forward or "Already up to date."

- [ ] **Step 2: Part 1 names and their paths**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && grep -rn "Split(SplitBlock)" crates/project/src/block/ && grep -n "pub use path_ref::{PathRef, PathSide}\|pub use split_block::{SplitBlock, SplitEnd}" crates/project/src/block/mod.rs && grep -n "pub fn split_param_specs\|pub fn default_split_params\|pub const MIX_MASTER_SUM" crates/project/src/block/split_params.rs && grep -n "pub struct EndpointDisables\|pub enum EndpointNode\|pub struct EndpointRef\|pub fn is_enabled\|pub fn is_empty" crates/project/src/endpoint_disables.rs && grep -n "pub fn endpoint_candidates" crates/project/src/endpoint_candidates.rs && grep -n "disabled_endpoints" crates/project/src/chain.rs
```
Expected: every grep prints a line. This plan imports `project::block::{SplitBlock, SplitEnd, PathRef, PathSide}`, `project::endpoint_disables::{…}` and `project::endpoint_candidates::endpoint_candidates` exactly as Part 1 exports them.

- [ ] **Step 3: Part 2 commands**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && grep -n "pub enum SplitCommand\|AddSplit {\|SetSplitEnd {\|RemoveSplit {" crates/application/src/command/split.rs && grep -n "Split(SplitCommand)\|pub use split::SplitCommand" crates/application/src/command.rs && grep -n "SetChainEndpointEnabled {" crates/application/src/command/chain.rs && grep -n "path: Option<PathRef>" crates/application/src/command/block.rs
```
Expected: `SplitCommand` with the three variants, `Command::Split(SplitCommand)`, `ChainCommand::SetChainEndpointEnabled`, and three `path` fields (`AddBlock`, `InsertPrebuiltBlock`, `MoveBlock`). Stop if any is missing.

- [ ] **Step 4: Part 5 GraphView surface**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && grep -n "pub use" crates/adapter-gui/src/graph_view_model/mod.rs && grep -n "pub fn anchor_id\|pub fn insert_anchors" crates/adapter-gui/src/graph_view_model/anchors.rs && grep -n "pub fn resolve_drop_anchor" crates/adapter-gui/src/graph_view_model/drop_target.rs && grep -n "export struct GraphNode\|export struct GraphAnchor\|import { ChainBlockItem }" crates/adapter-gui/ui/components/graph_view_types.slint && grep -n "anchors;\|markers_visible\|add-requested\|node-dropped\|resolve-drop-anchor\|bypass-toggled\|remove-requested\|callback node_dragged" crates/adapter-gui/ui/components/graph_view.slint && grep -n "root.node.selected\|root.node.neighbor" crates/adapter-gui/ui/components/graph_node_card.slint && ls crates/adapter-gui/ui/assets/graph-split.svg crates/adapter-gui/ui/assets/graph-mix.svg
```
Expected: `mod.rs` re-exports `insert_anchors, AnchorSlot, GraphAnchor, resolve_drop_anchor, NodeKind, ParallelEnd`; the anchor id format is `"stage:{index}"` / `"lane:{stage}:{lane}:{index}"`; `graph_view_types.slint` declares `GraphNode` and `GraphAnchor` and imports `ChainBlockItem` from `"../models.slint"` (Task 7 changes that one line); the canvas has the listed members and still writes a drag through `node_dragged` (the host moves the card, Task 11); the card reads `root.node.selected` / `root.node.neighbor` (Task 9 edits those reads); both SVGs exist. Stop and report if the surface differs.

- [ ] **Step 5: Tools present**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && command -v slint-tr-extractor && command -v msgmerge && command -v python3
```
If `slint-tr-extractor` is missing, stop and ask the owner before installing it (`scripts/extract-translations.sh:31-35` would `cargo install` it into his `~/.cargo`).

---

### Task 1: Split `app-window.slint` before it grows

`app-window.slint` is at 498/500 once Part 5 has added its `GraphViewHarness` import (Part 5 Task 1: 497 → 498, one line inserted at line 28, so every line below 28 is one further down than on the base branch). The chain graph adds two globals, two overlays and three harness exports. The three confirm dialogs, the toast and the preset picker only read `OverlayBridge`; they move out verbatim, in the same paint order.

**Files:**
- Create: `crates/adapter-gui/ui/components/root_modal_overlays.slint`
- Modify: `crates/adapter-gui/ui/app-window.slint:34-38` (imports; `:33-37` before Part 5), `:379-423` (the moved block; `:378-422` before Part 5)
- Test: `crates/adapter-gui/tests/issue_328_app_window_headroom.rs`

**Interfaces:**
- Consumes: `OverlayBridge` (`ui/overlay_globals.slint:13-60`).
- Produces: `export component RootModalOverlays inherits Rectangle` — no properties; the host sizes it to the window. Task 11 adds the remove-split confirmation to it.

- [ ] **Step 1: Write the failing test**

```rust
//! #328 — app-window.slint sits at 498/500 lines; the chain graph needs two
//! globals, two root overlays and three test harnesses exported from it
//! (spec §5.4). The modal dialogs that only read `OverlayBridge` move to their
//! own component first, behaviour-preserving.

use std::path::PathBuf;

fn read(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

const MOVED: [&str; 5] = [
    "ConfirmDeleteBlockDialog {",
    "ConfirmDeleteChainDialog {",
    "ConfirmDeleteRecentProjectDialog {",
    "PresetPickerOverlay {",
    "Toast {",
];

#[test]
fn app_window_leaves_room_for_the_chain_graph_overlays() {
    // Task 1 leaves ~451 lines; Tasks 8, 11, 14 and 15 add ~11 import and
    // overlay lines, so the part ends near 462 — still 20 under the cap.
    let lines = read("ui/app-window.slint").lines().count();
    assert!(
        lines <= 480,
        "app-window.slint has {lines} lines; the chain graph needs ~20 more under the 500 cap"
    );
}

#[test]
fn the_modal_dialogs_render_from_their_own_component() {
    let app = read("ui/app-window.slint");
    assert!(app.contains("RootModalOverlays {"), "app-window must host RootModalOverlays");
    for moved in MOVED {
        assert!(!app.contains(moved), "{moved} must render from root_modal_overlays.slint");
    }
    let overlays = read("ui/components/root_modal_overlays.slint");
    for kept in MOVED {
        assert!(overlays.contains(kept), "{kept} missing from root_modal_overlays.slint");
    }
}
```

- [ ] **Step 2: Run it — expect FAIL**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_app_window_headroom
```
Expected: `app-window.slint has 498 lines; the chain graph needs ~20 more under the 500 cap` and `app-window must host RootModalOverlays`.

- [ ] **Step 3: Create `root_modal_overlays.slint`** — the body is `app-window.slint:379-423` (after Part 5) moved verbatim. Inside the new component `root` is the overlay, which the host sizes to the window, so every `root.width` / `root.height` keeps its meaning.

```slint
// Responsibility: draws the window's root-level modal dialogs.
//
// #328: moved verbatim out of app-window.slint (498/500 lines) so the chain
// graph's overlays fit. Everything here reads `OverlayBridge` only; the host
// sizes this component to the whole window, so `root` below is the window
// area exactly as it was in AppWindow. Declaration order is paint order and is
// unchanged: the three confirm dialogs, the toast, then the preset picker.

import { OverlayBridge } from "../overlay_globals.slint";
import { Toast } from "toast.slint";
import { ConfirmDeleteBlockDialog } from "confirm_delete_dialog.slint";
import { ConfirmDeleteChainDialog } from "confirm_delete_chain_dialog.slint";
import { ConfirmDeleteRecentProjectDialog } from "confirm_delete_recent_project_dialog.slint";
import { PresetPickerOverlay } from "preset_picker_overlay.slint";

export component RootModalOverlays inherits Rectangle {
    if OverlayBridge.show-confirm-delete-block : ConfirmDeleteBlockDialog {
        x: 0; y: 0;
        width: root.width; height: root.height;
        block-name: OverlayBridge.confirm-delete-block-name;
        cancel => { OverlayBridge.cancel-delete-block(); }
        confirm => { OverlayBridge.confirm-delete-block(); }
    }

    if OverlayBridge.show-confirm-delete-chain : ConfirmDeleteChainDialog {
        x: 0; y: 0;
        width: root.width; height: root.height;
        chain-name: OverlayBridge.confirm-delete-chain-name;
        cancel => { OverlayBridge.cancel-delete-chain(); }
        confirm => { OverlayBridge.confirm-delete-chain(); }
    }

    if OverlayBridge.show-confirm-delete-recent-project : ConfirmDeleteRecentProjectDialog {
        x: 0; y: 0;
        width: root.width; height: root.height;
        project-name: OverlayBridge.confirm-delete-recent-project-name;
        cancel => { OverlayBridge.cancel-delete-recent-project(); }
        confirm => { OverlayBridge.confirm-delete-recent-project(); }
    }

    Toast {
        message: OverlayBridge.toast-message;
        level: OverlayBridge.toast-level;
        container-width: root.width;
        container-height: root.height;
    }

    // Root-level modal: works in BOTH desktop and touch modes so the
    // bundled presets are always reachable in-app (issue #479 — desktop
    // previously had only a native FileDialog and no visible list).
    PresetPickerOverlay {
        width: root.width;
        height: root.height;
        visible-overlay: OverlayBridge.show-preset-picker;
        items: OverlayBridge.preset-picker-items;
        search-query <=> OverlayBridge.preset-picker-search-query;
        confirm(i) => { OverlayBridge.preset-picker-confirm(i); }
        cancel() => { OverlayBridge.preset-picker-cancel(); }
        delete(i) => { OverlayBridge.preset-picker-delete(i); }
        query-changed(t) => { OverlayBridge.preset-picker-query-changed(t); }
    }
}
```

- [ ] **Step 4: Edit `app-window.slint`.** Replace lines 34-38 (33-37 before Part 5's line-28 insert):

```slint
import { Toast } from "components/toast.slint";
import { ConfirmDeleteBlockDialog } from "components/confirm_delete_dialog.slint";
import { ConfirmDeleteChainDialog } from "components/confirm_delete_chain_dialog.slint";
import { ConfirmDeleteRecentProjectDialog } from "components/confirm_delete_recent_project_dialog.slint";
import { PresetPickerOverlay } from "components/preset_picker_overlay.slint";
```
with
```slint
import { RootModalOverlays } from "components/root_modal_overlays.slint";
```
and replace lines 379-423 (from `if OverlayBridge.show-confirm-delete-block : ConfirmDeleteBlockDialog {` through the closing `}` of `PresetPickerOverlay`; find them by that text, not by number) with
```slint
    // #328: confirm dialogs, toast and preset picker — moved so this file stays under its cap.
    RootModalOverlays { x: 0; y: 0; width: root.width; height: root.height; }
```

- [ ] **Step 5: Run the new test and the whole crate — expect PASS**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2
```
Expected: `issue_328_app_window_headroom` 2 passed; every other adapter-gui test still passes (the confirm-delete and preset flows are exercised by the existing `issue_*` interaction tests).

- [ ] **Step 6: Commit and push**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; git -C "$S" add crates/adapter-gui/ui/app-window.slint crates/adapter-gui/ui/components/root_modal_overlays.slint crates/adapter-gui/tests/issue_328_app_window_headroom.rs && git -C "$S" commit -m "feat(#328): move the root modal dialogs out of app-window.slint"
```
Run the Push gate, push, comment on #328.

---

### Task 2: One block position for the strip, the highlight and the compact view

Model A (#716): the strip draws every entry of `chain.blocks` (`project_chains_refresh.rs:168-201`), and `ui_index_to_real_block_index` is the identity (`chain_block_helpers.rs:31-45`). `real_block_index_to_ui` (`chain_endpoint_labels.rs:22-43`) still skips "the first Input and the last Output", so on a chain with mid ports the highlight and the compact-view click land one chip off. The mapping goes; the position in `chain.blocks` is the chip index.

**Files:**
- Modify: `crates/adapter-gui/src/selection_highlight.rs:15-68`, `crates/adapter-gui/src/chain_endpoint_labels.rs:22-43` (delete fn), `crates/adapter-gui/src/project_view.rs:8`, `crates/adapter-gui/src/block_picker_items.rs:3,8,128-144`, `crates/adapter-gui/src/compact_chain_callbacks.rs:32,337-360`, and the `set_selected_block` callers: `block_drawer_save_delete_wiring.rs:98`, `block_insert_callbacks.rs:141`, `block_delete_wiring.rs:101`, `chain_block_crud_wiring.rs:104,177-181,254`, `block_editor_window_delete.rs:146,254`, `select_chain_block_callback.rs:280`, `block_editor_window_lifecycle.rs:450`, `block_drawer_close_wiring.rs:70`
- Test: `crates/adapter-gui/src/selection_highlight_tests.rs`, `crates/adapter-gui/src/lib_tests.rs:280-310`

**Interfaces:**
- Consumes: nothing new.
- Produces: `pub(crate) fn set_selected_block(window: &AppWindow, selected_block: Option<&SelectedBlock>)` (the `chain` parameter is gone); `active_highlight_indices` / `active_neighbor_block_ui_index` return positions in `chain.blocks`.

- [ ] **Step 1: Write the failing test** — append to `selection_highlight_tests.rs`:

```rust
/// #328 (spec §5.2): the strip draws EVERY entry of `chain.blocks` (model A,
/// #716 — `project_chains_refresh.rs:168-201`), so a mid `Input` port sits at
/// chip 0 and the block after it at chip 1. The old mapping skipped "the first
/// Input", so selecting the block after a port lit the port's chip instead.
#[test]
fn the_highlight_lands_on_the_chip_the_strip_draws_for_the_block() {
    let mut ported = chain("rig:input-1");
    ported.blocks = vec![io_block("port-in", true), core_block("amp"), io_block("port-out", false)];
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![ported],
        midi: None,
    };
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some("amp".to_string()),
        ..Default::default()
    };
    assert_eq!(active_highlight_indices(&project, &sel), (0, 1));
}
```

- [ ] **Step 2: Run it — expect FAIL**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib the_highlight_lands_on_the_chip
```
Expected: `assertion `left == right` failed` with `left: (0, 0)` `right: (0, 1)`.

- [ ] **Step 3: Rewrite the tests that pin the old skip.** They encode the pre-#716 strip (IO stripped), which spec §5.2 names as the defect. Same fixtures, model-A answers — say so in the issue comment of this task.

In `selection_highlight_tests.rs` replace `active_chain_and_block_marks_both_with_ui_block_index`, `neighbor_is_the_next_ui_block` and `neighbor_is_minus_one_when_next_block_is_io` with:

```rust
#[test]
fn active_chain_and_block_marks_both_with_the_blocks_position() {
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some("b1".to_string()),
        ..Default::default()
    };
    // chain 0 is [in, b0, b1, out]; the strip draws all four (model A, #716),
    // so "b1" is chip 2.
    assert_eq!(active_highlight_indices(&project(), &sel), (0, 2));
}

#[test]
fn neighbor_is_the_next_chip() {
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some("b0".to_string()), // chip 1
        ..Default::default()
    };
    // The toggle-neighbor command targets the raw-next block → b1, chip 2.
    assert_eq!(active_neighbor_block_ui_index(&project(), &sel), 2);
}

#[test]
fn neighbor_of_the_last_block_is_the_port_after_it() {
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some("b1".to_string()),
        ..Default::default()
    };
    // The raw-next block is the mid `Output` port, which the strip draws as
    // chip 3 — so it is markable.
    assert_eq!(active_neighbor_block_ui_index(&project(), &sel), 3);
}
```

In `lib_tests.rs` delete lines 280-310 (the `// --- real_block_index_to_ui ---` section and its three tests: the function they test is removed in Step 4). Their three fixtures move to `selection_highlight_tests.rs`, asserted against the highlight:

```rust
fn chain_of(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    let mut c = chain(id);
    c.blocks = blocks;
    c
}

fn highlight(blocks: Vec<AudioBlock>, active: &str) -> (i32, i32) {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain_of("rig:input-1", blocks)],
        midi: None,
    };
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some(active.to_string()),
        ..Default::default()
    };
    active_highlight_indices(&project, &sel)
}

#[test]
fn every_block_highlights_at_its_own_position() {
    // Was `real_block_index_to_ui_maps_effect_blocks_correctly`.
    let blocks = || vec![io_block("in", true), core_block("comp"), core_block("pre"), core_block("dly"), io_block("out", false)];
    assert_eq!(highlight(blocks(), "comp"), (0, 1));
    assert_eq!(highlight(blocks(), "pre"), (0, 2));
    assert_eq!(highlight(blocks(), "dly"), (0, 3));
}

#[test]
fn a_port_block_is_highlightable() {
    // Was `real_block_index_to_ui_hidden_blocks_return_none`: ports are chips now.
    let blocks = || vec![io_block("in", true), core_block("dly"), io_block("out", false)];
    assert_eq!(highlight(blocks(), "in"), (0, 0));
    assert_eq!(highlight(blocks(), "out"), (0, 2));
}

#[test]
fn a_block_that_is_not_in_the_chain_marks_no_chip() {
    // Was `real_block_index_to_ui_out_of_range_returns_none`.
    assert_eq!(highlight(vec![io_block("in", true), io_block("out", false)], "gone"), (0, -1));
}
```

- [ ] **Step 4: Minimal implementation.** In `selection_highlight.rs` delete line 15 (`use crate::project_view::real_block_index_to_ui;`) and replace lines 18-41 and 43-68 with:

```rust
/// `(chain_index, block_ui_index)` to highlight, or `-1` for "none".
///
/// `block_ui_index` is the block's position in `chain.blocks`: the strip draws
/// every entry (model A, #716), so the position IS the chip index (#328).
pub(crate) fn active_highlight_indices(project: &Project, sel: &SelectionState) -> (i32, i32) {
    let Some(active_chain) = sel.active_chain.as_deref() else {
        return (-1, -1);
    };
    let Some(chain_index) = project.chains.iter().position(|c| c.id.0 == active_chain) else {
        // Stale selection (chain removed) — mark nothing rather than a wrong row.
        return (-1, -1);
    };
    let chain = &project.chains[chain_index];

    let block_ui_index = sel
        .active_block
        .as_deref()
        .and_then(|bid| chain.blocks.iter().position(|b| b.id.0 == bid))
        .map(|position| position as i32)
        .unwrap_or(-1);

    (chain_index as i32, block_ui_index)
}

/// Chip index of the block that `toggle_active_block_neighbor_enabled` would
/// flip — the block immediately AFTER the active one in `chain.blocks` (wraps),
/// mirroring the dispatcher handler exactly. `-1` when there is no active
/// block or the chain has < 2 blocks.
pub(crate) fn active_neighbor_block_ui_index(project: &Project, sel: &SelectionState) -> i32 {
    let Some(active_chain) = sel.active_chain.as_deref() else {
        return -1;
    };
    let Some(chain) = project.chains.iter().find(|c| c.id.0 == active_chain) else {
        return -1;
    };
    if chain.blocks.len() < 2 {
        return -1;
    }
    let Some(active_block) = sel.active_block.as_deref() else {
        return -1;
    };
    let Some(active_raw) = chain.blocks.iter().position(|b| b.id.0 == active_block) else {
        return -1;
    };
    ((active_raw + 1) % chain.blocks.len()) as i32
}
```

Delete `real_block_index_to_ui` (`chain_endpoint_labels.rs:22-43`) and the now-unused `use project::block::AudioBlockKind;` / `use project::chain::Chain;` lines at its top if nothing else in the file uses them. In `project_view.rs:8` change the re-export to:

```rust
pub(crate) use crate::chain_endpoint_labels::format_channel_list;
```

In `block_picker_items.rs` delete line 3 (`use crate::chain_endpoint_labels::real_block_index_to_ui;`), delete `use project::chain::Chain;` (line 8, its only user was `set_selected_block`) and replace lines 128-144 with:

```rust
/// Mark the selected block on the strip. Its position in `chain.blocks` is the
/// chip index (model A, #716 — #328 removed the Input/Output skipping).
pub(crate) fn set_selected_block(window: &AppWindow, selected_block: Option<&SelectedBlock>) {
    if let Some(selected_block) = selected_block {
        window.set_selected_chain_block_chain_index(selected_block.chain_index as i32);
        window.set_selected_chain_block_index(selected_block.block_index as i32);
    } else {
        window.set_selected_chain_block_chain_index(-1);
        window.set_selected_chain_block_index(-1);
    }
}
```

Callers: every `set_selected_block(&window, None, None)` → `set_selected_block(&window, None)` (`block_drawer_save_delete_wiring.rs:98`, `block_insert_callbacks.rs:141`, `block_delete_wiring.rs:101`, `chain_block_crud_wiring.rs:104`, `chain_block_crud_wiring.rs:254`, `block_drawer_close_wiring.rs:70`), every `set_selected_block(&main, None, None)` → `set_selected_block(&main, None)` (`block_editor_window_delete.rs:146,254`, `block_editor_window_lifecycle.rs:450`). `select_chain_block_callback.rs:280` → `set_selected_block(&window, selected_block.borrow().as_ref());`. In `chain_block_crud_wiring.rs` replace lines 177-181:

```rust
            {
                let proj = session.project.borrow();
                let chain_ref = proj.chains.get(chain_index as usize);
                set_selected_block(&window, selected_block.borrow().as_ref(), chain_ref);
            }
```
with
```rust
            set_selected_block(&window, selected_block.borrow().as_ref());
```

In `compact_chain_callbacks.rs:32` drop `real_block_index_to_ui` from the import (`use crate::project_view::block_type_picker_items;`) and replace the open-block-detail block (lines 337-360, from `// Wire open-block-detail` through its closing `}`) with:

```rust
        // Wire open-block-detail (click on model select opens full editor)
        {
            let weak_main = window.as_weak();
            compact_win.on_open_block_detail(move |ci, bi| {
                let Some(main_win) = weak_main.upgrade() else {
                    return;
                };
                // `bi` is the block's position in `chain.blocks`; the chains
                // screen's rows are exactly `chain.blocks` (model A, #716), so
                // it is the row index as is (#328: no Input/Output skipping).
                main_win.invoke_select_chain_block(ci, bi);
                let _ = main_win.window().show();
            });
        }
```

- [ ] **Step 5: Run — expect PASS**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib selection_highlight && nice -n 19 cargo build -p adapter-gui -j 2
```
Expected: all `selection_highlight` tests pass; build shows no warning (no unused import left behind).

- [ ] **Step 6: Commit and push**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; git -C "$S" add crates/adapter-gui/src/selection_highlight.rs crates/adapter-gui/src/selection_highlight_tests.rs crates/adapter-gui/src/lib_tests.rs crates/adapter-gui/src/chain_endpoint_labels.rs crates/adapter-gui/src/project_view.rs crates/adapter-gui/src/block_picker_items.rs crates/adapter-gui/src/compact_chain_callbacks.rs crates/adapter-gui/src/block_drawer_save_delete_wiring.rs crates/adapter-gui/src/block_insert_callbacks.rs crates/adapter-gui/src/block_delete_wiring.rs crates/adapter-gui/src/chain_block_crud_wiring.rs crates/adapter-gui/src/block_editor_window_delete.rs crates/adapter-gui/src/select_chain_block_callback.rs crates/adapter-gui/src/block_editor_window_lifecycle.rs crates/adapter-gui/src/block_drawer_close_wiring.rs && git -C "$S" commit -m "feat(#328): one block position for strip, highlight and compact view"
```
Run the Push gate, push, comment on #328 (state that the old highlight tests pinned the pre-#716 skip and were rewritten to the model-A answer on the same fixtures).

### Task 3: Block lists by path and graph node ids

Two pure modules every later task stands on: "which list does `(index, path)` mean" and "what does a graph node id stand for". Node ids of blocks are their `BlockId`s (spec §5.2); I/O and routing nodes get fixed ids.

**Files:**
- Create: `crates/adapter-gui/src/chain_block_lists.rs`, `crates/adapter-gui/src/chain_graph_ids.rs`, `crates/adapter-gui/src/chain_graph_fixtures_tests.rs`
- Modify: `crates/adapter-gui/src/lib.rs` (module list, next to `mod chain_editor;` at line 188)
- Test: `crates/adapter-gui/src/chain_block_lists_tests.rs`, `crates/adapter-gui/src/chain_graph_ids_tests.rs`

**Interfaces:**
- Consumes (Part 1): `project::block::{AudioBlock, AudioBlockKind, PathRef, PathSide, SplitBlock, SplitEnd}`, `project::block::split_params::default_split_params`, `project::endpoint_disables::EndpointNode`.
- Produces:
  - `chain_block_lists::{split_of(&Chain) -> Option<(usize, &BlockId, &SplitBlock)>, list_at(&Chain, Option<&PathRef>) -> Option<&[AudioBlock]>, block_at(&Chain, usize, Option<&PathRef>) -> Option<&AudioBlock>, insert_index(&Chain, usize, Option<&PathRef>) -> Option<usize>, side_index(&PathSide) -> i32, side_from_index(i32) -> Option<PathSide>}`
  - `chain_graph_ids::{INPUT_NODE_ID, OUTPUT_NODE_ID, PATH_A_OUTPUT_NODE_ID, PATH_B_OUTPUT_NODE_ID, SPLIT_NODE_ID, MIXER_NODE_ID: &str, enum NodeRef { Block { id: BlockId, path: Option<PathRef>, index: usize }, Split, Mixer, Endpoints(EndpointNode) }, resolve_node(&Chain, &str) -> Option<NodeRef>}`. The card kind string is NOT re-derived here: the laid-out node carries Part 5's `NodeKind` (Task 6 sets it) and Task 7 publishes `node.kind.as_str()`. `SPLIT_NODE_ID` / `MIXER_NODE_ID` repeat Part 5's `routing_ids::{split_node_id, merge_node_id}(1)` (that module is private to `graph_view_model`); Task 6's `a_split_to_mix_chain_runs_two_lanes_between_split_and_mixer` pins that the laid-out graph uses exactly these ids.
  - test fixtures `chain_graph_fixtures_tests::{core, port_in, split, chain, mix_chain, y_chain, endpoint, registry, session_with, recording_session, RecordingDispatcher, rows, chain_in}`

- [ ] **Step 1: Write the fixtures module** (test-only, no cap) `crates/adapter-gui/src/chain_graph_fixtures_tests.rs`:

```rust
//! #328 — fixtures shared by the chain-graph tests. Test-only module.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use application::selection_state::SelectionState;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::ChannelMode;
use infra_filesystem::{IoBinding, IoEndpoint};
use project::block::split_params::default_split_params;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, SplitBlock, SplitEnd};
use project::chain::Chain;
use project::project::Project;
use slint::VecModel;

use crate::state::ProjectSession;
use crate::ProjectChainItem;

/// A native gain/volume block — a real catalog model
/// (`crates/block-gain/src/native_volume.rs`, param `volume` 0..100).
pub(crate) fn core(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: Default::default(),
        }),
    }
}

/// A mid `Input` port reading `endpoint` of binding `io`.
pub(crate) fn port_in(id: &str, io: &str, endpoint: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: io.into(),
            endpoint: endpoint.into(),
        }),
    }
}

pub(crate) fn split(id: &str, end: SplitEnd, a: Vec<AudioBlock>, b: Vec<AudioBlock>) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end,
            params: default_split_params(),
            a,
            b,
        }),
    }
}

pub(crate) fn chain(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("chain:0".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec!["main".into(), "aux".into()],
        blocks,
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
    }
}

/// `pre → split sp (A: a1, a2 | B: b1) → post`, the lanes summed by the mixer.
pub(crate) fn mix_chain() -> Chain {
    chain(vec![
        core("pre"),
        split("sp", SplitEnd::Mix, vec![core("a1"), core("a2")], vec![core("b1")]),
        core("post"),
    ])
}

/// `pre → split sp (A: a1 | B: b1)`, each lane to its own outputs.
pub(crate) fn y_chain() -> Chain {
    chain(vec![
        core("pre"),
        split("sp", SplitEnd::Y, vec![core("a1")], vec![core("b1")]),
    ])
}

pub(crate) fn endpoint(name: &str) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels: vec![0, 1],
    }
}

/// `main` ("Scarlett"): inputs In 1, In 2 — output Out L/R.
/// `aux` ("AUX"): output Out L/R (same endpoint name as main's).
pub(crate) fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "main".into(),
            name: "Scarlett".into(),
            inputs: vec![endpoint("In 1"), endpoint("In 2")],
            outputs: vec![endpoint("Out L/R")],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: vec![],
            outputs: vec![endpoint("Out L/R")],
        },
    ]
}

fn project(chains: Vec<Chain>) -> Project {
    Project {
        name: None,
        device_settings: vec![],
        chains,
        midi: None,
    }
}

/// A session on the real `LocalDispatcher` (Part 2 applies the commands).
pub(crate) fn session_with(chains: Vec<Chain>) -> Rc<RefCell<Option<ProjectSession>>> {
    let session = ProjectSession::new(
        project(chains),
        None,
        None,
        std::env::temp_dir().join("openrig-328-graph-tests"),
    );
    *session.io_bindings.borrow_mut() = registry();
    Rc::new(RefCell::new(Some(session)))
}

/// Records every command the GUI dispatched; applies nothing.
pub(crate) struct RecordingDispatcher {
    pub(crate) seen: RefCell<Vec<Command>>,
    selection: Arc<RwLock<SelectionState>>,
}

impl CommandDispatcher for RecordingDispatcher {
    fn dispatch(&self, cmd: Command) -> anyhow::Result<Vec<Event>> {
        self.seen.borrow_mut().push(cmd);
        Ok(Vec::new())
    }
    fn selection_state(&self) -> Arc<RwLock<SelectionState>> {
        Arc::clone(&self.selection)
    }
}

pub(crate) fn recording_session(
    chains: Vec<Chain>,
) -> (Rc<RefCell<Option<ProjectSession>>>, Rc<RecordingDispatcher>) {
    let recorder = Rc::new(RecordingDispatcher {
        seen: RefCell::new(Vec::new()),
        selection: Arc::new(RwLock::new(SelectionState::default())),
    });
    let session = ProjectSession::with_dispatcher(
        project(chains),
        Rc::clone(&recorder) as Rc<dyn CommandDispatcher>,
        None,
        None,
        PathBuf::from("presets"),
    );
    *session.io_bindings.borrow_mut() = registry();
    (Rc::new(RefCell::new(Some(session))), recorder)
}

pub(crate) fn rows() -> Rc<VecModel<ProjectChainItem>> {
    // Republishing the rows walks the asset paths, which panic until startup
    // set them (same guard as `block_delete_tests.rs:70-75`).
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    Rc::new(VecModel::from(Vec::<ProjectChainItem>::new()))
}

pub(crate) fn chain_in(session: &Rc<RefCell<Option<ProjectSession>>>, index: usize) -> Chain {
    session.borrow().as_ref().unwrap().project.borrow().chains[index].clone()
}
```

- [ ] **Step 2: Write the failing tests.** `crates/adapter-gui/src/chain_block_lists_tests.rs`:

```rust
//! #328 — an index inside a split path never reads the top level.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, mix_chain};
use domain::ids::BlockId;
use project::block::{PathRef, PathSide};

fn path(side: PathSide) -> PathRef {
    PathRef { split: BlockId("sp".into()), side }
}

fn ids(list: Option<&[project::block::AudioBlock]>) -> Vec<String> {
    list.unwrap_or_default().iter().map(|b| b.id.0.clone()).collect()
}

#[test]
fn the_top_level_is_the_list_without_a_path() {
    assert_eq!(ids(list_at(&mix_chain(), None)), vec!["pre", "sp", "post"]);
}

#[test]
fn a_path_names_the_split_lane() {
    let c = mix_chain();
    assert_eq!(ids(list_at(&c, Some(&path(PathSide::A)))), vec!["a1", "a2"]);
    assert_eq!(ids(list_at(&c, Some(&path(PathSide::B)))), vec!["b1"]);
}

#[test]
fn a_path_to_another_split_or_to_a_chain_without_one_is_nothing() {
    let other = PathRef { split: BlockId("gone".into()), side: PathSide::A };
    assert!(list_at(&mix_chain(), Some(&other)).is_none());
    assert!(list_at(&chain(vec![core("x")]), Some(&path(PathSide::A))).is_none());
}

#[test]
fn block_zero_of_path_a_is_not_block_zero_of_the_chain() {
    let c = mix_chain();
    assert_eq!(block_at(&c, 0, Some(&path(PathSide::A))).unwrap().id.0, "a1");
    assert_eq!(block_at(&c, 0, None).unwrap().id.0, "pre");
}

#[test]
fn an_insert_position_is_clamped_to_its_own_list() {
    let c = mix_chain();
    assert_eq!(insert_index(&c, 99, Some(&path(PathSide::B))), Some(1));
    assert_eq!(insert_index(&c, 99, None), Some(3));
}

#[test]
fn a_side_round_trips_through_its_index() {
    assert_eq!(side_index(&PathSide::A), 0);
    assert_eq!(side_index(&PathSide::B), 1);
    assert_eq!(side_from_index(0), Some(PathSide::A));
    assert_eq!(side_from_index(1), Some(PathSide::B));
    assert_eq!(side_from_index(2), None);
}

#[test]
fn split_of_finds_the_chains_split() {
    let c = mix_chain();
    let (index, id, split) = split_of(&c).expect("the chain has a split");
    assert_eq!((index, id.0.as_str(), split.a.len(), split.b.len()), (1, "sp", 2, 1));
}
```

`crates/adapter-gui/src/chain_graph_ids_tests.rs`:

```rust
//! #328 (spec §5.2) — every graph node id resolves to what it stands for.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, mix_chain};
use domain::ids::BlockId;
use project::block::{PathRef, PathSide};
use project::endpoint_disables::EndpointNode;

fn block(id: &str, path: Option<PathSide>, index: usize) -> NodeRef {
    NodeRef::Block {
        id: BlockId(id.into()),
        path: path.map(|side| PathRef { split: BlockId("sp".into()), side }),
        index,
    }
}

#[test]
fn io_nodes_name_their_endpoint_set() {
    let c = mix_chain();
    assert_eq!(resolve_node(&c, INPUT_NODE_ID), Some(NodeRef::Endpoints(EndpointNode::Input)));
    assert_eq!(resolve_node(&c, OUTPUT_NODE_ID), Some(NodeRef::Endpoints(EndpointNode::Output)));
    assert_eq!(resolve_node(&c, PATH_A_OUTPUT_NODE_ID), Some(NodeRef::Endpoints(EndpointNode::PathAOutput)));
    assert_eq!(resolve_node(&c, PATH_B_OUTPUT_NODE_ID), Some(NodeRef::Endpoints(EndpointNode::PathBOutput)));
}

#[test]
fn a_block_node_is_its_block_wherever_it_sits() {
    let c = mix_chain();
    assert_eq!(resolve_node(&c, "pre"), Some(block("pre", None, 0)));
    assert_eq!(resolve_node(&c, "post"), Some(block("post", None, 2)));
    assert_eq!(resolve_node(&c, "a2"), Some(block("a2", Some(PathSide::A), 1)));
    assert_eq!(resolve_node(&c, "b1"), Some(block("b1", Some(PathSide::B), 0)));
}

#[test]
fn split_and_mixer_nodes_name_the_chains_split() {
    let c = mix_chain();
    assert_eq!(resolve_node(&c, SPLIT_NODE_ID), Some(NodeRef::Split));
    assert_eq!(resolve_node(&c, MIXER_NODE_ID), Some(NodeRef::Mixer));
    assert_eq!(resolve_node(&chain(vec![core("x")]), SPLIT_NODE_ID), None);
}

#[test]
fn an_unknown_id_is_nothing() {
    assert_eq!(resolve_node(&mix_chain(), "gone"), None);
}
```

- [ ] **Step 3: Run — expect a compile error** (`unresolved import super::*` items). Register the modules in `lib.rs` next to `mod chain_editor;` (line 188):

```rust
#[cfg_attr(not(test), expect(dead_code, reason = "#328 part 6: first production caller lands in a later task"))]
mod chain_block_lists;
// Several fixtures are first used by later tasks' tests (the last one,
// `recording_session`, in Task 15): until then a test build would warn.
#[cfg(test)]
#[expect(dead_code, reason = "#328 part 6: fixtures of later tasks")]
mod chain_graph_fixtures_tests;
#[cfg_attr(not(test), expect(dead_code, reason = "#328 part 6: first production caller lands in a later task"))]
mod chain_graph_ids;
```

Add stubs so the run shows the behavioural red. `chain_block_lists.rs`:

```rust
//! Responsibility: finds a block list of a chain by its split path.

use domain::ids::BlockId;
use project::block::{AudioBlock, PathRef, PathSide, SplitBlock};
use project::chain::Chain;

pub(crate) fn split_of(_chain: &Chain) -> Option<(usize, &BlockId, &SplitBlock)> {
    None
}
pub(crate) fn list_at<'a>(_chain: &'a Chain, _path: Option<&PathRef>) -> Option<&'a [AudioBlock]> {
    None
}
pub(crate) fn block_at<'a>(_chain: &'a Chain, _index: usize, _path: Option<&PathRef>) -> Option<&'a AudioBlock> {
    None
}
pub(crate) fn insert_index(_chain: &Chain, _before: usize, _path: Option<&PathRef>) -> Option<usize> {
    None
}
pub(crate) fn side_index(_side: &PathSide) -> i32 {
    -1
}
pub(crate) fn side_from_index(_index: i32) -> Option<PathSide> {
    None
}

#[cfg(test)]
#[path = "chain_block_lists_tests.rs"]
mod tests;
```

`chain_graph_ids.rs` stub: the six constants with their final values (below), `NodeRef` as below, `resolve_node` returning `None`.

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib chain_block_lists; nice -n 19 cargo test -p adapter-gui -j 2 --lib chain_graph_ids
```
Expected: failures such as `block_zero_of_path_a_is_not_block_zero_of_the_chain ... panicked ... called Option::unwrap() on a None value` and `assertion left == right failed left: None right: Some(Endpoints(Input))`.

- [ ] **Step 4: Implement.** `crates/adapter-gui/src/chain_block_lists.rs`:

```rust
//! Responsibility: finds a block list of a chain by its split path.
//!
//! #328: a chain's blocks live in up to three lists — the top level and, when
//! the chain has a split, its path A and path B (spec §1.1). Every place that
//! turns "(index, path)" into a block goes through here, so an index inside a
//! path is never read against the top level.

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, PathRef, PathSide, SplitBlock};
use project::chain::Chain;

/// The chain's split: its top-level position, its id and the block itself.
pub(crate) fn split_of(chain: &Chain) -> Option<(usize, &BlockId, &SplitBlock)> {
    chain.blocks.iter().enumerate().find_map(|(index, block)| match &block.kind {
        AudioBlockKind::Split(split) => Some((index, &block.id, split)),
        _ => None,
    })
}

/// The list `path` names: the top level for `None`, else that lane of the
/// chain's split. `None` when the path names a split this chain does not have.
pub(crate) fn list_at<'a>(chain: &'a Chain, path: Option<&PathRef>) -> Option<&'a [AudioBlock]> {
    let Some(path) = path else {
        return Some(&chain.blocks);
    };
    let (_, id, split) = split_of(chain)?;
    if *id != path.split {
        return None;
    }
    Some(match &path.side {
        PathSide::A => &split.a,
        PathSide::B => &split.b,
    })
}

/// The block at `index` of the list `path` names.
pub(crate) fn block_at<'a>(
    chain: &'a Chain,
    index: usize,
    path: Option<&PathRef>,
) -> Option<&'a AudioBlock> {
    list_at(chain, path)?.get(index)
}

/// Where a block inserted "before `before`" lands in the list `path` names,
/// clamped to its end.
pub(crate) fn insert_index(chain: &Chain, before: usize, path: Option<&PathRef>) -> Option<usize> {
    list_at(chain, path).map(|list| before.min(list.len()))
}

/// The side as the Slint callbacks carry it: 0 = A, 1 = B.
pub(crate) fn side_index(side: &PathSide) -> i32 {
    match side {
        PathSide::A => 0,
        PathSide::B => 1,
    }
}

pub(crate) fn side_from_index(index: i32) -> Option<PathSide> {
    match index {
        0 => Some(PathSide::A),
        1 => Some(PathSide::B),
        _ => None,
    }
}

#[cfg(test)]
#[path = "chain_block_lists_tests.rs"]
mod tests;
```

`crates/adapter-gui/src/chain_graph_ids.rs`:

```rust
//! Responsibility: names every node of a chain graph.
//!
//! #328 (spec §5.2). A block node carries the block's own `BlockId`, so a
//! gesture on a card names the block wherever it sits. The input/output nodes
//! and the split/mixer routing nodes are not blocks; their ids are the fixed
//! strings below. The split/mixer ids are the ones `linear_chain_layout` gives
//! the chain's single `Parallel` stage (one split per chain, spec §1.1).

use domain::ids::BlockId;
use project::block::{AudioBlockKind, PathRef, PathSide};
use project::chain::Chain;
use project::endpoint_disables::EndpointNode;

use crate::chain_block_lists::split_of;

pub(crate) const INPUT_NODE_ID: &str = "__io_input";
pub(crate) const OUTPUT_NODE_ID: &str = "__io_output";
pub(crate) const PATH_A_OUTPUT_NODE_ID: &str = "__io_output_a";
pub(crate) const PATH_B_OUTPUT_NODE_ID: &str = "__io_output_b";
pub(crate) const SPLIT_NODE_ID: &str = "__split_1";
pub(crate) const MIXER_NODE_ID: &str = "__merge_1";

/// What a graph node stands for in its chain.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NodeRef {
    /// The block at `index` of the list `path` names (`None` = top level).
    Block {
        id: BlockId,
        path: Option<PathRef>,
        index: usize,
    },
    /// The chain's split node (one split per chain: `split_of` finds it).
    Split,
    /// The chain's mixer node (Split → Mix).
    Mixer,
    /// An input or output node — which endpoint set it shows.
    Endpoints(EndpointNode),
}

pub(crate) fn resolve_node(chain: &Chain, node_id: &str) -> Option<NodeRef> {
    match node_id {
        INPUT_NODE_ID => return Some(NodeRef::Endpoints(EndpointNode::Input)),
        OUTPUT_NODE_ID => return Some(NodeRef::Endpoints(EndpointNode::Output)),
        PATH_A_OUTPUT_NODE_ID => return Some(NodeRef::Endpoints(EndpointNode::PathAOutput)),
        PATH_B_OUTPUT_NODE_ID => return Some(NodeRef::Endpoints(EndpointNode::PathBOutput)),
        SPLIT_NODE_ID => return split_of(chain).map(|_| NodeRef::Split),
        MIXER_NODE_ID => return split_of(chain).map(|_| NodeRef::Mixer),
        _ => {}
    }
    for (index, block) in chain.blocks.iter().enumerate() {
        if block.id.0 == node_id {
            return Some(NodeRef::Block {
                id: block.id.clone(),
                path: None,
                index,
            });
        }
        if let AudioBlockKind::Split(split) = &block.kind {
            for (side, lane) in [(PathSide::A, &split.a), (PathSide::B, &split.b)] {
                if let Some(index) = lane.iter().position(|b| b.id.0 == node_id) {
                    return Some(NodeRef::Block {
                        id: lane[index].id.clone(),
                        path: Some(PathRef {
                            split: block.id.clone(),
                            side,
                        }),
                        index,
                    });
                }
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "chain_graph_ids_tests.rs"]
mod tests;
```

- [ ] **Step 5: Run — expect PASS**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib chain_block_lists && nice -n 19 cargo test -p adapter-gui -j 2 --lib chain_graph_ids
```
Expected: 7 + 4 passed. The `cfg_attr(not(test), expect(dead_code))` lines keep `cargo build` warning-free until Task 7 gives these modules their first production caller; `cargo test` is warning-free because the tests use every item.

- [ ] **Step 6: Commit**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; git -C "$S" add crates/adapter-gui/src/lib.rs crates/adapter-gui/src/chain_block_lists.rs crates/adapter-gui/src/chain_block_lists_tests.rs crates/adapter-gui/src/chain_graph_ids.rs crates/adapter-gui/src/chain_graph_ids_tests.rs crates/adapter-gui/src/chain_graph_fixtures_tests.rs && git -C "$S" commit -m "feat(#328): resolve graph node ids and path block lists"
```
Run the Push gate, push, comment on #328.

---

### Task 4: Graph anchors → insert and move targets

A "+" on a wire and a drop on the canvas both arrive as the anchor id Part 5 gives every wire (`AnchorSlot::anchor_id`: `"stage:{i}"` / `"lane:{stage}:{lane}:{i}"`). One pure module turns it back into its `AnchorSlot` and then into "position + path" (spec §3: `path: None` = top level) — for `AddBlock`/`InsertPrebuiltBlock` and for `MoveBlock` with the same "lifted first" rule as `block_reorder.rs:34-50`. The stage numbering is the one Task 6's adapter builds: stage 0 is the input node, then one stage per top-level block (the split is one `Parallel` stage), then the output node — so stage `i` is top-level position `i - 1`, and lane 0 / 1 of the split's stage (split position + 1) is path A / B. Task 6 pins that every anchor of a laid-out chain maps to a place.

**Files:**
- Create: `crates/adapter-gui/src/graph_anchor.rs`
- Modify: `crates/adapter-gui/src/lib.rs` (add `mod graph_anchor;` after `mod chain_graph_ids;`)
- Test: `crates/adapter-gui/src/graph_anchor_tests.rs`

**Interfaces:**
- Consumes: Task 3 (`resolve_node`, `NodeRef`, node id constants, `split_of`, `list_at`, `side_from_index`); Part 5 `graph_view_model::AnchorSlot` (`Copy + PartialEq + Debug`, `anchor_id(self) -> String`).
- Produces: `parse_anchor(&str) -> Option<AnchorSlot>` (inverse of `AnchorSlot::anchor_id`), `struct InsertTarget { position: usize, path: Option<PathRef> }`, `insert_target(&Chain, &AnchorSlot) -> Option<InsertTarget>`, `struct MoveTarget { block: BlockId, new_position: usize, path: Option<PathRef> }`, `move_target(&Chain, &str, &AnchorSlot) -> Option<MoveTarget>`.

- [ ] **Step 1: Write the failing tests** `crates/adapter-gui/src/graph_anchor_tests.rs`:

```rust
//! #328 (spec §5.1, §7 "drop-target resolution") — a "+" or a drop names a
//! place in the chain: a position in one list plus the path of that list.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, mix_chain, split, y_chain};
use crate::chain_graph_ids::{INPUT_NODE_ID, SPLIT_NODE_ID};
use domain::ids::BlockId;
use project::block::{PathRef, PathSide, SplitEnd};

// Stage numbering of Task 6's adapter. mix_chain() lays out as
// 0 input · 1 pre · 2 split(A: a1 a2 | B: b1) · 3 post · 4 output.
fn stage(index: usize) -> AnchorSlot {
    AnchorSlot::Stage { index }
}

fn lane(stage: usize, lane: usize, index: usize) -> AnchorSlot {
    AnchorSlot::Lane { stage, lane, index }
}

fn on(side: PathSide) -> Option<PathRef> {
    Some(PathRef { split: BlockId("sp".into()), side })
}

fn at(position: usize, path: Option<PathRef>) -> Option<InsertTarget> {
    Some(InsertTarget { position, path })
}

#[test]
fn an_anchor_id_parses_back_into_its_slot() {
    for slot in [stage(0), stage(3), lane(2, 0, 0), lane(2, 1, 4)] {
        assert_eq!(parse_anchor(&slot.anchor_id()), Some(slot), "{}", slot.anchor_id());
    }
    for bad in ["", "stage", "stage:x", "lane:1:0", "lane:1:0:0:9", "edge:1"] {
        assert_eq!(parse_anchor(bad), None, "{bad:?}");
    }
}

#[test]
fn a_top_level_anchor_inserts_before_its_stage() {
    let c = mix_chain();
    assert_eq!(insert_target(&c, &stage(1)), at(0, None)); // input → pre
    assert_eq!(insert_target(&c, &stage(2)), at(1, None)); // pre → split
    assert_eq!(insert_target(&c, &stage(3)), at(2, None)); // mixer → post
    assert_eq!(insert_target(&c, &stage(4)), at(3, None)); // post → output
    assert_eq!(insert_target(&c, &stage(0)), None, "nothing goes before the input node");
    assert_eq!(insert_target(&c, &stage(5)), None, "past the output node");
}

#[test]
fn a_lane_anchor_inserts_in_that_path() {
    let c = mix_chain();
    assert_eq!(insert_target(&c, &lane(2, 0, 0)), at(0, on(PathSide::A))); // split → a1
    assert_eq!(insert_target(&c, &lane(2, 0, 1)), at(1, on(PathSide::A))); // a1 → a2
    assert_eq!(insert_target(&c, &lane(2, 0, 2)), at(2, on(PathSide::A))); // a2 → mixer
    assert_eq!(insert_target(&c, &lane(2, 1, 1)), at(1, on(PathSide::B))); // b1 → mixer
    assert_eq!(insert_target(&c, &lane(2, 1, 2)), None, "past the end of path B");
    assert_eq!(insert_target(&c, &lane(1, 0, 0)), None, "stage 1 is not the split");
    assert_eq!(insert_target(&c, &lane(2, 2, 0)), None, "a split has two lanes");
}

#[test]
fn an_empty_lane_is_reached_by_its_own_anchor() {
    // 0 input · 1 split(A: — | B: —) · 2 output: each empty lane has its own
    // split → mixer wire, with its own anchor id.
    let c = chain(vec![split("sp", SplitEnd::Mix, vec![], vec![])]);
    assert_eq!(insert_target(&c, &lane(1, 0, 0)), at(0, on(PathSide::A)));
    assert_eq!(insert_target(&c, &lane(1, 1, 0)), at(0, on(PathSide::B)));
}

#[test]
fn a_y_lane_ends_before_its_own_output_node() {
    // 0 input · 1 pre · 2 split(A: a1 → out A | B: b1 → out B)
    let c = y_chain();
    assert_eq!(insert_target(&c, &lane(2, 0, 1)), at(1, on(PathSide::A))); // a1 → out A
    assert_eq!(insert_target(&c, &lane(2, 1, 0)), at(0, on(PathSide::B))); // split → b1
}

#[test]
fn a_lane_anchor_on_a_chain_without_a_split_is_nothing() {
    assert_eq!(insert_target(&chain(vec![core("x")]), &lane(1, 0, 0)), None);
}

fn moved(block: &str, new_position: usize, path: Option<PathRef>) -> Option<MoveTarget> {
    Some(MoveTarget { block: BlockId(block.into()), new_position, path })
}

#[test]
fn dragging_a_block_to_the_other_lane_moves_it_into_that_path() {
    let c = mix_chain();
    assert_eq!(move_target(&c, "a1", &lane(2, 1, 1)), moved("a1", 1, on(PathSide::B)));
    assert_eq!(move_target(&c, "b1", &lane(2, 0, 0)), moved("b1", 0, on(PathSide::A)));
}

#[test]
fn a_move_inside_one_list_counts_after_the_block_is_lifted() {
    let c = mix_chain();
    // a1 dropped at the end of lane A (after a2): lifted first, so it lands at 1.
    assert_eq!(move_target(&c, "a1", &lane(2, 0, 2)), moved("a1", 1, on(PathSide::A)));
    assert_eq!(move_target(&c, "a2", &lane(2, 0, 0)), moved("a2", 0, on(PathSide::A)));
}

#[test]
fn dropping_a_block_on_its_own_slot_is_no_move() {
    let c = mix_chain();
    assert_eq!(move_target(&c, "a1", &lane(2, 0, 0)), None);
    assert_eq!(move_target(&c, "a1", &lane(2, 0, 1)), None);
}

#[test]
fn shared_and_path_positions_trade_blocks() {
    let c = mix_chain();
    assert_eq!(move_target(&c, "pre", &lane(2, 0, 2)), moved("pre", 2, on(PathSide::A)));
    assert_eq!(move_target(&c, "b1", &stage(3)), moved("b1", 2, None));
}

#[test]
fn only_block_nodes_move() {
    let c = mix_chain();
    assert_eq!(move_target(&c, SPLIT_NODE_ID, &lane(2, 0, 0)), None);
    assert_eq!(move_target(&c, INPUT_NODE_ID, &lane(2, 0, 0)), None);
}
```

- [ ] **Step 2: Run — compile error; add the stub; run again — behavioural red.** Add to `lib.rs` after `mod chain_graph_ids;`:

```rust
#[cfg_attr(not(test), expect(dead_code, reason = "#328 part 6: first production caller lands in a later task"))]
mod graph_anchor;
```
 Stub `graph_anchor.rs`: the header and `use` lines, `InsertTarget` and `MoveTarget` as below, and `parse_anchor` / `insert_target` / `move_target` returning `None`.

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib graph_anchor
```
Expected: `an_anchor_id_parses_back_into_its_slot` fails with `left: None right: Some(Stage { index: 0 })`; the `insert_target` / `move_target` tests fail with `left: None right: Some(InsertTarget { position: 0, path: None })` and `left: None right: Some(MoveTarget { block: BlockId("a1"), … })`. `dropping_a_block_on_its_own_slot_is_no_move` and `only_block_nodes_move` pass on the stub: they are no-move guards.

- [ ] **Step 3: Implement** `crates/adapter-gui/src/graph_anchor.rs`:

```rust
//! Responsibility: turns a graph anchor into a place in the chain.
//!
//! #328 (spec §5.1). The graph reports a "+" or a drop by the anchor id Part 5
//! gives each wire (`AnchorSlot::anchor_id`). `chain_graph_adapter` lays a
//! chain out as stage 0 = the input node, one stage per top-level block (the
//! split is one `Parallel` stage), then the output node — so stage `i` is
//! top-level position `i - 1`, and lane 0 / 1 of the split's stage (split
//! position + 1) is path A / B. A place is a position in one block list plus
//! that list's path (`None` = top level, spec §3).

use domain::ids::BlockId;
use project::block::PathRef;
use project::chain::Chain;

use crate::chain_block_lists::{list_at, side_from_index, split_of};
use crate::chain_graph_ids::{resolve_node, NodeRef};
use crate::graph_view_model::AnchorSlot;

/// The slot an anchor id names — the inverse of `AnchorSlot::anchor_id`
/// (`"stage:{i}"`, `"lane:{stage}:{lane}:{i}"`).
pub(crate) fn parse_anchor(anchor_id: &str) -> Option<AnchorSlot> {
    let mut parts = anchor_id.split(':');
    let slot = match parts.next()? {
        "stage" => AnchorSlot::Stage {
            index: parts.next()?.parse().ok()?,
        },
        "lane" => AnchorSlot::Lane {
            stage: parts.next()?.parse().ok()?,
            lane: parts.next()?.parse().ok()?,
            index: parts.next()?.parse().ok()?,
        },
        _ => return None,
    };
    parts.next().is_none().then_some(slot)
}

/// Where a new block goes: `position` in the list `path` names.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InsertTarget {
    pub(crate) position: usize,
    pub(crate) path: Option<PathRef>,
}

/// The place an anchor stands for ("insert before" the slot's index).
pub(crate) fn insert_target(chain: &Chain, slot: &AnchorSlot) -> Option<InsertTarget> {
    match *slot {
        AnchorSlot::Stage { index } => {
            // Stage 0 is the input node: nothing goes before it.
            let position = index.checked_sub(1)?;
            (position <= chain.blocks.len()).then_some(InsertTarget {
                position,
                path: None,
            })
        }
        AnchorSlot::Lane { stage, lane, index } => {
            let (split_position, split_id, _) = split_of(chain)?;
            if stage != split_position + 1 {
                return None;
            }
            let path = PathRef {
                split: split_id.clone(),
                side: side_from_index(i32::try_from(lane).ok()?)?,
            };
            let len = list_at(chain, Some(&path))?.len();
            (index <= len).then_some(InsertTarget {
                position: index,
                path: Some(path),
            })
        }
    }
}

/// A drag: the block, its `MoveBlock.new_position` and the destination path.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MoveTarget {
    pub(crate) block: BlockId,
    pub(crate) new_position: usize,
    pub(crate) path: Option<PathRef>,
}

pub(crate) fn move_target(chain: &Chain, node_id: &str, slot: &AnchorSlot) -> Option<MoveTarget> {
    let NodeRef::Block {
        id,
        path: from_path,
        index: from,
    } = resolve_node(chain, node_id)?
    else {
        return None;
    };
    let target = insert_target(chain, slot)?;
    if target.path != from_path {
        // Lifting it out of another list does not shift this one.
        return Some(MoveTarget {
            block: id,
            new_position: target.position,
            path: target.path,
        });
    }
    // Same list: the block is lifted out first, so everything to its right
    // shifts one slot left (the `block_reorder.rs:34-50` rule). Its own slot
    // and the gap right after it change nothing.
    if target.position == from || target.position == from + 1 {
        return None;
    }
    let new_position = if target.position > from {
        target.position - 1
    } else {
        target.position
    };
    Some(MoveTarget {
        block: id,
        new_position,
        path: target.path,
    })
}

#[cfg(test)]
#[path = "graph_anchor_tests.rs"]
mod tests;
```

- [ ] **Step 4: Run — expect PASS**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib graph_anchor
```
Expected: 11 passed.

- [ ] **Step 5: Commit**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; git -C "$S" add crates/adapter-gui/src/lib.rs crates/adapter-gui/src/graph_anchor.rs crates/adapter-gui/src/graph_anchor_tests.rs && git -C "$S" commit -m "feat(#328): resolve graph anchors to insert and move targets"
```
Run the Push gate, push, comment on #328.

---

### Task 5: What an input or output node shows

The input node lists the chain's E/S inputs; the output nodes list its outputs (spec §5.3). The list is Part 1's `endpoint_candidates(&chain.io_binding_ids, registry)` — every endpoint of the bindings the chain selected, unchecked ones included — never `resolve_chain_ports`, which after Part 1 Task 11 drops an unchecked endpoint (the row the user just unchecked would vanish and could never be re-checked). Mid-chain `Input`/`Output` port blocks are not part of the chain's input/output node: they are not in `endpoint_candidates`. Each row carries its enabled state from `Chain.disabled_endpoints` (Part 1).

**Files:**
- Create: `crates/adapter-gui/src/endpoint_checklist_items.rs`
- Modify: `crates/adapter-gui/src/lib.rs` (add `mod endpoint_checklist_items;`), the nine `crates/adapter-gui/locales/*.yml` (key `label-endpoints-none`, Appendix A)
- Test: `crates/adapter-gui/src/endpoint_checklist_items_tests.rs`

**Interfaces:**
- Consumes: Part 1 `project::endpoint_candidates::endpoint_candidates(&[String], &[IoBinding]) -> (Vec<EndpointRef>, Vec<EndpointRef>)`, `EndpointDisables::is_enabled(&self, EndpointNode, &EndpointRef) -> bool` (`EndpointNode: Copy`).
- Produces: `struct EndpointRow { io: String, endpoint: String, label: String, enabled: bool }`, `endpoint_rows(&Chain, &[IoBinding], EndpointNode) -> Vec<EndpointRow>`, `node_label(&[EndpointRow], &str) -> String`, `struct IoLabels { input, output, path_a, path_b: String }`, `io_labels(&Chain, &[IoBinding], fallback_input: &str, fallback_output: &str) -> IoLabels`.

- [ ] **Step 1: Write the failing tests** `crates/adapter-gui/src/endpoint_checklist_items_tests.rs`:

```rust
//! #328 (spec §5.3) — the checklist lists every input (or output) of the
//! chain's E/S, checked unless that node disabled it; a mid-chain port is not
//! part of the input/output node.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, port_in, registry};
use project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef};

fn r(io: &str, endpoint: &str) -> EndpointRef {
    EndpointRef { io: io.into(), endpoint: endpoint.into() }
}

fn labels(rows: &[EndpointRow]) -> Vec<(&str, bool)> {
    rows.iter().map(|row| (row.label.as_str(), row.enabled)).collect()
}

#[test]
fn the_input_node_lists_the_chains_inputs_not_its_mid_ports() {
    let c = chain(vec![port_in("port", "main", "In 2"), core("amp")]);
    let rows = endpoint_rows(&c, &registry(), EndpointNode::Input);
    assert_eq!(labels(&rows), vec![("In 1", true), ("In 2", true)]);
    assert_eq!((rows[1].io.as_str(), rows[1].endpoint.as_str()), ("main", "In 2"));
}

#[test]
fn outputs_with_the_same_name_carry_their_binding_name() {
    let rows = endpoint_rows(&chain(vec![]), &registry(), EndpointNode::Output);
    assert_eq!(labels(&rows), vec![("Scarlett · Out L/R", true), ("AUX · Out L/R", true)]);
}

#[test]
fn an_unchecked_endpoint_stays_listed_disabled() {
    let mut c = chain(vec![]);
    c.disabled_endpoints = EndpointDisables {
        inputs: vec![r("main", "In 2")],
        outputs: vec![],
        path_a_outputs: vec![],
        path_b_outputs: vec![],
    };
    let rows = endpoint_rows(&c, &registry(), EndpointNode::Input);
    assert_eq!(labels(&rows), vec![("In 1", true), ("In 2", false)]);
}

#[test]
fn a_path_output_node_reads_only_its_own_disables() {
    let mut c = chain(vec![]);
    c.disabled_endpoints = EndpointDisables {
        inputs: vec![],
        outputs: vec![],
        path_a_outputs: vec![r("aux", "Out L/R")],
        path_b_outputs: vec![],
    };
    let a = endpoint_rows(&c, &registry(), EndpointNode::PathAOutput);
    let main = endpoint_rows(&c, &registry(), EndpointNode::Output);
    assert_eq!(labels(&a), vec![("Scarlett · Out L/R", true), ("AUX · Out L/R", false)]);
    assert!(main.iter().all(|row| row.enabled), "the chain output node is untouched");
}

#[test]
fn a_node_label_names_its_checked_endpoints() {
    let rows = vec![
        EndpointRow { io: "main".into(), endpoint: "In 1".into(), label: "In 1".into(), enabled: true },
        EndpointRow { io: "main".into(), endpoint: "In 2".into(), label: "In 2".into(), enabled: false },
    ];
    assert_eq!(node_label(&rows, "None"), "In 1");
    let off: Vec<EndpointRow> = rows.into_iter().map(|row| EndpointRow { enabled: false, ..row }).collect();
    assert_eq!(node_label(&off, "None"), "None");
}

#[test]
fn without_a_registry_the_nodes_fall_back_to_the_row_labels() {
    let io = io_labels(&chain(vec![]), &[], "In", "Out");
    assert_eq!((io.input.as_str(), io.output.as_str()), ("In", "Out"));
    assert_eq!((io.path_a.as_str(), io.path_b.as_str()), ("Out", "Out"));
}

#[test]
fn with_a_registry_each_node_names_its_endpoints() {
    let io = io_labels(&chain(vec![]), &registry(), "In", "Out");
    assert_eq!(io.input, "In 1, In 2");
    assert_eq!(io.output, "Scarlett · Out L/R, AUX · Out L/R");
}
```

- [ ] **Step 2: Run — compile error; stub; behavioural red.** Add to `lib.rs`: `#[cfg_attr(not(test), expect(dead_code, reason = "#328 part 6: first production caller lands in a later task"))] mod endpoint_checklist_items;`. Stub: types as below, `endpoint_rows` → `Vec::new()`, `node_label` → `String::new()`, `io_labels` → all four fields `String::new()`.

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib endpoint_checklist_items
```
Expected: `assertion left == right failed left: [] right: [("In 1", true), ("In 2", true)]` and `left: ("", "") right: ("In", "Out")`.

- [ ] **Step 3: Implement** `crates/adapter-gui/src/endpoint_checklist_items.rs`:

```rust
//! Responsibility: lists what a graph input or output node shows.
//!
//! #328 (spec §5.3). The input node stands for every input of the chain's E/S
//! bindings; an output node for every output — Part 1's `endpoint_candidates`,
//! which lists them unchecked ones included (`resolve_chain_ports` drops an
//! unchecked endpoint, so it cannot feed a checklist). A mid-chain
//! `Input`/`Output` port block is a card of its own, not part of these nodes.
//! A row is checked unless THIS node disabled it (`Chain.disabled_endpoints`,
//! Part 1) — nothing is added to or removed from the E/S.

use infra_filesystem::IoBinding;
use project::chain::Chain;
use project::endpoint_candidates::endpoint_candidates;
use project::endpoint_disables::{EndpointNode, EndpointRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EndpointRow {
    pub(crate) io: String,
    pub(crate) endpoint: String,
    pub(crate) label: String,
    pub(crate) enabled: bool,
}

pub(crate) fn endpoint_rows(chain: &Chain, registry: &[IoBinding], node: EndpointNode) -> Vec<EndpointRow> {
    let (inputs, outputs) = endpoint_candidates(&chain.io_binding_ids, registry);
    let refs = match node {
        EndpointNode::Input => inputs,
        EndpointNode::Output | EndpointNode::PathAOutput | EndpointNode::PathBOutput => outputs,
    };
    refs.iter()
        .map(|reference| EndpointRow {
            label: row_label(reference, &refs, registry),
            enabled: chain.disabled_endpoints.is_enabled(node, reference),
            io: reference.io.clone(),
            endpoint: reference.endpoint.clone(),
        })
        .collect()
}

/// The endpoint name, prefixed with its binding's NAME when the same endpoint
/// name repeats among the listed endpoints — the rule `chain_endpoint_labels`
/// (`binding_discovery.rs:156-195`) uses for the looper and DI selects.
fn row_label(reference: &EndpointRef, refs: &[EndpointRef], registry: &[IoBinding]) -> String {
    let repeated = refs
        .iter()
        .filter(|other| other.endpoint == reference.endpoint)
        .count()
        > 1;
    if !repeated {
        return reference.endpoint.clone();
    }
    let binding = registry
        .iter()
        .find(|b| b.id == reference.io)
        .map(|b| b.name.trim())
        .filter(|name| !name.is_empty())
        .unwrap_or(reference.io.as_str());
    format!("{binding} · {}", reference.endpoint)
}

/// The node's text: its checked endpoints, or `none` when every one is off.
pub(crate) fn node_label(rows: &[EndpointRow], none: &str) -> String {
    let names: Vec<&str> = rows
        .iter()
        .filter(|row| row.enabled)
        .map(|row| row.label.as_str())
        .collect();
    if names.is_empty() {
        none.to_string()
    } else {
        names.join(", ")
    }
}

/// The text of the four endpoint nodes a chain graph can have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IoLabels {
    pub(crate) input: String,
    pub(crate) output: String,
    pub(crate) path_a: String,
    pub(crate) path_b: String,
}

/// Rows republished without a registry (many callers pass `&[]` to
/// `replace_project_chains`) cannot name endpoints; they keep the row's own
/// `input_label` / `output_label` text instead of claiming "none".
pub(crate) fn io_labels(
    chain: &Chain,
    registry: &[IoBinding],
    fallback_input: &str,
    fallback_output: &str,
) -> IoLabels {
    let none = rust_i18n::t!("label-endpoints-none").to_string();
    let label = |node: EndpointNode, fallback: &str| {
        let rows = endpoint_rows(chain, registry, node);
        if rows.is_empty() {
            fallback.to_string()
        } else {
            node_label(&rows, &none)
        }
    };
    IoLabels {
        input: label(EndpointNode::Input, fallback_input),
        output: label(EndpointNode::Output, fallback_output),
        path_a: label(EndpointNode::PathAOutput, fallback_output),
        path_b: label(EndpointNode::PathBOutput, fallback_output),
    }
}

#[cfg(test)]
#[path = "endpoint_checklist_items_tests.rs"]
mod tests;
```

(`EndpointNode` is `Copy` in Part 1, so `node` is passed by value to `is_enabled` inside the closure.)

Add `label-endpoints-none` to the nine `locales/*.yml` files with the Appendix A values, appended under a `# --- #328 chain split graph` comment at the end of each file.

- [ ] **Step 4: Run — expect PASS**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib endpoint_checklist_items && nice -n 19 cargo test -p adapter-gui -j 2 --lib every_locale_carries_the_same_keys_as_english
```
Expected: 7 passed; the locale-key test passes.

- [ ] **Step 5: Commit**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; git -C "$S" add crates/adapter-gui/src/lib.rs crates/adapter-gui/src/endpoint_checklist_items.rs crates/adapter-gui/src/endpoint_checklist_items_tests.rs crates/adapter-gui/locales/de-DE.yml crates/adapter-gui/locales/en-US.yml crates/adapter-gui/locales/es-ES.yml crates/adapter-gui/locales/fr-FR.yml crates/adapter-gui/locales/hi-IN.yml crates/adapter-gui/locales/ja-JP.yml crates/adapter-gui/locales/ko-KR.yml crates/adapter-gui/locales/pt-BR.yml crates/adapter-gui/locales/zh-CN.yml && git -C "$S" commit -m "feat(#328): list the endpoints a graph input or output node shows"
```
Run the Push gate, push, comment on #328.

---

### Task 6: Chain → graph

`chain_graph_adapter.rs` turns a `Chain` (with its `Split`) into Part 5's `ChainStage`s, lays them out with `linear_chain_layout` and places the "+" anchors with `insert_anchors`. The stage order is the one Task 4's `insert_target` counts on: stage 0 = input node, one stage per top-level block, then the output node (none for a Y chain: its lanes end in their own output nodes). I/O nodes carry Part 5's `NodeKind::IoInput` / `NodeKind::IoOutput` (`BlockBlueprint::with_kind`): the card, the "always visible +" of an empty segment and the "only a block can be dropped" rule of `resolve_drop_anchor` all read the kind. Cards are 100 px wide with the strip's 32 px gap (`chain_row_blocks.slint:47-49`: 132 px per chip) and one lane per strip row (108 px, `chain_row.slint:100`).

**Files:**
- Create: `crates/adapter-gui/src/chain_graph_adapter.rs`
- Modify: `crates/adapter-gui/src/lib.rs` (add `mod chain_graph_adapter;`)
- Test: `crates/adapter-gui/src/chain_graph_adapter_tests.rs`

**Interfaces:**
- Consumes: Task 3 ids, Task 4 `parse_anchor`/`insert_target` (test only), Task 5 `IoLabels`; Part 5 `graph_view_model::{insert_anchors, linear_chain_layout, BlockBlueprint, ChainStage, GraphAnchor, GraphEdge, GraphNode, GridMetrics, NodeCategory, NodeKind, ParallelEnd}`; `project::catalog::model_display_name(&str, &str) -> String`.
- Produces: `COLUMN_SPACING = 132.0`, `LANE_SPACING = 108.0`, `CARD_HALF = 50.0`, `grid_metrics(lanes: usize) -> GridMetrics`, `chain_stages(&Chain, &IoLabels) -> Vec<ChainStage>`, `struct ChainGraph { nodes: Vec<GraphNode>, edges: Vec<GraphEdge>, anchors: Vec<GraphAnchor>, lanes: usize, columns: usize }`, `chain_graph(&Chain, &IoLabels) -> ChainGraph`.

- [ ] **Step 1: Write the failing tests** `crates/adapter-gui/src/chain_graph_adapter_tests.rs`:

```rust
//! #328 (spec §5.1) — left to right: input → shared blocks → split → lanes
//! (A on top) → mixer → shared blocks → output; for Y each lane ends in its
//! own output node.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, mix_chain, split, y_chain};
use crate::chain_graph_ids::{
    INPUT_NODE_ID, MIXER_NODE_ID, OUTPUT_NODE_ID, PATH_A_OUTPUT_NODE_ID, PATH_B_OUTPUT_NODE_ID,
    SPLIT_NODE_ID,
};
use crate::endpoint_checklist_items::IoLabels;
use crate::graph_anchor::{insert_target, parse_anchor};
use crate::graph_view_model::NodeKind;
use project::block::SplitEnd;

fn labels() -> IoLabels {
    IoLabels {
        input: "In 1".into(),
        output: "Out".into(),
        path_a: "Out A".into(),
        path_b: "Out B".into(),
    }
}

fn node<'a>(graph: &'a ChainGraph, id: &str) -> &'a crate::graph_view_model::GraphNode {
    graph.nodes.iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no node {id}"))
}

fn has_edge(graph: &ChainGraph, from: &str, to: &str) -> bool {
    graph.edges.iter().any(|e| e.from_id == from && e.to_id == to)
}

#[test]
fn a_linear_chain_is_one_lane_from_input_to_output() {
    let graph = chain_graph(&chain(vec![core("a"), core("b")]), &labels());
    let ids: Vec<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, vec![INPUT_NODE_ID, "a", "b", OUTPUT_NODE_ID]);
    assert_eq!((graph.lanes, graph.columns), (1, 4));
    for (column, id) in ids.iter().enumerate() {
        let n = node(&graph, id);
        assert_eq!((n.x, n.y), (CARD_HALF + column as f32 * COLUMN_SPACING, CARD_HALF));
    }
    assert!(has_edge(&graph, INPUT_NODE_ID, "a") && has_edge(&graph, "a", "b") && has_edge(&graph, "b", OUTPUT_NODE_ID));
    assert_eq!(node(&graph, INPUT_NODE_ID).label, "In 1");
    assert_eq!(node(&graph, OUTPUT_NODE_ID).label, "Out");
}

#[test]
fn a_split_to_mix_chain_runs_two_lanes_between_split_and_mixer() {
    let graph = chain_graph(&mix_chain(), &labels());
    assert_eq!(graph.lanes, 2);
    let split_y = node(&graph, SPLIT_NODE_ID).y;
    assert!(node(&graph, "a1").y < split_y && split_y < node(&graph, "b1").y, "lane A above, lane B below");
    assert_eq!(node(&graph, "pre").y, split_y);
    assert_eq!(node(&graph, "post").y, node(&graph, MIXER_NODE_ID).y);
    for (from, to) in [
        (INPUT_NODE_ID, "pre"),
        ("pre", SPLIT_NODE_ID),
        (SPLIT_NODE_ID, "a1"),
        ("a1", "a2"),
        ("a2", MIXER_NODE_ID),
        (SPLIT_NODE_ID, "b1"),
        ("b1", MIXER_NODE_ID),
        (MIXER_NODE_ID, "post"),
        ("post", OUTPUT_NODE_ID),
    ] {
        assert!(has_edge(&graph, from, to), "missing wire {from} → {to}");
    }
    assert!(graph.nodes.iter().all(|n| n.id != PATH_A_OUTPUT_NODE_ID && n.id != PATH_B_OUTPUT_NODE_ID));
}

#[test]
fn a_y_chain_ends_each_lane_in_its_own_output_node() {
    let graph = chain_graph(&y_chain(), &labels());
    assert!(graph.nodes.iter().all(|n| n.id != OUTPUT_NODE_ID && n.id != MIXER_NODE_ID));
    assert_eq!(node(&graph, PATH_A_OUTPUT_NODE_ID).y, node(&graph, "a1").y);
    assert_eq!(node(&graph, PATH_B_OUTPUT_NODE_ID).y, node(&graph, "b1").y);
    assert!(has_edge(&graph, "a1", PATH_A_OUTPUT_NODE_ID) && has_edge(&graph, "b1", PATH_B_OUTPUT_NODE_ID));
    assert_eq!(node(&graph, PATH_A_OUTPUT_NODE_ID).label, "Out A");
    assert_eq!(node(&graph, PATH_B_OUTPUT_NODE_ID).label, "Out B");
}

#[test]
fn empty_mix_lanes_still_draw_split_and_mixer() {
    let graph = chain_graph(&chain(vec![split("sp", SplitEnd::Mix, vec![], vec![])]), &labels());
    assert!(has_edge(&graph, SPLIT_NODE_ID, MIXER_NODE_ID));
    assert!(has_edge(&graph, MIXER_NODE_ID, OUTPUT_NODE_ID));
}

#[test]
fn a_switched_off_block_is_drawn_bypassed() {
    let mut off = core("a");
    off.enabled = false;
    let graph = chain_graph(&chain(vec![off]), &labels());
    assert!(node(&graph, "a").bypass);
}

#[test]
fn the_grid_centres_the_shared_lane_between_two_path_lanes() {
    assert_eq!(grid_metrics(1).origin_y, CARD_HALF);
    assert_eq!(grid_metrics(2).origin_y, CARD_HALF + LANE_SPACING / 2.0);
}

/// The card face, the "+" of an empty segment and the "only blocks move" rule
/// of `resolve_drop_anchor` all read Part 5's `NodeKind`.
#[test]
fn every_node_carries_its_kind() {
    let y = chain_graph(&y_chain(), &labels());
    assert_eq!(node(&y, INPUT_NODE_ID).kind, NodeKind::IoInput);
    assert_eq!(node(&y, PATH_A_OUTPUT_NODE_ID).kind, NodeKind::IoOutput);
    assert_eq!(node(&y, PATH_B_OUTPUT_NODE_ID).kind, NodeKind::IoOutput);
    assert_eq!(node(&y, SPLIT_NODE_ID).kind, NodeKind::Split);
    assert_eq!(node(&y, "a1").kind, NodeKind::Block);
    let mix = chain_graph(&mix_chain(), &labels());
    assert_eq!(node(&mix, MIXER_NODE_ID).kind, NodeKind::Mixer);
    assert_eq!(node(&mix, OUTPUT_NODE_ID).kind, NodeKind::IoOutput);
}

/// Pins the stage numbering Task 4's `insert_target` counts on: every "+"
/// Part 5 places on a laid-out chain names a place in that chain.
#[test]
fn every_plus_of_a_laid_out_chain_names_a_place_in_it() {
    let chains = [
        chain(vec![core("a"), core("b")]),
        mix_chain(),
        y_chain(),
        chain(vec![split("sp", SplitEnd::Mix, vec![], vec![])]),
    ];
    for c in chains {
        let graph = chain_graph(&c, &labels());
        assert!(!graph.anchors.is_empty(), "no + on {:?}", c.blocks);
        for anchor in &graph.anchors {
            let slot = parse_anchor(&anchor.id).unwrap_or_else(|| panic!("unparsed anchor {}", anchor.id));
            assert!(insert_target(&c, &slot).is_some(), "anchor {} names no place", anchor.id);
        }
    }
}
```

- [ ] **Step 2: Run — compile error; stub; behavioural red.** Add to `lib.rs`: `#[cfg_attr(not(test), expect(dead_code, reason = "#328 part 6: first production caller lands in a later task"))] mod chain_graph_adapter;`. Stub: constants with final values, `ChainGraph` as below, `grid_metrics` returning `GridMetrics::default()`, `chain_stages` → `Vec::new()`, `chain_graph` → `ChainGraph { nodes: vec![], edges: vec![], anchors: vec![], lanes: 0, columns: 0 }`.

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib chain_graph_adapter
```
Expected: `left: [] right: ["__io_input", "a", "b", "__io_output"]`, `no node __split_1`, `left: 200.0 right: 50.0`, `no + on [...]`.

- [ ] **Step 3: Implement** `crates/adapter-gui/src/chain_graph_adapter.rs`:

```rust
//! Responsibility: turns a chain into the graph its row draws.
//!
//! #328 (spec §5.1, §5.2). The chain's `Split` becomes Part 5's `Parallel`
//! stage: lane A on top, lane B below; `Merge` ends it in the mixer node, `Fan`
//! (Y → A/B) ends each lane in its own output node. Stage 0 is the input node
//! and every top-level block is one stage — `graph_anchor` counts on that
//! numbering. Positions and "+" anchors come from Part 5
//! (`linear_chain_layout`, `insert_anchors`); this file only picks the stages
//! and the grid.

use project::block::{AudioBlock, AudioBlockKind, SplitEnd};
use project::chain::Chain;

use crate::chain_graph_ids::{
    INPUT_NODE_ID, OUTPUT_NODE_ID, PATH_A_OUTPUT_NODE_ID, PATH_B_OUTPUT_NODE_ID,
};
use crate::endpoint_checklist_items::IoLabels;
use crate::graph_view_model::{
    insert_anchors, linear_chain_layout, BlockBlueprint, ChainStage, GraphAnchor, GraphEdge,
    GraphNode, GridMetrics, NodeCategory, NodeKind, ParallelEnd,
};

/// Centre-to-centre: a 100 px card plus the strip's 32 px gap (`chain_row_blocks.slint:47`).
pub(crate) const COLUMN_SPACING: f32 = 132.0;
/// One strip row per lane (`chain_row.slint:100`: 108 px per row).
pub(crate) const LANE_SPACING: f32 = 108.0;
/// The first card's centre sits half a card in from the corner.
pub(crate) const CARD_HALF: f32 = 50.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChainGraph {
    pub(crate) nodes: Vec<GraphNode>,
    pub(crate) edges: Vec<GraphEdge>,
    pub(crate) anchors: Vec<GraphAnchor>,
    pub(crate) lanes: usize,
    pub(crate) columns: usize,
}

/// With two lanes the shared blocks sit between them, so the top lane's cards
/// still start at the row's top edge.
pub(crate) fn grid_metrics(lanes: usize) -> GridMetrics {
    GridMetrics {
        column_spacing: COLUMN_SPACING,
        lane_spacing: LANE_SPACING,
        origin_x: CARD_HALF,
        origin_y: CARD_HALF + (lanes.max(1) - 1) as f32 * LANE_SPACING / 2.0,
    }
}

pub(crate) fn chain_stages(chain: &Chain, labels: &IoLabels) -> Vec<ChainStage> {
    let mut stages = vec![ChainStage::Single(endpoint_node(
        INPUT_NODE_ID,
        &labels.input,
        NodeCategory::Input,
        NodeKind::IoInput,
    ))];
    let mut lanes_end_in_outputs = false;
    for block in &chain.blocks {
        let AudioBlockKind::Split(split) = &block.kind else {
            stages.push(ChainStage::Single(blueprint(block)));
            continue;
        };
        let mut a: Vec<BlockBlueprint> = split.a.iter().map(blueprint).collect();
        let mut b: Vec<BlockBlueprint> = split.b.iter().map(blueprint).collect();
        let end = match split.end {
            SplitEnd::Mix => ParallelEnd::Merge,
            SplitEnd::Y => {
                a.push(endpoint_node(
                    PATH_A_OUTPUT_NODE_ID,
                    &labels.path_a,
                    NodeCategory::Output,
                    NodeKind::IoOutput,
                ));
                b.push(endpoint_node(
                    PATH_B_OUTPUT_NODE_ID,
                    &labels.path_b,
                    NodeCategory::Output,
                    NodeKind::IoOutput,
                ));
                lanes_end_in_outputs = true;
                ParallelEnd::Fan
            }
        };
        stages.push(ChainStage::Parallel {
            lanes: vec![a, b],
            end,
        });
    }
    if !lanes_end_in_outputs {
        stages.push(ChainStage::Single(endpoint_node(
            OUTPUT_NODE_ID,
            &labels.output,
            NodeCategory::Output,
            NodeKind::IoOutput,
        )));
    }
    stages
}

pub(crate) fn chain_graph(chain: &Chain, labels: &IoLabels) -> ChainGraph {
    let lanes = if chain
        .blocks
        .iter()
        .any(|b| matches!(b.kind, AudioBlockKind::Split(_)))
    {
        2
    } else {
        1
    };
    let metrics = grid_metrics(lanes);
    let stages = chain_stages(chain, labels);
    let (nodes, edges) = linear_chain_layout(&stages, metrics);
    let anchors = insert_anchors(&stages, &nodes);
    let columns = nodes
        .iter()
        .map(|n| ((n.x - metrics.origin_x) / metrics.column_spacing).round() as usize + 1)
        .max()
        .unwrap_or(1);
    ChainGraph {
        nodes,
        edges,
        anchors,
        lanes,
        columns,
    }
}

fn blueprint(block: &AudioBlock) -> BlockBlueprint {
    let label = match block.model_ref() {
        Some(model) => project::catalog::model_display_name(model.effect_type, model.model),
        None => block.kind.label().to_uppercase(),
    };
    let mut node = BlockBlueprint::new(block.id.0.clone(), label, NodeCategory::Other);
    node.bypass = !block.enabled;
    node
}

fn endpoint_node(id: &str, label: &str, category: NodeCategory, kind: NodeKind) -> BlockBlueprint {
    BlockBlueprint::new(id, label, category).with_kind(kind)
}

#[cfg(test)]
#[path = "chain_graph_adapter_tests.rs"]
mod tests;
```

(`NodeCategory::Other` for blocks: the card paints a block from `node.block` — the block's own accent, Part 5's `BlockTileStyle` — not from a category guess. Blocks keep `NodeKind::Block`, `BlockBlueprint::new`'s default.)

- [ ] **Step 4: Run — expect PASS**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib chain_graph_adapter
```
Expected: 8 passed.

- [ ] **Step 5: Commit**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; git -C "$S" add crates/adapter-gui/src/lib.rs crates/adapter-gui/src/chain_graph_adapter.rs crates/adapter-gui/src/chain_graph_adapter_tests.rs && git -C "$S" commit -m "feat(#328): lay a chain out as input, lanes and outputs"
```
Run the Push gate, push, comment on #328.

---

### Task 7: The row carries its graph; the meter tick keeps it

`ProjectChainItem` gains the graph models. They are built in `replace_project_chains` and never by the meter tick, which clones the row and writes it back (`meter_wiring_poll.rs:196` `row_data`, `:454` `set_row_data`) — the `ModelRc`s ride along as the same `Rc`s. Each block node carries its strip tile in Part 5's `GraphNode.block` (type label, icon, unavailable flag, tooltip data): the card and the canvas tooltip read it, so no parallel tile list is needed.

Slint import cycle first: Part 5's `graph_view_types.slint` imports `ChainBlockItem` from `models.slint`, so `models.slint` cannot import `GraphNode` back. `ChainBlockItem` and the struct it embeds, `BlockParamSummaryEntry` (`models.slint:31-35`, `:46-77`), move verbatim to a new `ui/chain_block_item.slint`; `models.slint` imports and re-exports them, so every existing `import { ChainBlockItem } from "…models.slint"` keeps working and the Rust type names do not change.

**Files:**
- Create: `crates/adapter-gui/ui/chain_block_item.slint`, `crates/adapter-gui/src/chain_graph_models.rs`
- Modify: `crates/adapter-gui/ui/models.slint:31-35,46-77` (moved out), `:1` (imports), `:122-198` (`ProjectChainItem` fields); Part 5's `crates/adapter-gui/ui/components/graph_view_types.slint` (its one `import { ChainBlockItem } from "../models.slint";` line); `crates/adapter-gui/src/project_chains_refresh.rs:26-95` (labels to locals, graph fields; the only full `ProjectChainItem { … }` literal — the three test literals `mcp_query_resolver_tests.rs:140`, `latency_badge_expiry_tests.rs:19`, `gui_live_source_tests.rs:47` end in `..Default::default()` and need no change); `crates/adapter-gui/src/lib.rs` (add `mod chain_graph_models;`)
- Test: `crates/adapter-gui/src/chain_graph_models_tests.rs`, `crates/adapter-gui/src/meter_wiring_poll_tests.rs` (new pin)

**Interfaces:**
- Consumes: Tasks 3, 5, 6; `chain_block_item_from_block(&AudioBlock) -> ChainBlockItem` (`chain_block_item.rs:10`); Part 5 `graph_view_model::{default_palette, NodeKind}`, Slint `GraphNode`, `GraphEdgeGeometry`, `GraphAnchor`.
- Produces: `ProjectChainItem` gains `graph_nodes: [GraphNode], graph_edges: [GraphEdgeGeometry], graph_anchors: [GraphAnchor], graph_lanes: int, graph_columns: int`; Rust `struct RowGraphModels { nodes: ModelRc<GraphNode>, edges: ModelRc<GraphEdgeGeometry>, anchors: ModelRc<GraphAnchor> }`, `row_graph_models(&Chain, &ChainGraph) -> RowGraphModels`.

- [ ] **Step 1: Write the failing tests** `crates/adapter-gui/src/chain_graph_models_tests.rs`:

```rust
//! #328 (spec §5.2) — the graph a row draws: one Slint node per laid-out node,
//! the node's kind, its block tile data, wires that join node centres.

use super::*;
use crate::chain_graph_adapter::chain_graph;
use crate::chain_graph_fixtures_tests::{chain, core, mix_chain, registry, rows};
use crate::chain_graph_ids::{INPUT_NODE_ID, MIXER_NODE_ID, OUTPUT_NODE_ID, SPLIT_NODE_ID};
use crate::endpoint_checklist_items::IoLabels;
use crate::project_view::replace_project_chains;
use project::project::Project;
use slint::Model;

fn labels() -> IoLabels {
    IoLabels { input: "In".into(), output: "Out".into(), path_a: "A".into(), path_b: "B".into() }
}

fn models_of(c: &project::chain::Chain) -> RowGraphModels {
    // Building a tile reads the asset paths (thumbnails), which panic until
    // startup set them (same guard as `block_delete_tests.rs:70-75`).
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    row_graph_models(c, &chain_graph(c, &labels()))
}

#[test]
fn each_node_carries_its_kind_and_its_block_tile() {
    let c = mix_chain();
    let models = models_of(&c);
    let nodes: Vec<GraphNode> = models.nodes.iter().collect();
    let find = |id: &str| nodes.iter().find(|n| n.id.as_str() == id).cloned().unwrap();
    assert_eq!(find(INPUT_NODE_ID).kind.as_str(), "io_input");
    assert_eq!(find(SPLIT_NODE_ID).kind.as_str(), "split");
    assert_eq!(find(MIXER_NODE_ID).kind.as_str(), "mixer");
    assert_eq!(find(OUTPUT_NODE_ID).kind.as_str(), "io_output");
    let a1 = find("a1");
    assert_eq!(a1.kind.as_str(), "block");
    // The card and the canvas tooltip read the strip's own tile (Part 5).
    let tile = crate::chain_block_item::chain_block_item_from_block(&core("a1"));
    assert_eq!(
        (a1.block.icon_kind.as_str(), a1.block.type_label.as_str(), a1.block.display_name.as_str()),
        (tile.icon_kind.as_str(), tile.type_label.as_str(), tile.display_name.as_str()),
    );
    assert_eq!(find(SPLIT_NODE_ID).block.display_name.as_str(), "", "no hover tooltip on routing nodes");
}

#[test]
fn every_plus_is_published_where_part_5_placed_it() {
    let c = mix_chain();
    let graph = chain_graph(&c, &labels());
    let models = models_of(&c);
    let anchors: Vec<crate::GraphAnchor> = models.anchors.iter().collect();
    assert_eq!(anchors.len(), graph.anchors.len());
    for (published, placed) in anchors.iter().zip(&graph.anchors) {
        assert_eq!(published.id.as_str(), placed.id);
        assert_eq!((published.layout_x, published.layout_y), (placed.x, placed.y));
        assert_eq!(published.always_visible, placed.always_visible);
    }
}

#[test]
fn every_wire_joins_its_two_node_centres() {
    let c = mix_chain();
    let models = models_of(&c);
    let nodes: Vec<GraphNode> = models.nodes.iter().collect();
    let centre = |id: &str| {
        let n = nodes.iter().find(|n| n.id.as_str() == id).unwrap();
        (n.layout_x, n.layout_y)
    };
    assert_eq!(models.edges.row_count(), 9);
    for edge in models.edges.iter() {
        assert_eq!((edge.from_x, edge.from_y), centre(edge.from_id.as_str()));
        assert_eq!((edge.to_x, edge.to_y), centre(edge.to_id.as_str()));
    }
}

#[test]
fn a_rebuilt_row_carries_its_chain_graph() {
    let rows = rows();
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain(vec![core("a"), core("b")]), mix_chain()],
        midi: None,
    };
    replace_project_chains(&rows, &project, &[], &[], &registry());
    let linear = rows.row_data(0).unwrap();
    assert_eq!(linear.graph_nodes.row_count(), 4);
    assert_eq!((linear.graph_lanes, linear.graph_columns), (1, 4));
    let split = rows.row_data(1).unwrap();
    assert_eq!(split.graph_lanes, 2);
    // mix_chain: 9 wires, one "+" each.
    assert_eq!(split.graph_anchors.row_count(), 9);
}
```

Append to `meter_wiring_poll_tests.rs` (regression pin — the invariant holds by construction once the fields exist; the red here is the compile error, the pin keeps a future "refresh the graph on every tick" from slipping in):

```rust
/// #328 (spec §5.2): the meter tick writes the row back ~15 times a second;
/// the graph models must ride along untouched. Rebuilding them each tick would
/// drop a hover or a drag in progress and allocate on every tick (#715).
#[test]
fn a_meter_tick_keeps_the_graph_models() {
    let cid = ChainId("chain:0".into());
    let session = session(vec![chain("chain:0", 100.0)]);
    let model = rows(1);
    let nodes = Rc::new(VecModel::from(vec![crate::GraphNode::default()]));
    let mut row = model.row_data(0).unwrap();
    row.graph_nodes = slint::ModelRc::from(nodes.clone());
    model.set_row_data(0, row);

    refresh(&session, &model, &cid, -20.0, -12.0, &FakeReads::default(), &Counters::new());

    let after = model.row_data(0).unwrap();
    let kept = after
        .graph_nodes
        .as_any()
        .downcast_ref::<VecModel<crate::GraphNode>>()
        .expect("still a VecModel");
    assert!(std::ptr::eq(kept, nodes.as_ref()), "the tick replaced the graph model");
}
```

- [ ] **Step 2: Run — compile error** (`no field graph_nodes on ProjectChainItem`, `unresolved import crate::GraphNode`).

- [ ] **Step 3: Break the import cycle (behaviour-preserving move), add the Slint fields, fill them empty, run for the behavioural red.**

Create `crates/adapter-gui/ui/chain_block_item.slint` with the two structs moved verbatim — cut `models.slint:31-35` (`export struct BlockParamSummaryEntry { … }`) and `models.slint:46-77` (`export struct ChainBlockItem { … }`, comments included) and paste them under this header:

```slint
// Responsibility: declares the tile data one chain block shows.
//
// #328: moved verbatim out of models.slint so the graph types
// (components/graph_view_types.slint), which embed a ChainBlockItem in every
// GraphNode, can be imported by models.slint without an import cycle.
// models.slint re-exports both structs; every existing import keeps working.
```

In `models.slint`, replace line 1 (`// Responsibility: declares the data shapes the UI binds to.`) with:

```slint
// Responsibility: declares the data shapes the UI binds to.
import { BlockParamSummaryEntry, ChainBlockItem } from "chain_block_item.slint";
import { GraphNode, GraphEdgeGeometry, GraphAnchor } from "components/graph_view_types.slint";
export { BlockParamSummaryEntry, ChainBlockItem }
```

In Part 5's `crates/adapter-gui/ui/components/graph_view_types.slint`, change its one line `import { ChainBlockItem } from "../models.slint";` to `import { ChainBlockItem } from "../chain_block_item.slint";`.

Inside `ProjectChainItem`, after `di_output_selected_index: int,` (line 197 before the move; find it by text):

```slint
    /// #328: the chain drawn as a graph on desktop rows (spec §5.2). Built once
    /// per row rebuild by `chain_graph_models.rs`; the meter tick keeps them.
    /// Each block node carries its strip tile in `GraphNode.block`.
    graph_nodes: [GraphNode],
    graph_edges: [GraphEdgeGeometry],
    /// The "+" on every wire (`graph_view_model::insert_anchors`).
    graph_anchors: [GraphAnchor],
    /// 1 for a linear chain, 2 with a split: the row height follows it.
    graph_lanes: int,
    /// Columns the graph spans: the row fits the whole chain on first paint.
    graph_columns: int,
```

`models.slint` goes from 467 to 467 − 37 (moved) + 3 (imports) + 12 (fields) ≈ 445 lines.

In `project_chains_refresh.rs`, inside the `ProjectChainItem { … }` literal after `looper_preset_options: ModelRc::default(),` (line 260) add the empty values:

```rust
                graph_nodes: ModelRc::default(),
                graph_edges: ModelRc::default(),
                graph_anchors: ModelRc::default(),
                graph_lanes: 0,
                graph_columns: 0,
```

Create the stub `crates/adapter-gui/src/chain_graph_models.rs` so its test module compiles — the header, the `use` lines and `RowGraphModels` of Step 4, `row_graph_models` returning `RowGraphModels { nodes: ModelRc::default(), edges: ModelRc::default(), anchors: ModelRc::default() }`, and the `#[cfg(test)] #[path = "chain_graph_models_tests.rs"] mod tests;` footer — and add `mod chain_graph_models;` to `lib.rs`.

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib chain_graph_models && nice -n 19 cargo test -p adapter-gui -j 2 --lib a_meter_tick_keeps_the_graph_models
```
Expected: `a_rebuilt_row_carries_its_chain_graph` fails `assertion left == right failed left: 0 right: 4`; `every_wire_joins_its_two_node_centres` `left: 0 right: 9`; `every_plus_is_published_where_part_5_placed_it` `left: 0 right: 9`; `each_node_carries_its_kind_and_its_block_tile` panics on `unwrap()` of the missing input node. (`a_meter_tick_keeps_the_graph_models` already passes here — it is the pin.)

- [ ] **Step 4: Implement** `crates/adapter-gui/src/chain_graph_models.rs`:

```rust
//! Responsibility: publishes a chain graph as the row's Slint models.
//!
//! #328 (spec §5.2). Built once per row rebuild (`project_chains_refresh.rs`).
//! The meter tick clones the row and writes it back
//! (`meter_wiring_poll.rs:196,454`); these models ride along as the same
//! `Rc`s, so a tick never rebuilds a graph. A block node carries its strip
//! tile in `GraphNode.block`, which Part 5's card and tooltip draw.

use std::rc::Rc;

use slint::{Color, ModelRc, VecModel};

use project::chain::Chain;

use crate::chain_block_item::chain_block_item_from_block;
use crate::chain_block_lists::block_at;
use crate::chain_graph_adapter::ChainGraph;
use crate::chain_graph_ids::{resolve_node, NodeRef};
use crate::graph_view_model::{default_palette, GraphNode as LaidOutNode, NodeCategory, NodeKind};
use crate::{ChainBlockItem, GraphAnchor, GraphEdgeGeometry, GraphNode};

pub(crate) struct RowGraphModels {
    pub(crate) nodes: ModelRc<GraphNode>,
    pub(crate) edges: ModelRc<GraphEdgeGeometry>,
    pub(crate) anchors: ModelRc<GraphAnchor>,
}

pub(crate) fn row_graph_models(chain: &Chain, graph: &ChainGraph) -> RowGraphModels {
    let nodes: Vec<GraphNode> = graph
        .nodes
        .iter()
        .map(|node| slint_node(chain, node))
        .collect();
    let anchors: Vec<GraphAnchor> = graph
        .anchors
        .iter()
        .map(|anchor| GraphAnchor {
            id: anchor.id.as_str().into(),
            layout_x: anchor.x,
            layout_y: anchor.y,
            always_visible: anchor.always_visible,
        })
        .collect();
    RowGraphModels {
        nodes: ModelRc::from(Rc::new(VecModel::from(nodes))),
        edges: ModelRc::from(Rc::new(VecModel::from(edge_geometry(graph)))),
        anchors: ModelRc::from(Rc::new(VecModel::from(anchors))),
    }
}

/// The strip's tile for a block node.
fn block_tile(chain: &Chain, node_id: &str) -> ChainBlockItem {
    match resolve_node(chain, node_id) {
        Some(NodeRef::Block { path, index, .. }) => block_at(chain, index, path.as_ref())
            .map(chain_block_item_from_block)
            .unwrap_or_default(),
        _ => ChainBlockItem::default(),
    }
}

fn slint_node(chain: &Chain, node: &LaidOutNode) -> GraphNode {
    let (fill, border) = category_colours(node.category);
    GraphNode {
        id: node.id.as_str().into(),
        label: node.label.as_str().into(),
        category: node.category.as_str().into(),
        fill,
        border,
        layout_x: node.x,
        layout_y: node.y,
        bypass: node.bypass,
        kind: node.kind.as_str().into(),
        // An empty tile on I/O, split and mixer nodes: no tooltip there.
        block: if node.kind == NodeKind::Block {
            block_tile(chain, &node.id)
        } else {
            ChainBlockItem::default()
        },
        ..Default::default()
    }
}

/// Every node takes the GraphView palette — its single source of truth
/// (`graph_view_model/palette.rs`).
fn category_colours(category: NodeCategory) -> (Color, Color) {
    default_palette()
        .into_iter()
        .find(|style| style.category == category.as_str())
        .map(|style| (rgb(style.fill), rgb(style.border)))
        .unwrap_or_default()
}

fn rgb(value: u32) -> Color {
    Color::from_rgb_u8((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

fn edge_geometry(graph: &ChainGraph) -> Vec<GraphEdgeGeometry> {
    let centre = |id: &str| graph.nodes.iter().find(|n| n.id == id).map(|n| (n.x, n.y));
    graph
        .edges
        .iter()
        .filter_map(|edge| {
            let (from_x, from_y) = centre(&edge.from_id)?;
            let (to_x, to_y) = centre(&edge.to_id)?;
            Some(GraphEdgeGeometry {
                from_id: edge.from_id.as_str().into(),
                to_id: edge.to_id.as_str().into(),
                from_x,
                from_y,
                to_x,
                to_y,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "chain_graph_models_tests.rs"]
mod tests;
```

In `project_chains_refresh.rs`, move the `input_label` block (lines 59-74) and the `output_label` block (lines 77-92) out of the literal into locals right before `ProjectChainItem {` (line 31), unchanged except for the binding:

```rust
            let input_label: SharedString = {
                let binding_name = chain_io_chip_label_from_bindings(chain, io_bindings, true);
                if binding_name.is_empty() {
                    // #716: device endpoints resolve from the binding
                    // registry (never from block `entries`).
                    let (resolved_inputs, _) =
                        engine::runtime_endpoints::resolve_chain_io(chain, io_bindings);
                    let input_chs: Vec<usize> = resolved_inputs
                        .iter()
                        .flat_map(|e| e.channels.iter().copied())
                        .collect();
                    chain_endpoint_label("In", &input_chs).into()
                } else {
                    binding_name.into()
                }
            };
            let output_label: SharedString = {
                let binding_name = chain_io_chip_label_from_bindings(chain, io_bindings, false);
                if binding_name.is_empty() {
                    // #716: device endpoints resolve from the binding
                    // registry (never from block `entries`).
                    let (_, resolved_outputs) =
                        engine::runtime_endpoints::resolve_chain_io(chain, io_bindings);
                    let output_chs: Vec<usize> = resolved_outputs
                        .iter()
                        .flat_map(|e| e.channels.iter().copied())
                        .collect();
                    chain_endpoint_label("Out", &output_chs).into()
                } else {
                    binding_name.into()
                }
            };
            // #328: the chain drawn as a graph for the desktop row (spec §5.2).
            let io_labels = crate::endpoint_checklist_items::io_labels(
                chain,
                io_bindings,
                &input_label,
                &output_label,
            );
            let graph = crate::chain_graph_adapter::chain_graph(chain, &io_labels);
            let graph_models = crate::chain_graph_models::row_graph_models(chain, &graph);
```

In the literal: `input_label,` and `output_label,` replace the two moved blocks, and the five empty values from Step 3 become:

```rust
                graph_nodes: graph_models.nodes,
                graph_edges: graph_models.edges,
                graph_anchors: graph_models.anchors,
                graph_lanes: graph.lanes as i32,
                graph_columns: graph.columns as i32,
```

(`mod chain_graph_models;` is already in `lib.rs` from Step 3.)

- [ ] **Step 5: Run — expect PASS**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib chain_graph_models && nice -n 19 cargo test -p adapter-gui -j 2 --lib meter_wiring_poll
```
Expected: 4 + all meter tests pass, including `a_meter_tick_keeps_the_graph_models`. Then the whole crate (`nice -n 19 cargo test -p adapter-gui -j 2`): every test that imports `ChainBlockItem` / `BlockParamSummaryEntry` still compiles through the `models.slint` re-export, and Part 5's `issue_328_graph_view_*` tests still pass with the moved import.

- [ ] **Step 6: Commit and push**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; git -C "$S" add crates/adapter-gui/src/lib.rs crates/adapter-gui/src/chain_graph_models.rs crates/adapter-gui/src/chain_graph_models_tests.rs crates/adapter-gui/src/meter_wiring_poll_tests.rs crates/adapter-gui/src/project_chains_refresh.rs crates/adapter-gui/ui/models.slint crates/adapter-gui/ui/chain_block_item.slint crates/adapter-gui/ui/components/graph_view_types.slint && git -C "$S" commit -m "feat(#328): each chain row carries its graph models"
```
`mod chain_graph_models;` carries no dead-code attribute — its caller is in this task. `chain_graph_adapter` (every item now has a production caller) makes `cargo build` print `this lint expectation is unfulfilled`: delete its `cfg_attr(not(test), expect(dead_code))` line. Run the Push gate, push, comment on #328.

### Task 8: ChainRow draws the graph on desktop

Required before the first `.slint` line: invoke `claude-plugin:ux-ui`, `slint:slint` and `slint-best-practices` (Global Constraints).

The graph replaces `ChainRowBlocks` inside `ChainRow` on desktop (spec §5.4). Touch keeps the wrapped strip. The row height follows the lane count. Gestures go to a new global, `ChainGraphBridge`, instead of being drilled through `ProjectChainsPage` → `DesktopMain`/`TouchMain` → `AppWindow`. The strip's latency badge is extracted so the graph row keeps it.

**Files:**
- Create: `crates/adapter-gui/ui/chain_graph_globals.slint`, `crates/adapter-gui/ui/pages/chain_row_graph.slint`, `crates/adapter-gui/ui/components/chain_latency_badge.slint`, `crates/adapter-gui/ui/components/chain_row_graph_test_harness.slint`
- Modify: `crates/adapter-gui/ui/pages/chain_row.slint:8-19` (imports), `:65-68` (new property), `:100` (height), `:133-154` (strip visibility, graph instance); `crates/adapter-gui/ui/pages/chain_row_blocks.slint:304-348` (badge → component); `crates/adapter-gui/ui/app-window.slint` (import + export `ChainGraphBridge`, the harness); `crates/adapter-gui/src/desktop_app_init.rs:72`; `crates/adapter-gui/src/lib.rs` (test module)
- Test: `crates/adapter-gui/tests/issue_328_chain_row_graph_source.rs`, `crates/adapter-gui/src/issue_328_graph_row_interaction_tests.rs`; the four `tests/chain_row_*.rs` stay unchanged and green

**Interfaces:**
- Consumes: Task 7 `ProjectChainItem.graph_*`; Part 5 `GraphView` (`nodes`, `edges`, `anchors`, `zoom`, `node_width`, `node_height`, `show_grid`, `background_color`, callbacks `node_clicked`, `node_dragged`, `add-requested`, `remove-requested`, `bypass-toggled`, `node-dropped`, `pure callback resolve-drop-anchor`). Slint treats `-` and `_` in names as the same identifier, so `node-clicked` and `node_clicked` name one callback.
- Produces: Slint `global ChainGraphBridge { in-out graph-enabled: bool; callback node-clicked(int, string); add-requested(int, string); remove-requested(int, string); bypass-toggled(int, string); node-dragged(int, string, length, length); node-dropped(int, string, string); pure callback resolve-drop-anchor(int, string, length, length) -> string; open-path-block(int, string, int, int); start-path-insert(int, string, int, int); in-out selected-chain-index: int; selected-block-id: string; neighbor-block-id: string }`; `component ChainRowGraph { in nodes, edges, anchors, columns, chain-index, latency-ms }`; `component ChainLatencyBadge { in latency-ms: float }`; harness windows `ChainRowGraphLinearHarness`, `ChainRowGraphSplitMixHarness` (exported to Rust), `ChainRowGraphYHarness`.

- [ ] **Step 1: Write the failing tests.** `crates/adapter-gui/tests/issue_328_chain_row_graph_source.rs`:

```rust
//! #328 (spec §5.4) — the desktop chain row draws the graph, the touch row
//! keeps the strip, and the row height follows the graph's lane count.

use std::path::PathBuf;

fn read(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn the_desktop_row_draws_the_graph_while_touch_keeps_the_strip() {
    let row = read("ui/pages/chain_row.slint");
    assert!(row.contains("if ChainGraphBridge.graph-enabled : ChainRowGraph {"));
    assert!(row.contains("visible: !ChainGraphBridge.graph-enabled;"), "the strip stays for touch");
}

#[test]
fn the_row_height_follows_the_lane_count_in_graph_mode() {
    let row = read("ui/pages/chain_row.slint");
    assert!(row.contains("root.chain.graph_lanes"));
    let at = row.find("height: 106px").expect("ChainRow height formula");
    assert!(row[at..at + 80].contains("content-row-count"), "height must use content-row-count");
}

#[test]
fn startup_draws_graphs_on_desktop_only() {
    let init = read("src/desktop_app_init.rs");
    assert!(init.contains("set_graph_enabled(!context.capabilities.touch_optimized)"));
}

#[test]
fn the_graph_row_keeps_the_latency_badge() {
    assert!(read("ui/pages/chain_row_graph.slint").contains("ChainLatencyBadge {"));
    assert!(read("ui/pages/chain_row_blocks.slint").contains("ChainLatencyBadge {"));
}
```

`crates/adapter-gui/src/issue_328_graph_row_interaction_tests.rs` (in-crate: it reaches the crate-private resolvers in later tasks):

```rust
//! #328 — real pointer events on a chain graph row (i-slint-backend-testing).
//! A render proves layout only; these prove the clicks land (#749/#761).
//! Node centres: the harness puts ChainRowGraph at (18, 20) at zoom 1, so a
//! node's window position is (18 + layout_x, 20 + layout_y).

use std::cell::RefCell;
use std::rc::Rc;

use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition};

use crate::{ChainGraphBridge, ChainRowGraphSplitMixHarness};

pub(crate) fn at(layout_x: f32, layout_y: f32) -> LogicalPosition {
    LogicalPosition::new(18.0 + layout_x, 20.0 + layout_y)
}

pub(crate) fn click_at(w: &impl ComponentHandle, p: LogicalPosition) {
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: p });
    win.dispatch_event(WindowEvent::PointerPressed { position: p, button: PointerEventButton::Left });
    win.dispatch_event(WindowEvent::PointerReleased { position: p, button: PointerEventButton::Left });
    win.dispatch_event(WindowEvent::PointerExited);
}

#[test]
fn clicking_a_lane_card_reports_its_row_and_node() {
    i_slint_backend_testing::init_no_event_loop();
    let h = ChainRowGraphSplitMixHarness::new().unwrap();
    let clicked: Rc<RefCell<Option<(i32, String)>>> = Rc::new(RefCell::new(None));
    let seen = clicked.clone();
    ChainGraphBridge::get(&h).on_node_clicked(move |ci, id| *seen.borrow_mut() = Some((ci, id.to_string())));
    h.show().unwrap();

    click_at(&h, at(446.0, 50.0)); // a1, lane A

    assert_eq!(*clicked.borrow(), Some((0, "a1".to_string())));
}

/// The row hands each node's strip tile to Part 5's canvas, which draws the
/// #333 tooltip for a block with a model and never for a routing node.
#[test]
fn hovering_a_block_card_shows_its_tooltip_and_a_routing_node_does_not() {
    i_slint_backend_testing::init_no_event_loop();
    let h = ChainRowGraphSplitMixHarness::new().unwrap();
    h.show().unwrap();
    let tooltips = || i_slint_backend_testing::ElementHandle::find_by_element_type_name(&h, "BlockHoverTooltip").count();

    h.window().dispatch_event(WindowEvent::PointerMoved { position: at(446.0, 50.0) }); // a1
    assert_eq!(tooltips(), 1, "a block card shows the #333 tooltip");

    h.window().dispatch_event(WindowEvent::PointerMoved { position: at(314.0, 104.0) }); // split node
    assert_eq!(tooltips(), 0, "the split node has no model to describe");
}
```

Register it in `lib.rs` next to the other `#[cfg(test)]` issue modules (line ~214):

```rust
#[cfg(test)]
mod issue_328_graph_row_interaction_tests;
```

- [ ] **Step 2: Run — expect FAIL**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_chain_row_graph_source
```
Expected: 4 failures (`assertion failed: row.contains("if ChainGraphBridge.graph-enabled : ChainRowGraph {")`, `read …chain_row_graph.slint: No such file`). The in-crate test fails to compile (`no ChainRowGraphSplitMixHarness in the root`) — that is its red until Step 3.

- [ ] **Step 3: The global.** `crates/adapter-gui/ui/chain_graph_globals.slint`:

```slint
// Responsibility: carries the chain graph row's gestures to Rust.
//
// #328. Every chain row's graph reports a gesture here, tagged with the row's
// chain index, so no callback is drilled through ProjectChainsPage →
// DesktopMain/TouchMain → AppWindow (app-window.slint lives at its line cap).
// Mirrors `OverlayBridge` (#873).

export global ChainGraphBridge {
    // Desktop draws every chain as a graph; touch keeps the pedal strip with a
    // single Split chip (spec §5.4). Set once at startup from the capabilities.
    in-out property <bool> graph-enabled: true;

    callback node-clicked(int, string);
    callback add-requested(int, string);
    callback remove-requested(int, string);
    callback bypass-toggled(int, string);
    // GraphView asks its host to move the dragged card (Part 5: the card
    // follows the pointer only through the host's model).
    callback node-dragged(int, string, length, length);
    callback node-dropped(int, string, string);
    // Part 5's `resolve-drop-anchor`, answered in Rust by
    // `graph_view_model::resolve_drop_anchor` ("" = no anchor in reach).
    // Args: chain index, dragged node id, layout x, layout y.
    pure callback resolve-drop-anchor(int, string, length, length) -> string;

    // Rust → the block flows: open, or insert into, a split path.
    // Args: chain index, split block id, side (0 = A, 1 = B), index in the path.
    callback open-path-block(int, string, int, int);
    callback start-path-insert(int, string, int, int);

    // #591 selection markers for the graph cards, set with the strip's.
    in-out property <int> selected-chain-index: -1;
    in-out property <string> selected-block-id;
    in-out property <string> neighbor-block-id;
}
```

- [ ] **Step 4: Extract the latency badge** (behaviour-preserving). `crates/adapter-gui/ui/components/chain_latency_badge.slint` — the body of `chain_row_blocks.slint:304-348` minus its `x`/`y`:

```slint
// Responsibility: shows a chain's probed latency.
//
// #328: extracted verbatim from chain_row_blocks.slint so the graph row shows
// the same badge. < 10 ms green, < 20 ms yellow, else red; #954: the badge
// follows its two labels.

export component ChainLatencyBadge inherits Rectangle {
    in property <float> latency-ms;

    width: lat-lbl.preferred-width + lat-ms.preferred-width + 14px;
    height: 22px;
    border-radius: 4px;
    background: root.latency-ms < 10.0 ? #4ade8018
              : root.latency-ms < 20.0 ? #facc1518
              : #f8717118;
    border-width: 1px;
    border-color: root.latency-ms < 10.0 ? #4ade8055
                : root.latency-ms < 20.0 ? #facc1555
                : #f8717155;

    lat-lbl := Text {
        x: 5px;
        y: 0px;
        width: self.preferred-width;
        height: parent.height;
        text: @tr("label-lat");
        color: root.latency-ms < 10.0 ? #4ade8099
             : root.latency-ms < 20.0 ? #facc1599
             : #f8717199;
        font-size: 18px;
        font-weight: 700;
        horizontal-alignment: right;
        vertical-alignment: center;
    }

    lat-ms := Text {
        x: lat-lbl.x + lat-lbl.width + 4px;
        y: 0px;
        width: self.preferred-width;
        height: parent.height;
        text: Math.round(root.latency-ms) + "ms";
        color: root.latency-ms < 10.0 ? #4ade80
             : root.latency-ms < 20.0 ? #facc15
             : #f87171;
        font-size: 18px;
        font-weight: 600;
        horizontal-alignment: left;
        vertical-alignment: center;
    }
}
```

In `chain_row_blocks.slint` add `import { ChainLatencyBadge } from "../components/chain_latency_badge.slint";` after line 12 and replace lines 304-348 (`if root.chain.latency_ms > 0 : Rectangle { … }`) with:

```slint
    if root.chain.latency_ms > 0 : ChainLatencyBadge {
        x: min(root.out-chip-x + 4px, parent.width - self.width);
        y: root.out-chip-y - 12px;
        latency-ms: root.chain.latency_ms;
    }
```

- [ ] **Step 5: The graph row.** `crates/adapter-gui/ui/pages/chain_row_graph.slint`:

```slint
// Responsibility: renders one chain card's signal graph.
//
// #328 (spec §5.1, §5.4): the graph that replaces the pedal strip on desktop.
// Nodes (each block node carries its strip tile), wires and "+" anchors come
// from Rust (`chain_graph_models.rs`); every gesture goes to
// `ChainGraphBridge` tagged with this row's chain index. The wheel is not
// captured here, so the chains list keeps scrolling (GraphView zooms on
// Cmd/Ctrl + wheel, Part 5).

import { GraphView, GraphNode, GraphEdgeGeometry, GraphAnchor } from "../components/graph_view.slint";
import { ChainLatencyBadge } from "../components/chain_latency_badge.slint";
import { ChainGraphBridge } from "../chain_graph_globals.slint";

export component ChainRowGraph inherits Rectangle {
    in property <[GraphNode]> nodes;
    in property <[GraphEdgeGeometry]> edges;
    in property <[GraphAnchor]> anchors;
    in property <int> columns: 1;
    in property <int> chain-index;
    in property <float> latency-ms: 0;

    background: transparent;

    graph := GraphView {
        x: 0px;
        y: 0px;
        width: parent.width;
        height: parent.height;
        nodes: root.nodes;
        edges: root.edges;
        anchors: root.anchors;
        node_width: 100px;
        node_height: 100px;
        show_grid: false;
        background_color: transparent;
        // First paint fits the whole chain (never above 1:1, never below the
        // GraphView floor); Cmd/Ctrl + wheel takes over from there.
        zoom: Math.max(0.3, Math.min(1.0, root.width / (Math.max(1, root.columns) * 132px)));
        node-clicked(id) => { ChainGraphBridge.node-clicked(root.chain-index, id); }
        add-requested(anchor) => { ChainGraphBridge.add-requested(root.chain-index, anchor); }
        remove-requested(id) => { ChainGraphBridge.remove-requested(root.chain-index, id); }
        bypass-toggled(id) => { ChainGraphBridge.bypass-toggled(root.chain-index, id); }
        node-dragged(id, x, y) => { ChainGraphBridge.node-dragged(root.chain-index, id, x, y); }
        node-dropped(id, anchor) => { ChainGraphBridge.node-dropped(root.chain-index, id, anchor); }
        resolve-drop-anchor(id, x, y) => { ChainGraphBridge.resolve-drop-anchor(root.chain-index, id, x, y) }
    }

    if root.latency-ms > 0 : ChainLatencyBadge {
        x: parent.width - self.width;
        y: 0px;
        latency-ms: root.latency-ms;
    }
}
```

- [ ] **Step 6: ChainRow.** In `chain_row.slint` add after line 17:

```slint
import { ChainRowGraph } from "chain_row_graph.slint";
import { ChainGraphBridge } from "../chain_graph_globals.slint";
```

After line 68 (`property <int> last-block-row: …`):

```slint
    // #328 (spec §5.4): desktop draws the chain as a graph — one lane for a
    // linear chain, two with a split; touch keeps the wrapped pedal strip.
    property <int> content-row-count: ChainGraphBridge.graph-enabled
        ? Math.max(1, root.chain.graph_lanes)
        : root.block-row-count;
```

Line 100: `height: 106px + (root.block-row-count * 108px)` → `height: 106px + (root.content-row-count * 108px)` (the rest of the formula is unchanged, so `chain_row_height_grows_with_streams` still finds `stream_meters.length` within 200 chars).

In `blocks := ChainRowBlocks { … }` (line 133) add as its first property `visible: !ChainGraphBridge.graph-enabled;`. After its closing `}` (line 154) add:

```slint
    if ChainGraphBridge.graph-enabled : ChainRowGraph {
        x: 18px;
        y: 62px;
        width: parent.width - 36px;
        height: root.content-row-count * 108px;
        nodes: root.chain.graph_nodes;
        edges: root.chain.graph_edges;
        anchors: root.chain.graph_anchors;
        columns: root.chain.graph_columns;
        chain-index: root.chain-index;
        latency-ms: root.chain.latency_ms;
    }
```

- [ ] **Step 7: Harness.** `crates/adapter-gui/ui/components/chain_row_graph_test_harness.slint` — positions are what `chain_graph_adapter.rs` lays out (132 px columns, 108 px lanes, first centre 50 px; two lanes centre the shared lane at 104 px):

```slint
// Responsibility: hosts chain graph rows for render checks.
//
// #328 TEST-ONLY: three ChainRowGraph rows with fixed data — a linear chain,
// a Split → Mix and a Y → A/B — for `tools/slint-render` PNGs and the headless
// interaction tests. Imported by app-window.slint only so slint-build emits
// the Rust type of the Split → Mix row; never shown in the running app.

import { ChainRowGraph } from "../pages/chain_row_graph.slint";
import "../fonts/BebasNeue.ttf";

export component ChainRowGraphLinearHarness inherits Window {
    width: 760px;
    height: 150px;
    background: #040708;
    ChainRowGraph {
        x: 18px;
        y: 20px;
        width: 724px;
        height: 108px;
        columns: 5;
        chain-index: 0;
        nodes: [
            { id: "__io_input", label: "In 1, In 2", category: "input", kind: "io_input", fill: #1e8aff, border: #1359a5, layout_x: 50px, layout_y: 50px },
            { id: "od", label: "TS9", category: "other", kind: "block", fill: #8a94a2, border: #5a6069, layout_x: 182px, layout_y: 50px,
              block: { kind: "gain", icon_kind: "gain", type_label: "GAIN", label: "ts9", enabled: true, accent_color: #d04a3a, display_name: "TS9", backend_label: "NATIVE" } },
            { id: "amp", label: "Plexi", category: "other", kind: "block", fill: #8a94a2, border: #5a6069, layout_x: 314px, layout_y: 50px,
              block: { kind: "amp", icon_kind: "amp", type_label: "AMP", label: "plexi", enabled: true, accent_color: #4a7fd0, display_name: "Plexi", backend_label: "NAM" } },
            { id: "rev", label: "Hall", category: "other", kind: "block", fill: #8a94a2, border: #5a6069, layout_x: 446px, layout_y: 50px, bypass: true,
              block: { kind: "reverb", icon_kind: "reverb", type_label: "REVERB", label: "hall", enabled: false, accent_color: #3aa86a, display_name: "Hall", backend_label: "NATIVE" } },
            { id: "__io_output", label: "Main L/R", category: "output", kind: "io_output", fill: #ff8a1e, border: #a55913, layout_x: 578px, layout_y: 50px },
        ];
        edges: [
            { from_id: "__io_input", to_id: "od", from_x: 50px, from_y: 50px, to_x: 182px, to_y: 50px },
            { from_id: "od", to_id: "amp", from_x: 182px, from_y: 50px, to_x: 314px, to_y: 50px },
            { from_id: "amp", to_id: "rev", from_x: 314px, from_y: 50px, to_x: 446px, to_y: 50px },
            { from_id: "rev", to_id: "__io_output", from_x: 446px, from_y: 50px, to_x: 578px, to_y: 50px },
        ];
    }
}

export component ChainRowGraphSplitMixHarness inherits Window {
    width: 980px;
    height: 260px;
    background: #040708;
    ChainRowGraph {
        x: 18px;
        y: 20px;
        width: 944px;
        height: 216px;
        columns: 7;
        chain-index: 0;
        nodes: [
            { id: "__io_input", label: "In 1", category: "input", kind: "io_input", fill: #1e8aff, border: #1359a5, layout_x: 50px, layout_y: 104px },
            { id: "od", label: "TS9", category: "other", kind: "block", fill: #8a94a2, border: #5a6069, layout_x: 182px, layout_y: 104px,
              block: { kind: "gain", icon_kind: "gain", type_label: "GAIN", label: "ts9", enabled: true, accent_color: #d04a3a, display_name: "TS9", backend_label: "NATIVE" } },
            { id: "__split_1", label: "", category: "util", kind: "split", fill: #6a7483, border: #454b55, layout_x: 314px, layout_y: 104px },
            { id: "a1", label: "Plexi", category: "other", kind: "block", fill: #8a94a2, border: #5a6069, layout_x: 446px, layout_y: 50px,
              block: { kind: "amp", icon_kind: "amp", type_label: "AMP", label: "plexi", enabled: true, accent_color: #4a7fd0, display_name: "Plexi", backend_label: "NAM" } },
            { id: "b1", label: "AC30", category: "other", kind: "block", fill: #8a94a2, border: #5a6069, layout_x: 446px, layout_y: 158px,
              block: { kind: "amp", icon_kind: "amp", type_label: "AMP", label: "ac30", enabled: true, unavailable: true, accent_color: #4a7fd0, display_name: "AC30", backend_label: "NAM" } },
            { id: "__merge_1", label: "", category: "util", kind: "mixer", fill: #6a7483, border: #454b55, layout_x: 578px, layout_y: 104px },
            { id: "rev", label: "Hall", category: "other", kind: "block", fill: #8a94a2, border: #5a6069, layout_x: 710px, layout_y: 104px,
              block: { kind: "reverb", icon_kind: "reverb", type_label: "REVERB", label: "hall", enabled: true, accent_color: #3aa86a, display_name: "Hall", backend_label: "NATIVE" } },
            { id: "__io_output", label: "Main L/R", category: "output", kind: "io_output", fill: #ff8a1e, border: #a55913, layout_x: 842px, layout_y: 104px },
        ];
        // What `insert_anchors` places on this chain (`od → split(A: a1 | B: b1) → rev`):
        // stage 0 input · 1 od · 2 split · 3 rev · 4 output; one "+" per wire midpoint.
        anchors: [
            { id: "stage:1", layout_x: 116px, layout_y: 104px },
            { id: "stage:2", layout_x: 248px, layout_y: 104px },
            { id: "lane:2:0:0", layout_x: 380px, layout_y: 77px },
            { id: "lane:2:0:1", layout_x: 512px, layout_y: 77px },
            { id: "lane:2:1:0", layout_x: 380px, layout_y: 131px },
            { id: "lane:2:1:1", layout_x: 512px, layout_y: 131px },
            { id: "stage:3", layout_x: 644px, layout_y: 104px },
            { id: "stage:4", layout_x: 776px, layout_y: 104px },
        ];
        edges: [
            { from_id: "__io_input", to_id: "od", from_x: 50px, from_y: 104px, to_x: 182px, to_y: 104px },
            { from_id: "od", to_id: "__split_1", from_x: 182px, from_y: 104px, to_x: 314px, to_y: 104px },
            { from_id: "__split_1", to_id: "a1", from_x: 314px, from_y: 104px, to_x: 446px, to_y: 50px },
            { from_id: "a1", to_id: "__merge_1", from_x: 446px, from_y: 50px, to_x: 578px, to_y: 104px },
            { from_id: "__split_1", to_id: "b1", from_x: 314px, from_y: 104px, to_x: 446px, to_y: 158px },
            { from_id: "b1", to_id: "__merge_1", from_x: 446px, from_y: 158px, to_x: 578px, to_y: 104px },
            { from_id: "__merge_1", to_id: "rev", from_x: 578px, from_y: 104px, to_x: 710px, to_y: 104px },
            { from_id: "rev", to_id: "__io_output", from_x: 710px, from_y: 104px, to_x: 842px, to_y: 104px },
        ];
    }
}

export component ChainRowGraphYHarness inherits Window {
    width: 620px;
    height: 260px;
    background: #040708;
    ChainRowGraph {
        x: 18px;
        y: 20px;
        width: 584px;
        height: 216px;
        columns: 4;
        chain-index: 0;
        nodes: [
            { id: "__io_input", label: "In 1", category: "input", kind: "io_input", fill: #1e8aff, border: #1359a5, layout_x: 50px, layout_y: 104px },
            { id: "__split_1", label: "", category: "util", kind: "split", fill: #6a7483, border: #454b55, layout_x: 182px, layout_y: 104px },
            { id: "a1", label: "Plexi", category: "other", kind: "block", fill: #8a94a2, border: #5a6069, layout_x: 314px, layout_y: 50px,
              block: { kind: "amp", icon_kind: "amp", type_label: "AMP", label: "plexi", enabled: true, accent_color: #4a7fd0, display_name: "Plexi", backend_label: "NAM" } },
            { id: "__io_output_a", label: "Out 1", category: "output", kind: "io_output", fill: #ff8a1e, border: #a55913, layout_x: 446px, layout_y: 50px },
            { id: "b1", label: "AC30", category: "other", kind: "block", fill: #8a94a2, border: #5a6069, layout_x: 314px, layout_y: 158px,
              block: { kind: "amp", icon_kind: "amp", type_label: "AMP", label: "ac30", enabled: true, accent_color: #4a7fd0, display_name: "AC30", backend_label: "NAM" } },
            { id: "__io_output_b", label: "Out 2", category: "output", kind: "io_output", fill: #ff8a1e, border: #a55913, layout_x: 446px, layout_y: 158px },
        ];
        edges: [
            { from_id: "__io_input", to_id: "__split_1", from_x: 50px, from_y: 104px, to_x: 182px, to_y: 104px },
            { from_id: "__split_1", to_id: "a1", from_x: 182px, from_y: 104px, to_x: 314px, to_y: 50px },
            { from_id: "a1", to_id: "__io_output_a", from_x: 314px, from_y: 50px, to_x: 446px, to_y: 50px },
            { from_id: "__split_1", to_id: "b1", from_x: 182px, from_y: 104px, to_x: 314px, to_y: 158px },
            { from_id: "b1", to_id: "__io_output_b", from_x: 314px, from_y: 158px, to_x: 446px, to_y: 158px },
        ];
    }
}
```

- [ ] **Step 8: Wire it into the app.** `app-window.slint` (line numbers moved with Part 5 and Task 1 — find each line by its text): after line 3 (`import { ChainEditorBridge } from "chain_editor_globals.slint";`) add `import { ChainGraphBridge } from "chain_graph_globals.slint";`; append `, ChainGraphBridge` to `export { SettingsBridge, BlockEditorBridge, OverlayBridge, AnalyzerBridge, ChainEditorBridge }` so it reads `export { SettingsBridge, BlockEditorBridge, OverlayBridge, AnalyzerBridge, ChainEditorBridge, ChainGraphBridge }`; after Part 5's `import { GraphViewHarness, GraphViewScrollHarness } from "components/graph_view_test_harness.slint";` add `import { ChainRowGraphSplitMixHarness } from "components/chain_row_graph_test_harness.slint";` and append `, ChainRowGraphSplitMixHarness` to the harness `export { DiLoopHarness, … }` line right below it.

`desktop_app_init.rs` after line 72 (`window.set_touch_optimized(…)`):

```rust
    // #328 (spec §5.4): desktop rows draw the chain graph; touch keeps the strip.
    crate::ChainGraphBridge::get(window).set_graph_enabled(!context.capabilities.touch_optimized);
```

- [ ] **Step 9: Run — expect PASS**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_chain_row_graph_source && nice -n 19 cargo test -p adapter-gui -j 2 --lib issue_328_graph_row_interaction && nice -n 19 cargo test -p adapter-gui -j 2 --test chain_row_height_grows_with_streams --test chain_row_meters_stack_upward --test chain_row_per_stream_full_row --test chain_row_stacks_stream_meters
```
Expected: 4 + 2 + 4 passed; the four `chain_row_*` files are untouched.

- [ ] **Step 10: Render and look.**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo build --release -j 2 --manifest-path tools/slint-render/Cargo.toml && mkdir -p target/issue-328-render && tools/slint-render/target/release/slint-render crates/adapter-gui/ui/components/chain_row_graph_test_harness.slint ChainRowGraphLinearHarness target/issue-328-render/linear.png 760 150
```
Open `target/issue-328-render/linear.png` (Read). Check: five cards on one line, wires between centres, nothing clipped at the row edges. Fix spacing in `chain_row_graph.slint` / the harness positions (keeping them equal to `chain_graph_adapter.rs`) until it reads right.

- [ ] **Step 11: Commit and push**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; git -C "$S" add crates/adapter-gui/ui/chain_graph_globals.slint crates/adapter-gui/ui/pages/chain_row_graph.slint crates/adapter-gui/ui/components/chain_latency_badge.slint crates/adapter-gui/ui/components/chain_row_graph_test_harness.slint crates/adapter-gui/ui/pages/chain_row.slint crates/adapter-gui/ui/pages/chain_row_blocks.slint crates/adapter-gui/ui/app-window.slint crates/adapter-gui/src/desktop_app_init.rs crates/adapter-gui/src/lib.rs crates/adapter-gui/src/issue_328_graph_row_interaction_tests.rs crates/adapter-gui/tests/issue_328_chain_row_graph_source.rs && git -C "$S" commit -m "feat(#328): desktop chain rows draw the chain graph"
```
Run the Push gate, push, comment on #328.

---

### Task 9: Graph cards show the #591 selection; the three renders

Required before the first `.slint` line: invoke `claude-plugin:ux-ui`, `slint:slint` and `slint-best-practices` (Global Constraints).

Spec §5.1: block cards keep today's `BlockChip` — icon, enabled LED, unavailable tint, MIDI markers, tooltip, selection. Part 5 already draws all of it from `GraphNode.block` / `bypass` / `selected` / `neighbor` and `markers_visible`, and its canvas draws the hover tooltip (Task 8's `hovering_a_block_card_shows_its_tooltip_and_a_routing_node_does_not` pins that the row feeds it). What is missing is WHO is selected. The strip marks by window-level indices, which survive a row rebuild; a per-node `selected` flag in the row's graph model would be wiped by every `replace_project_chains`. So the graph marks by block id: `ChainGraphBridge.selected-block-id` / `neighbor-block-id`, fed from the same `SelectionState` the strip and the footswitch read (#591), reach Part 5's card through two small string inputs.

**Files:**
- Modify: `crates/adapter-gui/ui/components/graph_view.slint` (Part 5 file: two inputs, forwarded to the card), `crates/adapter-gui/ui/components/graph_node_card.slint` (Part 5 file: two inputs, two derived flags read by its three marker bindings), `crates/adapter-gui/ui/pages/chain_row_graph.slint` (markers, ids), `crates/adapter-gui/ui/pages/chain_row.slint` (pass `markers-visible`), `crates/adapter-gui/ui/components/chain_row_graph_test_harness.slint` (a marked card for the render), `crates/adapter-gui/src/selection_highlight.rs` (ids + sync), `docs/screens.md:7`, `docs/gui/graph-view.md` (new section), `docs/gui/assets/chain-row-graph-{linear,split-mix,y}.png`
- Test: `crates/adapter-gui/src/selection_highlight_tests.rs`, `crates/adapter-gui/tests/issue_328_chain_row_graph_source.rs`

**Interfaces:**
- Consumes: Task 8 `ChainGraphBridge.selected-*`; Task 2 `active_highlight_indices`, `active_neighbor_block_ui_index`; Part 5 `GraphView.markers_visible`, `GraphNodeCard.markers-visible`, `BlockTileStyle.marker-*`.
- Produces: `GraphView` and `GraphNodeCard` gain `in property <string> selected-node-id` and `in property <string> neighbor-node-id`; `ChainRowGraph.markers-visible: bool`; Rust `selection_highlight::graph_selection_ids(&Project, &SelectionState) -> (i32, String, String)`.

- [ ] **Step 1: Write the failing tests.** Append to `selection_highlight_tests.rs`:

```rust
/// #328: the graph marks cards by block id; same rule as the strip's indices.
#[test]
fn the_graph_marks_the_selected_block_and_its_neighbor_by_id() {
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some("b0".to_string()),
        ..Default::default()
    };
    assert_eq!(graph_selection_ids(&project(), &sel), (0, "b0".to_string(), "b1".to_string()));
}

#[test]
fn nothing_selected_marks_no_card() {
    assert_eq!(
        graph_selection_ids(&project(), &SelectionState::default()),
        (-1, String::new(), String::new())
    );
}

/// The footswitch drain calls `sync_selection_markers`: the graph rows must
/// follow it like the strip does.
#[test]
fn syncing_the_markers_feeds_the_chain_graph_too() {
    use slint::Global;
    i_slint_backend_testing::init_no_event_loop();
    let window = crate::AppWindow::new().unwrap();
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some("b0".to_string()),
        ..Default::default()
    };
    sync_selection_markers(&window, &project(), &sel);
    let bridge = crate::ChainGraphBridge::get(&window);
    assert_eq!(
        (
            bridge.get_selected_chain_index(),
            bridge.get_selected_block_id().to_string(),
            bridge.get_neighbor_block_id().to_string(),
        ),
        (0, "b0".to_string(), "b1".to_string())
    );
}
```

Append to `tests/issue_328_chain_row_graph_source.rs` (source pin — the repo's convention for GUI wiring, `openrig-code-quality` "Pin GUI-wiring fixes with source-presence tests"; the marker itself is checked on the split-mix PNG of Step 6):

```rust
#[test]
fn graph_cards_mark_the_selection_by_block_id() {
    let card = read("ui/components/graph_node_card.slint");
    assert!(card.contains("root.node.id == root.selected-node-id"));
    assert!(card.contains("root.node.id == root.neighbor-node-id"));
    let view = read("ui/components/graph_view.slint");
    assert!(view.contains("selected-node-id: root.selected-node-id;"));
    assert!(view.contains("neighbor-node-id: root.neighbor-node-id;"));
    let row = read("ui/pages/chain_row_graph.slint");
    assert!(row.contains("ChainGraphBridge.selected-block-id"));
    assert!(row.contains("markers_visible: root.markers-visible;"));
    assert!(read("ui/pages/chain_row.slint").contains("markers-visible: root.midi-selection-active;"));
}
```

- [ ] **Step 2: Run — expect FAIL**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib selection_highlight
```
Expected: compile error `cannot find function graph_selection_ids`. Add the stub to `selection_highlight.rs`:

```rust
pub(crate) fn graph_selection_ids(_project: &Project, _sel: &SelectionState) -> (i32, String, String) {
    (-1, String::new(), String::new())
}
```
Run again: `left: (-1, "", "") right: (0, "b0", "b1")` for the id test and `left: (-1, "", "") right: (0, "b0", "b1")` for `syncing_the_markers_feeds_the_chain_graph_too` (the bridge keeps its defaults). Then `nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_chain_row_graph_source graph_cards_mark_the_selection_by_block_id`: `assertion failed: card.contains("root.node.id == root.selected-node-id")`.

- [ ] **Step 3: Implement the ids.** Replace the stub in `selection_highlight.rs`:

```rust
/// #328: `(chain_index, block_id, neighbor_id)` the chain graph marks — the
/// strip's rule above, by block id instead of chip index ("" = none).
pub(crate) fn graph_selection_ids(project: &Project, sel: &SelectionState) -> (i32, String, String) {
    let (chain_index, block_index) = active_highlight_indices(project, sel);
    let neighbor_index = active_neighbor_block_ui_index(project, sel);
    let Some(chain) = usize::try_from(chain_index)
        .ok()
        .and_then(|index| project.chains.get(index))
    else {
        return (-1, String::new(), String::new());
    };
    let id_at = |index: i32| {
        usize::try_from(index)
            .ok()
            .and_then(|i| chain.blocks.get(i))
            .map(|block| block.id.0.clone())
            .unwrap_or_default()
    };
    (chain_index, id_at(block_index), id_at(neighbor_index))
}
```

and in `sync_selection_markers` (line 74-79) append after the three strip setters:

```rust
    let (graph_chain, graph_block, graph_neighbor) = graph_selection_ids(project, sel);
    let bridge = crate::ChainGraphBridge::get(window);
    bridge.set_selected_chain_index(graph_chain);
    bridge.set_selected_block_id(graph_block.into());
    bridge.set_neighbor_block_id(graph_neighbor.into());
```
with `use slint::Global;` added to the imports.

- [ ] **Step 4: The canvas and the card (Part 5 files).** In `graph_view.slint`, right after Part 5's `in property <bool> markers_visible: false;` add:

```slint
    // #328 part 6: the #591 selection by block id ("" = none). A per-node
    // `selected` flag would be wiped by every row rebuild; these follow
    // `SelectionState` through `ChainGraphBridge`.
    in property <string> selected-node-id;
    in property <string> neighbor-node-id;
```

In the node loop's `GraphNodeCard { … }`, right after `markers-visible: root.markers_visible;` add:

```slint
            selected-node-id: root.selected-node-id;
            neighbor-node-id: root.neighbor-node-id;
```

In `graph_node_card.slint`, right after `in property <bool> markers-visible: false;` add:

```slint
    // #328 part 6: the host also marks the selection by block id.
    in property <string> selected-node-id;
    in property <string> neighbor-node-id;
    property <bool> is-selected: root.node.selected || (root.node.id != "" && root.node.id == root.selected-node-id);
    property <bool> is-neighbor: root.node.neighbor || (root.node.id != "" && root.node.id == root.neighbor-node-id);
```

and make Part 5's three marker bindings read the two flags (every other line of the card stays). Replace

```slint
    border-width: root.is-block
        ? BlockTileStyle.marker-border-width(root.markers-visible, root.node.selected, root.node.neighbor)
        : (root.node.selected ? 2px : 1px);
    border-color: root.is-block
        ? BlockTileStyle.marker-border-color(root.markers-visible, root.node.selected, root.node.neighbor, root.enabled)
        : (root.node.selected ? #f5f7fb : root.node.border);
```

with

```slint
    border-width: root.is-block
        ? BlockTileStyle.marker-border-width(root.markers-visible, root.is-selected, root.is-neighbor)
        : (root.is-selected ? 2px : 1px);
    border-color: root.is-block
        ? BlockTileStyle.marker-border-color(root.markers-visible, root.is-selected, root.is-neighbor, root.enabled)
        : (root.is-selected ? #f5f7fb : root.node.border);
```

and, in the block's marker-tint `Rectangle`, `background: BlockTileStyle.marker-background(root.markers-visible, root.node.selected);` with `background: BlockTileStyle.marker-background(root.markers-visible, root.is-selected);`. Afterwards `grep -n "root.node.selected\|root.node.neighbor" crates/adapter-gui/ui/components/graph_node_card.slint` prints only the two new `property <bool> is-…` lines. Part 5's own interaction tests (`issue_328_graph_view_interaction`) keep passing: `is-selected` / `is-neighbor` equal `node.selected` / `node.neighbor` while both ids are empty.

- [ ] **Step 5: The row.** In `chain_row_graph.slint` add the input `in property <bool> markers-visible: false;` after `in property <float> latency-ms: 0;`, and inside `graph := GraphView { … }` after `anchors: root.anchors;`:

```slint
        markers_visible: root.markers-visible;
        selected-node-id: ChainGraphBridge.selected-chain-index == root.chain-index
            ? ChainGraphBridge.selected-block-id : "";
        neighbor-node-id: ChainGraphBridge.selected-chain-index == root.chain-index
            ? ChainGraphBridge.neighbor-block-id : "";
```

In `chain_row.slint`'s `ChainRowGraph { … }` add `markers-visible: root.midi-selection-active;`.

In `chain_row_graph_test_harness.slint`, `ChainRowGraphSplitMixHarness`: add `markers-visible: true;` to its `ChainRowGraph { … }` and `selected: true,` to the `od` node literal, so the render shows the MIDI marker (`slint-render` cannot set the bridge).

- [ ] **Step 6: Run — expect PASS**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib selection_highlight && nice -n 19 cargo test -p adapter-gui -j 2 --lib issue_328_graph_row_interaction && nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_chain_row_graph_source && nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_interaction
```
Expected: all pass (the last one is Part 5's, unchanged).

- [ ] **Step 7: Render the three rows, self-critique, keep them as doc evidence.**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && R=tools/slint-render/target/release/slint-render H=crates/adapter-gui/ui/components/chain_row_graph_test_harness.slint && $R $H ChainRowGraphLinearHarness docs/gui/assets/chain-row-graph-linear.png 760 150 && $R $H ChainRowGraphSplitMixHarness docs/gui/assets/chain-row-graph-split-mix.png 980 260 && $R $H ChainRowGraphYHarness docs/gui/assets/chain-row-graph-y.png 620 260
```
Open the three PNGs. Self-critique (fix before moving on): lane A above lane B with the shared blocks centred between them; split and mixer read as routing nodes, not pedals; the bypassed Hall card is visibly dimmer; the unavailable AC30 is amber at half opacity, distinct from bypass grey; the selected TS9 card wears the amber MIDI marker; captions and names do not clip at zoom 1; an 8 px rhythm between cards; nothing overflows the row. Layout fixes go in `chain_row_graph.slint` and the adapter's constants (keeping harness positions equal to `chain_graph_adapter.rs`); a card-face defect is Part 5's `graph_node_card.slint` and is reported, not patched here.

- [ ] **Step 8: Docs.** `docs/screens.md` line 7 (the **Chains** bullet): append

> On desktop each chain row draws its chain as a graph (#328): an input node, the shared blocks, then — when the chain has a split — lane A on top and lane B below, joined by a mixer node (Split → Mix) or each ending in its own output node (Y → A/B), then the output node. Block cards keep the strip chip's icon, LED, unavailable tint, MIDI markers and hover tooltip; the wheel keeps scrolling the list and Cmd/Ctrl + wheel zooms the row. Touch mode keeps the pedal strip, where a split is a single Split chip.

`docs/gui/graph-view.md`: add before `## Future work`:

```markdown
## Chain row (#328)

Every desktop chain row hosts a `GraphView` through `ui/pages/chain_row_graph.slint`
(`ChainRow` keeps the pedal strip for touch mode, `ChainGraphBridge.graph-enabled`).

| Linear | Split → Mix | Y → A/B |
|---|---|---|
| ![linear](assets/chain-row-graph-linear.png) | ![split to mix](assets/chain-row-graph-split-mix.png) | ![y](assets/chain-row-graph-y.png) |

- `src/chain_graph_adapter.rs` turns a `Chain` into `ChainStage`s: stage 0 is the input
  node, every top-level block one stage, the split one `Parallel { lanes: [A, B], end }`
  stage (`Merge` for Split → Mix, `Fan` for Y → A/B with each lane ending in its own output
  node), then the output node. Cards sit 132 px apart, lanes 108 px apart.
- `src/chain_graph_ids.rs` names the nodes: a block node is its `BlockId`; the fixed ids
  are `__io_input`, `__io_output`, `__io_output_a`, `__io_output_b`, `__split_1`, `__merge_1`.
- `src/chain_graph_models.rs` publishes the nodes (each block node with its strip tile in
  `GraphNode.block`), the wires and the "+" anchors on `ProjectChainItem.graph_*` once per
  row rebuild; the meter tick never rebuilds them.
- `src/graph_anchor.rs` turns an anchor id (`stage:{i}` / `lane:{stage}:{lane}:{i}`) back
  into a position plus a split path — where a "+" inserts and where a drop moves a block.
- The #591 MIDI markers follow `ChainGraphBridge.selected-block-id` / `neighbor-block-id`
  (`GraphView.selected-node-id` / `neighbor-node-id`), fed from the same `SelectionState` the
  strip reads, so a row rebuild never loses them.
- Every gesture reaches Rust through `ChainGraphBridge`, tagged with the row's chain index;
  `resolve-drop-anchor` is answered by `graph_view_model::resolve_drop_anchor`.
```

- [ ] **Step 9: Commit and push**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; git -C "$S" add crates/adapter-gui/ui/components/graph_view.slint crates/adapter-gui/ui/components/graph_node_card.slint crates/adapter-gui/ui/pages/chain_row_graph.slint crates/adapter-gui/ui/pages/chain_row.slint crates/adapter-gui/ui/components/chain_row_graph_test_harness.slint crates/adapter-gui/src/selection_highlight.rs crates/adapter-gui/src/selection_highlight_tests.rs crates/adapter-gui/tests/issue_328_chain_row_graph_source.rs docs/screens.md docs/gui/graph-view.md docs/gui/assets/chain-row-graph-linear.png docs/gui/assets/chain-row-graph-split-mix.png docs/gui/assets/chain-row-graph-y.png && git -C "$S" commit -m "feat(#328): graph cards show the MIDI selection by block id"
```
The three PNGs go to a public repo: they are app renders with fake data, no commercial audio, no secrets — fine under LEI Nº 2. Run the Push gate, push, comment on #328 with the three PNGs attached.

---

### Task 10: The block editor addresses a block through its path

A block inside a split path is opened, edited, deleted and inserted through the same block editor as any block. The editor's draft carries the path its `block_index` / `before_index` count in; every lookup goes through `chain_block_lists::block_at` (Task 3), so block 0 of path A is never `chain.blocks[0]` (Review Focus 1). The graph reaches the flows through two `ChainGraphBridge` callbacks, wired inside the existing callback modules so they share their state.

**Files:**
- Modify: `crates/adapter-gui/src/block_editor_draft.rs:12-21`, `crates/adapter-gui/src/block_window.rs:9-15`, `crates/adapter-gui/src/block_param_apply.rs:71-94`, `crates/adapter-gui/src/block_editor_persist.rs:178-195,263-300`, `crates/adapter-gui/src/block_delete.rs:47-59`, `crates/adapter-gui/src/block_editor_window_delete.rs:95-111`, `crates/adapter-gui/src/block_editor_window_lifecycle.rs:318-339`, `crates/adapter-gui/src/block_parameter_extras.rs:244-270`, `crates/adapter-gui/src/block_editor_window_setup.rs:40-70,86,137-146`, `crates/adapter-gui/src/select_chain_block_callback.rs:136-412`, `crates/adapter-gui/src/block_insert_callbacks.rs:98-143`, `crates/adapter-gui/src/block_choose_type_callback.rs:384-410`; fixture literals `block_delete_tests.rs:59-69`, `block_param_apply_tests.rs:55-66`, `issue_915_native_amp_tabs_tests.rs:28`, `issue_815_add_block_tabs_tests.rs:32,111`
- Test: `crates/adapter-gui/src/issue_328_path_block_editor_tests.rs`

**Interfaces:**
- Consumes: Task 3 `block_at`, `insert_index`, `side_from_index`; Task 8 `ChainGraphBridge.open-path-block` / `start-path-insert`; Part 2 `InsertPrebuiltBlock { chain, block, position, path }`.
- Produces: `BlockEditorDraft.path: Option<PathRef>`, `BlockWindow.path: Option<PathRef>`, `BlockEditorWindowSetupCtx.path: Option<PathRef>`; `ChainGraphBridge.open-path-block(chain, split_id, side, index)` opens the editor of that path block; `ChainGraphBridge.start-path-insert(chain, split_id, side, position)` opens the picker for an insert into that path.

- [ ] **Step 1: Write the failing tests** `crates/adapter-gui/src/issue_328_path_block_editor_tests.rs`:

```rust
//! #328 — Review Focus 1: an index inside a split path is never read against
//! the top level. The editor's draft carries the path; every flow resolves the
//! block through it.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::{AudioBlock, AudioBlockKind, PathRef, PathSide};
use slint::{Global, Timer, VecModel};

use crate::block_delete::delete_drafted_block;
use crate::block_param_apply::{apply_block_parameter, ParamValue};
use crate::chain_graph_fixtures_tests::{chain_in, mix_chain, rows, session_with};
use crate::state::BlockEditorDraft;

fn on(side: PathSide) -> Option<PathRef> {
    Some(PathRef { split: BlockId("sp".into()), side })
}

fn draft(block_index: Option<usize>, before_index: usize, path: Option<PathRef>) -> BlockEditorDraft {
    BlockEditorDraft {
        chain_index: 0,
        block_index,
        before_index,
        instrument: "electric_guitar".into(),
        effect_type: "gain".into(),
        model_id: "volume".into(),
        enabled: true,
        is_select: false,
        path,
    }
}

fn volume(block: &AudioBlock) -> Option<f32> {
    match &block.kind {
        AudioBlockKind::Core(core) => core.params.get("volume").and_then(ParameterValue::as_f32),
        _ => None,
    }
}

fn lane(session: &Rc<RefCell<Option<crate::state::ProjectSession>>>, side: PathSide) -> Vec<AudioBlock> {
    let chain = chain_in(session, 0);
    let AudioBlockKind::Split(split) = &chain.blocks[1].kind else { panic!("block 1 is the split") };
    match side {
        PathSide::A => split.a.clone(),
        PathSide::B => split.b.clone(),
    }
}

#[test]
fn a_knob_edit_on_a_path_block_reaches_that_block_not_the_top_level_one() {
    let session = session_with(vec![mix_chain()]);
    let d = Rc::new(RefCell::new(Some(draft(Some(0), 0, on(PathSide::A)))));
    apply_block_parameter(&session, &d, "volume", ParamValue::Number(42.0), &rows(), &[], &[])
        .expect("the path block exists");
    assert_eq!(volume(&lane(&session, PathSide::A)[0]), Some(42.0), "a1 got the edit");
    assert_eq!(volume(&chain_in(&session, 0).blocks[0]), None, "pre was not touched");
}

#[test]
fn deleting_a_path_block_removes_it_from_its_path_only() {
    let session = session_with(vec![mix_chain()]);
    delete_drafted_block(&session, &draft(Some(0), 0, on(PathSide::B)), &rows(), &[], &[])
        .expect("delete");
    assert!(lane(&session, PathSide::B).is_empty(), "b1 removed");
    let top: Vec<String> = chain_in(&session, 0).blocks.iter().map(|b| b.id.0.clone()).collect();
    assert_eq!(top, vec!["pre", "sp", "post"], "the top level is intact");
}

#[test]
fn inserting_into_path_a_puts_the_block_in_path_a() {
    i_slint_backend_testing::init_no_event_loop();
    let window = crate::AppWindow::new().unwrap();
    let session = session_with(vec![mix_chain()]);
    let items = Rc::new(VecModel::from(crate::block_editor::block_parameter_items_for_model(
        "gain",
        "volume",
        &Default::default(),
    )));
    crate::block_editor::persist_block_editor_draft(
        &window,
        &draft(None, 1, on(PathSide::A)),
        &items,
        &session,
        &rows(),
        &Rc::new(RefCell::new(None)),
        &Rc::new(RefCell::new(false)),
        &[],
        &[],
        false,
    )
    .expect("insert");
    let a: Vec<String> = lane(&session, PathSide::A).iter().map(|b| b.id.0.clone()).collect();
    assert_eq!(a.len(), 3, "path A grew: {a:?}");
    assert_eq!((a[0].as_str(), a[2].as_str()), ("a1", "a2"), "the new block sits at 1");
    assert_eq!(chain_in(&session, 0).blocks.len(), 3, "the top level did not grow");
}

#[test]
fn the_graph_opens_the_editor_of_a_path_block() {
    let opened = open_path_block_through_the_bridge(on(PathSide::B).unwrap(), 0);
    let d = opened.borrow().clone().expect("a draft was opened");
    assert_eq!((d.block_index, d.path), (Some(0), on(PathSide::B)));
}

/// Wires `select_chain_block_callback` the way `issue_85_click_port_opens_editor_tests.rs:156-205`
/// does, then fires the bridge callback the graph fires for a path card.
fn open_path_block_through_the_bridge(path: PathRef, index: i32) -> Rc<RefCell<Option<BlockEditorDraft>>> {
    use crate::select_chain_block_callback::{wire, SelectChainBlockCallbackCtx};
    i_slint_backend_testing::init_no_event_loop();
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    let window = crate::AppWindow::new().unwrap();
    let insert_window = crate::ChainInsertWindow::new().unwrap();
    let port_window = crate::ChainPortWindow::new().unwrap();
    let draft: Rc<RefCell<Option<BlockEditorDraft>>> = Rc::new(RefCell::new(None));
    let session = session_with(vec![mix_chain()]);
    wire(
        &window,
        &insert_window,
        &port_window,
        SelectChainBlockCallbackCtx {
            open_compact_window: Rc::new(RefCell::new(None)),
            inline_tab_state: Rc::new(RefCell::new(Default::default())),
            selected_block: Rc::new(RefCell::new(None)),
            block_editor_draft: draft.clone(),
            insert_draft: Rc::new(RefCell::new(None)),
            block_type_options: Rc::new(VecModel::default()),
            block_model_options: Rc::new(VecModel::default()),
            filtered_block_model_options: Rc::new(VecModel::default()),
            block_model_option_labels: Rc::new(VecModel::default()),
            block_parameter_items: Rc::new(VecModel::default()),
            multi_slider_points: Rc::new(VecModel::default()),
            curve_editor_points: Rc::new(VecModel::default()),
            eq_band_curves: Rc::new(VecModel::default()),
            project_session: session,
            project_chains: Rc::new(VecModel::default()),
            saved_project_snapshot: Rc::new(RefCell::new(None)),
            project_dirty: Rc::new(RefCell::new(false)),
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(Vec::new())),
            open_block_windows: Rc::new(RefCell::new(Vec::new())),
            inline_stream_timer: Rc::new(RefCell::new(None)),
            toast_timer: Rc::new(Timer::default()),
            plugin_info_window: Rc::new(RefCell::new(None)),
            block_stream_reads: Rc::new(application::live_source::NoLiveSource),
            port_draft: Rc::new(RefCell::new(None)),
        },
    );
    crate::ChainGraphBridge::get(&window).invoke_open_path_block(
        0,
        path.split.0.as_str().into(),
        crate::chain_block_lists::side_index(&path.side),
        index,
    );
    draft
}
```

Register it in `lib.rs`: `#[cfg(test)] mod issue_328_path_block_editor_tests;`.

- [ ] **Step 2: Run — compile error** (`struct BlockEditorDraft has no field named path`). Add the field only — no consumer yet — and `path: None` at every literal: `block_editor_draft.rs`

```rust
    /// #328: the split path `block_index` / `before_index` count in — `None`
    /// for the chain's top level (spec §3). Every lookup goes through
    /// `chain_block_lists::block_at` with it.
    pub(crate) path: Option<project::block::PathRef>,
```

`path: None,` in `block_insert_callbacks.rs:122`, `block_editor_window_setup.rs:137`, `select_chain_block_callback.rs:251`, `block_delete_tests.rs:60`, `block_param_apply_tests.rs:56`. Run for the behavioural red:

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib issue_328_path_block_editor
```
Expected: `a1 got the edit left: None right: Some(42.0)` (the edit landed on `pre`), `b1 removed` fails (`pre` was deleted), `path A grew` fails (`left: 2 right: 3`), `a draft was opened` panics (the bridge callback is not wired).

- [ ] **Step 3: Resolve through the path.**

`block_param_apply.rs:71-94` — read the path with the indices and resolve through it:

```rust
    let (chain_index, block_index, path) = {
        let borrowed = draft.borrow();
        let Some(draft) = borrowed.as_ref() else {
            return Err(ApplyParamError::NotAddressable);
        };
        let Some(block_index) = draft.block_index else {
            return Err(ApplyParamError::NotAddressable);
        };
        (draft.chain_index, block_index, draft.path.clone())
    };
    let (chain_id, block_id) = {
        let borrowed = project_session.borrow();
        let Some(session) = borrowed.as_ref() else {
            return Err(ApplyParamError::NotAddressable);
        };
        let project = session.project.borrow();
        let Some(chain) = project.chains.get(chain_index) else {
            return Err(ApplyParamError::NotAddressable);
        };
        // #328: an index inside a split path counts in that path.
        let Some(block) = crate::chain_block_lists::block_at(chain, block_index, path.as_ref()) else {
            return Err(ApplyParamError::NotAddressable);
        };
        (chain.id.clone(), block.id.clone())
    };
```

`block_delete.rs:52-58`:

```rust
        let Some(block) =
            crate::chain_block_lists::block_at(chain, block_index, draft.path.as_ref())
        else {
            log::warn!("[block-drawer.delete] block {block_index} is gone");
            return Err(DeleteBlockError::Gone);
        };
```

`block_editor_persist.rs` edit branch (`:185-189`, the non-select lookup):

```rust
        let (block_id, block_enabled) = if let Some(block_index) = draft.block_index {
            // #328: an index inside a split path counts in that path.
            let block = crate::chain_block_lists::block_at(chain, block_index, draft.path.as_ref())
                .ok_or_else(|| anyhow!("{}", rust_i18n::t!("error-invalid-block")))?;
            (Some(block.id.clone()), block.enabled)
        } else {
            (None, false)
        };
```

insert branch (`:278-300`):

```rust
        let insert_index = {
            let proj = session.project.borrow();
            let chain = proj
                .chains
                .get(draft.chain_index)
                .ok_or_else(|| anyhow!("{}", rust_i18n::t!("error-invalid-chain")))?;
            // #328: clamped to the list the draft's path names.
            crate::chain_block_lists::insert_index(chain, draft.before_index, draft.path.as_ref())
                .ok_or_else(|| anyhow!("{}", rust_i18n::t!("error-invalid-block")))?
        };
```
and the dispatch gains the path: `BlockCommand::InsertPrebuiltBlock { chain: chain_id.clone(), block: new_block, position: insert_index, path: draft.path.clone() }` (Part 2 added `path: None` there; it now carries the draft's).

Select blocks cannot live in a path (spec §1.1), so the Select branch (`:211-220`) keeps `chain.blocks.get(block_idx)`.

`block_editor_window_delete.rs:95-111` and `block_editor_window_lifecycle.rs:318-339` read `win_draft`; replace their `chain.blocks.get(block_index)` with `crate::chain_block_lists::block_at(chain, block_index, draft.path.as_ref())` (lifecycle: capture `draft.path.clone()` next to `(draft.chain_index, bi)` and use it). `block_parameter_extras.rs:244-270` (file-pick commit): capture `draft.path.clone()` with `(draft.chain_index, bi)` and resolve with `block_at` the same way. Its `:74` lookup is Select-only and stays.

`block_window.rs`: add

```rust
    /// #328: the split path `block_index` counts in; part of the window's identity.
    pub(crate) path: Option<project::block::PathRef>,
```

`block_editor_window_setup.rs`: add to `BlockEditorWindowSetupCtx` (after `block_id`, line 57)

```rust
    /// #328: the split path `block_index` / `before_index` count in.
    pub path: Option<project::block::PathRef>,
```
destructure it at `:86` and set `path: path.clone(),` in the `win_draft` literal (`:137`). Add `path: None,` to the ctx literals in `issue_915_native_amp_tabs_tests.rs:28`, `issue_815_add_block_tabs_tests.rs:32,111`, and `path: draft.path.clone(),` in `block_choose_type_callback.rs:384` (the add flow's detached window) plus `path: None` in its `BlockWindow { … }` push (`:409`).

- [ ] **Step 4: One open flow for a strip row and a path card.** In `select_chain_block_callback.rs` turn the body of `window.on_select_chain_block(move |chain_index, ui_block_index| { … })` (lines 133-412) into a shared closure fed by either entry. Add above `wire`:

```rust
/// Which block a click names: a strip row (top level) or a card inside a
/// split path of the chain graph (#328).
enum BlockPick {
    Row(usize),
    Path { path: project::block::PathRef, index: usize },
}
```

Replace line 133 with `let open_block = Rc::new(move |chain_index: i32, pick: BlockPick| {` and lines 154-156 (`let block_index = ui_index_to_real_block_index(…)` … `let Some(block) = chain.blocks.get(block_index as usize) else {`) with:

```rust
        let (block_index, path) = match pick {
            BlockPick::Row(ui) => (ui_index_to_real_block_index(&chain, ui) as i32, None),
            BlockPick::Path { path, index } => (index as i32, Some(path)),
        };
        log::info!("[select_chain_block] index={} path={:?}", block_index, path);
        let Some(block) =
            crate::chain_block_lists::block_at(&chain, block_index as usize, path.as_ref())
        else {
```

Wrap the `SelectionCommand::SelectChainBlock` dispatch (`:164-171`) in `if path.is_none() { … }` — the dispatcher's block cursor addresses top-level blocks by index; a path card opens its editor without moving the MIDI cursor (see summary, open risk 3). Set `path: path.clone(),` in the `BlockEditorDraft` literal (`:251`). For a path block keep the strip's markers off: replace `*selected_block.borrow_mut() = Some(SelectedBlock { … });` (`:245-248`) with

```rust
        *selected_block.borrow_mut() = if path.is_none() {
            Some(SelectedBlock {
                chain_index: chain_index as usize,
                block_index: block_index as usize,
            })
        } else {
            None
        };
```

In the detached-window branch compare the path too — the three `bw.chain_index == ci && bw.block_index == bi` tests (lines 359, 368 and 373: the bring-to-front `find`, the stale-window `filter` and the `retain`) become `bw.chain_index == ci && bw.block_index == bi && bw.path == path` — set `path: path.clone(),` in the `BlockEditorWindowSetupCtx` literal (`:377`) and `path: path.clone(),` in the `BlockWindow { … }` push (`:410`). Close the closure with `});` and wire both entries after it:

```rust
    {
        let open_block = open_block.clone();
        window.on_select_chain_block(move |chain_index, ui_block_index| {
            open_block(chain_index, BlockPick::Row(ui_block_index as usize));
        });
    }
    // #328: a card inside a split path of the chain graph.
    crate::ChainGraphBridge::get(window).on_open_path_block(move |chain_index, split, side, index| {
        let Some(side) = crate::chain_block_lists::side_from_index(side) else {
            return;
        };
        let path = project::block::PathRef {
            split: domain::ids::BlockId(split.to_string()),
            side,
        };
        open_block(chain_index, BlockPick::Path { path, index: index as usize });
    });
```

(`use slint::Global;` is already imported at line 26.)

- [ ] **Step 5: One insert flow for a strip slot and a path "+".** In `block_insert_callbacks.rs` do the same with `on_start_block_insert` (lines 98-143): the closure becomes `let begin_insert = Rc::new(move |chain_index: i32, before: usize, path: Option<project::block::PathRef>| { … });` — inside it `before_index as usize` becomes `before` (lines 110, 113, 117) and the debug log prints `before` — and the index line (`:110`) becomes

```rust
                        let real_idx = match &path {
                            None => ui_index_to_real_block_index(chain, before),
                            Some(_) => crate::chain_block_lists::insert_index(chain, before, path.as_ref())
                                .unwrap_or(before),
                        };
```
the draft literal (`:122`) sets `path: path.clone(),`, and the two entries are wired after it:

```rust
        {
            let begin_insert = begin_insert.clone();
            window.on_start_block_insert(move |chain_index, before_index| {
                begin_insert(chain_index, before_index as usize, None);
            });
        }
        // #328: a "+" inside a split path of the chain graph.
        crate::ChainGraphBridge::get(window).on_start_path_insert(move |chain_index, split, side, position| {
            let Some(side) = crate::chain_block_lists::side_from_index(side) else {
                return;
            };
            let path = project::block::PathRef {
                split: domain::ids::BlockId(split.to_string()),
                side,
            };
            begin_insert(chain_index, position as usize, Some(path));
        });
```

- [ ] **Step 6: Run — expect PASS**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib issue_328_path_block_editor && nice -n 19 cargo test -p adapter-gui -j 2
```
Expected: 4 passed; the whole crate passes (the `issue_85_*`, `issue_815_*`, `issue_898_*` flows are unchanged for top-level blocks).

- [ ] **Step 7: Commit and push**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; git -C "$S" add crates/adapter-gui/src/block_editor_draft.rs crates/adapter-gui/src/block_window.rs crates/adapter-gui/src/block_param_apply.rs crates/adapter-gui/src/block_editor_persist.rs crates/adapter-gui/src/block_delete.rs crates/adapter-gui/src/block_editor_window_delete.rs crates/adapter-gui/src/block_editor_window_lifecycle.rs crates/adapter-gui/src/block_parameter_extras.rs crates/adapter-gui/src/block_editor_window_setup.rs crates/adapter-gui/src/select_chain_block_callback.rs crates/adapter-gui/src/block_insert_callbacks.rs crates/adapter-gui/src/block_choose_type_callback.rs crates/adapter-gui/src/block_delete_tests.rs crates/adapter-gui/src/block_param_apply_tests.rs crates/adapter-gui/src/issue_915_native_amp_tabs_tests.rs crates/adapter-gui/src/issue_815_add_block_tabs_tests.rs crates/adapter-gui/src/issue_328_path_block_editor_tests.rs crates/adapter-gui/src/lib.rs && git -C "$S" commit -m "feat(#328): the block editor addresses a block through its split path"
```
Delete exactly the dead-code expectation lines `cargo build` reports as unfulfilled, if any (none expected: `chain_block_lists::side_index` gets its production caller only in Task 11). Run the Push gate, push, comment on #328.

### Task 11: Graph gestures dispatch their commands

Every gesture on the graph becomes the Part 2 command it means (spec §3, §5.1): a click opens the right editor, bypass toggles the block, remove removes the block — or the split, asking first when path B holds blocks — "+" opens the picker at the right position and path, a drag becomes `MoveBlock { new_position, path }`. While a card is dragged, GraphView asks the host for the anchor under it (`resolve-drop-anchor`, Part 5) and to move the card (`node_dragged`); both are answered here. The overlays' state gets its own global, `ChainGraphOverlayState`.

**Files:**
- Create: `crates/adapter-gui/ui/chain_graph_overlay_globals.slint`, `crates/adapter-gui/src/graph_click.rs`, `crates/adapter-gui/src/graph_gesture_actions.rs`, `crates/adapter-gui/src/chain_graph_drop.rs`, `crates/adapter-gui/src/chain_graph_drag.rs`, `crates/adapter-gui/src/chain_graph_wiring.rs`
- Modify: `crates/adapter-gui/ui/components/root_modal_overlays.slint` (split confirmation), `crates/adapter-gui/ui/app-window.slint` (import + export `ChainGraphOverlayState`), `crates/adapter-gui/src/desktop_app_block_wiring.rs:306` (wire call before the closing `}`), `crates/adapter-gui/src/lib.rs`, nine `locales/*.yml` (`confirm-remove-split-name`, `error-graph-action`), `docs/screens.md:7`
- Test: `crates/adapter-gui/src/graph_click_tests.rs`, `crates/adapter-gui/src/graph_gesture_actions_tests.rs`, `crates/adapter-gui/src/chain_graph_drop_tests.rs`, `crates/adapter-gui/src/chain_graph_drag_tests.rs`, `crates/adapter-gui/src/issue_328_graph_row_interaction_tests.rs` (drag across lanes, confirm dialog)

**Interfaces:**
- Consumes: Tasks 3, 4, 6, 7, 8, 10; Part 2 `BlockCommand::{ToggleBlockEnabled, RemoveBlock, MoveBlock { chain, block, new_position, path }}` and `SplitCommand::RemoveSplit { chain, split_id }` (wrapped `Command::Split(…)`); Part 5 `graph_view_model::{resolve_drop_anchor, AnchorSlot}`; `runtime_sync_policy::request_chain_sync(&ProjectSession, &ChainId)`; `helpers::set_status_error(&AppWindow, &Timer, &str)`.
- Produces: Slint `global ChainGraphOverlayState` (below); `graph_click::{enum ClickAction { SelectRow(usize), OpenPathBlock { path: PathRef, index: usize }, OpenSplitEditor, OpenMixerEditor, OpenChecklist }, click_action(&Chain, &str) -> Option<ClickAction>}`; `graph_gesture_actions::{struct RowsTarget<'a> { model, inputs, outputs }, enum GestureError { NoProject, NoSuchChain, NotApplicable, NoMove, Failed(String) }, enum RemoveOutcome { Removed, ConfirmSplit { name: String } }, toggle_node, remove_node, remove_split, drop_node}`; `chain_graph_drop::drop_anchor_id(&Chain, &str, f32, f32) -> String`; `chain_graph_wiring::{struct ChainGraphWiringCtx, wire(&AppWindow, ChainGraphWiringCtx), chain_at(&Rc<RefCell<Option<ProjectSession>>>, i32) -> Option<Chain>}`; `chain_graph_drag::move_node(&VecModel<ProjectChainItem>, usize, &str, f32, f32)`.

- [ ] **Step 1: The overlay global** (declaration only — Tasks 14 and 15 wire the editor and checklist parts). `crates/adapter-gui/ui/chain_graph_overlay_globals.slint`:

```slint
// Responsibility: holds the chain graph overlays' state.
//
// #328. The split editor, the mixer editor, the endpoint checklist and the
// remove-split confirmation are root-level overlays (a PopupWindow's content
// does not receive clicks, #749/#761). The graph opens them through the
// callbacks below; Rust fills them. Mirrors `OverlayBridge` (#873).

import { ChannelOptionItem, BlockParameterItem } from "models.slint";

export global ChainGraphOverlayState {
    // ── Remove-split confirmation (spec §3: path B's blocks go with it) ──
    in-out property <bool> confirm-remove-split-open: false;
    in-out property <int> confirm-remove-split-chain-index: -1;
    in-out property <string> confirm-remove-split-name;
    callback confirm-remove-split();

    // ── Split / mixer editor (spec §1.2, §5.1) — kind 0 = split, 1 = mixer ──
    callback open-split-editor(int, int);
    in-out property <bool> split-editor-open: false;
    in-out property <int> split-editor-chain-index: -1;
    in-out property <int> split-editor-kind: 0;
    in-out property <string> split-editor-split-id;
    in-out property <string> split-editor-title;
    in-out property <[BlockParameterItem]> split-editor-items;
    // Args: chain index, split block id, parameter path, value.
    callback split-editor-number(int, string, string, float);
    callback split-editor-option(int, string, string, int);
    callback split-editor-bool(int, string, string, bool);

    // ── Endpoint checklist (spec §5.3) ──
    // Args: chain index, graph node id (`__io_input`, `__io_output`, …).
    callback open-checklist(int, string);
    in-out property <bool> checklist-open: false;
    in-out property <int> checklist-chain-index: -1;
    in-out property <string> checklist-node;
    in-out property <string> checklist-title;
    in-out property <[ChannelOptionItem]> checklist-items;
    // Args: chain index, node id, row, new checked state.
    callback checklist-toggled(int, string, int, bool);
}
```

`app-window.slint`: add `import { ChainGraphOverlayState } from "chain_graph_overlay_globals.slint";` next to the `ChainGraphBridge` import and `ChainGraphOverlayState` to the export list on line 44.

- [ ] **Step 2: Write the failing tests.** `crates/adapter-gui/src/graph_click_tests.rs`:

```rust
//! #328 (spec §5.1) — a click opens what the node stands for.

use super::*;
use crate::chain_graph_fixtures_tests::mix_chain;
use crate::chain_graph_ids::{INPUT_NODE_ID, MIXER_NODE_ID, OUTPUT_NODE_ID, SPLIT_NODE_ID};
use domain::ids::BlockId;
use project::block::{PathRef, PathSide};

#[test]
fn a_top_level_card_opens_through_its_strip_row() {
    let c = mix_chain();
    assert_eq!(click_action(&c, "pre"), Some(ClickAction::SelectRow(0)));
    assert_eq!(click_action(&c, "post"), Some(ClickAction::SelectRow(2)));
}

#[test]
fn a_path_card_opens_through_its_path() {
    let path = PathRef { split: BlockId("sp".into()), side: PathSide::A };
    assert_eq!(click_action(&mix_chain(), "a2"), Some(ClickAction::OpenPathBlock { path, index: 1 }));
}

#[test]
fn routing_nodes_open_their_editors() {
    let c = mix_chain();
    assert_eq!(click_action(&c, SPLIT_NODE_ID), Some(ClickAction::OpenSplitEditor));
    assert_eq!(click_action(&c, MIXER_NODE_ID), Some(ClickAction::OpenMixerEditor));
    assert_eq!(click_action(&c, INPUT_NODE_ID), Some(ClickAction::OpenChecklist));
    assert_eq!(click_action(&c, OUTPUT_NODE_ID), Some(ClickAction::OpenChecklist));
    assert_eq!(click_action(&c, "gone"), None);
}
```

`crates/adapter-gui/src/graph_gesture_actions_tests.rs` (real `LocalDispatcher`: Part 2 applies the commands):

```rust
//! #328 (spec §3) — graph gestures reach the project through the bus.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::chain_graph_adapter::{CARD_HALF, COLUMN_SPACING};
use crate::chain_graph_fixtures_tests::{chain, chain_in, core, mix_chain, port_in, registry, rows, session_with, split};
use crate::chain_graph_ids::{MIXER_NODE_ID, SPLIT_NODE_ID};
use crate::graph_view_model::AnchorSlot;
use crate::state::ProjectSession;
use project::block::{AudioBlock, AudioBlockKind, SplitEnd};
use slint::{Model, VecModel};

fn target(rows: &Rc<VecModel<ProjectChainItem>>) -> RowsTarget<'_> {
    RowsTarget { model: rows, inputs: &[], outputs: &[] }
}

fn ids(list: &[AudioBlock]) -> Vec<String> {
    list.iter().map(|b| b.id.0.clone()).collect()
}

fn lanes(session: &Rc<RefCell<Option<ProjectSession>>>) -> Option<(Vec<AudioBlock>, Vec<AudioBlock>)> {
    chain_in(session, 0).blocks.iter().find_map(|b| match &b.kind {
        AudioBlockKind::Split(s) => Some((s.a.clone(), s.b.clone())),
        _ => None,
    })
}

/// The anchor id Part 5 gives the wire before blueprint `index` of `lane`.
/// In both chains below the split is top-level block 1, so its stage is 2.
fn lane_anchor(lane: usize, index: usize) -> String {
    AnchorSlot::Lane { stage: 2, lane, index }.anchor_id()
}

#[test]
fn bypass_on_a_path_card_toggles_that_block() {
    let (session, rows) = (session_with(vec![mix_chain()]), rows());
    toggle_node(&session, 0, "b1", &target(&rows)).expect("toggle");
    assert!(!lanes(&session).unwrap().1[0].enabled);
    assert!(chain_in(&session, 0).blocks[0].enabled, "pre untouched");
}

#[test]
fn an_io_node_has_nothing_to_bypass() {
    let (session, rows) = (session_with(vec![mix_chain()]), rows());
    assert_eq!(toggle_node(&session, 0, crate::chain_graph_ids::INPUT_NODE_ID, &target(&rows)), Err(GestureError::NotApplicable));
}

#[test]
fn removing_a_path_block_removes_it_from_its_lane() {
    let (session, rows) = (session_with(vec![mix_chain()]), rows());
    assert_eq!(remove_node(&session, 0, "a2", &target(&rows)), Ok(RemoveOutcome::Removed));
    assert_eq!(ids(&lanes(&session).unwrap().0), vec!["a1"]);
}

#[test]
fn removing_a_split_with_blocks_in_path_b_asks_first() {
    let (session, rows) = (session_with(vec![mix_chain()]), rows());
    let outcome = remove_node(&session, 0, SPLIT_NODE_ID, &target(&rows)).expect("decided");
    assert!(matches!(outcome, RemoveOutcome::ConfirmSplit { .. }), "got {outcome:?}");
    assert!(lanes(&session).is_some(), "nothing removed before the confirmation");
}

#[test]
fn removing_a_split_with_an_empty_path_b_needs_no_confirmation() {
    let c = chain(vec![core("pre"), split("sp", SplitEnd::Mix, vec![core("a1")], vec![]), core("post")]);
    let (session, rows) = (session_with(vec![c]), rows());
    assert_eq!(remove_node(&session, 0, MIXER_NODE_ID, &target(&rows)), Ok(RemoveOutcome::Removed));
    // spec §3: path A's blocks take the split's place.
    assert_eq!(ids(&chain_in(&session, 0).blocks), vec!["pre", "a1", "post"]);
}

#[test]
fn confirming_removes_the_split_and_keeps_path_a() {
    let (session, rows) = (session_with(vec![mix_chain()]), rows());
    remove_split(&session, 0, &target(&rows)).expect("remove");
    assert_eq!(ids(&chain_in(&session, 0).blocks), vec!["pre", "a1", "a2", "post"]);
}

#[test]
fn dragging_a_card_across_lanes_moves_it_into_the_other_path() {
    let (session, rows) = (session_with(vec![mix_chain()]), rows());
    // b1 → mixer: the end of lane B.
    drop_node(&session, 0, "a1", &lane_anchor(1, 1), &target(&rows)).expect("move");
    let (a, b) = lanes(&session).unwrap();
    assert_eq!((ids(&a), ids(&b)), (vec!["a2".to_string()], vec!["b1".to_string(), "a1".to_string()]));
}

fn a1_x(rows: &Rc<VecModel<ProjectChainItem>>) -> f32 {
    rows.row_data(0).unwrap().graph_nodes.iter().find(|n| n.id.as_str() == "a1").unwrap().layout_x
}

#[test]
fn a_no_op_drop_snaps_the_card_back() {
    let (session, rows) = (session_with(vec![mix_chain()]), rows());
    crate::project_view::replace_project_chains(&rows, &session.borrow().as_ref().unwrap().project.borrow(), &[], &[], &registry());
    crate::chain_graph_drag::move_node(&rows, 0, "a1", 999.0, 999.0);
    // split → a1: a1's own slot.
    let own_slot = lane_anchor(0, 0);
    assert_eq!(drop_node(&session, 0, "a1", &own_slot, &target(&rows)), Err(GestureError::NoMove));
    assert_eq!(a1_x(&rows), CARD_HALF + 3.0 * COLUMN_SPACING, "the card is back in its slot");
}

#[test]
fn a_refused_drop_snaps_the_card_back() {
    // spec §1.1: an Input port may not live in a path, so Part 2 refuses the move.
    let c = chain(vec![port_in("port", "main", "In 1"), split("sp", SplitEnd::Mix, vec![core("a1")], vec![])]);
    let (session, rows) = (session_with(vec![c]), rows());
    crate::project_view::replace_project_chains(&rows, &session.borrow().as_ref().unwrap().project.borrow(), &[], &[], &registry());
    crate::chain_graph_drag::move_node(&rows, 0, "port", 999.0, 999.0);
    // split → mixer of the empty lane B.
    assert!(matches!(drop_node(&session, 0, "port", &lane_anchor(1, 0), &target(&rows)), Err(GestureError::Failed(_))));
    let x = rows.row_data(0).unwrap().graph_nodes.iter().find(|n| n.id.as_str() == "port").unwrap().layout_x;
    assert_eq!(x, CARD_HALF + COLUMN_SPACING, "the port is back in its slot");
}
```

`crates/adapter-gui/src/chain_graph_drag_tests.rs`:

```rust
//! #328 — the dragged card and the ends of its wires follow the pointer.

use super::*;
use crate::chain_graph_fixtures_tests::{mix_chain, registry, rows};
use crate::project_view::replace_project_chains;
use project::project::Project;

#[test]
fn a_dragged_node_carries_the_ends_of_its_wires() {
    let rows = rows();
    let project = Project { name: None, device_settings: vec![], chains: vec![mix_chain()], midi: None };
    replace_project_chains(&rows, &project, &[], &[], &registry());
    move_node(&rows, 0, "a1", 999.0, 777.0);
    let row = rows.row_data(0).unwrap();
    let a1 = row.graph_nodes.iter().find(|n| n.id.as_str() == "a1").unwrap();
    assert_eq!((a1.layout_x, a1.layout_y), (999.0, 777.0));
    for edge in row.graph_edges.iter() {
        if edge.to_id.as_str() == "a1" {
            assert_eq!((edge.to_x, edge.to_y), (999.0, 777.0));
        }
        if edge.from_id.as_str() == "a1" {
            assert_eq!((edge.from_x, edge.from_y), (999.0, 777.0));
        }
    }
}
```

`crates/adapter-gui/src/chain_graph_drop_tests.rs`:

```rust
//! #328 (spec §5.1, §7 "drop-target resolution") — the row answers GraphView's
//! `resolve-drop-anchor` from the chain's own layout.

use super::*;
use crate::chain_graph_fixtures_tests::mix_chain;

// mix_chain on the row grid: a1 (446, 50) · a2 (578, 50) · b1 (446, 158) ·
// mixer (710, 104). Wire midpoints: a1 → a2 (512, 50) = "lane:2:0:1",
// b1 → mixer (578, 131) = "lane:2:1:1". Reach: half a column, 66 px.

#[test]
fn a_lane_a_card_dropped_by_the_end_of_lane_b_lands_there() {
    assert_eq!(drop_anchor_id(&mix_chain(), "a1", 578.0, 135.0), "lane:2:1:1");
}

#[test]
fn a_card_on_its_own_wire_or_out_of_reach_lands_nowhere() {
    assert_eq!(drop_anchor_id(&mix_chain(), "a1", 512.0, 50.0), "", "a1 → a2 is a1's own wire");
    assert_eq!(drop_anchor_id(&mix_chain(), "a1", 2000.0, 2000.0), "");
}

#[test]
fn only_a_block_card_moves() {
    assert_eq!(drop_anchor_id(&mix_chain(), "__split_1", 578.0, 135.0), "");
}
```

Append to `issue_328_graph_row_interaction_tests.rs`:

```rust
use std::cell::Cell;

use crate::chain_graph_fixtures_tests::{chain, core, split};
use crate::graph_anchor::{move_target, parse_anchor};
use project::block::{PathSide, SplitEnd};

#[test]
fn dragging_a_lane_a_card_onto_lane_b_drops_it_into_path_b() {
    i_slint_backend_testing::init_no_event_loop();
    let h = ChainRowGraphSplitMixHarness::new().unwrap();
    // The harness row is `od → split(A: a1 | B: b1) → rev`, laid out exactly
    // as `chain_graph_adapter` lays this chain out.
    let row = chain(vec![core("od"), split("sp", SplitEnd::Mix, vec![core("a1")], vec![core("b1")]), core("rev")]);
    let bridge = ChainGraphBridge::get(&h);
    {
        let row = row.clone();
        bridge.on_resolve_drop_anchor(move |_, id, x, y| {
            crate::chain_graph_drop::drop_anchor_id(&row, &id, x, y).into()
        });
    }
    let dropped: Rc<RefCell<Option<(String, String)>>> = Rc::new(RefCell::new(None));
    let seen = dropped.clone();
    bridge.on_node_dropped(move |_, id, anchor| {
        *seen.borrow_mut() = Some((id.to_string(), anchor.to_string()));
    });
    h.show().unwrap();
    let win = h.window();
    // a1 → next to the b1 → mixer wire (midpoint 512, 131), lane B.
    let (from, to) = (at(446.0, 50.0), at(512.0, 150.0));
    win.dispatch_event(WindowEvent::PointerMoved { position: from });
    win.dispatch_event(WindowEvent::PointerPressed { position: from, button: PointerEventButton::Left });
    for step in 1..=10 {
        let t = step as f32 / 10.0;
        let p = LogicalPosition::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        win.dispatch_event(WindowEvent::PointerMoved { position: p });
    }
    win.dispatch_event(WindowEvent::PointerReleased { position: to, button: PointerEventButton::Left });

    let (id, anchor) = dropped.borrow().clone().expect("the drop reached the bridge");
    assert_eq!((id.as_str(), anchor.as_str()), ("a1", "lane:2:1:1"));
    let target = parse_anchor(&anchor)
        .and_then(|slot| move_target(&row, "a1", &slot))
        .unwrap_or_else(|| panic!("anchor {anchor:?} names no place"));
    assert_eq!(target.path.map(|p| p.side), Some(PathSide::B));
}

#[test]
fn the_confirm_dialog_fires_the_split_removal() {
    i_slint_backend_testing::init_no_event_loop();
    let w = crate::AppWindow::new().unwrap();
    let fired = Rc::new(Cell::new(false));
    let f = fired.clone();
    let overlay = crate::ChainGraphOverlayState::get(&w);
    overlay.on_confirm_remove_split(move || f.set(true));
    overlay.set_confirm_remove_split_name("Split (+ 1 blocks in path B)".into());
    overlay.set_confirm_remove_split_open(true);
    w.show().unwrap();
    let button = i_slint_backend_testing::ElementHandle::find_by_element_id(&w, "ConfirmDeleteBlockDialog::confirm-area")
        .next()
        .expect("the confirmation is up");
    let (pos, size) = (button.absolute_position(), button.size());
    click_at(&w, LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0));
    assert!(fired.get(), "confirm reached Rust");
    assert!(!overlay.get_confirm_remove_split_open(), "the dialog closed");
}
```

- [ ] **Step 3: Run — compile error; stubs; behavioural red.** Register in `lib.rs` (no dead-code attribute: the wiring below calls them in this task): `mod graph_click;`, `mod graph_gesture_actions;`, `mod chain_graph_drop;`, `mod chain_graph_drag;`, `mod chain_graph_wiring;`. Stubs: `click_action` → `None`; `toggle_node` / `remove_split` / `drop_node` → `Err(GestureError::NotApplicable)`, `remove_node` → `Err(GestureError::NotApplicable)`; `drop_anchor_id` → `String::new()`; `move_node` → empty body; `chain_graph_wiring::wire` → empty body.

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib graph_click && nice -n 19 cargo test -p adapter-gui -j 2 --lib graph_gesture_actions && nice -n 19 cargo test -p adapter-gui -j 2 --lib chain_graph_drop && nice -n 19 cargo test -p adapter-gui -j 2 --lib chain_graph_drag && nice -n 19 cargo test -p adapter-gui -j 2 --lib issue_328_graph_row_interaction
```
Expected: every new test fails on its first assertion — `left: None right: Some(SelectRow(0))`; `toggle: NotApplicable` / `remove: NotApplicable` / `move: NotApplicable` from the `expect`s; `left: Err(NotApplicable) right: Ok(Removed)`; `left: Err(NotApplicable) right: Err(NoMove)`; `left: "" right: "lane:2:1:1"`; `the drop reached the bridge` (the stub resolver names no anchor, so GraphView never fires `node-dropped`); `the confirmation is up` (no dialog yet); `a_dragged_node_carries_the_ends_of_its_wires` `left: (446.0, 50.0) right: (999.0, 777.0)`. `a_card_on_its_own_wire_or_out_of_reach_lands_nowhere` and `only_a_block_card_moves` pass on the stub: they are no-move guards.

- [ ] **Step 4: Implement.** `crates/adapter-gui/src/graph_click.rs`:

```rust
//! Responsibility: decides what a click on a graph node opens.
//!
//! #328 (spec §5.1): a block card opens its block editor (a top-level card
//! through the strip's own row flow, a path card through `open-path-block`),
//! the split and mixer nodes open their knob editors, an input or output node
//! opens the endpoint checklist.

use project::block::PathRef;
use project::chain::Chain;

use crate::chain_graph_ids::{resolve_node, NodeRef};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ClickAction {
    SelectRow(usize),
    OpenPathBlock { path: PathRef, index: usize },
    OpenSplitEditor,
    OpenMixerEditor,
    OpenChecklist,
}

pub(crate) fn click_action(chain: &Chain, node_id: &str) -> Option<ClickAction> {
    Some(match resolve_node(chain, node_id)? {
        NodeRef::Block { path: None, index, .. } => ClickAction::SelectRow(index),
        NodeRef::Block {
            path: Some(path),
            index,
            ..
        } => ClickAction::OpenPathBlock { path, index },
        NodeRef::Split => ClickAction::OpenSplitEditor,
        NodeRef::Mixer => ClickAction::OpenMixerEditor,
        NodeRef::Endpoints(_) => ClickAction::OpenChecklist,
    })
}

#[cfg(test)]
#[path = "graph_click_tests.rs"]
mod tests;
```

`crates/adapter-gui/src/graph_gesture_actions.rs`:

```rust
//! Responsibility: turns a chain graph gesture into the command it asks for.
//!
//! #328 (spec §3, §5.1). A gesture names a node by id and becomes one
//! command on the bus (`BlockCommand`, or `SplitCommand::RemoveSplit`). A
//! structural change is then resynced into the live chain (#614: a dispatch
//! alone only records it) and the rows are republished. A drop that does not
//! apply republishes too: the card followed the pointer in place and has to
//! go back to its slot.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{BlockCommand, Command, SplitCommand};
use domain::AudioDeviceDescriptor;
use project::chain::Chain;
use slint::VecModel;

use crate::chain_block_lists::split_of;
use crate::chain_graph_ids::{resolve_node, NodeRef};
use crate::graph_anchor::{move_target, parse_anchor};
use crate::project_view::replace_project_chains;
use crate::runtime_sync_policy::request_chain_sync;
use crate::state::ProjectSession;
use crate::ProjectChainItem;

/// Where the rows get republished.
pub(crate) struct RowsTarget<'a> {
    pub(crate) model: &'a Rc<VecModel<ProjectChainItem>>,
    pub(crate) inputs: &'a [AudioDeviceDescriptor],
    pub(crate) outputs: &'a [AudioDeviceDescriptor],
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum GestureError {
    NoProject,
    NoSuchChain,
    /// The node cannot take this gesture (an I/O node has nothing to bypass).
    NotApplicable,
    /// The drop changes nothing.
    NoMove,
    /// The dispatcher or the runtime sync refused; carries the message.
    Failed(String),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RemoveOutcome {
    Removed,
    /// Path B holds blocks and removing the split deletes them (spec §3):
    /// ask first. `name` is what the confirm dialog shows.
    ConfirmSplit { name: String },
}

type Session = Rc<RefCell<Option<ProjectSession>>>;

fn failed(error: impl std::fmt::Display) -> GestureError {
    GestureError::Failed(error.to_string())
}

fn chain_of(session: &ProjectSession, chain_index: usize) -> Result<Chain, GestureError> {
    session
        .project
        .borrow()
        .chains
        .get(chain_index)
        .cloned()
        .ok_or(GestureError::NoSuchChain)
}

fn republish(session: &ProjectSession, rows: &RowsTarget<'_>) {
    replace_project_chains(
        rows.model,
        &session.project.borrow(),
        rows.inputs,
        rows.outputs,
        &session.io_bindings.borrow(),
    );
}

/// Dispatch a structural change, resync the chain's runtime, republish.
fn apply(
    session: &ProjectSession,
    chain: &Chain,
    command: Command,
    rows: &RowsTarget<'_>,
) -> Result<(), GestureError> {
    session.dispatcher.dispatch(command).map_err(failed)?;
    request_chain_sync(session, &chain.id).map_err(failed)?;
    republish(session, rows);
    Ok(())
}

pub(crate) fn toggle_node(
    session: &Session,
    chain_index: usize,
    node_id: &str,
    rows: &RowsTarget<'_>,
) -> Result<(), GestureError> {
    let borrowed = session.borrow();
    let session = borrowed.as_ref().ok_or(GestureError::NoProject)?;
    let chain = chain_of(session, chain_index)?;
    let Some(NodeRef::Block { id, .. }) = resolve_node(&chain, node_id) else {
        return Err(GestureError::NotApplicable);
    };
    // #127: the dispatcher applies the live toggle itself (`block_toggle.rs:63-64`).
    session
        .dispatcher
        .dispatch(Command::Block(BlockCommand::ToggleBlockEnabled {
            chain: chain.id.clone(),
            block: id,
        }))
        .map_err(failed)?;
    republish(session, rows);
    Ok(())
}

pub(crate) fn remove_node(
    session: &Session,
    chain_index: usize,
    node_id: &str,
    rows: &RowsTarget<'_>,
) -> Result<RemoveOutcome, GestureError> {
    {
        let borrowed = session.borrow();
        let s = borrowed.as_ref().ok_or(GestureError::NoProject)?;
        let chain = chain_of(s, chain_index)?;
        match resolve_node(&chain, node_id).ok_or(GestureError::NotApplicable)? {
            NodeRef::Block { id, .. } => {
                let command = Command::Block(BlockCommand::RemoveBlock {
                    chain: chain.id.clone(),
                    block: id,
                });
                apply(s, &chain, command, rows)?;
                return Ok(RemoveOutcome::Removed);
            }
            NodeRef::Split | NodeRef::Mixer => {
                let (_, _, split) = split_of(&chain).ok_or(GestureError::NotApplicable)?;
                if !split.b.is_empty() {
                    let name = rust_i18n::t!("confirm-remove-split-name", n = split.b.len());
                    return Ok(RemoveOutcome::ConfirmSplit {
                        name: name.to_string(),
                    });
                }
            }
            NodeRef::Endpoints(_) => return Err(GestureError::NotApplicable),
        }
    }
    remove_split(session, chain_index, rows)?;
    Ok(RemoveOutcome::Removed)
}

/// `RemoveSplit`: path A's blocks take the split's place, path B's go (spec §3).
pub(crate) fn remove_split(
    session: &Session,
    chain_index: usize,
    rows: &RowsTarget<'_>,
) -> Result<(), GestureError> {
    let borrowed = session.borrow();
    let s = borrowed.as_ref().ok_or(GestureError::NoProject)?;
    let chain = chain_of(s, chain_index)?;
    let (_, split_id, _) = split_of(&chain).ok_or(GestureError::NotApplicable)?;
    let command = Command::Split(SplitCommand::RemoveSplit {
        chain: chain.id.clone(),
        split_id: split_id.clone(),
    });
    apply(s, &chain, command, rows)
}

pub(crate) fn drop_node(
    session: &Session,
    chain_index: usize,
    node_id: &str,
    anchor: &str,
    rows: &RowsTarget<'_>,
) -> Result<(), GestureError> {
    let borrowed = session.borrow();
    let s = borrowed.as_ref().ok_or(GestureError::NoProject)?;
    let chain = chain_of(s, chain_index)?;
    let result = match parse_anchor(anchor).and_then(|slot| move_target(&chain, node_id, &slot)) {
        None => Err(GestureError::NoMove),
        Some(target) => {
            let command = Command::Block(BlockCommand::MoveBlock {
                chain: chain.id.clone(),
                block: target.block,
                new_position: target.new_position,
                path: target.path,
            });
            apply(s, &chain, command, rows)
        }
    };
    if result.is_err() {
        // The card followed the pointer in place: put it back in its slot.
        republish(s, rows);
    }
    result
}

#[cfg(test)]
#[path = "graph_gesture_actions_tests.rs"]
mod tests;
```

`crates/adapter-gui/src/chain_graph_drop.rs`:

```rust
//! Responsibility: answers which anchor a dragged graph card lands on.
//!
//! #328 (spec §5.1). GraphView asks on every drag move (`resolve-drop-anchor`,
//! Part 5) so the target lights up before release, and fires `node-dropped`
//! with the answer. The geometry is Part 5's `resolve_drop_anchor`; this lays
//! the row's chain out again — the same stages and grid its row was published
//! from — and asks it.

use project::chain::Chain;

use crate::chain_graph_adapter::{chain_graph, grid_metrics};
use crate::endpoint_checklist_items::IoLabels;
use crate::graph_view_model::resolve_drop_anchor;

/// The anchor id a card `node_id` dragged to layout `(x, y)` lands on, or `""`.
pub(crate) fn drop_anchor_id(chain: &Chain, node_id: &str, x: f32, y: f32) -> String {
    // Labels move no node and no anchor, so none are resolved here.
    let labels = IoLabels {
        input: String::new(),
        output: String::new(),
        path_a: String::new(),
        path_b: String::new(),
    };
    let graph = chain_graph(chain, &labels);
    resolve_drop_anchor(&graph.nodes, &graph.anchors, node_id, x, y, grid_metrics(graph.lanes))
        .map(|anchor| anchor.id.clone())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "chain_graph_drop_tests.rs"]
mod tests;
```

`crates/adapter-gui/src/chain_graph_drag.rs` (its own file: moving a card mid-drag changes for other reasons than publishing the graph):

```rust
//! Responsibility: moves a dragged card within its row's published graph.
//!
//! #328 (spec §5.1). Part 5's GraphView follows the pointer only through its
//! host's model (`node_dragged`), so the dragged node — and the ends of its
//! wires — move in place, row by row, in the models `chain_graph_models`
//! published. A rebuild would recreate the card mid-drag.

use slint::{Model, VecModel};

use crate::{GraphEdgeGeometry, GraphNode, ProjectChainItem};

pub(crate) fn move_node(rows: &VecModel<ProjectChainItem>, chain_index: usize, node_id: &str, x: f32, y: f32) {
    let Some(row) = rows.row_data(chain_index) else {
        return;
    };
    if let Some(nodes) = row.graph_nodes.as_any().downcast_ref::<VecModel<GraphNode>>() {
        for i in 0..nodes.row_count() {
            let Some(mut node) = nodes.row_data(i) else { continue };
            if node.id.as_str() == node_id {
                node.layout_x = x;
                node.layout_y = y;
                nodes.set_row_data(i, node);
            }
        }
    }
    if let Some(edges) = row.graph_edges.as_any().downcast_ref::<VecModel<GraphEdgeGeometry>>() {
        for i in 0..edges.row_count() {
            let Some(mut edge) = edges.row_data(i) else { continue };
            let mut moved = false;
            if edge.from_id.as_str() == node_id {
                (edge.from_x, edge.from_y, moved) = (x, y, true);
            }
            if edge.to_id.as_str() == node_id {
                (edge.to_x, edge.to_y, moved) = (x, y, true);
            }
            if moved {
                edges.set_row_data(i, edge);
            }
        }
    }
}

#[cfg(test)]
#[path = "chain_graph_drag_tests.rs"]
mod tests;
```

`crates/adapter-gui/src/chain_graph_wiring.rs`:

```rust
//! Responsibility: connects the chain graph's gestures to their handlers.
//!
//! #328 (spec §5.1). Thin by design: each `ChainGraphBridge` callback resolves
//! its chain row, hands the node id or anchor to the module that owns the rule
//! (`graph_click`, `graph_anchor`, `graph_gesture_actions`, `chain_graph_drop`,
//! `chain_graph_drag`) and shows a refusal as a toast. Wired once from
//! `desktop_app_block_wiring::wire_all`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, Timer, VecModel};

use domain::AudioDeviceDescriptor;
use project::chain::Chain;

use crate::chain_block_lists::side_index;
use crate::graph_anchor::{insert_target, parse_anchor, InsertTarget};
use crate::graph_click::{click_action, ClickAction};
use crate::graph_gesture_actions::{
    drop_node, remove_node, remove_split, toggle_node, GestureError, RemoveOutcome, RowsTarget,
};
use crate::helpers::set_status_error;
use crate::state::ProjectSession;
use crate::{AppWindow, ChainGraphBridge, ChainGraphOverlayState, ProjectChainItem};

pub(crate) struct ChainGraphWiringCtx {
    pub(crate) project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub(crate) project_chains: Rc<VecModel<ProjectChainItem>>,
    pub(crate) input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub(crate) output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub(crate) toast_timer: Rc<Timer>,
}

pub(crate) fn wire(window: &AppWindow, ctx: ChainGraphWiringCtx) {
    let ctx = Rc::new(ctx);
    wire_clicks(window, &ctx);
    wire_edits(window, &ctx);
    wire_drags(window, &ctx);
    wire_split_removal(window, &ctx);
}

/// A snapshot of the chain at a row, released before any flow re-borrows it.
pub(crate) fn chain_at(session: &Rc<RefCell<Option<ProjectSession>>>, chain_index: i32) -> Option<Chain> {
    let borrowed = session.borrow();
    let session = borrowed.as_ref()?;
    let project = session.project.borrow();
    project.chains.get(usize::try_from(chain_index).ok()?).cloned()
}

fn with_rows<T>(ctx: &ChainGraphWiringCtx, f: impl FnOnce(&RowsTarget<'_>) -> T) -> T {
    let inputs = ctx.input_chain_devices.borrow();
    let outputs = ctx.output_chain_devices.borrow();
    f(&RowsTarget {
        model: &ctx.project_chains,
        inputs: &inputs,
        outputs: &outputs,
    })
}

fn report(window: &AppWindow, ctx: &ChainGraphWiringCtx, result: Result<(), GestureError>) {
    match result {
        Ok(()) | Err(GestureError::NoMove) => {}
        Err(GestureError::Failed(err)) => set_status_error(
            window,
            &ctx.toast_timer,
            &rust_i18n::t!("error-graph-action", err = err),
        ),
        Err(other) => log::warn!("[chain-graph] gesture ignored: {other:?}"),
    }
}

fn wire_clicks(window: &AppWindow, ctx: &Rc<ChainGraphWiringCtx>) {
    let (weak, ctx) = (window.as_weak(), ctx.clone());
    ChainGraphBridge::get(window).on_node_clicked(move |chain_index, node_id| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let Some(action) = chain_at(&ctx.project_session, chain_index)
            .and_then(|chain| click_action(&chain, &node_id))
        else {
            return;
        };
        match action {
            ClickAction::SelectRow(row) => window.invoke_select_chain_block(chain_index, row as i32),
            ClickAction::OpenPathBlock { path, index } => ChainGraphBridge::get(&window)
                .invoke_open_path_block(
                    chain_index,
                    path.split.0.as_str().into(),
                    side_index(&path.side),
                    index as i32,
                ),
            ClickAction::OpenSplitEditor => {
                ChainGraphOverlayState::get(&window).invoke_open_split_editor(chain_index, 0)
            }
            ClickAction::OpenMixerEditor => {
                ChainGraphOverlayState::get(&window).invoke_open_split_editor(chain_index, 1)
            }
            ClickAction::OpenChecklist => {
                ChainGraphOverlayState::get(&window).invoke_open_checklist(chain_index, node_id)
            }
        }
    });
}

fn wire_edits(window: &AppWindow, ctx: &Rc<ChainGraphWiringCtx>) {
    let bridge = ChainGraphBridge::get(window);
    {
        let (weak, ctx) = (window.as_weak(), ctx.clone());
        bridge.on_add_requested(move |chain_index, anchor| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let place = chain_at(&ctx.project_session, chain_index)
                .and_then(|chain| parse_anchor(&anchor).and_then(|slot| insert_target(&chain, &slot)));
            match place {
                Some(InsertTarget { position, path: None }) => {
                    window.invoke_start_block_insert(chain_index, position as i32)
                }
                Some(InsertTarget {
                    position,
                    path: Some(path),
                }) => ChainGraphBridge::get(&window).invoke_start_path_insert(
                    chain_index,
                    path.split.0.as_str().into(),
                    side_index(&path.side),
                    position as i32,
                ),
                None => log::warn!("[chain-graph] no insert place for anchor {anchor}"),
            }
        });
    }
    {
        let (weak, ctx) = (window.as_weak(), ctx.clone());
        bridge.on_bypass_toggled(move |chain_index, node_id| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let result = with_rows(&ctx, |rows| {
                toggle_node(&ctx.project_session, chain_index as usize, &node_id, rows)
            });
            report(&window, &ctx, result);
        });
    }
    {
        let (weak, ctx) = (window.as_weak(), ctx.clone());
        bridge.on_remove_requested(move |chain_index, node_id| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let outcome = with_rows(&ctx, |rows| {
                remove_node(&ctx.project_session, chain_index as usize, &node_id, rows)
            });
            match outcome {
                Ok(RemoveOutcome::ConfirmSplit { name }) => {
                    let overlay = ChainGraphOverlayState::get(&window);
                    overlay.set_confirm_remove_split_chain_index(chain_index);
                    overlay.set_confirm_remove_split_name(name.into());
                    overlay.set_confirm_remove_split_open(true);
                }
                other => report(&window, &ctx, other.map(|_| ())),
            }
        });
    }
}

fn wire_drags(window: &AppWindow, ctx: &Rc<ChainGraphWiringCtx>) {
    let bridge = ChainGraphBridge::get(window);
    {
        let ctx = ctx.clone();
        bridge.on_node_dragged(move |chain_index, node_id, x, y| {
            crate::chain_graph_drag::move_node(&ctx.project_chains, chain_index as usize, &node_id, x, y);
        });
    }
    {
        // Asked on every drag move so the target "+" lights up (Part 5).
        let ctx = ctx.clone();
        bridge.on_resolve_drop_anchor(move |chain_index, node_id, x, y| {
            chain_at(&ctx.project_session, chain_index)
                .map(|chain| crate::chain_graph_drop::drop_anchor_id(&chain, &node_id, x, y))
                .unwrap_or_default()
                .into()
        });
    }
    {
        let (weak, ctx) = (window.as_weak(), ctx.clone());
        bridge.on_node_dropped(move |chain_index, node_id, anchor| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let result = with_rows(&ctx, |rows| {
                drop_node(&ctx.project_session, chain_index as usize, &node_id, &anchor, rows)
            });
            report(&window, &ctx, result);
        });
    }
}

fn wire_split_removal(window: &AppWindow, ctx: &Rc<ChainGraphWiringCtx>) {
    let (weak, ctx) = (window.as_weak(), ctx.clone());
    ChainGraphOverlayState::get(window).on_confirm_remove_split(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let chain_index = ChainGraphOverlayState::get(&window).get_confirm_remove_split_chain_index();
        let result = with_rows(&ctx, |rows| {
            remove_split(&ctx.project_session, chain_index as usize, rows)
        });
        report(&window, &ctx, result);
    });
}
```

In `root_modal_overlays.slint` import `ChainGraphOverlayState` (`import { ChainGraphOverlayState } from "../chain_graph_overlay_globals.slint";`) and add after the third confirm dialog:

```slint
    // #328: removing a split whose path B holds blocks deletes them (spec §3).
    if ChainGraphOverlayState.confirm-remove-split-open : ConfirmDeleteBlockDialog {
        x: 0; y: 0;
        width: root.width; height: root.height;
        block-name: ChainGraphOverlayState.confirm-remove-split-name;
        cancel => { ChainGraphOverlayState.confirm-remove-split-open = false; }
        confirm => {
            ChainGraphOverlayState.confirm-remove-split-open = false;
            ChainGraphOverlayState.confirm-remove-split();
        }
    }
```

In `desktop_app_block_wiring.rs`, before the closing `}` of `wire_all` (line 307):

```rust
    // --- #328: chain graph gestures (click, "+", bypass, remove, drag) ---
    crate::chain_graph_wiring::wire(
        deps.window,
        crate::chain_graph_wiring::ChainGraphWiringCtx {
            project_session: deps.project_session.clone(),
            project_chains: deps.project_chains.clone(),
            input_chain_devices: deps.input_chain_devices.clone(),
            output_chain_devices: deps.output_chain_devices.clone(),
            toast_timer: deps.toast_timer.clone(),
        },
    );
```

Add `confirm-remove-split-name` and `error-graph-action` to the nine `locales/*.yml` (Appendix A).

- [ ] **Step 5: Run — expect PASS**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib graph_click && nice -n 19 cargo test -p adapter-gui -j 2 --lib graph_gesture_actions && nice -n 19 cargo test -p adapter-gui -j 2 --lib chain_graph_drop && nice -n 19 cargo test -p adapter-gui -j 2 --lib chain_graph_drag && nice -n 19 cargo test -p adapter-gui -j 2 --lib issue_328_graph_row_interaction && nice -n 19 cargo test -p adapter-gui -j 2 --lib every_locale_carries_the_same_keys_as_english
```
Expected: all pass.

- [ ] **Step 6: Docs.** `docs/screens.md` line 7, after the Task 9 sentence:

> Click a card to open its editor, the split or mixer node for its knobs, an input or output node for its endpoint checklist. "+" on a wire, or at the end of a lane, adds a block at exactly that place. Drag a card within its lane, across to the other lane, or between shared and lane positions; a drop the chain cannot take snaps back. Removing the split (or mixer) node asks first when lane B holds blocks — lane A's blocks take the split's place, lane B's are deleted.

- [ ] **Step 7: Commit and push**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; git -C "$S" add crates/adapter-gui/ui/chain_graph_overlay_globals.slint crates/adapter-gui/ui/components/root_modal_overlays.slint crates/adapter-gui/ui/app-window.slint crates/adapter-gui/src/graph_click.rs crates/adapter-gui/src/graph_click_tests.rs crates/adapter-gui/src/graph_gesture_actions.rs crates/adapter-gui/src/graph_gesture_actions_tests.rs crates/adapter-gui/src/chain_graph_drop.rs crates/adapter-gui/src/chain_graph_drop_tests.rs crates/adapter-gui/src/chain_graph_drag.rs crates/adapter-gui/src/chain_graph_drag_tests.rs crates/adapter-gui/src/chain_graph_wiring.rs crates/adapter-gui/src/issue_328_graph_row_interaction_tests.rs crates/adapter-gui/src/desktop_app_block_wiring.rs crates/adapter-gui/src/lib.rs crates/adapter-gui/locales/de-DE.yml crates/adapter-gui/locales/en-US.yml crates/adapter-gui/locales/es-ES.yml crates/adapter-gui/locales/fr-FR.yml crates/adapter-gui/locales/hi-IN.yml crates/adapter-gui/locales/ja-JP.yml crates/adapter-gui/locales/ko-KR.yml crates/adapter-gui/locales/pt-BR.yml crates/adapter-gui/locales/zh-CN.yml docs/screens.md && git -C "$S" commit -m "feat(#328): graph gestures dispatch their commands"
```
Delete exactly the dead-code expectation lines `cargo build` reports as unfulfilled (expected now: `graph_anchor`, `chain_block_lists`; `chain_graph_ids` keeps its line until Task 15 reads the `Endpoints` payload). Run the Push gate, push, comment on #328.

---

### Task 12: "Split → Mix" and "Y → A/B" in the block picker

Spec §5.1: the add-block picker gets two entries, hidden when the chain already has a split. They are also hidden inside a path (no split inside a path, spec §1.1), and `Y → A/B` is offered only where no block follows (AddSplit refuses it otherwise, spec §3). A pick dispatches `AddSplit`, resyncs, republishes.

**Files:**
- Create: `crates/adapter-gui/src/split_picker_entries.rs`, `crates/adapter-gui/src/split_insert.rs`
- Modify: `crates/adapter-gui/src/block_insert_callbacks.rs` (inside `begin_insert`, where `block_type_options.set_vec(block_type_picker_items(&instrument))` sits, today `:131`), `crates/adapter-gui/src/block_choose_type_callback.rs:130-134` (the split pick before the type lookup), `crates/adapter-gui/src/lib.rs`, nine `locales/*.yml` (four `picker-split-*` keys), `docs/screens.md:7`, `README.md:61` (feature list) and `:237` (roadmap), `README.pt-BR.md:61,237`, `README.es-ES.md:61,237`
- Test: `crates/adapter-gui/src/split_picker_entries_tests.rs`, `crates/adapter-gui/src/issue_328_split_picker_tests.rs`

**Interfaces:**
- Consumes: Part 2 `SplitCommand::AddSplit { chain, position, end }` (wrapped `Command::Split(…)`); Task 11 `RowsTarget`, `GestureError`; Task 10 `BlockEditorDraft.path`.
- Produces: `split_picker_entries::{split_picker_ends(&Chain, usize, Option<&PathRef>) -> Vec<SplitEnd>, split_picker_items(&[SplitEnd]) -> Vec<BlockTypePickerItem>, split_end_for_pick(usize, usize, &[SplitEnd]) -> Option<SplitEnd>}`; `split_insert::{struct SplitPick { chain_index: usize, position: usize, end: SplitEnd }, split_pick(&Rc<RefCell<Option<ProjectSession>>>, &BlockEditorDraft, usize, usize) -> Option<SplitPick>, add_split(&Rc<RefCell<Option<ProjectSession>>>, &SplitPick, &RowsTarget) -> Result<(), GestureError>}`.

- [ ] **Step 1: Write the failing tests.** `crates/adapter-gui/src/split_picker_entries_tests.rs`:

```rust
//! #328 — Review Focus 5: a split is offered only where it can go.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, mix_chain};
use domain::ids::BlockId;
use project::block::{PathRef, PathSide, SplitEnd};

#[test]
fn a_chain_without_a_split_offers_both_at_its_end() {
    let c = chain(vec![core("a"), core("b")]);
    assert_eq!(split_picker_ends(&c, 2, None), vec![SplitEnd::Mix, SplitEnd::Y]);
}

#[test]
fn y_is_offered_only_at_the_end_of_the_chain() {
    let c = chain(vec![core("a"), core("b")]);
    assert_eq!(split_picker_ends(&c, 1, None), vec![SplitEnd::Mix]);
}

#[test]
fn no_split_entry_when_the_chain_already_has_one() {
    assert!(split_picker_ends(&mix_chain(), 3, None).is_empty());
}

#[test]
fn no_split_entry_inside_a_path() {
    let path = PathRef { split: BlockId("sp".into()), side: PathSide::A };
    assert!(split_picker_ends(&chain(vec![]), 0, Some(&path)).is_empty());
}

#[test]
fn the_entries_sit_after_the_block_types() {
    let ends = [SplitEnd::Mix, SplitEnd::Y];
    assert_eq!(split_end_for_pick(4, 5, &ends), None, "a block type");
    assert_eq!(split_end_for_pick(5, 5, &ends), Some(SplitEnd::Mix));
    assert_eq!(split_end_for_pick(6, 5, &ends), Some(SplitEnd::Y));
    assert_eq!(split_end_for_pick(7, 5, &ends), None);
}

#[test]
fn each_entry_reads_as_a_split() {
    let items = split_picker_items(&[SplitEnd::Mix, SplitEnd::Y]);
    let labels: Vec<String> = items.iter().map(|i| i.label.to_string()).collect();
    assert_eq!(labels, vec![rust_i18n::t!("picker-split-mix").to_string(), rust_i18n::t!("picker-split-y").to_string()]);
    assert!(items.iter().all(|i| i.icon_kind.as_str() == "split" && !i.uses_model_catalog));
}
```

`crates/adapter-gui/src/issue_328_split_picker_tests.rs` — the flow the user runs: open the picker at a position, pick the entry. The harness wires `block_insert_callbacks` and `block_choose_type_callback` exactly as `issue_898_compact_insert_refresh_tests.rs:70-178` does, keeping `block_type_options`:

```rust
//! #328 (spec §5.1) — picking "Split → Mix" in the add-block picker adds a
//! split at the picked position.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Global, Model, Timer, VecModel};

use application::live_source::NoLiveSource;
use project::block::{AudioBlockKind, SplitEnd};

use crate::block_choose_type_callback::{self, BlockChooseTypeCallbackCtx};
use crate::block_insert_callbacks::{self, BlockInsertCallbacksCtx};
use crate::chain_graph_fixtures_tests::{chain, chain_in, core, session_with};
use crate::{AppWindow, BlockTypePickerItem};

struct Picker {
    app: AppWindow,
    session: Rc<RefCell<Option<crate::state::ProjectSession>>>,
    options: Rc<VecModel<BlockTypePickerItem>>,
}

fn picker() -> Picker {
    i_slint_backend_testing::init_no_event_loop();
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    let app = AppWindow::new().unwrap();
    let session = session_with(vec![chain(vec![core("a"), core("b")])]);
    let options: Rc<VecModel<BlockTypePickerItem>> = Rc::new(VecModel::default());
    let draft = Rc::new(RefCell::new(None));
    let rows = Rc::new(VecModel::default());
    let devices_in = Rc::new(RefCell::new(Vec::new()));
    let devices_out = Rc::new(RefCell::new(Vec::new()));
    let snapshot = Rc::new(RefCell::new(None));
    let dirty = Rc::new(RefCell::new(false));
    let selected = Rc::new(RefCell::new(None));
    let tabs = Rc::new(RefCell::new(Default::default()));
    block_insert_callbacks::wire(
        &app,
        BlockInsertCallbacksCtx {
            inline_tab_state: tabs.clone(),
            selected_block: selected.clone(),
            block_editor_draft: draft.clone(),
            block_type_options: options.clone(),
            block_model_options: Rc::new(VecModel::default()),
            filtered_block_model_options: Rc::new(VecModel::default()),
            block_model_option_labels: Rc::new(VecModel::default()),
            block_parameter_items: Rc::new(VecModel::default()),
            multi_slider_points: Rc::new(VecModel::default()),
            curve_editor_points: Rc::new(VecModel::default()),
            eq_band_curves: Rc::new(VecModel::default()),
            project_session: session.clone(),
            project_chains: rows.clone(),
            saved_project_snapshot: snapshot.clone(),
            project_dirty: dirty.clone(),
            input_chain_devices: devices_in.clone(),
            output_chain_devices: devices_out.clone(),
            block_editor_persist_timer: Rc::new(Timer::default()),
        },
    );
    let insert_window = crate::ChainInsertWindow::new().unwrap();
    let port_window = crate::ChainPortWindow::new().unwrap();
    block_choose_type_callback::wire(
        &app,
        &insert_window,
        &port_window,
        BlockChooseTypeCallbackCtx {
            inline_tab_state: tabs,
            block_editor_draft: draft,
            insert_draft: Rc::new(RefCell::new(None)),
            block_model_options: Rc::new(VecModel::default()),
            filtered_block_model_options: Rc::new(VecModel::default()),
            block_model_option_labels: Rc::new(VecModel::default()),
            block_parameter_items: Rc::new(VecModel::default()),
            multi_slider_points: Rc::new(VecModel::default()),
            curve_editor_points: Rc::new(VecModel::default()),
            eq_band_curves: Rc::new(VecModel::default()),
            project_session: session.clone(),
            project_chains: rows,
            block_stream_reads: Rc::new(NoLiveSource),
            saved_project_snapshot: snapshot,
            project_dirty: dirty,
            input_chain_devices: devices_in,
            output_chain_devices: devices_out,
            selected_block: selected,
            open_block_windows: Rc::new(RefCell::new(Vec::new())),
            plugin_info_window: Rc::new(RefCell::new(None)),
            port_draft: Rc::new(RefCell::new(None)),
            open_compact_window: Rc::new(RefCell::new(None)),
        },
    );
    Picker { app, session, options }
}

#[test]
fn the_picker_offers_the_split_entries_after_the_block_types() {
    let p = picker();
    p.app.invoke_start_block_insert(0, 2);
    let last: Vec<String> = (p.options.row_count() - 2..p.options.row_count())
        .map(|i| p.options.row_data(i).unwrap().label.to_string())
        .collect();
    assert_eq!(last, vec![rust_i18n::t!("picker-split-mix").to_string(), rust_i18n::t!("picker-split-y").to_string()]);
}

#[test]
fn picking_split_to_mix_adds_a_split_at_the_position() {
    let p = picker();
    p.app.invoke_start_block_insert(0, 1);
    let mix_row = p.options.row_count() - 1; // only Mix is offered at 1 (b follows)
    crate::BlockEditorBridge::get(&p.app).invoke_choose_block_type(mix_row as i32);
    let c = chain_in(&p.session, 0);
    assert_eq!(c.blocks.len(), 3);
    assert!(matches!(&c.blocks[1].kind, AudioBlockKind::Split(s) if s.end == SplitEnd::Mix && s.a.is_empty() && s.b.is_empty()));
}
```

Register both test modules in `lib.rs` (`split_picker_entries` has `#[path]` inside its own file; add `#[cfg(test)] mod issue_328_split_picker_tests;`).

- [ ] **Step 2: Run — compile error; stubs; behavioural red.** Add `mod split_picker_entries;` and `mod split_insert;` to `lib.rs`. Stubs: `split_picker_ends` → `Vec::new()`, `split_picker_items` → `Vec::new()`, `split_end_for_pick` → `None`, `split_pick` → `None`, `add_split` → `Err(GestureError::NotApplicable)`.

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib split_picker && nice -n 19 cargo test -p adapter-gui -j 2 --lib issue_328_split_picker
```
Expected: `left: [] right: [Mix, Y]`; the flow test fails with the last two labels being block types, and `left: 2 right: 3`.

- [ ] **Step 3: Implement.** `crates/adapter-gui/src/split_picker_entries.rs`:

```rust
//! Responsibility: offers the split entries of the add-block picker.
//!
//! #328 (spec §5.1): "Split → Mix" and "Y → A/B" follow the block types in the
//! picker. One split per chain and none inside a path (spec §1.1); a Y split
//! must be the chain's last processing block, so it is offered only where
//! nothing follows (AddSplit refuses it otherwise, spec §3).

use project::block::{AudioBlockKind, PathRef, SplitEnd};
use project::chain::Chain;

use crate::BlockTypePickerItem;

/// `effect_type` of both entries — not a catalog type; the choose-type flow
/// recognises the entries by their position after the block types.
const SPLIT_ENTRY_EFFECT_TYPE: &str = "split";

pub(crate) fn split_picker_ends(chain: &Chain, position: usize, path: Option<&PathRef>) -> Vec<SplitEnd> {
    let has_split = chain
        .blocks
        .iter()
        .any(|b| matches!(b.kind, AudioBlockKind::Split(_)));
    if path.is_some() || has_split {
        return Vec::new();
    }
    if position < chain.blocks.len() {
        vec![SplitEnd::Mix]
    } else {
        vec![SplitEnd::Mix, SplitEnd::Y]
    }
}

pub(crate) fn split_picker_items(ends: &[SplitEnd]) -> Vec<BlockTypePickerItem> {
    ends.iter()
        .map(|end| {
            let (label, subtitle) = match end {
                SplitEnd::Mix => (
                    rust_i18n::t!("picker-split-mix"),
                    rust_i18n::t!("picker-split-mix-subtitle"),
                ),
                SplitEnd::Y => (
                    rust_i18n::t!("picker-split-y"),
                    rust_i18n::t!("picker-split-y-subtitle"),
                ),
            };
            BlockTypePickerItem {
                effect_type: SPLIT_ENTRY_EFFECT_TYPE.into(),
                label: label.as_ref().into(),
                subtitle: subtitle.as_ref().into(),
                icon_kind: "split".into(),
                use_panel_editor: false,
                uses_model_catalog: false,
                accent_color: crate::ui_state::accent_color_for_icon_kind("split"),
                icon_source: slint::Image::default(),
            }
        })
        .collect()
}

/// The split end a picker row names, when the row is past the `base_len`
/// block types.
pub(crate) fn split_end_for_pick(index: usize, base_len: usize, ends: &[SplitEnd]) -> Option<SplitEnd> {
    ends.get(index.checked_sub(base_len)?).cloned()
}

#[cfg(test)]
#[path = "split_picker_entries_tests.rs"]
mod tests;
```

`crates/adapter-gui/src/split_insert.rs`:

```rust
//! Responsibility: adds a split to a chain from the add-block picker.
//!
//! #328 (spec §3, §5.1). `AddSplit` creates an empty split with default knobs
//! at the picked position; the live chain is resynced (#614) and the rows are
//! republished so the new lanes appear.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, SplitCommand};
use project::block::SplitEnd;

use crate::graph_gesture_actions::{GestureError, RowsTarget};
use crate::project_view::replace_project_chains;
use crate::runtime_sync_policy::request_chain_sync;
use crate::split_picker_entries::{split_end_for_pick, split_picker_ends};
use crate::state::{BlockEditorDraft, ProjectSession};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SplitPick {
    pub(crate) chain_index: usize,
    pub(crate) position: usize,
    pub(crate) end: SplitEnd,
}

/// The split the picker row at `index` stands for, if it is a split entry.
pub(crate) fn split_pick(
    session: &Rc<RefCell<Option<ProjectSession>>>,
    draft: &BlockEditorDraft,
    index: usize,
    base_len: usize,
) -> Option<SplitPick> {
    let borrowed = session.borrow();
    let project = borrowed.as_ref()?.project.borrow();
    let chain = project.chains.get(draft.chain_index)?;
    let ends = split_picker_ends(chain, draft.before_index, draft.path.as_ref());
    Some(SplitPick {
        chain_index: draft.chain_index,
        position: draft.before_index,
        end: split_end_for_pick(index, base_len, &ends)?,
    })
}

pub(crate) fn add_split(
    session: &Rc<RefCell<Option<ProjectSession>>>,
    pick: &SplitPick,
    rows: &RowsTarget<'_>,
) -> Result<(), GestureError> {
    let borrowed = session.borrow();
    let session = borrowed.as_ref().ok_or(GestureError::NoProject)?;
    let chain_id = session
        .project
        .borrow()
        .chains
        .get(pick.chain_index)
        .map(|c| c.id.clone())
        .ok_or(GestureError::NoSuchChain)?;
    session
        .dispatcher
        .dispatch(Command::Split(SplitCommand::AddSplit {
            chain: chain_id.clone(),
            position: pick.position,
            end: pick.end,
        }))
        .map_err(|e| GestureError::Failed(e.to_string()))?;
    request_chain_sync(session, &chain_id).map_err(|e| GestureError::Failed(e.to_string()))?;
    replace_project_chains(
        rows.model,
        &session.project.borrow(),
        rows.inputs,
        rows.outputs,
        &session.io_bindings.borrow(),
    );
    Ok(())
}
```

In `block_insert_callbacks.rs`, inside `begin_insert` (Task 10), extend the borrow that reads `(instrument, real_before_index)` to also compute the entries, and publish them after the block types. Replace `block_type_options.set_vec(block_type_picker_items(&instrument));` with:

```rust
            // #328: "Split → Mix" / "Y → A/B" follow the block types (spec §5.1).
            let split_ends = {
                let borrowed = project_session.borrow();
                borrowed
                    .as_ref()
                    .and_then(|s| {
                        s.project.borrow().chains.get(chain_index as usize).map(|chain| {
                            crate::split_picker_entries::split_picker_ends(chain, real_before_index, path.as_ref())
                        })
                    })
                    .unwrap_or_default()
            };
            let mut types = block_type_picker_items(&instrument);
            types.extend(crate::split_picker_entries::split_picker_items(&split_ends));
            block_type_options.set_vec(types);
```

In `block_choose_type_callback.rs`, right after `let block_types = block_type_picker_items(&instrument);` (line 130):

```rust
        // #328: rows past the block types are the split entries (block_insert_callbacks).
        let split_pick = block_editor_draft.borrow().as_ref().and_then(|draft| {
            crate::split_insert::split_pick(&project_session, draft, index as usize, block_types.len())
        });
        if let Some(pick) = split_pick {
            crate::BlockEditorBridge::get(&window).set_show_block_type_picker(false);
            let devices_in = input_chain_devices.borrow();
            let devices_out = output_chain_devices.borrow();
            let rows = crate::graph_gesture_actions::RowsTarget {
                model: &project_chains,
                inputs: &devices_in,
                outputs: &devices_out,
            };
            if let Err(error) = crate::split_insert::add_split(&project_session, &pick, &rows) {
                log::warn!("[block-picker] split refused: {error:?}");
                window.set_status_message(
                    rust_i18n::t!("error-graph-action", err = format!("{error:?}")).as_ref().into(),
                );
            }
            return;
        }
```

Add the four `picker-split-*` keys to the nine `locales/*.yml` (Appendix A).

- [ ] **Step 4: Run — expect PASS**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib split_picker && nice -n 19 cargo test -p adapter-gui -j 2 --lib issue_328_split_picker && nice -n 19 cargo test -p adapter-gui -j 2 --lib issue_898
```
Expected: all pass (the #898 compact-insert flow picks by the same index as before — the split entries only append).

- [ ] **Step 5: Docs.** `docs/screens.md` line 7, append: *The add-block picker ends with **Split → Mix** and **Y → A/B** while the chain has no split (Y → A/B only at the chain's end).* README feature list — add after the "Truly parallel chains" bullet:
  - `README.md`: `- **Two amps, one chain.** Split any chain into two parallel paths — **Split → Mix** sums them in a mixer (level, pan and polarity per path), **Y → A/B** sends each path to its own outputs. Every chain is drawn as a graph you edit in place.`
  - `README.pt-BR.md`: `- **Dois amps, uma chain.** Divida qualquer chain em dois caminhos paralelos — **Split → Mix** soma os dois num mixer (nível, pan e polaridade por caminho), **Y → A/B** manda cada caminho para as próprias saídas. Toda chain aparece como um grafo que você edita ali mesmo.`
  - `README.es-ES.md`: `- **Dos amplis, una cadena.** Divide cualquier cadena en dos caminos paralelos — **Split → Mix** los suma en un mezclador (nivel, paneo y polaridad por camino), **Y → A/B** envía cada camino a sus propias salidas. Cada cadena se dibuja como un grafo que editas ahí mismo.`

  and tick the roadmap line of #328 in each file (line 237): `- [ ] Parallel routing / chain splits ([#328](…))` → `- [x] Parallel routing / chain splits ([#328](…))`, `- [ ] Roteamento paralelo / splits de chain ([#328](…))` → `- [x] …`, `- [ ] Routing paralelo / splits de cadena ([#328](…))` → `- [x] …` (only the box changes).

- [ ] **Step 6: Commit and push**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; git -C "$S" add crates/adapter-gui/src/split_picker_entries.rs crates/adapter-gui/src/split_picker_entries_tests.rs crates/adapter-gui/src/split_insert.rs crates/adapter-gui/src/issue_328_split_picker_tests.rs crates/adapter-gui/src/block_insert_callbacks.rs crates/adapter-gui/src/block_choose_type_callback.rs crates/adapter-gui/src/lib.rs crates/adapter-gui/locales/de-DE.yml crates/adapter-gui/locales/en-US.yml crates/adapter-gui/locales/es-ES.yml crates/adapter-gui/locales/fr-FR.yml crates/adapter-gui/locales/hi-IN.yml crates/adapter-gui/locales/ja-JP.yml crates/adapter-gui/locales/ko-KR.yml crates/adapter-gui/locales/pt-BR.yml crates/adapter-gui/locales/zh-CN.yml docs/screens.md README.md README.pt-BR.md README.es-ES.md && git -C "$S" commit -m "feat(#328): add a split from the block picker"
```
Run the Push gate, push, comment on #328.

### Task 13: One Split chip in the strip and the compact view

Spec §5.4: touch and compact views show the split as one "Split" chip that opens the split editor; paths are not drawn there. The strip tile (`chain_block_item.rs`) gets a Split arm, the icon set learns `split` and `mixer` (Part 5's text-free, colorizable `graph-split.svg` / `graph-mix.svg` — the older `splitter.svg` / `mixer.svg` bake in Arial "SPLITTER"/"MIXER" text and a dark backing square, so `colorize` cannot tint them; never a glyph), and a click on the chip opens the split editor through `ChainGraphOverlayState.open-split-editor` (Task 11; Task 14 fills it).

**Files:**
- Modify: `crates/adapter-gui/src/chain_block_item.rs:15-59` (Split arm, type label), `:128` (test module), `crates/adapter-gui/ui/components/effect_type_icon.slint:54-55` (two icon kinds), `crates/adapter-gui/src/select_chain_block_callback.rs` (Split branch, right after the `SelectionCommand` dispatch)
- Test: `crates/adapter-gui/src/chain_block_item_tests.rs` (new), `crates/adapter-gui/src/issue_328_split_chip_tests.rs`, `crates/adapter-gui/tests/issue_328_chain_row_graph_source.rs` (icon map)

**Interfaces:**
- Consumes: Task 11 `ChainGraphOverlayState.open-split-editor(int, int)`; Task 12 `picker-split-mix` / `picker-split-y` keys; Part 1 `AudioBlockKind::label()` for `Split` (the compact view keys its icon on it, `compact_block_view.rs:114-121`).
- Produces: a Split tile `{ kind: "split", icon_kind: "split", type_label: "SPLIT", label: t!("picker-split-mix" | "picker-split-y"), display_name: "" }`; `EffectTypeIcon` draws `split` and `mixer`.

- [ ] **Step 1: Write the failing tests.** `crates/adapter-gui/src/chain_block_item_tests.rs`:

```rust
//! #328 (spec §5.4) — the split is one chip in the strip and the compact view.

use super::chain_block_item_from_block;
use crate::chain_graph_fixtures_tests::{core, split};
use project::block::SplitEnd;

#[test]
fn a_split_is_one_split_chip() {
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    let chip = chain_block_item_from_block(&split("sp", SplitEnd::Mix, vec![core("a1")], vec![]));
    assert_eq!(
        (chip.kind.as_str(), chip.icon_kind.as_str(), chip.type_label.as_str()),
        ("split", "split", "SPLIT")
    );
    assert_eq!(chip.label.to_string(), rust_i18n::t!("picker-split-mix").to_string());
    assert_eq!(chip.display_name.as_str(), "", "no model tooltip on a split");
}

#[test]
fn a_y_split_says_so() {
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    let chip = chain_block_item_from_block(&split("sp", SplitEnd::Y, vec![], vec![]));
    assert_eq!(chip.label.to_string(), rust_i18n::t!("picker-split-y").to_string());
}
```

Add at the end of `chain_block_item.rs`:

```rust
#[cfg(test)]
#[path = "chain_block_item_tests.rs"]
mod tests;
```

`crates/adapter-gui/src/issue_328_split_chip_tests.rs`:

```rust
//! #328 (spec §5.4) — clicking the Split chip (strip row or compact view, both
//! go through `select-chain-block`) opens the split editor.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Global, Timer, VecModel};

use crate::chain_graph_fixtures_tests::{mix_chain, session_with};
use crate::select_chain_block_callback::{wire, SelectChainBlockCallbackCtx};

#[test]
fn clicking_the_split_chip_opens_the_split_editor() {
    i_slint_backend_testing::init_no_event_loop();
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    let window = crate::AppWindow::new().unwrap();
    let insert_window = crate::ChainInsertWindow::new().unwrap();
    let port_window = crate::ChainPortWindow::new().unwrap();
    wire(
        &window,
        &insert_window,
        &port_window,
        SelectChainBlockCallbackCtx {
            open_compact_window: Rc::new(RefCell::new(None)),
            inline_tab_state: Rc::new(RefCell::new(Default::default())),
            selected_block: Rc::new(RefCell::new(None)),
            block_editor_draft: Rc::new(RefCell::new(None)),
            insert_draft: Rc::new(RefCell::new(None)),
            block_type_options: Rc::new(VecModel::default()),
            block_model_options: Rc::new(VecModel::default()),
            filtered_block_model_options: Rc::new(VecModel::default()),
            block_model_option_labels: Rc::new(VecModel::default()),
            block_parameter_items: Rc::new(VecModel::default()),
            multi_slider_points: Rc::new(VecModel::default()),
            curve_editor_points: Rc::new(VecModel::default()),
            eq_band_curves: Rc::new(VecModel::default()),
            project_session: session_with(vec![mix_chain()]),
            project_chains: Rc::new(VecModel::default()),
            saved_project_snapshot: Rc::new(RefCell::new(None)),
            project_dirty: Rc::new(RefCell::new(false)),
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(Vec::new())),
            open_block_windows: Rc::new(RefCell::new(Vec::new())),
            inline_stream_timer: Rc::new(RefCell::new(None)),
            toast_timer: Rc::new(Timer::default()),
            plugin_info_window: Rc::new(RefCell::new(None)),
            block_stream_reads: Rc::new(application::live_source::NoLiveSource),
            port_draft: Rc::new(RefCell::new(None)),
        },
    );
    let opened: Rc<RefCell<Option<(i32, i32)>>> = Rc::new(RefCell::new(None));
    let seen = opened.clone();
    crate::ChainGraphOverlayState::get(&window).on_open_split_editor(move |ci, kind| *seen.borrow_mut() = Some((ci, kind)));

    window.invoke_select_chain_block(0, 1); // mix_chain: [pre, sp, post]

    assert_eq!(*opened.borrow(), Some((0, 0)), "the split editor (kind 0) opened for chain 0");
}
```

Register `#[cfg(test)] mod issue_328_split_chip_tests;` in `lib.rs`. Append to `tests/issue_328_chain_row_graph_source.rs`:

```rust
#[test]
fn the_icon_set_draws_split_and_mixer_from_svg() {
    let icons = read("ui/components/effect_type_icon.slint");
    assert!(icons.contains("root.icon-kind == \"split\"") && icons.contains("graph-split.svg"));
    assert!(icons.contains("root.icon-kind == \"mixer\"") && icons.contains("graph-mix.svg"));
}
```

- [ ] **Step 2: Run — expect FAIL**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib chain_block_item && nice -n 19 cargo test -p adapter-gui -j 2 --lib issue_328_split_chip && nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_chain_row_graph_source
```
Expected: `left: ("core", "core", "BLOCK") right: ("split", "split", "SPLIT")`; `the split editor (kind 0) opened for chain 0 left: None right: Some((0, 0))` (today `block_editor_data` answers None for a split and the flow toasts "not editable"); the icon assertion fails.

- [ ] **Step 3: Implement.** `chain_block_item.rs`: add `use project::block::SplitEnd;` and, in the `(kind, label)` match (lines 15-28), before the `_ =>` arm:

```rust
        // #328 (spec §5.4): the split is one chip in the strip and the compact view.
        AudioBlockKind::Split(split) => (
            "split".to_string(),
            match &split.end {
                SplitEnd::Mix => rust_i18n::t!("picker-split-mix").to_string(),
                SplitEnd::Y => rust_i18n::t!("picker-split-y").to_string(),
            },
        ),
```

Lines 33-37: the split is not a registered effect type either —

```rust
    // I/O, Insert and Split blocks are not registered effect types, so resolve icon_kind/type_label directly
    let is_io = matches!(
        block.kind,
        AudioBlockKind::Input(_)
            | AudioBlockKind::Output(_)
            | AudioBlockKind::Insert(_)
            | AudioBlockKind::Split(_)
    );
```

and in the `resolved_type_label` match (lines 48-53) add `AudioBlockKind::Split(_) => "SPLIT",`.

`effect_type_icon.slint`: before the `: @image-url("../assets/gear.svg");` fallback (line 55) add

```slint
            : root.icon-kind == "split"          ? @image-url("../assets/graph-split.svg")
            : root.icon-kind == "mixer"          ? @image-url("../assets/graph-mix.svg")
```
(The compact view keys its routing-row icon on `AudioBlockKind::label()`, which Part 1 makes `"split"` for a split — `compact_block_view.rs:114-121` — so the same line draws the compact Split row.)

`select_chain_block_callback.rs`, right after the `SelectionCommand::SelectChainBlock` dispatch block (the `if path.is_none() { … }` from Task 10):

```rust
        // #328 (spec §5.4): the Split chip opens the split editor; the split
        // has no model for the block editor below.
        if matches!(block.kind, AudioBlockKind::Split(_)) {
            drop(session_borrow);
            crate::ChainGraphOverlayState::get(&window).invoke_open_split_editor(chain_index, 0);
            return;
        }
```

- [ ] **Step 4: Run — expect PASS** (same commands as Step 2). No new layout: the Split chip is a `BlockChip` like every other tile, only its data changes.

- [ ] **Step 5: Commit and push**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; git -C "$S" add crates/adapter-gui/src/chain_block_item.rs crates/adapter-gui/src/chain_block_item_tests.rs crates/adapter-gui/src/issue_328_split_chip_tests.rs crates/adapter-gui/src/select_chain_block_callback.rs crates/adapter-gui/src/lib.rs crates/adapter-gui/ui/components/effect_type_icon.slint crates/adapter-gui/tests/issue_328_chain_row_graph_source.rs && git -C "$S" commit -m "feat(#328): the split is one chip in the strip and the compact view"
```
Run the Push gate, push, comment on #328.

---

### Task 14: The split editor and the mixer editor

Required before the first `.slint` line: invoke `claude-plugin:ux-ui`, `slint:slint` and `slint-best-practices` (Global Constraints).

Spec §5.1: "the split and mixer nodes … open the split editor or the mixer editor: a small panel with the knobs from 1.2, rendered from the parameter schema like any block editor". One root-level overlay; the rows come from `split_param_specs()` through the block editor's own row builder; the grid is the block editor's `BlockParamGrid`. An edit is a `SetBlockParameter*` addressed to the split block (spec §1.2: MIDI, scenes and MCP get it for free) through the same apply path as any knob. The drawer's persist path is **not** used: it rebuilds the block from `effect_type`/`model_id` (`block_editor_persist.rs:197-199`), which would destroy the split.

**Files:**
- Create: `crates/adapter-gui/src/split_editor_items.rs`, `crates/adapter-gui/src/split_editor_wiring.rs`, `crates/adapter-gui/ui/components/split_editor_overlay.slint`, `crates/adapter-gui/ui/components/split_editor_test_harness.slint`
- Modify: `crates/adapter-gui/src/block_editor_param_items.rs:109-243` (extract the per-spec builder), `crates/adapter-gui/src/block_param_apply.rs:58-153` (extract the per-block apply), `crates/adapter-gui/ui/app-window.slint` (overlay instance after `ToneDoctorOverlay`, harness export), `crates/adapter-gui/src/desktop_app_block_wiring.rs` (wire call), `crates/adapter-gui/src/lib.rs`, nine `locales/*.yml` (`title-split-editor`, `title-mixer-editor`), `docs/blocks-catalog.md`, `docs/screens.md:7`
- Test: `crates/adapter-gui/src/block_editor_param_items_tests.rs`, `crates/adapter-gui/src/block_param_apply_tests.rs`, `crates/adapter-gui/src/split_editor_items_tests.rs`, `crates/adapter-gui/src/issue_328_split_editor_interaction_tests.rs`

**Interfaces:**
- Consumes: Part 1 `split_params::{split_param_specs, SPLIT_MODE, LEVEL_TO_A, LEVEL_TO_B, BALANCE_A, BALANCE_B, MIX_LEVEL_A, MIX_LEVEL_B, MIX_PAN_A, MIX_PAN_B, MIX_B_POLARITY, MIX_MASTER, MIX_MASTER_SUM}`; Part 2 `SetBlockParameter*` on the split's params; Task 11 `ChainGraphOverlayState` split-editor members, `chain_graph_wiring::chain_at`; `BlockParamGrid` (`block_param_grid.slint:20-103`).
- Produces: `block_editor_param_items::block_parameter_items_for_specs(&[ParameterSpec], &ParameterSet) -> Vec<BlockParameterItem>`; `block_param_apply::apply_parameter_to_block(&Rc<RefCell<Option<ProjectSession>>>, ChainId, BlockId, &str, ParamValue, &Rc<VecModel<ProjectChainItem>>, &[AudioDeviceDescriptor], &[AudioDeviceDescriptor]) -> Result<bool, ApplyParamError>`; `split_editor_items::{enum SplitEditorKind { Split, Mixer }, SplitEditorKind::from_index(i32), SplitEditorKind::title(self) -> String, editor_specs(SplitEditorKind) -> Vec<ParameterSpec>, split_editor_items(&SplitBlock, SplitEditorKind) -> Vec<BlockParameterItem>, option_value(SplitEditorKind, &str, usize) -> Option<String>}`; `split_editor_wiring::{struct SplitEditorWiringCtx, wire(&AppWindow, SplitEditorWiringCtx)}`; Slint `SplitEditorOverlay`, `SplitEditorHarness`.

- [ ] **Step 1: Write the failing tests.** Append to `block_editor_param_items_tests.rs`:

```rust
/// #328: the split editor feeds its own specs (no catalog model) to the same
/// row builder the block editor uses.
#[test]
fn rows_build_from_a_bare_spec_list() {
    use project::param::{bool_parameter, float_parameter, ParameterSet, ParameterUnit};
    let specs = vec![
        bool_parameter("sum", "Sum", None, Some(false)),
        float_parameter("level", "Level", None, Some(100.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
    ];
    let rows = super::block_parameter_items_for_specs(&specs, &ParameterSet::default());
    let shape: Vec<(String, String)> = rows.iter().map(|r| (r.path.to_string(), r.widget_kind.to_string())).collect();
    assert_eq!(shape[0], ("sum".to_string(), "bool".to_string()));
    assert_eq!(shape[1].0, "level");
    assert_eq!(rows[1].numeric_value, 100.0, "the spec default fills an unset value");
}
```

Append to `block_param_apply_tests.rs`:

```rust
/// #328 (spec §1.2): a split knob is an ordinary `SetBlockParameter*` on the
/// split block — no draft, no model.
#[test]
fn a_split_knob_edit_reaches_the_split_params() {
    use crate::chain_graph_fixtures_tests::{chain_in, mix_chain, rows as graph_rows, session_with};
    use project::block::split_params::MIX_PAN_A;
    let session = session_with(vec![mix_chain()]);
    let chain_id = chain_in(&session, 0).id;
    super::apply_parameter_to_block(
        &session,
        chain_id,
        BlockId("sp".into()),
        MIX_PAN_A,
        ParamValue::Number(-50.0),
        &graph_rows(),
        &[],
        &[],
    )
    .expect("the split exists");
    let AudioBlockKind::Split(split) = &chain_in(&session, 0).blocks[1].kind else { panic!("block 1 is the split") };
    assert_eq!(split.params.get(MIX_PAN_A).and_then(|v| v.as_f32()), Some(-50.0));
}
```

`crates/adapter-gui/src/split_editor_items_tests.rs`:

```rust
//! #328 (spec §1.2) — the split editor and the mixer editor show their knobs.

use super::*;
use project::block::split_params::{
    default_split_params, BALANCE_A, BALANCE_B, LEVEL_TO_A, LEVEL_TO_B, MIX_B_POLARITY,
    MIX_LEVEL_A, MIX_LEVEL_B, MIX_MASTER, MIX_MASTER_SUM, MIX_PAN_A, MIX_PAN_B, SPLIT_MODE,
};
use project::block::{SplitBlock, SplitEnd};

fn split_block() -> SplitBlock {
    SplitBlock { end: SplitEnd::Mix, params: default_split_params(), a: vec![], b: vec![] }
}

fn paths(kind: SplitEditorKind) -> Vec<String> {
    let mut p: Vec<String> = split_editor_items(&split_block(), kind).iter().map(|i| i.path.to_string()).collect();
    p.sort();
    p
}

fn sorted(keys: &[&str]) -> Vec<String> {
    let mut k: Vec<String> = keys.iter().map(|s| s.to_string()).collect();
    k.sort();
    k
}

#[test]
fn the_split_editor_shows_mode_levels_and_balances() {
    assert_eq!(paths(SplitEditorKind::Split), sorted(&[SPLIT_MODE, LEVEL_TO_A, LEVEL_TO_B, BALANCE_A, BALANCE_B]));
}

#[test]
fn the_mixer_editor_shows_levels_pans_polarity_and_master() {
    assert_eq!(
        paths(SplitEditorKind::Mixer),
        sorted(&[MIX_LEVEL_A, MIX_LEVEL_B, MIX_PAN_A, MIX_PAN_B, MIX_B_POLARITY, MIX_MASTER, MIX_MASTER_SUM])
    );
}

#[test]
fn switches_and_choices_get_their_widgets() {
    let rows = split_editor_items(&split_block(), SplitEditorKind::Mixer);
    let kind = |path: &str| rows.iter().find(|r| r.path.as_str() == path).unwrap().widget_kind.to_string();
    assert_eq!(kind(MIX_MASTER_SUM), "bool");
    assert_eq!(kind(MIX_B_POLARITY), "enum");
}

#[test]
fn an_option_index_names_its_value() {
    assert_eq!(option_value(SplitEditorKind::Split, SPLIT_MODE, 1).as_deref(), Some("dual_mono"));
    assert_eq!(option_value(SplitEditorKind::Mixer, MIX_B_POLARITY, 1).as_deref(), Some("invert"));
    assert_eq!(option_value(SplitEditorKind::Mixer, MIX_PAN_A, 0), None, "not a choice");
}

#[test]
fn the_overlay_kind_index_maps_both_ways() {
    assert_eq!(SplitEditorKind::from_index(0), Some(SplitEditorKind::Split));
    assert_eq!(SplitEditorKind::from_index(1), Some(SplitEditorKind::Mixer));
    assert_eq!(SplitEditorKind::from_index(2), None);
}
```

`crates/adapter-gui/src/issue_328_split_editor_interaction_tests.rs`:

```rust
//! #328 — the split/mixer editor overlay, driven by real pointer events, and
//! the wiring end to end (open → rows → edit → project).

use std::cell::RefCell;
use std::rc::Rc;

use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, Model, ModelRc, Timer, VecModel};

use project::block::split_params::{default_split_params, MIX_MASTER_SUM, MIX_PAN_A};
use project::block::{AudioBlockKind, SplitBlock, SplitEnd};

use crate::chain_graph_fixtures_tests::{chain_in, mix_chain, rows, session_with};
use crate::split_editor_items::{split_editor_items, SplitEditorKind};
use crate::split_editor_wiring::{wire, SplitEditorWiringCtx};
use crate::{ChainGraphOverlayState, SplitEditorHarness};

fn click(w: &impl ComponentHandle, el: &i_slint_backend_testing::ElementHandle) {
    let (pos, size) = (el.absolute_position(), el.size());
    let p = LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: p });
    win.dispatch_event(WindowEvent::PointerPressed { position: p, button: PointerEventButton::Left });
    win.dispatch_event(WindowEvent::PointerReleased { position: p, button: PointerEventButton::Left });
    win.dispatch_event(WindowEvent::PointerExited);
}

fn open_mixer(h: &SplitEditorHarness) {
    let split = SplitBlock { end: SplitEnd::Mix, params: default_split_params(), a: vec![], b: vec![] };
    let state = ChainGraphOverlayState::get(h);
    state.set_split_editor_items(ModelRc::new(VecModel::from(split_editor_items(&split, SplitEditorKind::Mixer))));
    state.set_split_editor_chain_index(0);
    state.set_split_editor_split_id("sp".into());
    state.set_split_editor_title("Mixer".into());
    state.set_split_editor_open(true);
}

#[test]
fn toggling_master_sum_reports_the_split_and_the_knob() {
    i_slint_backend_testing::init_no_event_loop();
    let h = SplitEditorHarness::new().unwrap();
    open_mixer(&h);
    let got: Rc<RefCell<Option<(i32, String, String, bool)>>> = Rc::new(RefCell::new(None));
    let seen = got.clone();
    ChainGraphOverlayState::get(&h).on_split_editor_bool(move |ci, id, path, on| {
        *seen.borrow_mut() = Some((ci, id.to_string(), path.to_string(), on));
    });
    h.show().unwrap();
    let switch = i_slint_backend_testing::ElementHandle::find_by_element_type_name(&h, "ToggleSwitch")
        .next()
        .expect("the master-sum switch is drawn");
    click(&h, &switch);
    assert_eq!(*got.borrow(), Some((0, "sp".to_string(), MIX_MASTER_SUM.to_string(), true)));
}

#[test]
fn the_close_button_closes_the_editor() {
    i_slint_backend_testing::init_no_event_loop();
    let h = SplitEditorHarness::new().unwrap();
    open_mixer(&h);
    h.show().unwrap();
    let close = i_slint_backend_testing::ElementHandle::find_by_element_id(&h, "SplitEditorOverlay::close-ta")
        .next()
        .expect("close button");
    click(&h, &close);
    assert!(!ChainGraphOverlayState::get(&h).get_split_editor_open());
}

#[test]
fn opening_the_mixer_lists_its_knobs_and_an_edit_reaches_the_split() {
    i_slint_backend_testing::init_no_event_loop();
    let app = crate::AppWindow::new().unwrap();
    let session = session_with(vec![mix_chain()]);
    wire(
        &app,
        SplitEditorWiringCtx {
            project_session: session.clone(),
            project_chains: rows(),
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(Vec::new())),
            toast_timer: Rc::new(Timer::default()),
        },
    );
    let state = ChainGraphOverlayState::get(&app);
    state.invoke_open_split_editor(0, 1);
    assert!(state.get_split_editor_open());
    assert_eq!(state.get_split_editor_items().row_count(), 7);
    assert_eq!(state.get_split_editor_split_id().as_str(), "sp");

    state.invoke_split_editor_number(0, "sp".into(), MIX_PAN_A.into(), -50.0);

    let AudioBlockKind::Split(split) = &chain_in(&session, 0).blocks[1].kind else { panic!("block 1 is the split") };
    assert_eq!(split.params.get(MIX_PAN_A).and_then(|v| v.as_f32()), Some(-50.0));
    let row = state.get_split_editor_items().iter().find(|r| r.path.as_str() == MIX_PAN_A).unwrap();
    assert_eq!(row.numeric_value, -50.0, "the row follows the edit in place");
}
```

Register `#[cfg(test)] mod issue_328_split_editor_interaction_tests;` in `lib.rs`.

- [ ] **Step 2: Run — compile error; stubs; behavioural red.** Stubs: `block_parameter_items_for_specs` → `Vec::new()`; `apply_parameter_to_block` → `Err(ApplyParamError::NotAddressable)`; `split_editor_items` → `Vec::new()`, `option_value` → `None`, `from_index` → `None`, `title` → `String::new()`, `editor_specs` → `Vec::new()`; `split_editor_wiring::wire` → empty body; `split_editor_overlay.slint` with an empty `export component SplitEditorOverlay inherits Rectangle { }`; the harness as in Step 3. Register `mod split_editor_items;` and `mod split_editor_wiring;` in `lib.rs`; add the harness import/export to `app-window.slint` (Step 4).

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib rows_build_from_a_bare_spec_list && nice -n 19 cargo test -p adapter-gui -j 2 --lib a_split_knob_edit_reaches && nice -n 19 cargo test -p adapter-gui -j 2 --lib split_editor
```
Expected: `index out of bounds: the len is 0`, `the split exists: NotAddressable`, `left: [] right: ["balance_a", …]`, `the master-sum switch is drawn`, `assertion failed: state.get_split_editor_open()`.

- [ ] **Step 3: Implement.**

`block_editor_param_items.rs` — behaviour-preserving extraction. The row closure (`block_editor_param_items.rs:118-241`, from `.map(|spec| {` through the `})` before `.collect()` at line 242) does not move and no line of it changes: only the head of the iterator chain is split off into a function over a spec slice. Replace lines 111-117:

```rust
    let Ok(schema) = schema_for_block_model(effect_type, model_id) else {
        return Vec::new();
    };
    schema
        .parameters
        .iter()
        .filter(|spec| spec.path != "enabled")
```

with

```rust
    let Ok(schema) = schema_for_block_model(effect_type, model_id) else {
        return Vec::new();
    };
    block_parameter_items_for_specs(&schema.parameters, params)
}

/// One row per spec (#328: the split editor has specs but no catalog model).
pub(crate) fn block_parameter_items_for_specs(
    specs: &[ParameterSpec],
    params: &ParameterSet,
) -> Vec<BlockParameterItem> {
    specs
        .iter()
        .filter(|spec| spec.path != "enabled")
```

so the untouched `.map(|spec| { … })` and `.collect()` / `}` that follow now end `block_parameter_items_for_specs`. Change line 19 to `use project::param::{ParameterDomain, ParameterSet, ParameterSpec, ParameterWidget};`. The file grows by 8 lines (247 → 255). Existing tests of `block_parameter_items_for_model` stay green unchanged.

`block_param_apply.rs` — after the `(chain_id, block_id)` resolution (end of Task 10's version of `apply_block_parameter`), the rest of the function (`let command = match value {` through the final `Ok(true)`, lines 95-153 before Task 10) moves unchanged into `apply_parameter_to_block`, and `apply_block_parameter` ends with the call:

```rust
    apply_parameter_to_block(
        project_session,
        chain_id,
        block_id,
        path,
        value,
        project_chains,
        input_chain_devices,
        output_chain_devices,
    )
}

/// Commit `value` at `path` on the block `block_id` of chain `chain_id`: the
/// command on the bus, the live resync, the republished rows. `Ok(false)` ⇒
/// the dispatcher reported no change. #328: the split editor calls this
/// directly — a split knob has a block id but no editor draft.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_parameter_to_block(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    chain_id: ChainId,
    block_id: BlockId,
    path: &str,
    value: ParamValue,
    project_chains: &Rc<VecModel<ProjectChainItem>>,
    input_chain_devices: &[AudioDeviceDescriptor],
    output_chain_devices: &[AudioDeviceDescriptor],
) -> Result<bool, ApplyParamError> {
    let command = match value {
        ParamValue::Number(value) => Command::Block(BlockCommand::SetBlockParameterNumber {
            chain: chain_id.clone(),
            block: block_id,
            path: path.to_string(),
            value,
        }),
        ParamValue::Text(value) => Command::Block(BlockCommand::SetBlockParameterText {
            chain: chain_id.clone(),
            block: block_id,
            path: path.to_string(),
            value,
        }),
        ParamValue::Bool(value) => Command::Block(BlockCommand::SetBlockParameterBool {
            chain: chain_id.clone(),
            block: block_id,
            path: path.to_string(),
            value,
        }),
        ParamValue::Option { value, index } => {
            Command::Block(BlockCommand::SelectBlockParameterOption {
                chain: chain_id.clone(),
                block: block_id,
                path: path.to_string(),
                value,
                index,
            })
        }
    };
    let changed = {
        let borrowed = project_session.borrow();
        let Some(session) = borrowed.as_ref() else {
            return Err(ApplyParamError::NotAddressable);
        };
        session
            .dispatcher
            .dispatch(command)
            .map_err(|e| ApplyParamError::Failed(e.to_string()))?
            .into_iter()
            .any(|event| matches!(event, Event::BlockParameterChanged { .. }))
    };
    if !changed {
        return Ok(false);
    }
    let mut borrowed = project_session.borrow_mut();
    let Some(session) = borrowed.as_mut() else {
        return Err(ApplyParamError::NotAddressable);
    };
    // #614: the command records the value; the live chain only plays it once
    // its runtime is rebuilt.
    request_chain_sync(session, &chain_id).map_err(|e| ApplyParamError::Failed(e.to_string()))?;
    replace_project_chains(
        project_chains,
        &session.project.borrow(),
        input_chain_devices,
        output_chain_devices,
        &[],
    );
    Ok(true)
}
```
with `use domain::ids::{BlockId, ChainId};` added to the imports. (`#[allow(clippy::too_many_arguments)]` is the repo's existing convention for wiring functions with eight or more parameters, e.g. `meter_wiring_poll.rs:171`.) The file grows by about 25 lines (158 → ~185).

`crates/adapter-gui/src/split_editor_items.rs`:

```rust
//! Responsibility: lists the knobs a split editor shows.
//!
//! #328 (spec §1.2): the split side (mode, levels into A and B, the Mode II
//! balances) and the mixer side (levels, pans, B polarity, master, master
//! sum) are two views of one `SplitBlock.params`. The rows are the block
//! editor's own (`block_parameter_items_for_specs`), fed `split_param_specs()`.

use project::block::split_params::{
    split_param_specs, BALANCE_A, BALANCE_B, LEVEL_TO_A, LEVEL_TO_B, MIX_B_POLARITY, MIX_LEVEL_A,
    MIX_LEVEL_B, MIX_MASTER, MIX_MASTER_SUM, MIX_PAN_A, MIX_PAN_B, SPLIT_MODE,
};
use project::block::SplitBlock;
use project::param::{ParameterDomain, ParameterSpec};

use crate::block_editor_param_items::block_parameter_items_for_specs;
use crate::BlockParameterItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplitEditorKind {
    Split,
    Mixer,
}

const SPLIT_KEYS: [&str; 5] = [SPLIT_MODE, LEVEL_TO_A, LEVEL_TO_B, BALANCE_A, BALANCE_B];
const MIXER_KEYS: [&str; 7] = [
    MIX_LEVEL_A,
    MIX_LEVEL_B,
    MIX_PAN_A,
    MIX_PAN_B,
    MIX_B_POLARITY,
    MIX_MASTER,
    MIX_MASTER_SUM,
];

impl SplitEditorKind {
    /// The overlay's `split-editor-kind`: 0 = split, 1 = mixer.
    pub(crate) fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::Split),
            1 => Some(Self::Mixer),
            _ => None,
        }
    }

    fn keys(self) -> &'static [&'static str] {
        match self {
            Self::Split => &SPLIT_KEYS,
            Self::Mixer => &MIXER_KEYS,
        }
    }

    pub(crate) fn title(self) -> String {
        match self {
            Self::Split => rust_i18n::t!("title-split-editor"),
            Self::Mixer => rust_i18n::t!("title-mixer-editor"),
        }
        .to_string()
    }
}

/// The specs one editor shows, in `split_param_specs()` order.
pub(crate) fn editor_specs(kind: SplitEditorKind) -> Vec<ParameterSpec> {
    let keys = kind.keys();
    split_param_specs()
        .into_iter()
        .filter(|spec| keys.contains(&spec.path.as_str()))
        .collect()
}

pub(crate) fn split_editor_items(split: &SplitBlock, kind: SplitEditorKind) -> Vec<BlockParameterItem> {
    block_parameter_items_for_specs(&editor_specs(kind), &split.params)
}

/// The value a choice row's `index` stands for (`SelectBlockParameterOption`
/// carries both, `block_param_apply.rs:30-36`).
pub(crate) fn option_value(kind: SplitEditorKind, path: &str, index: usize) -> Option<String> {
    let spec = editor_specs(kind).into_iter().find(|spec| spec.path == path)?;
    match spec.domain {
        ParameterDomain::Enum { options } => options.into_iter().nth(index).map(|o| o.value),
        _ => None,
    }
}

#[cfg(test)]
#[path = "split_editor_items_tests.rs"]
mod tests;
```

`crates/adapter-gui/src/split_editor_wiring.rs`:

```rust
//! Responsibility: drives the split editor overlay.
//!
//! #328 (spec §1.2, §5.1). One overlay edits either side of the split: kind 0
//! shows the split's knobs, kind 1 the mixer's. An edit goes on the bus as a
//! `SetBlockParameter*` on the split block (`apply_parameter_to_block`), never
//! through the drawer's persist, which would rebuild the block from a model.
//! Rows update in place so a knob being dragged keeps its identity (#715).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, Model, ModelRc, Timer, VecModel};

use domain::ids::BlockId;
use domain::AudioDeviceDescriptor;

use crate::block_param_apply::{apply_parameter_to_block, ApplyParamError, ParamValue};
use crate::chain_block_lists::split_of;
use crate::chain_graph_wiring::chain_at;
use crate::helpers::set_status_error;
use crate::split_editor_items::{option_value, split_editor_items, SplitEditorKind};
use crate::state::ProjectSession;
use crate::{AppWindow, BlockParameterItem, ChainGraphOverlayState, ProjectChainItem};

pub(crate) struct SplitEditorWiringCtx {
    pub(crate) project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub(crate) project_chains: Rc<VecModel<ProjectChainItem>>,
    pub(crate) input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub(crate) output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub(crate) toast_timer: Rc<Timer>,
}

struct Editor {
    ctx: SplitEditorWiringCtx,
    rows: Rc<VecModel<BlockParameterItem>>,
}

pub(crate) fn wire(window: &AppWindow, ctx: SplitEditorWiringCtx) {
    let editor = Rc::new(Editor {
        ctx,
        rows: Rc::new(VecModel::default()),
    });
    let state = ChainGraphOverlayState::get(window);
    state.set_split_editor_items(ModelRc::from(editor.rows.clone()));
    {
        let (weak, editor) = (window.as_weak(), editor.clone());
        state.on_open_split_editor(move |chain_index, kind_index| {
            let Some(window) = weak.upgrade() else { return };
            let Some(kind) = SplitEditorKind::from_index(kind_index) else { return };
            let Some(chain) = chain_at(&editor.ctx.project_session, chain_index) else { return };
            let Some((_, split_id, split)) = split_of(&chain) else { return };
            editor.rows.set_vec(split_editor_items(split, kind));
            let state = ChainGraphOverlayState::get(&window);
            state.set_split_editor_chain_index(chain_index);
            state.set_split_editor_kind(kind_index);
            state.set_split_editor_split_id(split_id.0.as_str().into());
            state.set_split_editor_title(kind.title().into());
            state.set_split_editor_open(true);
        });
    }
    {
        let (weak, editor) = (window.as_weak(), editor.clone());
        state.on_split_editor_number(move |chain_index, split_id, path, value| {
            let Some(window) = weak.upgrade() else { return };
            commit(&window, &editor, chain_index, &split_id, &path, ParamValue::Number(value as f64));
        });
    }
    {
        let (weak, editor) = (window.as_weak(), editor.clone());
        state.on_split_editor_option(move |chain_index, split_id, path, index| {
            let Some(window) = weak.upgrade() else { return };
            let kind = SplitEditorKind::from_index(ChainGraphOverlayState::get(&window).get_split_editor_kind());
            let Some(value) = kind.and_then(|k| option_value(k, &path, index as usize)) else { return };
            let choice = ParamValue::Option { value, index: index as usize };
            commit(&window, &editor, chain_index, &split_id, &path, choice);
        });
    }
    {
        let (weak, editor) = (window.as_weak(), editor.clone());
        state.on_split_editor_bool(move |chain_index, split_id, path, on| {
            let Some(window) = weak.upgrade() else { return };
            commit(&window, &editor, chain_index, &split_id, &path, ParamValue::Bool(on));
        });
    }
}

fn commit(window: &AppWindow, editor: &Editor, chain_index: i32, split_id: &str, path: &str, value: ParamValue) {
    let Some(chain) = chain_at(&editor.ctx.project_session, chain_index) else { return };
    let result = {
        let inputs = editor.ctx.input_chain_devices.borrow();
        let outputs = editor.ctx.output_chain_devices.borrow();
        apply_parameter_to_block(
            &editor.ctx.project_session,
            chain.id.clone(),
            BlockId(split_id.to_string()),
            path,
            value,
            &editor.ctx.project_chains,
            &inputs,
            &outputs,
        )
    };
    match result {
        Ok(_) => refresh_rows(window, editor, chain_index),
        Err(ApplyParamError::Failed(err)) => set_status_error(
            window,
            &editor.ctx.toast_timer,
            &rust_i18n::t!("error-graph-action", err = err),
        ),
        Err(ApplyParamError::NotAddressable) => log::warn!("[split-editor] split {split_id} is gone"),
    }
}

/// Same row count ⇒ update each row in place (the knob under the pointer
/// keeps its identity); otherwise swap the list.
fn refresh_rows(window: &AppWindow, editor: &Editor, chain_index: i32) {
    let Some(kind) = SplitEditorKind::from_index(ChainGraphOverlayState::get(window).get_split_editor_kind()) else { return };
    let Some(chain) = chain_at(&editor.ctx.project_session, chain_index) else { return };
    let Some((_, _, split)) = split_of(&chain) else { return };
    let fresh = split_editor_items(split, kind);
    if fresh.len() != editor.rows.row_count() {
        editor.rows.set_vec(fresh);
        return;
    }
    for (index, row) in fresh.into_iter().enumerate() {
        editor.rows.set_row_data(index, row);
    }
}
```

`crates/adapter-gui/ui/components/split_editor_overlay.slint`:

```slint
// Responsibility: renders the split editor overlay.
//
// #328 (spec §5.1): a small root-level panel — not a PopupWindow, whose
// content does not receive clicks (#749/#761) — with the split's or the
// mixer's knobs drawn by the block editor's own grid. State lives in
// ChainGraphOverlayState; a click outside the card closes it.

import { ChainGraphOverlayState } from "../chain_graph_overlay_globals.slint";
import { BlockParamGrid } from "block_param_grid.slint";

export component SplitEditorOverlay inherits Rectangle {
    private property <int> count: ChainGraphOverlayState.split-editor-items.length;
    private property <int> cols: Math.max(1, Math.min(4, root.count));
    private property <length> cell: 96px;
    private property <length> grid-height: Math.ceil(root.count / root.cols) * root.cell;

    x: 0;
    y: 0;
    visible: ChainGraphOverlayState.split-editor-open;

    backdrop := TouchArea {
        clicked => { ChainGraphOverlayState.split-editor-open = false; }
    }

    if ChainGraphOverlayState.split-editor-open : Rectangle {
        x: (root.width - self.width) / 2;
        y: (root.height - self.height) / 2;
        width: root.cols * root.cell + 32px;
        height: 16px + 44px + 12px + root.grid-height + 16px;
        background: #111114;
        border-radius: 10px;
        border-width: 1px;
        border-color: #27272a;
        drop-shadow-blur: 20px;
        drop-shadow-color: #000000c0;

        // Swallow clicks on the card so only the backdrop closes it.
        TouchArea { }

        VerticalLayout {
            padding: 16px;
            spacing: 12px;

            HorizontalLayout {
                height: 44px;
                spacing: 8px;
                Text {
                    horizontal-stretch: 1;
                    text: ChainGraphOverlayState.split-editor-title;
                    color: #f1f4f8;
                    font-size: 22px;
                    font-weight: 800;
                    vertical-alignment: center;
                }
                Rectangle {
                    width: 88px;
                    border-radius: 6px;
                    background: close-ta.has-hover ? #2b3340 : #1b2230;
                    close-ta := TouchArea {
                        mouse-cursor: pointer;
                        clicked => { ChainGraphOverlayState.split-editor-open = false; }
                    }
                    Text {
                        text: @tr("btn-close");
                        color: #d8e0ff;
                        font-size: 18px;
                        horizontal-alignment: center;
                        vertical-alignment: center;
                    }
                }
            }

            BlockParamGrid {
                height: root.grid-height;
                param-items: ChainGraphOverlayState.split-editor-items;
                show-params: true;
                show-overlays: false;
                has-param-tabs: false;
                has-eq-widget: false;
                grid-area-x: 0px;
                grid-cell-w: root.cell;
                grid-cols: root.cols;
                grid-base-y: 0px;
                grid-row-stride: root.cell;
                content-height: root.grid-height;
                panel-text: #f1f4f8;
                update-number(path, value) => {
                    ChainGraphOverlayState.split-editor-number(ChainGraphOverlayState.split-editor-chain-index, ChainGraphOverlayState.split-editor-split-id, path, value);
                }
                select-option(path, index) => {
                    ChainGraphOverlayState.split-editor-option(ChainGraphOverlayState.split-editor-chain-index, ChainGraphOverlayState.split-editor-split-id, path, index);
                }
                update-bool(path, value) => {
                    ChainGraphOverlayState.split-editor-bool(ChainGraphOverlayState.split-editor-chain-index, ChainGraphOverlayState.split-editor-split-id, path, value);
                }
                pick-file(path) => { }
            }
        }
    }
}
```

`crates/adapter-gui/ui/components/split_editor_test_harness.slint`:

```slint
// Responsibility: hosts the split editor overlay for a test.
// #328 TEST-ONLY: SplitEditorOverlay in a real Window so the headless tests
// (i-slint-backend-testing) can click its knobs. Imported by app-window.slint
// only so slint-build emits the Rust type; never shown in the running app.
import { SplitEditorOverlay } from "split_editor_overlay.slint";
import "../fonts/BebasNeue.ttf";

export component SplitEditorHarness inherits Window {
    width: 800px;
    height: 500px;
    background: #040708;
    SplitEditorOverlay {
        width: root.width;
        height: root.height;
    }
}
```

- [ ] **Step 4: Wire it in.** `app-window.slint`: `import { SplitEditorOverlay } from "components/split_editor_overlay.slint";`, `import { SplitEditorHarness } from "components/split_editor_test_harness.slint";` (add `SplitEditorHarness` to the harness export on line 28), and after the `ToneDoctorOverlay { … }` instance:

```slint
    // #328: the split / mixer knobs (root level: never clipped by the chain list).
    SplitEditorOverlay { width: root.width; height: root.height; }
```

`desktop_app_block_wiring.rs`, after the Task 11 call:

```rust
    // --- #328: the split / mixer editor overlay ---
    crate::split_editor_wiring::wire(
        deps.window,
        crate::split_editor_wiring::SplitEditorWiringCtx {
            project_session: deps.project_session.clone(),
            project_chains: deps.project_chains.clone(),
            input_chain_devices: deps.input_chain_devices.clone(),
            output_chain_devices: deps.output_chain_devices.clone(),
            toast_timer: deps.toast_timer.clone(),
        },
    );
```

Add `title-split-editor` and `title-mixer-editor` to the nine `locales/*.yml` (Appendix A).

- [ ] **Step 5: Run — expect PASS**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib block_editor_param_items && nice -n 19 cargo test -p adapter-gui -j 2 --lib block_param_apply && nice -n 19 cargo test -p adapter-gui -j 2 --lib split_editor && nice -n 19 cargo test -p adapter-gui -j 2 --lib issue_328_split_editor_interaction
```
Expected: all pass, including the unchanged `block_param_apply` and `block_editor_param_items` tests.

- [ ] **Step 6: Render and look.** `slint-render` cannot call Rust, so the overlay's state is set from a scratch render file under the git-ignored `target/`. Create `target/issue-328-render/split_editor_render.slint`:

```slint
// Responsibility: renders the mixer editor with fixed knobs for a PNG check.
import { SplitEditorOverlay } from "../../crates/adapter-gui/ui/components/split_editor_overlay.slint";
import { ChainGraphOverlayState } from "../../crates/adapter-gui/ui/chain_graph_overlay_globals.slint";
import "../../crates/adapter-gui/ui/fonts/BebasNeue.ttf";

export component SplitEditorRender inherits Window {
    width: 800px;
    height: 500px;
    background: #040708;
    init => {
        ChainGraphOverlayState.split-editor-title = "Mixer";
        ChainGraphOverlayState.split-editor-items = [
            { path: "mix_level_a", label: "LEVEL A", widget_kind: "slider", value_text: "100", numeric_value: 100, numeric_min: 0, numeric_max: 100, numeric_step: 1, tab_slot: 0, strip_line: -1 },
            { path: "mix_level_b", label: "LEVEL B", widget_kind: "slider", value_text: "100", numeric_value: 100, numeric_min: 0, numeric_max: 100, numeric_step: 1, tab_slot: 0, strip_line: -1 },
            { path: "mix_pan_a", label: "PAN A", widget_kind: "slider", value_text: "-50", numeric_value: -50, numeric_min: -50, numeric_max: 50, numeric_step: 1, tab_slot: 0, strip_line: -1 },
            { path: "mix_pan_b", label: "PAN B", widget_kind: "slider", value_text: "50", numeric_value: 50, numeric_min: -50, numeric_max: 50, numeric_step: 1, tab_slot: 0, strip_line: -1 },
            { path: "mix_b_polarity", label: "B POLARITY", widget_kind: "enum", option_labels: ["Normal", "Invert"], option_values: ["normal", "invert"], selected_option_index: 0, tab_slot: 0, strip_line: -1 },
            { path: "mix_master", label: "MASTER", widget_kind: "slider", value_text: "50", numeric_value: 50, numeric_min: 0, numeric_max: 100, numeric_step: 1, tab_slot: 0, strip_line: -1 },
            { path: "mix_master_sum", label: "MASTER SUM", widget_kind: "bool", bool_value: false, tab_slot: 0, strip_line: -1 },
        ];
        ChainGraphOverlayState.split-editor-open = true;
    }
    SplitEditorOverlay { width: root.width; height: root.height; }
}
```

then

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && tools/slint-render/target/release/slint-render target/issue-328-render/split_editor_render.slint SplitEditorRender target/issue-328-render/mixer.png 800 500
```
Open the PNG. Self-critique: two rows of knobs, labels readable, close target ≥ 44 px, 8 px rhythm, the card centred, nothing clipped. Fix in `split_editor_overlay.slint`.

- [ ] **Step 7: Docs.** `docs/blocks-catalog.md` already has the `**Split**` row and the "## Chain split (#328)" section with the full knob table (Part 1 Tasks 1 and 4). Do not repeat them; append this paragraph at the end of that section, after Part 1's "Rules (…)" paragraph:

```markdown
In the GUI, clicking the split node of a chain graph (or the Split chip in touch and compact views) opens the
**split editor** — mode, levels into A and B, balances — and clicking the mixer node opens the **mixer editor** —
levels, pans, B polarity, master, master sum. Both are a small root-level panel drawn from `split_param_specs()`
by the block editor's own grid; each knob is an ordinary `SetBlockParameter*` on the split block, so MIDI
mapping, scenes and MCP reach them like any knob. Mode II needs a stereo or dual-mono signal before any mono
block; with a mono source both balances give the same signal. On a one-channel output, pan has no audible
effect (the route averages L and R).
```

`docs/screens.md` line 7, append: *The split and mixer nodes open small knob panels (split: mode, levels into A/B, balances; mixer: levels, pans, B polarity, master, master sum).*

- [ ] **Step 8: Commit and push**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; git -C "$S" add crates/adapter-gui/src/block_editor_param_items.rs crates/adapter-gui/src/block_editor_param_items_tests.rs crates/adapter-gui/src/block_param_apply.rs crates/adapter-gui/src/block_param_apply_tests.rs crates/adapter-gui/src/split_editor_items.rs crates/adapter-gui/src/split_editor_items_tests.rs crates/adapter-gui/src/split_editor_wiring.rs crates/adapter-gui/src/issue_328_split_editor_interaction_tests.rs crates/adapter-gui/src/desktop_app_block_wiring.rs crates/adapter-gui/src/lib.rs crates/adapter-gui/ui/components/split_editor_overlay.slint crates/adapter-gui/ui/components/split_editor_test_harness.slint crates/adapter-gui/ui/app-window.slint crates/adapter-gui/locales/de-DE.yml crates/adapter-gui/locales/en-US.yml crates/adapter-gui/locales/es-ES.yml crates/adapter-gui/locales/fr-FR.yml crates/adapter-gui/locales/hi-IN.yml crates/adapter-gui/locales/ja-JP.yml crates/adapter-gui/locales/ko-KR.yml crates/adapter-gui/locales/pt-BR.yml crates/adapter-gui/locales/zh-CN.yml docs/blocks-catalog.md docs/screens.md && git -C "$S" commit -m "feat(#328): split and mixer editors"
```
Run the Push gate, push, comment on #328.

---

### Task 15: The endpoint checklist

Required before the first `.slint` line: invoke `claude-plugin:ux-ui`, `slint:slint` and `slint-best-practices` (Global Constraints).

Spec §5.3: clicking an input or output node lists every input (or output) of the chain's E/S, checked by default; toggling one dispatches `SetChainEndpointEnabled`; nothing is added to or removed from the E/S; unchecking every endpoint of a node is allowed. Root-level overlay, rows from `ChannelPicker` (#880).

**Files:**
- Create: `crates/adapter-gui/src/endpoint_toggle.rs`, `crates/adapter-gui/src/endpoint_checklist_wiring.rs`, `crates/adapter-gui/ui/components/endpoint_checklist_overlay.slint`, `crates/adapter-gui/ui/components/endpoint_checklist_test_harness.slint`
- Modify: `crates/adapter-gui/ui/components/channel_picker.slint:90-96,159-164` (caption input), `crates/adapter-gui/ui/app-window.slint` (overlay instance after `SplitEditorOverlay`, harness export), `crates/adapter-gui/src/desktop_app_block_wiring.rs` (wire call), `crates/adapter-gui/src/lib.rs`, nine `locales/*.yml` (four `title-endpoints-*` keys), `crates/adapter-gui/translations/adapter-gui.pot` + nine `.po` (three keys, via the script), `docs/audio-config.md` (new section before `## JACK lifecycle (Linux only)`), `docs/screens.md:7`
- Test: `crates/adapter-gui/src/endpoint_toggle_tests.rs`, `crates/adapter-gui/src/issue_328_endpoint_checklist_interaction_tests.rs`; `tests/issue_880_channel_picker.rs` stays unchanged and green

**Interfaces:**
- Consumes: Task 5 `endpoint_rows`, `EndpointRow`; Task 11 `RowsTarget`, `GestureError`, `chain_at`, `ChainGraphOverlayState` checklist members; Part 2 `ChainCommand::SetChainEndpointEnabled { chain, node, io, endpoint, enabled }`.
- Produces: `endpoint_toggle::set_endpoint_enabled(&Rc<RefCell<Option<ProjectSession>>>, usize, &str, usize, bool, &RowsTarget) -> Result<(), GestureError>`; `endpoint_checklist_wiring::{struct EndpointChecklistWiringCtx, wire(&AppWindow, EndpointChecklistWiringCtx), checklist_items(&[EndpointRow]) -> Vec<ChannelOptionItem>, checklist_title(&EndpointNode) -> String}`; `ChannelPicker.total-caption: string`; Slint `EndpointChecklistOverlay`, `EndpointChecklistHarness`.

- [ ] **Step 1: Write the failing tests.** `crates/adapter-gui/src/endpoint_toggle_tests.rs` (recording dispatcher: the GUI's job is the exact command; Part 2 owns its effect):

```rust
//! #328 (spec §5.3) — a checklist row becomes exactly one
//! `SetChainEndpointEnabled` for that node and that endpoint, then a resync.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, recording_session, rows};
use crate::chain_graph_ids::{INPUT_NODE_ID, OUTPUT_NODE_ID, PATH_B_OUTPUT_NODE_ID};
use application::command::{ChainCommand, Command};
use project::endpoint_disables::EndpointNode;
use slint::VecModel;
use std::rc::Rc;

fn target(rows: &Rc<VecModel<crate::ProjectChainItem>>) -> RowsTarget<'_> {
    RowsTarget { model: rows, inputs: &[], outputs: &[] }
}

#[test]
fn unchecking_an_input_names_that_endpoint_then_resyncs() {
    let (session, recorder) = recording_session(vec![chain(vec![core("amp")])]);
    let rows = rows();
    set_endpoint_enabled(&session, 0, INPUT_NODE_ID, 1, false, &target(&rows)).expect("toggle");
    let seen = recorder.seen.borrow();
    assert!(
        matches!(&seen[0], Command::Chain(ChainCommand::SetChainEndpointEnabled { node: EndpointNode::Input, io, endpoint, enabled: false, .. }) if io == "main" && endpoint == "In 2"),
        "got {:?}",
        seen[0]
    );
    assert!(matches!(&seen[1], Command::Chain(ChainCommand::SyncChainRuntime { .. })), "#614 resync");
}

#[test]
fn unchecking_the_last_output_is_allowed() {
    let mut only_main = chain(vec![]);
    only_main.io_binding_ids = vec!["main".into()];
    let (session, recorder) = recording_session(vec![only_main]);
    let rows = rows();
    set_endpoint_enabled(&session, 0, OUTPUT_NODE_ID, 0, false, &target(&rows)).expect("spec §5.3: allowed");
    assert!(matches!(
        &recorder.seen.borrow()[0],
        Command::Chain(ChainCommand::SetChainEndpointEnabled { node: EndpointNode::Output, enabled: false, .. })
    ));
}

#[test]
fn a_lane_output_node_addresses_its_own_node() {
    let (session, recorder) = recording_session(vec![chain(vec![])]);
    let rows = rows();
    set_endpoint_enabled(&session, 0, PATH_B_OUTPUT_NODE_ID, 1, false, &target(&rows)).expect("toggle");
    assert!(matches!(
        &recorder.seen.borrow()[0],
        Command::Chain(ChainCommand::SetChainEndpointEnabled { node: EndpointNode::PathBOutput, io, .. }) if io == "aux"
    ));
}

#[test]
fn a_block_node_has_no_endpoints() {
    let (session, _) = recording_session(vec![chain(vec![core("amp")])]);
    let rows = rows();
    assert_eq!(set_endpoint_enabled(&session, 0, "amp", 0, false, &target(&rows)), Err(GestureError::NotApplicable));
}
```

`crates/adapter-gui/src/issue_328_endpoint_checklist_interaction_tests.rs`:

```rust
//! #328 (spec §5.3) — the checklist overlay under real pointer events, and
//! the wiring end to end (open → rows → toggle → command, rows stay listed).

use std::cell::RefCell;
use std::rc::Rc;

use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, Model, ModelRc, Timer, VecModel};

use application::command::{ChainCommand, Command};

use crate::chain_graph_fixtures_tests::{chain, core, recording_session, rows};
use crate::chain_graph_ids::INPUT_NODE_ID;
use crate::endpoint_checklist_wiring::{wire, EndpointChecklistWiringCtx};
use crate::{ChainGraphOverlayState, ChannelOptionItem, EndpointChecklistHarness};

fn click_at(w: &impl ComponentHandle, p: LogicalPosition) {
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: p });
    win.dispatch_event(WindowEvent::PointerPressed { position: p, button: PointerEventButton::Left });
    win.dispatch_event(WindowEvent::PointerReleased { position: p, button: PointerEventButton::Left });
    win.dispatch_event(WindowEvent::PointerExited);
}

fn centre(el: &i_slint_backend_testing::ElementHandle) -> LogicalPosition {
    let (pos, size) = (el.absolute_position(), el.size());
    LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0)
}

fn item(index: i32, label: &str, selected: bool) -> ChannelOptionItem {
    ChannelOptionItem { index, label: label.into(), selected, available: true }
}

fn open(h: &EndpointChecklistHarness) {
    let state = ChainGraphOverlayState::get(h);
    state.set_checklist_items(ModelRc::new(VecModel::from(vec![item(0, "In 1", true), item(1, "In 2", true)])));
    state.set_checklist_chain_index(0);
    state.set_checklist_node(INPUT_NODE_ID.into());
    state.set_checklist_title("Inputs".into());
    state.set_checklist_open(true);
}

#[test]
fn clicking_a_row_reports_the_node_the_row_and_the_new_state() {
    i_slint_backend_testing::init_no_event_loop();
    let h = EndpointChecklistHarness::new().unwrap();
    open(&h);
    let got: Rc<RefCell<Option<(i32, String, i32, bool)>>> = Rc::new(RefCell::new(None));
    let seen = got.clone();
    ChainGraphOverlayState::get(&h).on_checklist_toggled(move |ci, node, row, on| {
        *seen.borrow_mut() = Some((ci, node.to_string(), row, on));
    });
    h.show().unwrap();
    let cell = i_slint_backend_testing::ElementHandle::find_by_element_id(&h, "ChannelPicker::chan-cell")
        .nth(1)
        .expect("the second endpoint row");
    click_at(&h, centre(&cell));
    assert_eq!(*got.borrow(), Some((0, INPUT_NODE_ID.to_string(), 1, false)));
}

#[test]
fn a_click_outside_the_card_closes_the_checklist() {
    i_slint_backend_testing::init_no_event_loop();
    let h = EndpointChecklistHarness::new().unwrap();
    open(&h);
    h.show().unwrap();
    click_at(&h, LogicalPosition::new(5.0, 5.0));
    assert!(!ChainGraphOverlayState::get(&h).get_checklist_open());
}

#[test]
fn opening_the_input_node_lists_the_chains_inputs_and_a_toggle_dispatches() {
    i_slint_backend_testing::init_no_event_loop();
    let app = crate::AppWindow::new().unwrap();
    let (session, recorder) = recording_session(vec![chain(vec![core("amp")])]);
    wire(
        &app,
        EndpointChecklistWiringCtx {
            project_session: session,
            project_chains: rows(),
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(Vec::new())),
            toast_timer: Rc::new(Timer::default()),
        },
    );
    let state = ChainGraphOverlayState::get(&app);
    state.invoke_open_checklist(0, INPUT_NODE_ID.into());
    assert!(state.get_checklist_open());
    assert_eq!(state.get_checklist_title().to_string(), rust_i18n::t!("title-endpoints-input").to_string());
    let labels: Vec<String> = state.get_checklist_items().iter().map(|i| i.label.to_string()).collect();
    assert_eq!(labels, vec!["In 1", "In 2"]);

    state.invoke_checklist_toggled(0, INPUT_NODE_ID.into(), 1, false);

    assert!(matches!(
        &recorder.seen.borrow()[0],
        Command::Chain(ChainCommand::SetChainEndpointEnabled { enabled: false, .. })
    ));
    assert_eq!(state.get_checklist_items().row_count(), 2, "an unchecked endpoint stays listed");
}
```

Register both in `lib.rs` (`endpoint_toggle` carries its `#[path]` test module; add `#[cfg(test)] mod issue_328_endpoint_checklist_interaction_tests;`).

- [ ] **Step 2: Run — compile error; stubs; behavioural red.** Stubs: `set_endpoint_enabled` → `Err(GestureError::NotApplicable)`; `endpoint_checklist_wiring::wire` → empty body; `checklist_items` → `Vec::new()`; `checklist_title` → `String::new()`; an empty `EndpointChecklistOverlay`; the harness as in Step 3; app-window import/export (Step 4); `mod endpoint_toggle;`, `mod endpoint_checklist_wiring;` in `lib.rs`.

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib endpoint_toggle && nice -n 19 cargo test -p adapter-gui -j 2 --lib issue_328_endpoint_checklist
```
Expected: `toggle: NotApplicable`, `left: Err(NotApplicable)` where `Ok` was expected, `the second endpoint row` panics, `assertion failed: state.get_checklist_open()`.

- [ ] **Step 3: Implement.** `crates/adapter-gui/src/endpoint_toggle.rs`:

```rust
//! Responsibility: switches one endpoint of a graph node on or off.
//!
//! #328 (spec §5.3). The row index is the checklist's (`endpoint_rows`); the
//! command names the endpoint by E/S id and endpoint name, so a reordered
//! registry never flips the wrong one. Unchecking every endpoint of a node is
//! allowed — that node's segments are simply not built. The live chain is
//! resynced (#614): which segments exist just changed.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{ChainCommand, Command};

use crate::chain_graph_ids::{resolve_node, NodeRef};
use crate::endpoint_checklist_items::endpoint_rows;
use crate::graph_gesture_actions::{GestureError, RowsTarget};
use crate::project_view::replace_project_chains;
use crate::runtime_sync_policy::request_chain_sync;
use crate::state::ProjectSession;

pub(crate) fn set_endpoint_enabled(
    session: &Rc<RefCell<Option<ProjectSession>>>,
    chain_index: usize,
    node_id: &str,
    row: usize,
    enabled: bool,
    rows: &RowsTarget<'_>,
) -> Result<(), GestureError> {
    let borrowed = session.borrow();
    let s = borrowed.as_ref().ok_or(GestureError::NoProject)?;
    let chain = s
        .project
        .borrow()
        .chains
        .get(chain_index)
        .cloned()
        .ok_or(GestureError::NoSuchChain)?;
    let Some(NodeRef::Endpoints(node)) = resolve_node(&chain, node_id) else {
        return Err(GestureError::NotApplicable);
    };
    let endpoint = endpoint_rows(&chain, &s.io_bindings.borrow(), node)
        .into_iter()
        .nth(row)
        .ok_or(GestureError::NotApplicable)?;
    s.dispatcher
        .dispatch(Command::Chain(ChainCommand::SetChainEndpointEnabled {
            chain: chain.id.clone(),
            node,
            io: endpoint.io,
            endpoint: endpoint.endpoint,
            enabled,
        }))
        .map_err(|e| GestureError::Failed(e.to_string()))?;
    request_chain_sync(s, &chain.id).map_err(|e| GestureError::Failed(e.to_string()))?;
    replace_project_chains(
        rows.model,
        &s.project.borrow(),
        rows.inputs,
        rows.outputs,
        &s.io_bindings.borrow(),
    );
    Ok(())
}

#[cfg(test)]
#[path = "endpoint_toggle_tests.rs"]
mod tests;
```

`crates/adapter-gui/src/endpoint_checklist_wiring.rs`:

```rust
//! Responsibility: drives the endpoint checklist overlay.
//!
//! #328 (spec §5.3). Opening lists the node's endpoints (`endpoint_rows`); a
//! toggle goes through `set_endpoint_enabled`; the rows are refreshed in place
//! from the chain afterwards, so an unchecked endpoint stays listed, unchecked.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, Model, ModelRc, Timer, VecModel};

use domain::AudioDeviceDescriptor;
use project::endpoint_disables::EndpointNode;

use crate::chain_graph_ids::{resolve_node, NodeRef};
use crate::chain_graph_wiring::chain_at;
use crate::endpoint_checklist_items::{endpoint_rows, EndpointRow};
use crate::endpoint_toggle::set_endpoint_enabled;
use crate::graph_gesture_actions::{GestureError, RowsTarget};
use crate::helpers::set_status_error;
use crate::state::ProjectSession;
use crate::{AppWindow, ChainGraphOverlayState, ChannelOptionItem, ProjectChainItem};

pub(crate) struct EndpointChecklistWiringCtx {
    pub(crate) project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub(crate) project_chains: Rc<VecModel<ProjectChainItem>>,
    pub(crate) input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub(crate) output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub(crate) toast_timer: Rc<Timer>,
}

struct Checklist {
    ctx: EndpointChecklistWiringCtx,
    items: Rc<VecModel<ChannelOptionItem>>,
}

pub(crate) fn checklist_items(rows: &[EndpointRow]) -> Vec<ChannelOptionItem> {
    rows.iter()
        .enumerate()
        .map(|(index, row)| ChannelOptionItem {
            index: index as i32,
            label: row.label.as_str().into(),
            selected: row.enabled,
            available: true,
        })
        .collect()
}

pub(crate) fn checklist_title(node: &EndpointNode) -> String {
    match node {
        EndpointNode::Input => rust_i18n::t!("title-endpoints-input"),
        EndpointNode::Output => rust_i18n::t!("title-endpoints-output"),
        EndpointNode::PathAOutput => rust_i18n::t!("title-endpoints-path-a"),
        EndpointNode::PathBOutput => rust_i18n::t!("title-endpoints-path-b"),
    }
    .to_string()
}

/// The node's rows as they are now, or `None` when the node is gone.
fn current_rows(checklist: &Checklist, chain_index: i32, node_id: &str) -> Option<(EndpointNode, Vec<EndpointRow>)> {
    let chain = chain_at(&checklist.ctx.project_session, chain_index)?;
    let Some(NodeRef::Endpoints(node)) = resolve_node(&chain, node_id) else {
        return None;
    };
    let borrowed = checklist.ctx.project_session.borrow();
    let registry = borrowed.as_ref()?.io_bindings.borrow().clone();
    Some((node, endpoint_rows(&chain, &registry, node)))
}

pub(crate) fn wire(window: &AppWindow, ctx: EndpointChecklistWiringCtx) {
    let checklist = Rc::new(Checklist {
        ctx,
        items: Rc::new(VecModel::default()),
    });
    let state = ChainGraphOverlayState::get(window);
    state.set_checklist_items(ModelRc::from(checklist.items.clone()));
    {
        let (weak, checklist) = (window.as_weak(), checklist.clone());
        state.on_open_checklist(move |chain_index, node_id| {
            let Some(window) = weak.upgrade() else { return };
            let Some((node, rows)) = current_rows(&checklist, chain_index, &node_id) else { return };
            checklist.items.set_vec(checklist_items(&rows));
            let state = ChainGraphOverlayState::get(&window);
            state.set_checklist_chain_index(chain_index);
            state.set_checklist_node(node_id);
            state.set_checklist_title(checklist_title(&node).into());
            state.set_checklist_open(true);
        });
    }
    {
        let (weak, checklist) = (window.as_weak(), checklist.clone());
        state.on_checklist_toggled(move |chain_index, node_id, row, enabled| {
            let Some(window) = weak.upgrade() else { return };
            let result = {
                let inputs = checklist.ctx.input_chain_devices.borrow();
                let outputs = checklist.ctx.output_chain_devices.borrow();
                let rows = RowsTarget {
                    model: &checklist.ctx.project_chains,
                    inputs: &inputs,
                    outputs: &outputs,
                };
                set_endpoint_enabled(
                    &checklist.ctx.project_session,
                    chain_index as usize,
                    &node_id,
                    row as usize,
                    enabled,
                    &rows,
                )
            };
            match result {
                Ok(()) => {}
                Err(GestureError::Failed(err)) => set_status_error(
                    &window,
                    &checklist.ctx.toast_timer,
                    &rust_i18n::t!("error-graph-action", err = err),
                ),
                Err(other) => log::warn!("[endpoint-checklist] toggle ignored: {other:?}"),
            }
            // Same rows in the same order: update in place, the list never jumps.
            if let Some((_, fresh)) = current_rows(&checklist, chain_index, &node_id) {
                for (index, item) in checklist_items(&fresh).into_iter().enumerate() {
                    if index < checklist.items.row_count() {
                        checklist.items.set_row_data(index, item);
                    }
                }
            }
        });
    }
}
```

`channel_picker.slint`: after `in property <string> empty-hint;` (line 94) add

```slint
    // #328: the endpoint checklist reuses this list; it counts endpoints, not
    // channels. Default is the channel wording every existing picker shows.
    in property <string> total-caption: @tr("label-channels-total", root.channels.length);
```
and on line 160 `text: @tr("label-channels-total", root.channels.length);` becomes `text: root.total-caption;`.

`crates/adapter-gui/ui/components/endpoint_checklist_overlay.slint`:

```slint
// Responsibility: renders the endpoint checklist overlay.
//
// #328 (spec §5.3): clicking an input or output node of the chain graph lists
// every endpoint of the chain's E/S, checked unless this node disabled it.
// Root-level (a PopupWindow's content does not receive clicks, #749/#761);
// the rows are the app's one checklist, ChannelPicker (#880). A click outside
// the card closes it. Nothing is added to or removed from the E/S here.

import { ChainGraphOverlayState } from "../chain_graph_overlay_globals.slint";
import { ChannelPicker } from "channel_picker.slint";

export component EndpointChecklistOverlay inherits Rectangle {
    x: 0;
    y: 0;
    visible: ChainGraphOverlayState.checklist-open;

    backdrop := TouchArea {
        clicked => { ChainGraphOverlayState.checklist-open = false; }
    }

    if ChainGraphOverlayState.checklist-open : Rectangle {
        x: (root.width - self.width) / 2;
        y: (root.height - self.height) / 2;
        width: min(root.width - 32px, 440px);
        height: min(root.height - 32px, card.preferred-height);
        background: #111114;
        border-radius: 10px;
        border-width: 1px;
        border-color: #27272a;
        drop-shadow-blur: 20px;
        drop-shadow-color: #000000c0;
        clip: true;

        // Swallow clicks on the card so only the backdrop closes it.
        TouchArea { }

        card := VerticalLayout {
            padding: 16px;
            spacing: 12px;

            HorizontalLayout {
                height: 44px;
                spacing: 8px;
                Text {
                    horizontal-stretch: 1;
                    text: ChainGraphOverlayState.checklist-title;
                    color: #f1f4f8;
                    font-size: 22px;
                    font-weight: 800;
                    vertical-alignment: center;
                }
                Rectangle {
                    width: 88px;
                    border-radius: 6px;
                    background: close-ta.has-hover ? #2b3340 : #1b2230;
                    close-ta := TouchArea {
                        mouse-cursor: pointer;
                        clicked => { ChainGraphOverlayState.checklist-open = false; }
                    }
                    Text {
                        text: @tr("btn-close");
                        color: #d8e0ff;
                        font-size: 18px;
                        horizontal-alignment: center;
                        vertical-alignment: center;
                    }
                }
            }

            Text {
                text: @tr("hint-endpoint-checklist");
                color: #a1a1aa;
                font-size: 18px;
                wrap: word-wrap;
            }

            ChannelPicker {
                channels: ChainGraphOverlayState.checklist-items;
                empty-hint: @tr("hint-endpoints-empty");
                total-caption: @tr("label-endpoints-total", ChainGraphOverlayState.checklist-items.length);
                toggled(row, on) => {
                    ChainGraphOverlayState.checklist-toggled(ChainGraphOverlayState.checklist-chain-index, ChainGraphOverlayState.checklist-node, row, on);
                }
            }
        }
    }
}
```

`crates/adapter-gui/ui/components/endpoint_checklist_test_harness.slint`:

```slint
// Responsibility: hosts the endpoint checklist overlay for a test.
// #328 TEST-ONLY: EndpointChecklistOverlay in a real Window so the headless
// tests can click its rows. Imported by app-window.slint only so slint-build
// emits the Rust type; never shown in the running app.
import { EndpointChecklistOverlay } from "endpoint_checklist_overlay.slint";
import "../fonts/BebasNeue.ttf";

export component EndpointChecklistHarness inherits Window {
    width: 600px;
    height: 600px;
    background: #040708;
    EndpointChecklistOverlay {
        width: root.width;
        height: root.height;
    }
}
```

- [ ] **Step 4: Wire it in.** `app-window.slint`: `import { EndpointChecklistOverlay } from "components/endpoint_checklist_overlay.slint";`, `import { EndpointChecklistHarness } from "components/endpoint_checklist_test_harness.slint";` (add `EndpointChecklistHarness` to the harness export on line 28), and after the `SplitEditorOverlay` instance:

```slint
    // #328: the input/output node checklist (root level: never clipped).
    EndpointChecklistOverlay { width: root.width; height: root.height; }
```

`desktop_app_block_wiring.rs`, after the Task 14 call:

```rust
    // --- #328: the endpoint checklist of the graph's input / output nodes ---
    crate::endpoint_checklist_wiring::wire(
        deps.window,
        crate::endpoint_checklist_wiring::EndpointChecklistWiringCtx {
            project_session: deps.project_session.clone(),
            project_chains: deps.project_chains.clone(),
            input_chain_devices: deps.input_chain_devices.clone(),
            output_chain_devices: deps.output_chain_devices.clone(),
            toast_timer: deps.toast_timer.clone(),
        },
    );
```

Translations: add the four `title-endpoints-*` keys to the nine `locales/*.yml`; then refresh the gettext catalogs and fill the three new msgids (`hint-endpoint-checklist`, `hint-endpoints-empty`, `label-endpoints-total`) in all nine `.po` with the Appendix A values:

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && scripts/extract-translations.sh && for k in hint-endpoint-checklist hint-endpoints-empty label-endpoints-total; do grep -L "msgid \"$k\"" crates/adapter-gui/translations/*/LC_MESSAGES/adapter-gui.po; done
```
The grep must print nothing; then edit each `.po` so every one of the three msgids has a non-empty `msgstr`.

- [ ] **Step 5: Run — expect PASS**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && nice -n 19 cargo test -p adapter-gui -j 2 --lib endpoint_toggle && nice -n 19 cargo test -p adapter-gui -j 2 --lib issue_328_endpoint_checklist && nice -n 19 cargo test -p adapter-gui -j 2 --test issue_880_channel_picker && nice -n 19 cargo test -p adapter-gui -j 2 --lib every_tr_key_has_translation_in_en_pt_es && nice -n 19 cargo test -p adapter-gui -j 2 --test issue_791_po_catalog_is_flat
```
Expected: all pass; the #880 picker tests are unchanged.

- [ ] **Step 6: Render and look.** Scratch render file (git-ignored) `target/issue-328-render/checklist_render.slint`:

```slint
// Responsibility: renders the endpoint checklist with fixed rows for a PNG check.
import { EndpointChecklistOverlay } from "../../crates/adapter-gui/ui/components/endpoint_checklist_overlay.slint";
import { ChainGraphOverlayState } from "../../crates/adapter-gui/ui/chain_graph_overlay_globals.slint";
import "../../crates/adapter-gui/ui/fonts/BebasNeue.ttf";

export component EndpointChecklistRender inherits Window {
    width: 600px;
    height: 600px;
    background: #040708;
    init => {
        ChainGraphOverlayState.checklist-title = "Outputs";
        ChainGraphOverlayState.checklist-items = [
            { index: 0, label: "Scarlett · Out L/R", selected: true, available: true },
            { index: 1, label: "AUX · Out L/R", selected: false, available: true },
            { index: 2, label: "Headphones", selected: true, available: true },
        ];
        ChainGraphOverlayState.checklist-open = true;
    }
    EndpointChecklistOverlay { width: root.width; height: root.height; }
}

export component EndpointChecklistEmptyRender inherits Window {
    width: 600px;
    height: 600px;
    background: #040708;
    init => {
        ChainGraphOverlayState.checklist-title = "Outputs";
        ChainGraphOverlayState.checklist-items = [];
        ChainGraphOverlayState.checklist-open = true;
    }
    EndpointChecklistOverlay { width: root.width; height: root.height; }
}
```

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && R=tools/slint-render/target/release/slint-render F=target/issue-328-render/checklist_render.slint && $R $F EndpointChecklistRender target/issue-328-render/checklist.png 600 600 && $R $F EndpointChecklistEmptyRender target/issue-328-render/checklist-empty.png 600 600
```
Open both PNGs. Self-critique: the unchecked AUX row is still listed and readable, the selection echo names only the checked endpoints, the empty state shows the hint instead of a blank card, rows ≥ 34 px, close ≥ 44 px. Fix in `endpoint_checklist_overlay.slint`.

- [ ] **Step 7: Docs.** `docs/audio-config.md` already has Part 1's `### Endpoint checklist (issue #328)` section (model, projection, runtime filter) and Part 4's `### Y → A/B outputs (issue #328)` section. Do not repeat them; append this paragraph at the end of the `### Endpoint checklist (issue #328)` section (before its `Contract tests:` line):

```markdown
In the chains screen (desktop), clicking a chain graph's input node — or its output node, or on a
Y → A/B chain a lane's own output node — opens this checklist as a root-level panel: every input (or
output) endpoint of the chain's E/S, checked unless that node leaves it out. Each click dispatches
`SetChainEndpointEnabled` for that node and that endpoint and resyncs the chain; an unchecked
endpoint stays listed so it can be checked again. The node's label names its checked endpoints
(`None` when every one is off).
```

`docs/screens.md` line 7, append: *An input or output node opens its endpoint checklist (every endpoint of the chain's E/S, checked by default).*

- [ ] **Step 8: Commit and push**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; git -C "$S" add crates/adapter-gui/src/endpoint_toggle.rs crates/adapter-gui/src/endpoint_toggle_tests.rs crates/adapter-gui/src/endpoint_checklist_wiring.rs crates/adapter-gui/src/issue_328_endpoint_checklist_interaction_tests.rs crates/adapter-gui/src/desktop_app_block_wiring.rs crates/adapter-gui/src/lib.rs crates/adapter-gui/ui/components/endpoint_checklist_overlay.slint crates/adapter-gui/ui/components/endpoint_checklist_test_harness.slint crates/adapter-gui/ui/components/channel_picker.slint crates/adapter-gui/ui/app-window.slint crates/adapter-gui/locales/de-DE.yml crates/adapter-gui/locales/en-US.yml crates/adapter-gui/locales/es-ES.yml crates/adapter-gui/locales/fr-FR.yml crates/adapter-gui/locales/hi-IN.yml crates/adapter-gui/locales/ja-JP.yml crates/adapter-gui/locales/ko-KR.yml crates/adapter-gui/locales/pt-BR.yml crates/adapter-gui/locales/zh-CN.yml crates/adapter-gui/translations/adapter-gui.pot crates/adapter-gui/translations/de_DE/LC_MESSAGES/adapter-gui.po crates/adapter-gui/translations/en_US/LC_MESSAGES/adapter-gui.po crates/adapter-gui/translations/es_ES/LC_MESSAGES/adapter-gui.po crates/adapter-gui/translations/fr_FR/LC_MESSAGES/adapter-gui.po crates/adapter-gui/translations/hi_IN/LC_MESSAGES/adapter-gui.po crates/adapter-gui/translations/ja_JP/LC_MESSAGES/adapter-gui.po crates/adapter-gui/translations/ko_KR/LC_MESSAGES/adapter-gui.po crates/adapter-gui/translations/pt_BR/LC_MESSAGES/adapter-gui.po crates/adapter-gui/translations/zh_CN/LC_MESSAGES/adapter-gui.po docs/audio-config.md docs/screens.md && git -C "$S" commit -m "feat(#328): endpoint checklist on the graph's input and output nodes"
```
Before committing, `git -C "$S" status --short` must show no other changed file (the extract script touches only the `.pot` and the nine `.po`). Delete exactly the dead-code expectation lines now unfulfilled: `cargo build` reports `endpoint_checklist_items` (its `io`/`endpoint` fields are read here) and `chain_graph_ids` (the `Endpoints` payload is read here); `cargo test` reports the fixtures module's `#[expect(dead_code, reason = "#328 part 6: fixtures of later tasks")]` (its last fixture, `recording_session`, is used here). Run the Push gate, push, comment on #328.

---

### Task 16: Final gate and the owner's checklist

**Files:** none new. Verification, push, handoff.

- [ ] **Step 1: No dead-code expectation left from this part**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && grep -n "#328 part 6: first production caller\|#328 part 6: fixtures of later tasks" crates/adapter-gui/src/lib.rs; nice -n 19 cargo test -p adapter-gui -j 2 --no-run 2>&1 | grep -E "^warning" ; nice -n 19 cargo build -p adapter-gui -j 2 2>&1 | grep -E "^warning"
```
Expected: no output from any of the three. If an expectation line remains, an item it covers is still unused — find it and either give it its caller from this plan or report it (never keep dead code).

- [ ] **Step 2: Catalogs in sync**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && scripts/extract-translations.sh --check && nice -n 19 cargo test -p adapter-gui -j 2 --lib i18n
```
Expected: `✓ translation catalogs in sync`; `every_tr_key_has_translation_in_en_pt_es` and `every_locale_carries_the_same_keys_as_english` pass.

- [ ] **Step 3: Files stay inside their caps and declare one responsibility**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && wc -l crates/adapter-gui/ui/app-window.slint crates/adapter-gui/ui/models.slint crates/adapter-gui/ui/components/graph_view.slint crates/adapter-gui/ui/components/graph_node_card.slint crates/adapter-gui/ui/pages/chain_row.slint crates/adapter-gui/src/select_chain_block_callback.rs crates/adapter-gui/src/block_choose_type_callback.rs crates/adapter-gui/src/block_insert_callbacks.rs crates/adapter-gui/src/desktop_app_block_wiring.rs crates/adapter-gui/src/project_chains_refresh.rs crates/adapter-gui/src/lib.rs && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates
```
Expected: `.slint` ≤ 500, `.rs` ≤ 600; validate green.

- [ ] **Step 4: Push gate** (Global Constraints), push, `gh issue comment 328` with the last hash, the files of the part and "push gate green".

- [ ] **Step 5: Owner handoff.** Set up the solver run line (the script completes the workspace and prints the `run:` line):

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328; cd "$S" && scripts/solver-setup.sh 328 feature/issue-328
```

Send the checklist in chat and as `gh issue comment 328` — the two commands, then only what the owner validates by ear, eye or hardware:

```bash
git fetch && git checkout feature/issue-328 && git pull
```
```bash
<the exact run: line printed by scripts/solver-setup.sh, absolute path>
```
1. [ ] A linear chain row reads like the old strip: cards, LEDs, amber unavailable tint, hover tooltip, latency badge.
2. [ ] Picker → **Split → Mix** in the middle of a chain: two lanes appear between split and mixer.
3. [ ] Drag an amp from lane A to lane B, and a pedal from the shared part into a lane.
4. [ ] Mixer: pan A −50, pan B +50 — amp A only on the left, amp B only on the right.
5. [ ] Two identical lanes, B polarity invert — near silence.
6. [ ] **Y → A/B** at the end of a chain: path A and path B output nodes each send to the output checked in their checklist.
7. [ ] Input node checklist: uncheck one input — that guitar leaves the chain; check it again — it comes back.
8. [ ] Remove the split with blocks in lane B — the confirmation appears; lane A's blocks stay in the chain.
9. [ ] Touch mode: the split is one Split chip; tapping it opens the split editor.
10. [ ] Mouse wheel over a chain row scrolls the list; Cmd + wheel zooms the row.
11. [ ] Linux/JACK (Orange Pi): a **Y → A/B** chain with path B on its own output — path B is heard (Part 4 Task 10; only the Linux CI build compiles that code, nothing here plays it).

---

## Appendix A — translation values

rust-i18n keys go at the end of each `crates/adapter-gui/locales/<locale>.yml` under a `# --- #328 chain split graph` comment, as `key: "value"` lines.

| Key | en-US | pt-BR | es-ES | de-DE | fr-FR | hi-IN | ja-JP | ko-KR | zh-CN |
|---|---|---|---|---|---|---|---|---|---|
| `label-endpoints-none` | None | Nenhuma | Ninguna | Keine | Aucune | कोई नहीं | なし | 없음 | 无 |
| `confirm-remove-split-name` | Split (+ %{n} blocks in path B) | Split (+ %{n} blocos no caminho B) | Split (+ %{n} bloques en el camino B) | Split (+ %{n} Blöcke in Pfad B) | Split (+ %{n} blocs du chemin B) | स्प्लिट (+ पाथ B में %{n} ब्लॉक) | スプリット（+ パスBの%{n}ブロック） | 스플릿 (+ 경로 B의 블록 %{n}개) | 分路（+ 路径 B 中的 %{n} 个模块） |
| `error-graph-action` | Could not apply the change: %{err} | Não foi possível aplicar a mudança: %{err} | No se pudo aplicar el cambio: %{err} | Änderung nicht möglich: %{err} | Impossible d'appliquer la modification : %{err} | बदलाव लागू नहीं हो सका: %{err} | 変更を適用できません: %{err} | 변경을 적용할 수 없음: %{err} | 无法应用更改：%{err} |
| `picker-split-mix` | Split → Mix | Split → Mix | Split → Mix | Split → Mix | Split → Mix | Split → Mix | Split → Mix | Split → Mix | Split → Mix |
| `picker-split-mix-subtitle` | Two paths summed by a mixer | Dois caminhos somados por um mixer | Dos caminos sumados por un mezclador | Zwei Pfade, von einem Mixer summiert | Deux chemins additionnés par un mixeur | दो पाथ, एक मिक्सर में जुड़ते हैं | 2つのパスをミキサーで合成 | 두 경로를 믹서로 합산 | 两条路径由混音器合并 |
| `picker-split-y` | Y → A/B | Y → A/B | Y → A/B | Y → A/B | Y → A/B | Y → A/B | Y → A/B | Y → A/B | Y → A/B |
| `picker-split-y-subtitle` | Each path to its own outputs | Cada caminho para as próprias saídas | Cada camino a sus propias salidas | Jeder Pfad zu eigenen Ausgängen | Chaque chemin vers ses propres sorties | हर पाथ अपने आउटपुट पर | 各パスを個別の出力へ | 각 경로를 자체 출력으로 | 每条路径通往各自的输出 |
| `title-split-editor` | Split | Split | Split | Split | Split | स्प्लिट | スプリット | 스플릿 | 分路 |
| `title-mixer-editor` | Mixer | Mixer | Mezclador | Mixer | Mixeur | मिक्सर | ミキサー | 믹서 | 混音器 |
| `title-endpoints-input` | Inputs | Entradas | Entradas | Eingänge | Entrées | इनपुट | 入力 | 입력 | 输入 |
| `title-endpoints-output` | Outputs | Saídas | Salidas | Ausgänge | Sorties | आउटपुट | 出力 | 출력 | 输出 |
| `title-endpoints-path-a` | Path A outputs | Saídas do caminho A | Salidas del camino A | Ausgänge von Pfad A | Sorties du chemin A | पाथ A के आउटपुट | パスAの出力 | 경로 A 출력 | 路径 A 的输出 |
| `title-endpoints-path-b` | Path B outputs | Saídas do caminho B | Salidas del camino B | Ausgänge von Pfad B | Sorties du chemin B | पाथ B के आउटपुट | パスBの出力 | 경로 B 출력 | 路径 B 的输出 |

gettext msgids (Slint `@tr`) go in `crates/adapter-gui/translations/<locale>/LC_MESSAGES/adapter-gui.po` (locale dirs `de_DE en_US es_ES fr_FR hi_IN ja_JP ko_KR pt_BR zh_CN`), after `scripts/extract-translations.sh` adds them:

| msgid | en_US | pt_BR | es_ES | de_DE | fr_FR | hi_IN | ja_JP | ko_KR | zh_CN |
|---|---|---|---|---|---|---|---|---|---|
| `hint-endpoint-checklist` | Unchecked endpoints stay listed but are left out of this node. | Endpoints desmarcados continuam na lista, mas ficam fora deste nó. | Los endpoints desmarcados siguen en la lista, pero quedan fuera de este nodo. | Abgewählte Endpunkte bleiben gelistet, sind aber von diesem Knoten getrennt. | Les points décochés restent listés mais sont exclus de ce nœud. | अनचेक किए गए एंडपॉइंट सूची में रहते हैं, पर इस नोड से बाहर रहते हैं। | チェックを外したエンドポイントは一覧に残りますが、このノードからは外れます。 | 선택 해제한 엔드포인트는 목록에 남지만 이 노드에서 제외됩니다. | 取消勾选的端点仍保留在列表中，但不再连接到此节点。 |
| `hint-endpoints-empty` | This chain's E/S has no endpoints for this node. | A E/S desta chain não tem endpoints para este nó. | La E/S de esta cadena no tiene endpoints para este nodo. | Die E/A dieser Kette hat keine Endpunkte für diesen Knoten. | Les E/S de cette chaîne n'ont aucun point pour ce nœud. | इस चेन के E/S में इस नोड के लिए कोई एंडपॉइंट नहीं है। | このチェーンのE/Sには、このノード用のエンドポイントがありません。 | 이 체인의 E/S에는 이 노드용 엔드포인트가 없습니다. | 此链的 E/S 没有此节点的端点。 |
| `label-endpoints-total` | {} endpoints | {} endpoints | {} endpoints | {} Endpunkte | {} points | {} एंडपॉइंट | {} 個のエンドポイント | 엔드포인트 {}개 | {} 个端点 |

