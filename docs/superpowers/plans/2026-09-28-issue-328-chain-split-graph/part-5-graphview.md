# #328 Part 5: GraphView component upgrades — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Part 5 is a pure component: it uses no `Command` variant, so the question of what the chain-id field is called does not come up here. Every name from the shared interface contract that this part touches is used exactly as written: `ChainStage::Parallel { lanes, end }`, `ParallelEnd { Merge, Fan }`, the Slint `GraphNode.kind` string (`"block" | "io_input" | "io_output" | "split" | "mixer"`), and the `add-requested`, `remove-requested`, `bypass-toggled` and `node-dropped` callbacks, plus the files `graph_node_card.slint` and `graph_wire.slint`. `chain_graph_adapter.rs` and `endpoint_checklist_overlay.slint` belong to Part 6, not to this part.

**Goal:** Turn the standalone `GraphView` (#435) into a canvas that can serve as the chain editor. It must:

- lay out a Split → Mix chain and a Y → A/B chain;
- draw a card for every node kind, with each card clickable;
- give block cards the same features as `BlockChip`;
- offer "+" insert anchors and drag-drop onto them;
- zoom only with Cmd/Ctrl + wheel, so that the chains list around it keeps scrolling.

All of this has to happen without growing `graph_view.slint` past its cap.

**Architecture:** The work is split between Rust and Slint.

- **Rust model (`crates/adapter-gui/src/graph_view_model/`, pure, no Slint).** It produces:
  - positioned nodes, with a `NodeKind` on each;
  - edges;
  - one insert anchor per wire, each carrying the slot that a block added or dropped on it lands in;
  - drop resolution: which anchor a dragged block lands on.
- **Slint canvas (`graph_view.slint`).** It renders what the host gives it and routes pointer events. The drop geometry is not duplicated in Slint: the canvas asks the host through a `pure callback`, following the `slot-at` pattern from #787.
- **File split.** The canvas is split before it grows. The data types, the node card and the wire each move to their own file, and a render-only harness plus a test-only harness host the canvas for `tools/slint-render` and `i-slint-backend-testing`.
- **Shared tile colours.** The block card and the chain row's `BlockChip` read their state colours from one global, `BlockTileStyle`.

**Tech Stack:**

- Rust 2021.
- Slint 1.16.1 (`slint-build` with debug info in debug builds, and `i-slint-backend-testing` 1.16 for headless interaction tests).
- `tools/slint-render`, which uses `slint-interpreter` and the software renderer to produce PNGs.
- gettext catalogs in 9 locales.

**Spec:** `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328/docs/superpowers/specs/2026-09-28-issue-328-chain-split-graph-design.md`. Sections covered:

- **§5.2**, in full: the `Parallel` end (`Merge`/`Fan`), node kinds, the new callbacks, the file split, `@tr` accessible labels, and id-based addressing through anchor slots.
- **§5.1**, component side only: the graph content, cards per kind with `BlockChip` parity, the "+" on every wire and at every lane end, drag across lanes resolving to a slot, and the wheel/zoom rule.
- **§7 "GraphView model" and "Slint"**: the Fan layout, node kinds, drop-target resolution, interaction tests, and the `slint-render` PNGs of a linear chain, a Split → Mix chain and a Y → A/B chain.
- **§8**: `docs/gui/graph-view.md` and all nine translation files.

The following are Part 6, not this part: `chain_graph_adapter.rs`, placing the graph in `ChainRow` (§5.4), the endpoint checklist (§5.3), and the split/mixer editors.

**Depends on:** nothing. Part 5 is independent of Parts 1–4. **Consumed by:** Part 6, which builds `ChainStage`s from a `Chain`, calls `linear_chain_layout`, `insert_anchors` and `resolve_drop_anchor`, maps an `AnchorSlot` onto `MoveBlock`/`AddBlock { position, path }`, and wires the callbacks.

**Global Constraints** (copied verbatim from the spec, the repo `CLAUDE.md` and `.claude/skills/openrig-code-quality/SKILL.md`):

- **LEI ZERO — work only in the solver.** Every command runs in `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328`, and every git command is `git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 …`. Never run git in the main folder. Never use `git worktree`.
- **File caps.** "Cap de linhas (`.rs` 600, `.slint` 500) é só o alarme de fumaça — o limite real é a responsabilidade." Test files have no cap.
- **Responsibility headers.** "todo arquivo de produção DECLARA sua responsabilidade no cabeçalho — `//! Responsibility: <uma frase>` em `.rs`, `// Responsibility: <uma frase>` em `.slint`." `validate.sh` rejects a declaration that contains ` and `, ` e `, `plus`, `also`, `,`, `;`, `+`, `&` or `/`. "Vai adicionar responsabilidade nova em arquivo existente? PARA e cria o arquivo/módulo dela."
- **Split is behaviour-preserving.** "Split é sempre behavior-preserving: move código, não muda comportamento, e nenhum teste existente é reescrito pra caber na forma nova."
- **Audio thread.** "Zero alocação, lock, syscall ou I/O no audio thread. Sem exceção." Part 5 is UI only and touches no audio thread, DSP, routing or I/O code.
- **Stream isolation.** "N streams = N pipelines independentes, NADA misturado no nosso código." Part 5 touches no runtime or stream.
- **Volume invariants.** "`volume_invariants_tests.rs` quebra, a fonte está errada, não o teste" (`crates/engine/src/volume_invariants_tests.rs`). This file stays **untouched**.
- **TDD red-first.** "TDD red-first OBRIGATÓRIO — proibido implementar/alterar produção sem um teste que falhou ANTES." Every task shows a behavioural RED: an assertion failure, not only a compile error. When a test needs a new type or signature just to compile, that minimum surface is added first, the assertion is seen failing, and only then is the behaviour implemented. Watch the assertion fail and paste the FAILED line.
- **Zero warnings.** "Zero warnings (`cargo build` limpo)" … "Warning conta como quebrado".
- **English in the repo.** "Conteúdo de repo sempre em inglês": code, comments, docs, commit messages.
- **Cross-platform.** "Fix de Linux/Orange Pi/JACK fica atrás de `cfg` guards. NUNCA mudar comportamento cross-platform pra resolver UM SO." Part 5 needs no `cfg`. Slint already maps ⌘ (macOS) and Ctrl (Windows, Linux) to `KeyboardModifiers.control` (`i-slint-common-1.16.1/builtin_structs.rs`), so one check covers every platform.
- **UI work.** Before the first line of `.slint`, invoke `claude-plugin:ux-ui` (this environment's `ui-ux-pro-max`), `slint:slint` and `slint-best-practices` — `CLAUDE.md` and `openrig-code-quality` require all three; if the session does not list `slint:slint`, say so in the task's issue comment instead of skipping silently. "PROIBIDO supor/inventar layout — RENDERIZE com `tools/slint-render` … e confira o PNG ANTES de dizer 'pronto'." Every click, overlay and drag is proven with an `i-slint-backend-testing` interaction test before the push. No `PopupWindow` (#749/#761). No glyph used as an icon: use SVG through `@image-url`. Text is at least `Theme.min-font` (18px), and a Text box with a literal height is at least 22px (`validate.sh` check 6).
- **i18n.** "Toda string visível ao usuário passa por i18n." Every new `@tr` key goes into `translations/adapter-gui.pot` and into all nine `.po` files in the same commit, with no empty `msgstr`.
- **Builds.** `nice -n 19 cargo … -j 2`, one build at a time.
- **Pre-push gate (the user's rule: "Antes de TODO push").** Run these from the solver before every push (exact commands in Task 1, Step 11):
  - `cargo fmt --all`, then `cargo fmt --all -- --check`
  - `nice -n 19 cargo test --workspace -j 2`
  - `nice -n 19 cargo build --workspace -j 2`, which must produce zero warnings
  - `VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates`
- **Commits.** Stage explicit paths only; never `git add -A`. The message is `feat(#328): …` in English. There is **no `Co-Authored-By` trailer**: `openrig-code-quality` → "Naming OpenRig" says "Commits in English, no `Co-Authored-By` trailers", and that user rule outranks the workflow's default trailer. Push right after the commit (`git fetch` first; if the remote moved, `git pull --rebase` and rerun the gate). After every push, run `gh issue comment 328` with the hash, the files and the tests that ran.

**Review Focus** — the five failure modes the spec implies that are most likely to bite a user, each pinned by a test:

1. **The mouse wheel over a chain's graph zooms it, so the chains list cannot be scrolled** (spec owner decision 6). Pinned in Task 2 by `a_plain_wheel_over_the_graph_scrolls_the_list_instead_of_zooming`, with the canvas inside a `Flickable` as the list will host it, and by `cmd_or_ctrl_wheel_zooms_the_graph_and_leaves_the_list_still`.
2. **The split, mixer and I/O nodes cannot be clicked, so the split/mixer editor and the endpoint checklist never open.** They were 8px routing dots with a zero-size hit area, chosen by `label == ""`. Pinned in Task 8 by `every_node_kind_is_a_clickable_card` and the source pin `the_graph_view_no_longer_picks_routing_dots_by_an_empty_label`.
3. **Dropping a block moves it to the wrong place.** A drop on the block's own wire must be a no-op and must not fall through to the other lane's anchor 60px away. A drop onto the other lane must land in that lane. Pinned in Task 6 by `a_drop_on_the_blocks_own_wire_resolves_to_nothing` and `a_drop_into_the_other_lane_resolves_to_that_lanes_slot`, and end to end in Task 10 by `dragging_a_block_onto_the_other_lanes_anchor_fires_node_dropped` and `dropping_a_block_on_its_own_wire_fires_no_node_dropped`.
4. **A Y → A/B graph draws a phantom mixer, misaligned outputs, or a floating stage after the split.** Pinned in Task 3 by `fan_draws_no_mixer_node`, `fan_terminals_share_the_last_column`, `a_stage_after_a_fan_is_reported` and `topological_layout_lines_up_fan_terminals_on_the_last_column`.
5. **A block card loses parity with `BlockChip`**: a bypassed or unavailable block reads as live, the tooltip is gone, or the colours drift apart. Pinned in Task 9 by `a_block_cards_led_shows_bypass_and_toggles_it_without_a_click`, `an_unavailable_block_card_reads_as_disabled`, `hovering_a_block_with_a_model_shows_its_tooltip` and `the_graph_block_card_paints_its_states_from_block_tile_style`, and in Task 7 by `block_chip_paints_its_states_from_block_tile_style`. A zoomed-out card whose fixed-size × covers its body — so a click meant to select removes the block — is pinned by `a_block_cards_led_and_remove_hit_zones_scale_with_the_zoom`.

**File Structure** (paths relative to the solver; line counts as of 2026-09-28):

| File | Status | Lines | One responsibility |
|---|---|---|---|
| `crates/adapter-gui/ui/components/graph_view.slint` | modify | 420 → ~340 | renders a node canvas |
| `crates/adapter-gui/ui/components/graph_view_types.slint` | create | ~75 | declares the data the graph view renders (Slint forbids the cyclic import card ↔ canvas, so the structs move here) |
| `crates/adapter-gui/ui/components/graph_node_card.slint` | create | ~190 | draws one node card of the graph view |
| `crates/adapter-gui/ui/components/graph_wire.slint` | create | ~50 | draws one wire of the graph view |
| `crates/adapter-gui/ui/components/graph_view_test_harness.slint` | create | ~85 | hosts the graph view in test windows |
| `crates/adapter-gui/ui/components/_harness_graph_view.slint` | create | ~190 | hosts the graph view for a render check (not compiled into the app, like `_harness_channel_picker.slint`) |
| `crates/adapter-gui/ui/components/block_tile_style.slint` | create | ~50 | holds the colours a block tile paints its states with |
| `crates/adapter-gui/ui/assets/graph-split.svg`, `graph-mix.svg` | create | 7 / 8 | text-free routing artwork for the split and mixer cards (Task 8) |
| `crates/adapter-gui/ui/components/block_chip.slint` | modify | 181 → ~172 | renders one block tile inside a chain row (unchanged) |
| `crates/adapter-gui/ui/components/block_insert_slot.slint` | modify | 82 → ~88 | renders the slot where a block can be inserted (unchanged) |
| `crates/adapter-gui/ui/app-window.slint` | modify | 497 → 498 | (unchanged; one harness import line) |
| `crates/adapter-gui/src/graph_view_model/mod.rs` | modify | 29 → ~47 | routes the `GraphView` model to the file that owns each job |
| `crates/adapter-gui/src/graph_view_model/types.rs` | modify | 144 → ~205 | describes the graph the `GraphView` component renders |
| `crates/adapter-gui/src/graph_view_model/chain_builder.rs` | modify | 139 → ~185 | builds a positioned graph from a chain's stages |
| `crates/adapter-gui/src/graph_view_model/layout.rs` | modify | 139 → ~150 | places existing nodes by the topology of their edges |
| `crates/adapter-gui/src/graph_view_model/validation.rs` | modify | 38 → ~70 | reports what makes a graph description ill-formed |
| `crates/adapter-gui/src/graph_view_model/routing_ids.rs` | create | ~17 | names the routing nodes a parallel stage inserts |
| `crates/adapter-gui/src/graph_view_model/anchors.rs` | create | ~125 | places the insert anchors on a laid-out chain's wires |
| `crates/adapter-gui/src/graph_view_model/drop_target.rs` | create | ~45 | resolves which insert anchor a dragged block lands on |
| `crates/adapter-gui/src/graph_view_model_tests.rs` | modify (test) | 585 → ~760 | — |
| `crates/adapter-gui/src/graph_view_model_anchor_tests.rs` | create (test) | ~140 | — |
| `crates/adapter-gui/src/graph_view_model_drop_tests.rs` | create (test) | ~70 | — |
| `crates/adapter-gui/tests/issue_328_graph_view_interaction.rs` | create (test) | ~420 | — |
| `crates/adapter-gui/tests/issue_328_graph_view_sources.rs` | create (test) | ~90 | — |
| `crates/adapter-gui/translations/adapter-gui.pot` + 9 × `<locale>/LC_MESSAGES/adapter-gui.po` | modify | 967 / 969–981 | — |
| `docs/gui/graph-view.md` | modify | 183 | — |

---

## Task 1: Split `graph_view.slint` behind a pinned harness

**Files:**
- Create: `crates/adapter-gui/ui/components/graph_view_test_harness.slint`
- Modify: `crates/adapter-gui/ui/app-window.slint:27-28` (harness import and export)
- Create: `crates/adapter-gui/ui/components/graph_view_types.slint` (moved from `graph_view.slint:13-61`)
- Create: `crates/adapter-gui/ui/components/graph_node_card.slint` (moved from `graph_view.slint:63-107`)
- Create: `crates/adapter-gui/ui/components/graph_wire.slint` (moved from `graph_view.slint:109-155`)
- Modify: `crates/adapter-gui/ui/components/graph_view.slint:1-155`
- Test: `crates/adapter-gui/tests/issue_328_graph_view_interaction.rs` (create), `crates/adapter-gui/tests/issue_328_graph_view_sources.rs` (create)
- Docs: `docs/gui/graph-view.md:3-4,24,172`

**Interfaces:**
- Consumes: the Slint `GraphView` as it is today (`graph_view.slint:158-420`), with `node_clicked(string)` and `node_drag_ended(string, length, length)`.
- Produces: the Rust types `adapter_gui::GraphViewHarness` and `adapter_gui::GraphNode` (Slint struct: `id, label, category: SharedString; fill, border: Color; layout_x, layout_y: f32; bypass, selected: bool`). Also `export component GraphNodeCard` in `graph_node_card.slint` and `export component GraphWire` in `graph_wire.slint`. `graph_view.slint` re-exports `GraphNode, GraphEdge, GraphEdgeGeometry, CategoryColor`.

- [ ] **Step 1: Invoke the UI skills.** Invoke `claude-plugin:ux-ui`, `slint:slint` and `slint-best-practices`. This task only moves code, but it is the first `.slint` edit of the part.

- [ ] **Step 2: Write the pin tests.** GraphView is compiled into no binary today, so it has no safety net. Create `crates/adapter-gui/tests/issue_328_graph_view_interaction.rs`:

```rust
//! #328 — headless proof of the GraphView pointer contract, the canvas the
//! chain editor is built on. REAL pointer / wheel / key events dispatched at
//! real geometry through i-slint-backend-testing: a render PNG proves layout
//! only, this proves the gesture lands (#749/#761).
//!
//! The harness canvas sits at the window origin with zoom 1 and no pan, so a
//! node's layout coordinates ARE its window coordinates.

use adapter_gui::{GraphNode, GraphViewHarness};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

fn node(id: &str, label: &str, x: f32, y: f32) -> GraphNode {
    GraphNode {
        id: id.into(),
        label: label.into(),
        category: "drive".into(),
        layout_x: x,
        layout_y: y,
        ..Default::default()
    }
}

fn harness(nodes: Vec<GraphNode>) -> GraphViewHarness {
    i_slint_backend_testing::init_no_event_loop();
    let w = GraphViewHarness::new().unwrap();
    w.set_nodes(ModelRc::new(VecModel::from(nodes)));
    w.show().unwrap();
    w
}

fn at(x: f32, y: f32) -> LogicalPosition {
    LogicalPosition::new(x, y)
}

/// Collects what a callback fired with.
fn recorder<T: 'static>() -> Rc<RefCell<Vec<T>>> {
    Rc::default()
}

fn click_at(w: &impl ComponentHandle, p: LogicalPosition) {
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: p });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: p,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerReleased {
        position: p,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerExited);
}

/// Press at `from`, move in ten steps to `to`, release there.
fn drag(w: &impl ComponentHandle, from: LogicalPosition, to: LogicalPosition) {
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: from });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: from,
        button: PointerEventButton::Left,
    });
    for step in 1..=10 {
        let t = step as f32 / 10.0;
        let p = at(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        win.dispatch_event(WindowEvent::PointerMoved { position: p });
    }
    win.dispatch_event(WindowEvent::PointerReleased {
        position: to,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerExited);
}

#[test]
fn clicking_a_node_card_fires_node_clicked_with_its_id() {
    let w = harness(vec![
        node("od", "Drive", 240.0, 200.0),
        node("amp", "Amp", 400.0, 200.0),
    ]);
    let clicked = recorder::<String>();
    let c = clicked.clone();
    w.on_node_clicked(move |id| c.borrow_mut().push(id.to_string()));

    click_at(&w, at(400.0, 200.0));

    assert_eq!(
        *clicked.borrow(),
        ["amp"],
        "clicking the Amp card must fire node-clicked(\"amp\") once"
    );
}

#[test]
fn dragging_a_node_past_the_click_threshold_ends_a_drag_not_a_click() {
    let w = harness(vec![node("od", "Drive", 240.0, 200.0)]);
    let clicked = recorder::<String>();
    let ended = recorder::<(String, f32, f32)>();
    let c = clicked.clone();
    w.on_node_clicked(move |id| c.borrow_mut().push(id.to_string()));
    let e = ended.clone();
    w.on_node_drag_ended(move |id, x, y| e.borrow_mut().push((id.to_string(), x, y)));

    drag(&w, at(240.0, 200.0), at(300.0, 200.0));

    assert!(
        clicked.borrow().is_empty(),
        "a 60px drag is not a click, got {:?}",
        clicked.borrow()
    );
    assert_eq!(
        *ended.borrow(),
        [("od".to_string(), 240.0, 200.0)],
        "drag-ended fires once with the node's layout position — the host moves nodes, \
         the canvas does not"
    );
}
```

- [ ] **Step 3: Run the pin tests. Expect FAIL: the harness does not exist yet.** This red is a compile error. The behaviour already exists; these tests pin it before the split.

Run: `cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 && nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_interaction`

Expected: `error[E0432]: unresolved imports adapter_gui::GraphNode, adapter_gui::GraphViewHarness`

- [ ] **Step 4: Create the harness.** Create `crates/adapter-gui/ui/components/graph_view_test_harness.slint`:

```slint
// Responsibility: hosts the graph view in test windows.
// #328 TEST-ONLY harness: hosts GraphView in a real Window so the headless
// interaction tests (i-slint-backend-testing) can dispatch real pointer,
// wheel and key events at it. Imported by app-window.slint purely so
// slint-build emits the Rust types; never shown in the running app.

import { GraphView, GraphNode, GraphEdgeGeometry } from "graph_view.slint";

// The canvas alone, every callback forwarded.
export component GraphViewHarness inherits Window {
    width: 900px;
    height: 420px;
    in property <[GraphNode]> nodes;
    in property <[GraphEdgeGeometry]> edges;
    in-out property <float> zoom: 1.0;
    callback node-clicked(string);
    callback node-drag-ended(string, length, length);

    GraphView {
        x: 0px;
        y: 0px;
        width: 100%;
        height: 100%;
        nodes: root.nodes;
        edges: root.edges;
        zoom <=> root.zoom;
        node_clicked(id) => { root.node-clicked(id); }
        node_drag_ended(id, x, y) => { root.node-drag-ended(id, x, y); }
    }
}
```

In `crates/adapter-gui/ui/app-window.slint`, replace lines 27-28:

```slint
import { ParamTabBarHarness } from "components/param_tab_bar_test_harness.slint";
export { DiLoopHarness, ParamTabBarHarness, LooperHarness, LooperOverlayHarness, LooperEditorHarness }
```

with:

```slint
import { ParamTabBarHarness } from "components/param_tab_bar_test_harness.slint";
import { GraphViewHarness } from "components/graph_view_test_harness.slint";
export { DiLoopHarness, ParamTabBarHarness, LooperHarness, LooperOverlayHarness, LooperEditorHarness, GraphViewHarness }
```

After this change `app-window.slint` is 498 lines, still under the 500 cap.

- [ ] **Step 5: Run the pin tests. Expect PASS.** This is the safety net for the split.

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_interaction`

Expected: `test result: ok. 2 passed`

- [ ] **Step 6: Write the structural test.** Create `crates/adapter-gui/tests/issue_328_graph_view_sources.rs`:

```rust
//! #328 — source-presence pins for the GraphView chain editor (the
//! `no_native_dialogs.rs` convention): cheap, obvious checks that flip RED
//! the moment a layout or styling rule regresses.

use std::path::PathBuf;

fn read_component(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui/components")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Spec §5.2: graph_view.slint (420/500) is split BEFORE it grows.
#[test]
fn the_graph_card_wire_and_types_each_live_in_their_own_file() {
    let canvas = read_component("graph_view.slint");
    for (declaration, owner) in [
        ("component GraphNodeCard", "graph_node_card.slint"),
        ("component GraphWire", "graph_wire.slint"),
        ("struct GraphNode {", "graph_view_types.slint"),
    ] {
        assert!(
            !canvas.contains(declaration),
            "graph_view.slint still declares `{declaration}` — it belongs in {owner} (#328 §5.2)"
        );
        assert!(
            read_component(owner).contains(declaration),
            "{owner} must declare `{declaration}`"
        );
    }
}
```

- [ ] **Step 7: Run the structural test. Expect FAIL.**

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_sources`

Expected: ``graph_view.slint still declares `component GraphNodeCard` — it belongs in graph_node_card.slint (#328 §5.2)``

- [ ] **Step 8: Split the file.** Every piece of code is moved verbatim.

Create `crates/adapter-gui/ui/components/graph_view_types.slint`:

```slint
// Responsibility: declares the data the graph view renders.
// Split out of graph_view.slint (#328 §5.2) so the node card can import the
// node type without importing the canvas that imports the card — Slint
// rejects cyclic imports.

export struct GraphNode {
    // Stable identifier — unique within the graph. Must match what the
    // host gave when computing the layout.
    id: string,
    label: string,
    // Stable category slug ("drive", "amp", "reverb", "util", …). Kept
    // for accessibility / debugging; the component does NOT derive
    // colour from it (Slint can't search an array). The host resolves
    // colour from its palette and passes fill/border below — the single
    // source of truth is default_palette() in graph_view_model.rs.
    category: string,
    // Resolved colours, host-supplied. Component is fully colour-agnostic.
    fill: color,
    border: color,
    // Layout-space position (centre of the node).
    layout_x: length,
    layout_y: length,
    // Visual state.
    bypass: bool,
    selected: bool,
}

export struct GraphEdge {
    from_id: string,
    to_id: string,
}

// Internal: GraphEdge resolved to actual coordinates, ready to render
// as a Bézier curve. Host computes this from the node list; we keep the
// shape simple so the resolved set can be derived in Slint via a
// helper component or pre-computed in Rust.
export struct GraphEdgeGeometry {
    from_id: string,
    to_id: string,
    from_x: length,
    from_y: length,
    to_x: length,
    to_y: length,
}

// Host-supplied category → colour map. The component owns NO colours;
// the single source of truth is default_palette() in graph_view_model.rs.
// The host resolves each node's fill/border from this and writes them
// onto the GraphNode (Slint cannot search an array in a binding).
export struct CategoryColor {
    category: string,
    fill: color,
    border: color,
}
```

Create `crates/adapter-gui/ui/components/graph_node_card.slint`:

```slint
// Responsibility: draws one node card of the graph view.
// Single node card. Pure visual; drag/click are owned by the parent
// GraphView TouchArea so we keep one input source per gesture.

import { GraphNode } from "graph_view_types.slint";

export component GraphNodeCard inherits Rectangle {
    in property <GraphNode> node;
    in property <bool> hovered;
    in property <bool> dragging;
    in property <length> card_width;
    in property <length> card_height;

    width: root.card_width;
    height: root.card_height;
    border-radius: 8px;
    border-width: root.node.selected ? 2px : 1px;
    border-color: root.node.selected
        ? #f5f7fb
        : root.node.border;
    background: root.node.bypass
        ? root.node.fill.with-alpha(0.35)
        : root.dragging
        ? root.node.fill.brighter(0.20)
        : root.hovered
        ? root.node.fill.brighter(0.10)
        : root.node.fill;
    opacity: root.node.bypass ? 0.7 : 1.0;
    drop-shadow-blur: root.dragging ? 12px : (root.hovered ? 6px : 2px);
    drop-shadow-color: #00000080;
    drop-shadow-offset-y: 2px;

    Text {
        x: 8px;
        y: 0px;
        width: parent.width - 16px;
        height: parent.height;
        text: root.node.label;
        color: #f5f7fb;
        font-size: 18px;
        font-weight: 700;
        horizontal-alignment: center;
        vertical-alignment: center;
        overflow: elide;
    }

    accessible-role: button;
    accessible-label: root.node.label;
}
```

Create `crates/adapter-gui/ui/components/graph_wire.slint`:

```slint
// Responsibility: draws one wire of the graph view.
// Bezier wire between two viewport-space points. Drawn behind the nodes.
//
// The Path fills the whole canvas and declares its viewbox in canvas
// pixels so MoveTo/CubicTo can use absolute coordinates. Without a
// viewbox, Slint normalises the path commands to the Path's own bbox,
// which collapses every wire to the same screen-space rectangle — the
// reason the earlier version drew all edges converging in one point.
export component GraphWire inherits Path {
    in property <length> from_x;
    in property <length> from_y;
    in property <length> to_x;
    in property <length> to_y;
    in property <length> canvas_width;
    in property <length> canvas_height;
    in property <color> stroke_color: #8aaac8;
    in property <length> stroke_thickness: 2px;

    x: 0px;
    y: 0px;
    width: root.canvas_width;
    height: root.canvas_height;
    viewbox-x: 0;
    viewbox-y: 0;
    viewbox-width: root.canvas_width / 1px;
    viewbox-height: root.canvas_height / 1px;

    stroke: root.stroke_color;
    stroke-width: root.stroke_thickness;
    fill: transparent;

    // Horizontal-S bezier — control points offset by half the
    // horizontal distance keeps wires readable even with vertical
    // separation between parallel paths.
    MoveTo {
        x: root.from_x / 1px;
        y: root.from_y / 1px;
    }

    CubicTo {
        control-1-x: (root.from_x + (root.to_x - root.from_x) / 2) / 1px;
        control-1-y: root.from_y / 1px;
        control-2-x: (root.from_x + (root.to_x - root.from_x) / 2) / 1px;
        control-2-y: root.to_y / 1px;
        x: root.to_x / 1px;
        y: root.to_y / 1px;
    }
}
```

In `crates/adapter-gui/ui/components/graph_view.slint`:

- Keep lines 1-11 (the header and the coordinate-system comment).
- Delete lines 13-155: the four structs, `GraphNodeCard` and `GraphWire`, now in the three files above.
- Put these lines in their place:

```slint
import { GraphNode, GraphEdge, GraphEdgeGeometry, CategoryColor } from "graph_view_types.slint";
import { GraphNodeCard } from "graph_node_card.slint";
import { GraphWire } from "graph_wire.slint";

export { GraphNode, GraphEdge, GraphEdgeGeometry, CategoryColor }
```

Lines 157-420 of the old file (`// The actual GraphView…` through the final `}`) stay unchanged.

- [ ] **Step 9: Run both test files. Expect PASS.**

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_sources --test issue_328_graph_view_interaction`

Expected: `test result: ok. 1 passed` and `test result: ok. 2 passed`. The pins are unchanged, which proves the split preserves behaviour.

- [ ] **Step 10: Docs.** Edit `docs/gui/graph-view.md`.

Replace lines 3-4:

```markdown
**Status:** introduced in #435.
**Source:** `crates/adapter-gui/ui/components/graph_view.slint` + `crates/adapter-gui/src/graph_view_model.rs`.
```

with:

```markdown
**Status:** introduced in #435; grown into the chain editor's canvas in #328.
**Source:** `crates/adapter-gui/ui/components/` — `graph_view.slint` (the canvas), `graph_node_card.slint` (one node card), `graph_wire.slint` (one wire), `graph_view_types.slint` (the structs) — and the Rust model in `crates/adapter-gui/src/graph_view_model/`.
```

Replace line 24 (`  domain  ───►  │  graph_view_model.rs                         │`) with `  domain  ───►  │  graph_view_model/                           │`.

Replace line 172 (`Visual behaviour (drag, zoom, click thresholds) is validated by running the demo example. There is no automated UI test harness for Slint at this time — that's a project-wide gap, not specific to this component.`) with:

```markdown
Pointer behaviour is proven headlessly: `crates/adapter-gui/tests/issue_328_graph_view_interaction.rs` dispatches real pointer events at `GraphViewHarness` (`ui/components/graph_view_test_harness.slint`, test-only, exported through `app-window.slint`). `tests/issue_328_graph_view_sources.rs` pins the file split.
```

- [ ] **Step 11: Pre-push gate.** Every later task runs these same commands. `cargo fmt --all` goes first so the Rust this task wrote takes rustfmt's layout; if it rewrites a file outside the task's `git add` list, stop and report it (the branch was not fmt-clean before the task).

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328
cargo fmt --all
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status --porcelain    # expected: only this task's files
cargo fmt --all -- --check
nice -n 19 cargo test --workspace -j 2 2>&1 | tee target/ppg-test.log | grep -E "^test result|FAILED|^error" | tail -40
grep -c '^warning' target/ppg-test.log    # expected: 0
nice -n 19 cargo build --workspace -j 2 2>&1 | grep -c '^warning'    # expected: 0
VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates    # expected: VALIDATE PASSED
```

- [ ] **Step 12: Commit and push.**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328
git -C $S add crates/adapter-gui/ui/components/graph_view.slint \
  crates/adapter-gui/ui/components/graph_view_types.slint \
  crates/adapter-gui/ui/components/graph_node_card.slint \
  crates/adapter-gui/ui/components/graph_wire.slint \
  crates/adapter-gui/ui/components/graph_view_test_harness.slint \
  crates/adapter-gui/ui/app-window.slint \
  crates/adapter-gui/tests/issue_328_graph_view_interaction.rs \
  crates/adapter-gui/tests/issue_328_graph_view_sources.rs \
  docs/gui/graph-view.md
git -C $S commit -m "feat(#328): split graph_view.slint into card, wire and types behind a pinned harness"
git -C $S fetch && git -C $S push
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 5 task 1 pushed: $(git -C $S rev-parse --short HEAD). GraphView split into graph_view_types/graph_node_card/graph_wire + GraphViewHarness; pins issue_328_graph_view_interaction (2) + issue_328_graph_view_sources (1) green; workspace gate green."
```

---

## Task 2: Wheel — a plain wheel scrolls the parent, Cmd/Ctrl + wheel zooms

**Files:**
- Modify: `crates/adapter-gui/ui/components/graph_view.slint`, in the `pan_area` `scroll-event` handler (old lines 258-276, now about 130-148)
- Modify: `crates/adapter-gui/ui/components/graph_view_test_harness.slint` (add `GraphViewScrollHarness`)
- Modify: `crates/adapter-gui/ui/app-window.slint:28-29` (import and export the new harness on the existing lines)
- Test: `crates/adapter-gui/tests/issue_328_graph_view_interaction.rs`
- Docs: `docs/gui/graph-view.md:132,145`

**Interfaces:**
- Consumes: the `PointerScrollEvent.modifiers.control` field of the Slint builtin. It is ⌘ on macOS and Ctrl on Windows and Linux.
- Produces: `adapter_gui::GraphViewScrollHarness` with `set_nodes`, `get_zoom() -> f32` and `get_list_scroll_y() -> f32`. `GraphView` returns `reject` for a wheel event without the modifier.

- [ ] **Step 1: Add the scroll harness.** Append to `graph_view_test_harness.slint`:

```slint
// The canvas inside a scroll area, the way the chains list hosts it
// (#328 §5.4): proves a plain wheel scrolls the list, not the graph.
export component GraphViewScrollHarness inherits Window {
    width: 900px;
    height: 400px;
    in property <[GraphNode]> nodes;
    in-out property <float> zoom: 1.0;
    out property <length> list-scroll-y: list.viewport-y;

    list := Flickable {
        x: 0px;
        y: 0px;
        width: 100%;
        height: 100%;
        viewport-height: 1200px;

        GraphView {
            x: 0px;
            y: 100px;
            width: 900px;
            height: 300px;
            nodes: root.nodes;
            zoom <=> root.zoom;
        }
    }
}
```

In `app-window.slint`, edit the harness import line added in Task 1 so it reads `import { GraphViewHarness, GraphViewScrollHarness } from "components/graph_view_test_harness.slint";`. Append `, GraphViewScrollHarness` to the export on the next line, just before its closing `}`. The line count stays at 498.

- [ ] **Step 2: Write the failing tests.** In `issue_328_graph_view_interaction.rs`, replace the first two `use` lines with:

```rust
use adapter_gui::{GraphNode, GraphViewHarness, GraphViewScrollHarness};
use slint::platform::{Key, PointerEventButton, WindowEvent};
```

and append:

```rust
/// The graph inside a 1200px-tall scroll area; the canvas spans window
/// y 100..400, so (600, 250) is empty canvas.
fn scroll_harness() -> GraphViewScrollHarness {
    i_slint_backend_testing::init_no_event_loop();
    let w = GraphViewScrollHarness::new().unwrap();
    w.set_nodes(ModelRc::new(VecModel::from(vec![node(
        "od", "Drive", 240.0, 150.0,
    )])));
    w.show().unwrap();
    w
}

fn wheel(w: &impl ComponentHandle, p: LogicalPosition, delta_y: f32) {
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: p });
    win.dispatch_event(WindowEvent::PointerScrolled {
        position: p,
        delta_x: 0.0,
        delta_y,
    });
}

#[test]
fn a_plain_wheel_over_the_graph_scrolls_the_list_instead_of_zooming() {
    let w = scroll_harness();

    wheel(&w, at(600.0, 250.0), -120.0);

    assert_eq!(
        w.get_zoom(),
        1.0,
        "a plain wheel must not zoom the graph — it belongs to the chains list \
         around it (#328 owner decision 6)"
    );
    assert!(
        w.get_list_scroll_y() < 0.0,
        "the wheel over the graph must scroll the list around it; viewport-y stayed {}",
        w.get_list_scroll_y()
    );
}

#[test]
fn cmd_or_ctrl_wheel_zooms_the_graph_and_leaves_the_list_still() {
    let w = scroll_harness();

    // Slint reports ⌘ on macOS and Ctrl on Windows/Linux as `control`.
    w.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    wheel(&w, at(600.0, 250.0), 120.0);
    w.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });

    assert!(
        (w.get_zoom() - 1.1).abs() < 1e-4,
        "Cmd/Ctrl + wheel must zoom one step, zoom is {}",
        w.get_zoom()
    );
    assert_eq!(
        w.get_list_scroll_y(),
        0.0,
        "a zoom gesture must not also scroll the list"
    );
}
```

- [ ] **Step 3: Run. Expect FAIL on the plain-wheel test.** The Cmd/Ctrl test passes already: it is a guard that the fix keeps zoom working.

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_interaction wheel`

Expected:
```
---- a_plain_wheel_over_the_graph_scrolls_the_list_instead_of_zooming stdout ----
assertion `left == right` failed: a plain wheel must not zoom the graph — it belongs to the chains list around it (#328 owner decision 6)
  left: 0.9
 right: 1.0
```

- [ ] **Step 4: Implement.** In `graph_view.slint`, replace:

```slint
        scroll-event(event) => {
            // Zoom around cursor.
            let old_zoom = root.zoom;
```

with:

```slint
        scroll-event(event) => {
            // #328: the graph sits inside the chains list (spec §5.4), so a
            // plain wheel must reach the list and scroll it. Zoom needs the
            // platform shortcut modifier — Slint reports ⌘ on macOS and Ctrl
            // on Windows and Linux as `modifiers.control`.
            if (!event.modifiers.control) {
                return reject;
            }
            // Zoom around cursor.
            let old_zoom = root.zoom;
```

The rest of the handler, including the final `accept`, stays unchanged.

- [ ] **Step 5: Run. Expect PASS.**

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_interaction`

Expected: `test result: ok. 4 passed`

- [ ] **Step 6: Docs.** In `docs/gui/graph-view.md`, replace line 132:

```markdown
- **Zoom:** scroll wheel anywhere over the canvas. Zooms around the cursor (the point under the cursor stays fixed in layout space). Clamped to `[min_zoom, max_zoom]`. Fires `viewport_changed`.
```

with:

```markdown
- **Zoom:** Cmd (macOS) or Ctrl (Windows, Linux) + scroll wheel over the canvas — Slint reports both as `modifiers.control`. Zooms around the cursor (the point under the cursor stays fixed in layout space). Clamped to `[min_zoom, max_zoom]`. Fires `viewport_changed`.
- **Plain wheel:** not accepted. The canvas rejects it so the scroll area around the graph (the chains list, #328) scrolls instead.
```

Replace line 145 (`| Touch gestures (pinch-to-zoom) | future — scroll-wheel only for now |`) with `| Touch gestures (pinch-to-zoom) | future — Cmd/Ctrl + wheel only for now |`.

- [ ] **Step 7: Pre-push gate.** Run the same gate commands as Task 1, Step 11, and expect the same results.

- [ ] **Step 8: Commit and push.**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328
git -C $S add crates/adapter-gui/ui/components/graph_view.slint \
  crates/adapter-gui/ui/components/graph_view_test_harness.slint \
  crates/adapter-gui/ui/app-window.slint \
  crates/adapter-gui/tests/issue_328_graph_view_interaction.rs docs/gui/graph-view.md
git -C $S commit -m "feat(#328): graph view zooms only on Cmd/Ctrl + wheel so the list around it scrolls"
git -C $S fetch && git -C $S push
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 5 task 2 pushed: $(git -C $S rev-parse --short HEAD). Plain wheel rejected (parent Flickable scrolls), control+wheel zooms; 2 wheel tests in issue_328_graph_view_interaction; workspace gate green."
```

---

## Task 3: `ChainStage::Parallel { lanes, end }` with `ParallelEnd::Fan`

**Files:**
- Modify: `crates/adapter-gui/src/graph_view_model/types.rs:89-99` (`ChainStage`, plus the new `ParallelEnd`)
- Modify: `crates/adapter-gui/src/graph_view_model/chain_builder.rs` (whole file, 139 lines)
- Modify: `crates/adapter-gui/src/graph_view_model/layout.rs:34-61` (`longest_path_ranks`)
- Modify: `crates/adapter-gui/src/graph_view_model/validation.rs` (whole file, 38 lines)
- Modify: `crates/adapter-gui/src/graph_view_model/mod.rs:10-25`
- Test: `crates/adapter-gui/src/graph_view_model_tests.rs`. It gets a new module `fan_out_stage`, and the existing `Parallel(…)` constructors change mechanically, with no assertion changed.
- Docs: `docs/gui/graph-view.md:113,116,118`

**Interfaces:**
- Produces:
  - `pub enum ChainStage { Single(BlockBlueprint), Parallel { lanes: Vec<Vec<BlockBlueprint>>, end: ParallelEnd } }`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum ParallelEnd { Merge, Fan }`
  - `pub fn validate_stages(stages: &[ChainStage]) -> Vec<String>`
  - `linear_chain_layout` keeps its signature `(stages: &[ChainStage], metrics: GridMetrics) -> (Vec<GraphNode>, Vec<GraphEdge>)`.
  - `topological_layout` keeps its signature. It now moves terminals to the last rank.

- [ ] **Step 1: Add the minimum surface so the new tests compile.** No behaviour changes in this step.

In `types.rs`, replace lines 89-99 (the `ChainStage` doc and enum) with:

```rust
/// Logical stage of a signal chain. The layout helpers consume a
/// sequence of stages and produce positioned [`GraphNode`]s and
/// [`GraphEdge`]s.
#[derive(Debug, Clone, PartialEq)]
pub enum ChainStage {
    /// A single block — sits alone in one column.
    Single(BlockBlueprint),
    /// Parallel lanes after an auto-generated split node. Each inner `Vec`
    /// is one lane, top to bottom; `end` decides whether the lanes merge
    /// again or fan out to one terminal each (#328).
    Parallel {
        lanes: Vec<Vec<BlockBlueprint>>,
        end: ParallelEnd,
    },
}

/// How a [`ChainStage::Parallel`] ends (#328).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParallelEnd {
    /// The lanes meet again at an auto-generated merge node and the next
    /// stage continues from it (Split → Mix).
    Merge,
    /// No merge node: each lane's last blueprint is its terminal — a Y
    /// chain's output node — and nothing may follow (Y → A/B).
    Fan,
}
```

In `chain_builder.rs`, change only the two match arms so they compile. The body stays as it is and treats every end as a merge:

- `ChainStage::Parallel(paths) if paths.is_empty() => {` becomes `ChainStage::Parallel { lanes: paths, .. } if paths.is_empty() => {`
- `ChainStage::Parallel(paths) => {` becomes `ChainStage::Parallel { lanes: paths, .. } => {`

In `validation.rs`, change the `use` line to `use super::types::{ChainStage, GraphEdge, GraphNode};` and append the stub:

```rust
/// Validate a stage sequence before it is laid out (#328).
pub fn validate_stages(_stages: &[ChainStage]) -> Vec<String> {
    Vec::new()
}
```

In `mod.rs`, change lines 24-25 to:

```rust
pub use types::{
    BlockBlueprint, ChainStage, GraphEdge, GraphNode, GridMetrics, NodeCategory, ParallelEnd,
};
pub use validation::{validate_graph, validate_stages};
```

- [ ] **Step 2: Rewrite the existing constructors mechanically.** Only the constructor changes; no assertion changes. The `Parallel` variant is now a struct variant (contract), so in `graph_view_model_tests.rs`:

- Change lines 5-8 to:

```rust
use super::{
    linear_chain_layout, validate_graph, BlockBlueprint, ChainStage, GraphEdge, GraphNode,
    GridMetrics, NodeCategory, ParallelEnd,
};
```

- Rewrite each `ChainStage::Parallel(vec![ <lanes> ])` as `ChainStage::Parallel { lanes: vec![ <lanes> ], end: ParallelEnd::Merge }`. The occurrences are at lines 321, 345, 377, 393, 409, 431, 454, 569, and the empty one at line 476 (`ChainStage::Parallel(vec![])` becomes `ChainStage::Parallel { lanes: vec![], end: ParallelEnd::Merge }`).

- [ ] **Step 3: Write the failing tests.** Append to `graph_view_model_tests.rs`:

```rust
mod fan_out_stage {
    use super::*;
    use crate::graph_view_model::{topological_layout, validate_stages};

    fn metrics() -> GridMetrics {
        GridMetrics {
            origin_x: 0.0,
            origin_y: 0.0,
            column_spacing: 100.0,
            lane_spacing: 80.0,
        }
    }

    /// Y → A/B: path A runs an amp into its output, path B goes straight
    /// to its own output.
    fn y_split() -> ChainStage {
        ChainStage::Parallel {
            lanes: vec![
                vec![
                    block("amp_a", "Amp A", NodeCategory::Amp),
                    block("out_a", "Out A", NodeCategory::Output),
                ],
                vec![block("out_b", "Out B", NodeCategory::Output)],
            ],
            end: ParallelEnd::Fan,
        }
    }

    #[test]
    fn fan_draws_no_mixer_node() {
        let (nodes, _) = linear_chain_layout(&[y_split()], metrics());
        assert!(
            !nodes.iter().any(|n| n.id.starts_with("__merge_")),
            "a Y split has no mixer, got nodes {:?}",
            nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn fan_lanes_each_end_at_their_own_terminal() {
        let (_, edges) = linear_chain_layout(&[y_split()], metrics());
        let wires: Vec<(&str, &str)> = edges
            .iter()
            .map(|e| (e.from_id.as_str(), e.to_id.as_str()))
            .collect();
        assert_eq!(
            wires,
            [
                ("__split_1", "amp_a"),
                ("amp_a", "out_a"),
                ("__split_1", "out_b")
            ],
            "each lane ends at its terminal; nothing leaves a terminal"
        );
    }

    #[test]
    fn fan_terminals_share_the_last_column() {
        let (nodes, _) = linear_chain_layout(&[y_split()], metrics());
        // Split on column 0, longest lane = 2 blueprints → terminals on column 2.
        assert_eq!(find_node(&nodes, "amp_a").x, 100.0);
        assert_eq!(find_node(&nodes, "out_a").x, 200.0);
        assert_eq!(
            find_node(&nodes, "out_b").x,
            200.0,
            "path B's output must line up with path A's"
        );
    }

    #[test]
    fn fan_keeps_path_a_above_path_b() {
        let (nodes, _) = linear_chain_layout(&[y_split()], metrics());
        assert_eq!(find_node(&nodes, "out_a").y, -40.0);
        assert_eq!(find_node(&nodes, "out_b").y, 40.0);
    }

    #[test]
    fn fan_layout_is_a_valid_graph() {
        let stages = [
            ChainStage::Single(block("in", "In", NodeCategory::Input)),
            y_split(),
        ];
        let (nodes, edges) = linear_chain_layout(&stages, metrics());
        let errs = validate_graph(&nodes, &edges);
        assert!(errs.is_empty(), "fan layout produced invalid graph: {errs:?}");
    }

    #[test]
    fn a_stage_after_a_fan_is_reported() {
        let stages = [
            y_split(),
            ChainStage::Single(block("rev", "Rev", NodeCategory::Reverb)),
        ];
        let errs = validate_stages(&stages);
        assert!(
            errs.iter()
                .any(|e| e.contains("stage 1 follows the fan-out at stage 0")),
            "got: {errs:?}"
        );
    }

    #[test]
    fn an_empty_fan_lane_is_reported() {
        let stages = [ChainStage::Parallel {
            lanes: vec![vec![block("out_a", "Out A", NodeCategory::Output)], vec![]],
            end: ParallelEnd::Fan,
        }];
        let errs = validate_stages(&stages);
        assert!(
            errs.iter().any(|e| e.contains("fan lane 1 of stage 0 is empty")),
            "got: {errs:?}"
        );
    }

    #[test]
    fn merge_and_single_stages_validate_clean() {
        let stages = [
            ChainStage::Single(block("in", "In", NodeCategory::Input)),
            ChainStage::Parallel {
                lanes: vec![vec![block("l", "L", NodeCategory::Amp)], vec![]],
                end: ParallelEnd::Merge,
            },
            ChainStage::Single(block("out", "Out", NodeCategory::Output)),
        ];
        assert!(validate_stages(&stages).is_empty());
    }

    #[test]
    fn topological_layout_lines_up_fan_terminals_on_the_last_column() {
        let (nodes, edges) = linear_chain_layout(&[y_split()], metrics());
        let out = topological_layout(&nodes, &edges, metrics());
        let x = |id: &str| out.iter().find(|n| n.id == id).unwrap().x;
        assert_eq!(
            x("out_b"),
            x("out_a"),
            "auto layout must keep the Y outputs side by side"
        );
    }
}
```

- [ ] **Step 4: Run. Expect FAIL.** The 28 existing tests must stay green.

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --lib graph_view_model`

Expected, among others:
```
---- graph_view_model::tests::fan_out_stage::fan_draws_no_mixer_node stdout ----
a Y split has no mixer, got nodes ["__split_1", "amp_a", "out_a", "out_b", "__merge_1"]
---- graph_view_model::tests::fan_out_stage::a_stage_after_a_fan_is_reported stdout ----
got: []
---- graph_view_model::tests::fan_out_stage::topological_layout_lines_up_fan_terminals_on_the_last_column stdout ----
assertion `left == right` failed: auto layout must keep the Y outputs side by side
```

Every test in the old modules passes. `merge_and_single_stages_validate_clean` also passes: it guards against false positives.

- [ ] **Step 5: Implement the layout.** Replace `crates/adapter-gui/src/graph_view_model/chain_builder.rs` with:

```rust
//! Responsibility: builds a positioned graph from a chain's stages
//!
//! Layout strategy:
//!
//! - [`super::types::ChainStage::Single`] blocks sit on the central lane and
//!   advance the column cursor by one.
//! - `Parallel` starts with an auto-generated split node and places each
//!   lane on its own row (above/below the centre, distributed
//!   symmetrically). With [`ParallelEnd::Merge`] the lanes meet again at an
//!   auto-generated merge node on the column after the longest lane, and
//!   the next stage continues from it. With [`ParallelEnd::Fan`] there is
//!   no merge node: each lane's last blueprint is its terminal, and the
//!   terminals line up on the longest lane's last column (#328).

use super::types::{
    BlockBlueprint, ChainStage, GraphEdge, GraphNode, GridMetrics, NodeCategory, ParallelEnd,
};

/// Build a positioned graph from a sequence of [`ChainStage`]s.
///
/// Returns the (nodes, edges) pair ready to push to the Slint side. IDs
/// must be unique across the whole input — duplicates produce undefined
/// behaviour at the UI level (the panic-free contract is kept here, but
/// the UI may render only one of the duplicates). A stage after a
/// [`ParallelEnd::Fan`] has nothing to connect from and is left
/// unconnected; `validate_stages` reports it.
pub fn linear_chain_layout(
    stages: &[ChainStage],
    metrics: GridMetrics,
) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut col: usize = 0;
    let mut prev_tail: Option<String> = None;
    let mut split_counter: usize = 0;

    for stage in stages {
        match stage {
            ChainStage::Single(block) => {
                let node = position_block(block, col, 0, &metrics);
                if let Some(prev) = prev_tail.take() {
                    edges.push(GraphEdge {
                        from_id: prev,
                        to_id: node.id.clone(),
                    });
                }
                prev_tail = Some(node.id.clone());
                nodes.push(node);
                col += 1;
            }
            ChainStage::Parallel { lanes, .. } if lanes.is_empty() => {
                // No-op — nothing to render, no column consumed.
            }
            ChainStage::Parallel { lanes, end } => {
                split_counter += 1;
                let split_id = format!("__split_{split_counter}");
                let merge_id = format!("__merge_{split_counter}");

                let longest = lanes.iter().map(Vec::len).max().unwrap_or(0);
                let split_col = col;

                // Split node sits at split_col on the centre lane.
                nodes.push(routing_node(&split_id, split_col, &metrics));
                if let Some(prev) = prev_tail.take() {
                    edges.push(GraphEdge {
                        from_id: prev,
                        to_id: split_id.clone(),
                    });
                }

                // Each lane occupies its own row. With N lanes, rows are
                // -N/2..N/2 around the centre; 2 lanes → -0.5 / +0.5.
                let n_lanes = lanes.len() as f32;
                for (lane_idx, lane) in lanes.iter().enumerate() {
                    let lane_offset = lane_idx as f32 - (n_lanes - 1.0) / 2.0;
                    let mut last_in_lane = split_id.clone();
                    for (block_idx, block) in lane.iter().enumerate() {
                        let column = lane_column(*end, split_col, block_idx, lane.len(), longest);
                        let node = position_block_lane(block, column, lane_offset, &metrics);
                        edges.push(GraphEdge {
                            from_id: last_in_lane,
                            to_id: node.id.clone(),
                        });
                        last_in_lane = node.id.clone();
                        nodes.push(node);
                    }
                    if *end == ParallelEnd::Merge {
                        edges.push(GraphEdge {
                            from_id: last_in_lane,
                            to_id: merge_id.clone(),
                        });
                    }
                }

                match end {
                    ParallelEnd::Merge => {
                        // Merge node sits on the column after the longest
                        // lane, on the centre lane.
                        let merge_col = split_col + longest + 1;
                        nodes.push(routing_node(&merge_id, merge_col, &metrics));
                        prev_tail = Some(merge_id);
                        col = merge_col + 1;
                    }
                    ParallelEnd::Fan => {
                        // Every lane already ended at its own terminal.
                        prev_tail = None;
                        col = split_col + longest + 1;
                    }
                }
            }
        }
    }

    (nodes, edges)
}

/// Column of blueprint `index` in a lane of `len` blueprints. In a Fan the
/// lane's last blueprint is its terminal and lines up with every other
/// lane's terminal on the longest lane's last column, the way a Y chain's
/// output nodes sit side by side.
fn lane_column(
    end: ParallelEnd,
    split_col: usize,
    index: usize,
    len: usize,
    longest: usize,
) -> usize {
    if end == ParallelEnd::Fan && index + 1 == len {
        split_col + longest
    } else {
        split_col + 1 + index
    }
}

/// An auto-generated split or merge node: no label, `Util` category, on
/// the centre lane.
fn routing_node(id: &str, col: usize, metrics: &GridMetrics) -> GraphNode {
    GraphNode {
        id: id.to_string(),
        label: String::new(),
        category: NodeCategory::Util,
        x: metrics.origin_x + col as f32 * metrics.column_spacing,
        y: metrics.origin_y,
        bypass: false,
    }
}

fn position_block(
    block: &BlockBlueprint,
    col: usize,
    lane: i32,
    metrics: &GridMetrics,
) -> GraphNode {
    position_block_lane(block, col, lane as f32, metrics)
}

fn position_block_lane(
    block: &BlockBlueprint,
    col: usize,
    lane: f32,
    metrics: &GridMetrics,
) -> GraphNode {
    GraphNode {
        id: block.id.clone(),
        label: block.label.clone(),
        category: block.category,
        x: metrics.origin_x + col as f32 * metrics.column_spacing,
        y: metrics.origin_y + lane * metrics.lane_spacing,
        bypass: block.bypass,
    }
}
```

- [ ] **Step 6: Implement stage validation.** Replace `crates/adapter-gui/src/graph_view_model/validation.rs` with:

```rust
//! Responsibility: reports what makes a graph description ill-formed
//!
//! Guards a chain's stages before layout and the layout output before it
//! reaches the UI.

use std::collections::HashMap;

use super::types::{ChainStage, GraphEdge, GraphNode, ParallelEnd};

/// Validate that the (nodes, edges) pair is a well-formed graph:
///
/// - every node id is unique,
/// - every edge references existing node ids,
/// - no node references itself.
///
/// Returns a list of error messages — empty means valid.
pub fn validate_graph(nodes: &[GraphNode], edges: &[GraphEdge]) -> Vec<String> {
    let mut errors = Vec::new();
    let mut ids: HashMap<&str, usize> = HashMap::new();
    for node in nodes {
        let count = ids.entry(node.id.as_str()).or_insert(0);
        *count += 1;
        if *count == 2 {
            errors.push(format!("duplicate node id: {}", node.id));
        }
    }
    for edge in edges {
        if !ids.contains_key(edge.from_id.as_str()) {
            errors.push(format!("edge references unknown source: {}", edge.from_id));
        }
        if !ids.contains_key(edge.to_id.as_str()) {
            errors.push(format!("edge references unknown target: {}", edge.to_id));
        }
        if edge.from_id == edge.to_id {
            errors.push(format!("self-loop on node: {}", edge.from_id));
        }
    }
    errors
}

/// Validate a stage sequence before it is laid out (#328):
///
/// - nothing may follow a [`ParallelEnd::Fan`] — every lane already ended
///   at its own terminal, so a following stage would float unconnected;
/// - a Fan lane may not be empty — its last blueprint IS its terminal.
///
/// Returns a list of error messages — empty means valid.
pub fn validate_stages(stages: &[ChainStage]) -> Vec<String> {
    let mut errors = Vec::new();
    let mut fan_out: Option<usize> = None;
    for (index, stage) in stages.iter().enumerate() {
        if let Some(fan) = fan_out {
            errors.push(format!(
                "stage {index} follows the fan-out at stage {fan} and has no input"
            ));
        }
        if let ChainStage::Parallel {
            lanes,
            end: ParallelEnd::Fan,
        } = stage
        {
            for (lane, blueprints) in lanes.iter().enumerate() {
                if blueprints.is_empty() {
                    errors.push(format!(
                        "fan lane {lane} of stage {index} is empty and has no terminal"
                    ));
                }
            }
            if !lanes.is_empty() && fan_out.is_none() {
                fan_out = Some(index);
            }
        }
    }
    errors
}
```

- [ ] **Step 7: Align terminals in the auto layout.** In `layout.rs`, replace lines 34-61 (`longest_path_ranks`) with:

```rust
/// Longest-path rank per node index (column). `None` if the graph has a
/// cycle — caller falls back to input order. A terminal (a node with
/// inputs and no outputs) moves to the last rank, so the terminals of a
/// fan-out — a Y chain's output nodes — line up on one column (#328).
fn longest_path_ranks(node_ids: &[&str], edges: &[GraphEdge]) -> Option<Vec<usize>> {
    let n = node_ids.len();
    let node_idx: HashMap<&str, usize> =
        node_ids.iter().enumerate().map(|(i, s)| (*s, i)).collect();
    let (outs, indeg) = build_adjacency(&node_idx, n, edges);
    let mut rank = vec![0usize; n];
    let mut queue: Vec<usize> = (0..n).filter(|i| indeg[*i] == 0).collect();
    let mut indeg_w = indeg.clone();
    let mut head = 0;
    let mut processed = 0usize;
    while head < queue.len() {
        let u = queue[head];
        head += 1;
        processed += 1;
        for &v in &outs[u] {
            if rank[u] + 1 > rank[v] {
                rank[v] = rank[u] + 1;
            }
            indeg_w[v] -= 1;
            if indeg_w[v] == 0 {
                queue.push(v);
            }
        }
    }
    if processed != n {
        return None;
    }
    let last = rank.iter().copied().max().unwrap_or(0);
    for (i, r) in rank.iter_mut().enumerate() {
        if outs[i].is_empty() && indeg[i] > 0 {
            *r = last;
        }
    }
    Some(rank)
}
```

- [ ] **Step 8: Run. Expect PASS.** All 28 old tests plus the 9 new ones pass.

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --lib graph_view_model`

Expected: `test result: ok. 37 passed`

- [ ] **Step 9: Docs.** In `docs/gui/graph-view.md`:

Replace line 113 (``| `ChainStage` | `Single(...)` or `Parallel(Vec<Vec<...>>)` |``) with:

```markdown
| `ChainStage` | `Single(...)` or `Parallel { lanes, end }` — `lanes` top to bottom |
| `ParallelEnd` | `Merge`: the lanes meet again at an auto-generated merge node. `Fan`: no merge node; each lane's last blueprint is its terminal (a Y chain's output node), the terminals share the last column, and nothing may follow (#328) |
```

After line 116 (the `validate_graph` row), add:

```markdown
| `validate_stages(stages)` | returns error strings for a stage list: a stage after a `Fan`, an empty `Fan` lane |
```

Replace line 118 with:

```markdown
`linear_chain_layout` is pure — same input, same output. Used in tests + at runtime to compute positions from a logical chain description. Splits and merges are auto-generated with id prefix `__split_N` / `__merge_N` (a `Fan` has no `__merge_N`). `topological_layout` (auto mode) moves every terminal — a node with inputs and no outputs — to the last column, so a fan-out's terminals stay side by side there too.
```

- [ ] **Step 10: Pre-push gate.** Run the same gate commands as Task 1, Step 11.

- [ ] **Step 11: Commit and push.**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328
git -C $S add crates/adapter-gui/src/graph_view_model/types.rs \
  crates/adapter-gui/src/graph_view_model/chain_builder.rs \
  crates/adapter-gui/src/graph_view_model/layout.rs \
  crates/adapter-gui/src/graph_view_model/validation.rs \
  crates/adapter-gui/src/graph_view_model/mod.rs \
  crates/adapter-gui/src/graph_view_model_tests.rs docs/gui/graph-view.md
git -C $S commit -m "feat(#328): parallel stages merge or fan out; fan terminals line up and nothing may follow"
git -C $S fetch && git -C $S push
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 5 task 3 pushed: $(git -C $S rev-parse --short HEAD). ChainStage::Parallel { lanes, end: ParallelEnd::{Merge,Fan} }, validate_stages, auto-layout terminals aligned; graph_view_model 37 tests green (old constructors rewritten mechanically, assertions unchanged); workspace gate green."
```

---

## Task 4: Node kinds in the Rust model

**Files:**
- Modify: `crates/adapter-gui/src/graph_view_model/types.rs`. Add `NodeKind` after the `NodeCategory` impl (after line 58), add `kind` to `GraphNode` (after line 71), and add `kind` and `with_kind` to `BlockBlueprint`.
- Modify: `crates/adapter-gui/src/graph_view_model/chain_builder.rs` (`routing_node`, `position_block_lane`, the two `routing_node` calls)
- Modify: `crates/adapter-gui/src/graph_view_model/mod.rs` (export `NodeKind`)
- Test: `crates/adapter-gui/src/graph_view_model_tests.rs`. It gets a new module `node_kinds`, and the existing `GraphNode { … }` literals gain `kind: NodeKind::Block` mechanically.
- Docs: `docs/gui/graph-view.md:111`

**Interfaces:**
- Produces:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)] pub enum NodeKind { #[default] Block, IoInput, IoOutput, Split, Mixer }`
  - `NodeKind::as_str(self) -> &'static str`, which returns `"block" | "io_input" | "io_output" | "split" | "mixer"` (the Slint `GraphNode.kind` contract)
  - `GraphNode.kind: NodeKind`
  - `BlockBlueprint.kind: NodeKind`
  - `BlockBlueprint::with_kind(self, kind: NodeKind) -> Self`

- [ ] **Step 1: Add the minimum surface.** Kinds are not propagated yet.

In `types.rs`, insert after line 58 (the end of `impl NodeCategory`):

```rust

/// What a node IS in the chain editor — picks the card the UI draws
/// (#328). Distinct from [`NodeCategory`], which only picks a colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NodeKind {
    /// A processing block.
    #[default]
    Block,
    /// The chain's input node.
    IoInput,
    /// A chain output node (one per lane in a fan-out).
    IoOutput,
    /// Where the signal becomes parallel lanes.
    Split,
    /// Where parallel lanes are summed back into one.
    Mixer,
}

impl NodeKind {
    /// Slug the Slint `GraphNode.kind` field carries.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::IoInput => "io_input",
            Self::IoOutput => "io_output",
            Self::Split => "split",
            Self::Mixer => "mixer",
        }
    }
}
```

In `GraphNode`, after the `category` field, add:

```rust
    /// What the node is — picks its card on the Slint side.
    pub kind: NodeKind,
```

Replace the `BlockBlueprint` struct and its `impl` with:

```rust
/// Logical description of one block, without position. Position is
/// assigned by the chain builder.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockBlueprint {
    pub id: String,
    pub label: String,
    pub category: NodeCategory,
    pub bypass: bool,
    /// What the node is. [`BlockBlueprint::new`] makes a block; the host
    /// marks its I/O nodes with [`BlockBlueprint::with_kind`].
    pub kind: NodeKind,
}

impl BlockBlueprint {
    pub fn new(id: impl Into<String>, label: impl Into<String>, category: NodeCategory) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            category,
            bypass: false,
            kind: NodeKind::Block,
        }
    }

    /// The same blueprint as another kind of node (#328).
    pub fn with_kind(mut self, kind: NodeKind) -> Self {
        self.kind = kind;
        self
    }
}
```

In `chain_builder.rs`, import `NodeKind` (`use super::types::{BlockBlueprint, ChainStage, GraphEdge, GraphNode, GridMetrics, NodeCategory, NodeKind, ParallelEnd};`). Then add `kind: NodeKind::Block,` after the `category` line of both `routing_node` and `position_block_lane`. This is the surface only: every node reads as a block.

In `mod.rs`, add `NodeKind` to the `pub use types::{…}` list.

- [ ] **Step 2: Update the existing literals mechanically.** No assertion changes. In `graph_view_model_tests.rs`:

- Add `NodeKind` to the top `use super::{…}` list.
- Insert `kind: NodeKind::Block,` after the `category:` line of every `GraphNode { … }` literal. These are the three `tn()` helpers (old lines 77-86, 140-149, 196-205) and the four literals in `validate_graph_invariants` (old lines 500-515, 526-533, 544-551).

- [ ] **Step 3: Write the failing tests.** Append:

```rust
mod node_kinds {
    use super::*;

    /// Contract pin: these slugs are what the Slint `GraphNode.kind` field
    /// carries (graph_view_types.slint).
    #[test]
    fn kind_slugs_match_the_slint_graph_node_contract() {
        assert_eq!(NodeKind::Block.as_str(), "block");
        assert_eq!(NodeKind::IoInput.as_str(), "io_input");
        assert_eq!(NodeKind::IoOutput.as_str(), "io_output");
        assert_eq!(NodeKind::Split.as_str(), "split");
        assert_eq!(NodeKind::Mixer.as_str(), "mixer");
    }

    #[test]
    fn a_blueprint_kind_reaches_its_positioned_node() {
        let stages = [
            ChainStage::Single(
                block("in", "In 1", NodeCategory::Input).with_kind(NodeKind::IoInput),
            ),
            ChainStage::Single(block("od", "OD", NodeCategory::Drive)),
            ChainStage::Single(
                block("out", "Out 1", NodeCategory::Output).with_kind(NodeKind::IoOutput),
            ),
        ];
        let (nodes, _) = linear_chain_layout(&stages, GridMetrics::default());
        assert_eq!(find_node(&nodes, "in").kind, NodeKind::IoInput);
        assert_eq!(
            find_node(&nodes, "od").kind,
            NodeKind::Block,
            "a plain blueprint is a block"
        );
        assert_eq!(find_node(&nodes, "out").kind, NodeKind::IoOutput);
    }

    #[test]
    fn a_merge_parallel_marks_its_split_and_mixer_nodes() {
        let stages = [ChainStage::Parallel {
            lanes: vec![
                vec![block("l", "L", NodeCategory::Amp)],
                vec![block("r", "R", NodeCategory::Amp)],
            ],
            end: ParallelEnd::Merge,
        }];
        let (nodes, _) = linear_chain_layout(&stages, GridMetrics::default());
        assert_eq!(find_node(&nodes, "__split_1").kind, NodeKind::Split);
        assert_eq!(find_node(&nodes, "__merge_1").kind, NodeKind::Mixer);
    }

    #[test]
    fn a_fan_parallel_has_a_split_node_and_no_mixer() {
        let stages = [ChainStage::Parallel {
            lanes: vec![
                vec![
                    block("amp_a", "Amp A", NodeCategory::Amp),
                    block("out_a", "Out A", NodeCategory::Output).with_kind(NodeKind::IoOutput),
                ],
                vec![block("out_b", "Out B", NodeCategory::Output).with_kind(NodeKind::IoOutput)],
            ],
            end: ParallelEnd::Fan,
        }];
        let (nodes, _) = linear_chain_layout(&stages, GridMetrics::default());
        assert_eq!(find_node(&nodes, "__split_1").kind, NodeKind::Split);
        assert!(
            nodes.iter().all(|n| n.kind != NodeKind::Mixer),
            "a Y split has no mixer node"
        );
        assert_eq!(find_node(&nodes, "out_b").kind, NodeKind::IoOutput);
    }
}
```

- [ ] **Step 4: Run. Expect FAIL.**

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --lib graph_view_model::tests::node_kinds`

Expected (`a_fan_parallel_has_a_split_node_and_no_mixer` fails the same way as the merge case):
```
---- graph_view_model::tests::node_kinds::a_blueprint_kind_reaches_its_positioned_node stdout ----
assertion `left == right` failed
  left: Block
 right: IoInput
---- graph_view_model::tests::node_kinds::a_merge_parallel_marks_its_split_and_mixer_nodes stdout ----
  left: Block
 right: Split
```

`kind_slugs_match_the_slint_graph_node_contract` passes: it is a contract pin, and the table is part of the surface.

- [ ] **Step 5: Implement.** In `chain_builder.rs`:

- Change `routing_node` to take the kind:

```rust
/// An auto-generated split or merge node: no label, `Util` category, on
/// the centre lane. Its card is picked by `kind`; the label stays empty —
/// the Slint card translates the name.
fn routing_node(id: &str, kind: NodeKind, col: usize, metrics: &GridMetrics) -> GraphNode {
    GraphNode {
        id: id.to_string(),
        label: String::new(),
        category: NodeCategory::Util,
        kind,
        x: metrics.origin_x + col as f32 * metrics.column_spacing,
        y: metrics.origin_y,
        bypass: false,
    }
}
```

- Change the calls to `routing_node(&split_id, NodeKind::Split, split_col, &metrics)` and `routing_node(&merge_id, NodeKind::Mixer, merge_col, &metrics)`.
- In `position_block_lane`, replace `kind: NodeKind::Block,` with `kind: block.kind,`.

- [ ] **Step 6: Run. Expect PASS.**

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --lib graph_view_model`

Expected: `test result: ok. 41 passed`

- [ ] **Step 7: Docs.** In `docs/gui/graph-view.md`, after line 111 (the `NodeCategory` row), add:

```markdown
| `NodeKind` | what a node IS — `Block`, `IoInput`, `IoOutput`, `Split`, `Mixer`; `as_str()` gives the slug the Slint `GraphNode.kind` carries. The auto-generated split node is `Split`, the merge node `Mixer`; `BlockBlueprint::with_kind` marks the host's I/O nodes |
```

- [ ] **Step 8: Pre-push gate.** Run the same gate commands as Task 1, Step 11.

- [ ] **Step 9: Commit and push.**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328
git -C $S add crates/adapter-gui/src/graph_view_model/types.rs \
  crates/adapter-gui/src/graph_view_model/chain_builder.rs \
  crates/adapter-gui/src/graph_view_model/mod.rs \
  crates/adapter-gui/src/graph_view_model_tests.rs docs/gui/graph-view.md
git -C $S commit -m "feat(#328): graph nodes carry a kind — block, io_input, io_output, split, mixer"
git -C $S fetch && git -C $S push
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 5 task 4 pushed: $(git -C $S rev-parse --short HEAD). NodeKind on GraphNode/BlockBlueprint, routing nodes Split/Mixer; graph_view_model 41 tests green; workspace gate green."
```

---

## Task 5: Insert anchors on the wires (pure Rust)

**Files:**
- Create: `crates/adapter-gui/src/graph_view_model/routing_ids.rs`
- Create: `crates/adapter-gui/src/graph_view_model/anchors.rs`
- Modify: `crates/adapter-gui/src/graph_view_model/chain_builder.rs` (use `routing_ids`, the two `format!` lines)
- Modify: `crates/adapter-gui/src/graph_view_model/mod.rs`
- Test: `crates/adapter-gui/src/graph_view_model_anchor_tests.rs` (create)
- Docs: `docs/gui/graph-view.md` (Rust helpers table)

**Interfaces:**
- Consumes: `linear_chain_layout`, `NodeKind`, `ParallelEnd`.
- Produces:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum AnchorSlot { Stage { index: usize }, Lane { stage: usize, lane: usize, index: usize } }`
  - `AnchorSlot::anchor_id(self) -> String`, which returns `"stage:{i}"` or `"lane:{stage}:{lane}:{i}"`
  - `#[derive(Debug, Clone, PartialEq)] pub struct GraphAnchor { pub id: String, pub from_id: String, pub to_id: String, pub slot: AnchorSlot, pub x: f32, pub y: f32, pub always_visible: bool }`
  - `pub fn insert_anchors(stages: &[ChainStage], nodes: &[GraphNode]) -> Vec<GraphAnchor>`
  - Crate-internal: `routing_ids::{split_node_id, merge_node_id}(n: usize) -> String`

- [ ] **Step 1: Add the minimum surface.** Create `crates/adapter-gui/src/graph_view_model/anchors.rs`:

```rust
//! Responsibility: places the insert anchors on a laid-out chain's wires
//!
//! One anchor per wire, at the wire's midpoint — exactly where the
//! horizontal-S Bézier the canvas draws crosses t = 0.5. Each anchor
//! carries the slot a block added or dropped there lands in (#328 §5.1).

use super::types::{ChainStage, GraphNode};

/// Where a block added or dropped on an anchor lands. Indices are in the
/// ORIGINAL stage list / lane, before any move: "insert before this one".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorSlot {
    /// Before top-level stage `index`.
    Stage { index: usize },
    /// Before blueprint `index` of lane `lane` in the parallel stage `stage`.
    Lane {
        stage: usize,
        lane: usize,
        index: usize,
    },
}

impl AnchorSlot {
    /// Stable id of the anchor at this slot — unique within one graph.
    pub fn anchor_id(self) -> String {
        match self {
            Self::Stage { index } => format!("stage:{index}"),
            Self::Lane { stage, lane, index } => format!("lane:{stage}:{lane}:{index}"),
        }
    }
}

/// A "+" and drop target on one wire of the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphAnchor {
    /// [`AnchorSlot::anchor_id`] of `slot`.
    pub id: String,
    /// The wire this anchor sits on.
    pub from_id: String,
    pub to_id: String,
    pub slot: AnchorSlot,
    /// Layout-space midpoint of the wire.
    pub x: f32,
    pub y: f32,
    /// An empty segment — no block at either end of the wire — keeps its
    /// "+" visible, like the chain row's empty insert slot.
    pub always_visible: bool,
}

/// Anchors for the graph `linear_chain_layout(stages, ..)` returned as
/// `nodes`.
pub fn insert_anchors(_stages: &[ChainStage], _nodes: &[GraphNode]) -> Vec<GraphAnchor> {
    Vec::new()
}
```

In `mod.rs`, add `mod anchors;` and `pub use anchors::{insert_anchors, AnchorSlot, GraphAnchor};`. After the existing `tests` module, add:

```rust
#[cfg(test)]
#[path = "../graph_view_model_anchor_tests.rs"]
mod anchor_tests;
```

- [ ] **Step 2: Write the failing tests.** Create `crates/adapter-gui/src/graph_view_model_anchor_tests.rs`:

```rust
//! Tests for `graph_view_model::anchors` (#328 §5.1): one insert anchor
//! per wire, at its midpoint, carrying the slot a block lands in.

use super::{
    insert_anchors, linear_chain_layout, AnchorSlot, BlockBlueprint, ChainStage, GraphAnchor,
    GridMetrics, NodeCategory, NodeKind, ParallelEnd,
};

fn block(id: &str) -> BlockBlueprint {
    BlockBlueprint::new(id, id.to_uppercase(), NodeCategory::Drive)
}

fn io_in(id: &str) -> BlockBlueprint {
    BlockBlueprint::new(id, "In 1", NodeCategory::Input).with_kind(NodeKind::IoInput)
}

fn io_out(id: &str) -> BlockBlueprint {
    BlockBlueprint::new(id, "Out 1", NodeCategory::Output).with_kind(NodeKind::IoOutput)
}

fn anchors_of(stages: &[ChainStage]) -> Vec<GraphAnchor> {
    let (nodes, _) = linear_chain_layout(stages, GridMetrics::default());
    insert_anchors(stages, &nodes)
}

fn slots(anchors: &[GraphAnchor]) -> Vec<(&str, &str, AnchorSlot)> {
    anchors
        .iter()
        .map(|a| (a.from_id.as_str(), a.to_id.as_str(), a.slot))
        .collect()
}

/// in → split → [a1, a2] ∥ [b1] → mixer → out
fn split_mix() -> Vec<ChainStage> {
    vec![
        ChainStage::Single(io_in("in")),
        ChainStage::Parallel {
            lanes: vec![vec![block("a1"), block("a2")], vec![block("b1")]],
            end: ParallelEnd::Merge,
        },
        ChainStage::Single(io_out("out")),
    ]
}

/// in → split → [a1 → out_a] ∥ [out_b]
fn split_y() -> Vec<ChainStage> {
    vec![
        ChainStage::Single(io_in("in")),
        ChainStage::Parallel {
            lanes: vec![vec![block("a1"), io_out("out_a")], vec![io_out("out_b")]],
            end: ParallelEnd::Fan,
        },
    ]
}

#[test]
fn every_wire_gets_exactly_one_anchor() {
    for stages in [split_mix(), split_y()] {
        let (nodes, edges) = linear_chain_layout(&stages, GridMetrics::default());
        let anchors = insert_anchors(&stages, &nodes);
        let mut wires: Vec<(String, String)> = edges
            .iter()
            .map(|e| (e.from_id.clone(), e.to_id.clone()))
            .collect();
        let mut anchored: Vec<(String, String)> = anchors
            .iter()
            .map(|a| (a.from_id.clone(), a.to_id.clone()))
            .collect();
        wires.sort();
        anchored.sort();
        assert_eq!(
            anchored, wires,
            "anchors drifted from the wires the chain builder draws"
        );
        let mut ids: Vec<&str> = anchors.iter().map(|a| a.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), anchors.len(), "anchor ids must be unique");
    }
}

#[test]
fn a_linear_chain_anchors_before_each_stage() {
    let stages = [
        ChainStage::Single(io_in("in")),
        ChainStage::Single(block("od")),
        ChainStage::Single(block("amp")),
        ChainStage::Single(io_out("out")),
    ];
    assert_eq!(
        slots(&anchors_of(&stages)),
        [
            ("in", "od", AnchorSlot::Stage { index: 1 }),
            ("od", "amp", AnchorSlot::Stage { index: 2 }),
            ("amp", "out", AnchorSlot::Stage { index: 3 }),
        ]
    );
}

#[test]
fn merge_lanes_get_a_slot_per_gap_and_one_at_each_lane_end() {
    let lane = |lane, index| AnchorSlot::Lane {
        stage: 1,
        lane,
        index,
    };
    assert_eq!(
        slots(&anchors_of(&split_mix())),
        [
            ("in", "__split_1", AnchorSlot::Stage { index: 1 }),
            ("__split_1", "a1", lane(0, 0)),
            ("a1", "a2", lane(0, 1)),
            ("a2", "__merge_1", lane(0, 2)),
            ("__split_1", "b1", lane(1, 0)),
            ("b1", "__merge_1", lane(1, 1)),
            ("__merge_1", "out", AnchorSlot::Stage { index: 2 }),
        ]
    );
}

#[test]
fn a_fan_lane_end_anchor_sits_before_its_terminal() {
    let lane = |lane, index| AnchorSlot::Lane {
        stage: 1,
        lane,
        index,
    };
    assert_eq!(
        slots(&anchors_of(&split_y())),
        [
            ("in", "__split_1", AnchorSlot::Stage { index: 1 }),
            ("__split_1", "a1", lane(0, 0)),
            ("a1", "out_a", lane(0, 1)),
            ("__split_1", "out_b", lane(1, 0)),
        ],
        "nothing is anchored after a terminal"
    );
}

#[test]
fn an_anchor_sits_on_its_wire_midpoint() {
    let anchors = anchors_of(&split_mix());
    let split_to_b1 = anchors
        .iter()
        .find(|a| a.from_id == "__split_1" && a.to_id == "b1")
        .expect("split → b1 anchor");
    // Default grid: split at (240, 200), b1 at (400, 260).
    assert_eq!((split_to_b1.x, split_to_b1.y), (320.0, 230.0));
}

#[test]
fn anchor_ids_name_their_slot() {
    assert_eq!(AnchorSlot::Stage { index: 3 }.anchor_id(), "stage:3");
    assert_eq!(
        AnchorSlot::Lane {
            stage: 1,
            lane: 0,
            index: 2
        }
        .anchor_id(),
        "lane:1:0:2"
    );
}

#[test]
fn an_empty_segment_keeps_its_plus_visible() {
    let empty_chain = [
        ChainStage::Single(io_in("in")),
        ChainStage::Single(io_out("out")),
    ];
    assert!(
        anchors_of(&empty_chain).iter().all(|a| a.always_visible),
        "an empty chain shows its +"
    );

    let empty_lane = [
        ChainStage::Single(io_in("in")),
        ChainStage::Parallel {
            lanes: vec![vec![block("a1")], vec![]],
            end: ParallelEnd::Merge,
        },
        ChainStage::Single(io_out("out")),
    ];
    let visible: Vec<AnchorSlot> = anchors_of(&empty_lane)
        .iter()
        .filter(|a| a.always_visible)
        .map(|a| a.slot)
        .collect();
    let lane = |lane, index| AnchorSlot::Lane {
        stage: 1,
        lane,
        index,
    };
    assert!(
        visible.contains(&lane(1, 0)),
        "the empty lane B shows its +: {visible:?}"
    );
    assert!(
        !visible.contains(&lane(0, 0)),
        "a wire into a block hides its + until hover: {visible:?}"
    );

    assert!(
        anchors_of(&split_y())
            .iter()
            .any(|a| a.slot == lane(1, 0) && a.always_visible),
        "an empty Y path (only its output) shows its +"
    );
}
```

- [ ] **Step 3: Run. Expect FAIL.**

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --lib graph_view_model::anchor_tests`

Expected, among others:
```
---- graph_view_model::anchor_tests::a_linear_chain_anchors_before_each_stage stdout ----
assertion `left == right` failed
  left: []
 right: [("in", "od", Stage { index: 1 }), ("od", "amp", Stage { index: 2 }), ("amp", "out", Stage { index: 3 })]
```

`anchor_ids_name_their_slot` passes: it pins the id format, which is part of the surface.

- [ ] **Step 4: Give the routing ids a single source.** This is a behaviour-preserving change: `chain_builder` and the anchors must agree on the `__split_N`/`__merge_N` names. Create `crates/adapter-gui/src/graph_view_model/routing_ids.rs`:

```rust
//! Responsibility: names the routing nodes a parallel stage inserts
//!
//! The chain builder emits them and the anchor placer finds them again by
//! the same name, so the format lives once, here.

/// Id of the split node the `n`-th parallel stage (1-based) starts with.
pub fn split_node_id(n: usize) -> String {
    format!("__split_{n}")
}

/// Id of the merge node the `n`-th merging parallel stage (1-based) ends with.
pub fn merge_node_id(n: usize) -> String {
    format!("__merge_{n}")
}
```

In `chain_builder.rs`, add `use super::routing_ids::{merge_node_id, split_node_id};`. Replace `let split_id = format!("__split_{split_counter}");` with `let split_id = split_node_id(split_counter);`, and `let merge_id = format!("__merge_{split_counter}");` with `let merge_id = merge_node_id(split_counter);`. In `mod.rs`, add `mod routing_ids;`.

- [ ] **Step 5: Implement `insert_anchors`.** In `anchors.rs`, change the imports and replace the stub:

```rust
use super::routing_ids::{merge_node_id, split_node_id};
use super::types::{ChainStage, GraphNode, NodeKind, ParallelEnd};
```

```rust
/// Anchors for the graph `linear_chain_layout(stages, ..)` returned as
/// `nodes`. The walk mirrors the chain builder's; `every_wire_gets_exactly_
/// one_anchor` pins that they agree. A wire whose ends are missing from
/// `nodes` gets no anchor (panic-free).
pub fn insert_anchors(stages: &[ChainStage], nodes: &[GraphNode]) -> Vec<GraphAnchor> {
    let mut anchors = Vec::new();
    let mut prev_tail: Option<String> = None;
    let mut split_counter: usize = 0;

    for (index, stage) in stages.iter().enumerate() {
        match stage {
            ChainStage::Single(block) => {
                if let Some(prev) = prev_tail.take() {
                    let slot = AnchorSlot::Stage { index };
                    push_anchor(&mut anchors, nodes, &prev, &block.id, slot);
                }
                prev_tail = Some(block.id.clone());
            }
            ChainStage::Parallel { lanes, .. } if lanes.is_empty() => {}
            ChainStage::Parallel { lanes, end } => {
                split_counter += 1;
                let split_id = split_node_id(split_counter);
                let merge_id = merge_node_id(split_counter);
                if let Some(prev) = prev_tail.take() {
                    let slot = AnchorSlot::Stage { index };
                    push_anchor(&mut anchors, nodes, &prev, &split_id, slot);
                }
                for (lane, blueprints) in lanes.iter().enumerate() {
                    let mut from = split_id.clone();
                    for (at, block) in blueprints.iter().enumerate() {
                        let slot = AnchorSlot::Lane {
                            stage: index,
                            lane,
                            index: at,
                        };
                        push_anchor(&mut anchors, nodes, &from, &block.id, slot);
                        from = block.id.clone();
                    }
                    if *end == ParallelEnd::Merge {
                        let slot = AnchorSlot::Lane {
                            stage: index,
                            lane,
                            index: blueprints.len(),
                        };
                        push_anchor(&mut anchors, nodes, &from, &merge_id, slot);
                    }
                }
                prev_tail = match end {
                    ParallelEnd::Merge => Some(merge_id),
                    ParallelEnd::Fan => None,
                };
            }
        }
    }

    anchors
}

fn push_anchor(
    anchors: &mut Vec<GraphAnchor>,
    nodes: &[GraphNode],
    from: &str,
    to: &str,
    slot: AnchorSlot,
) {
    let find = |id: &str| nodes.iter().find(|n| n.id == id);
    let (Some(a), Some(b)) = (find(from), find(to)) else {
        return;
    };
    anchors.push(GraphAnchor {
        id: slot.anchor_id(),
        from_id: from.to_string(),
        to_id: to.to_string(),
        slot,
        x: (a.x + b.x) / 2.0,
        y: (a.y + b.y) / 2.0,
        always_visible: a.kind != NodeKind::Block && b.kind != NodeKind::Block,
    });
}
```

Then update the module doc of `mod.rs` (lines 6-11) to list:

```rust
//! - [`types`] — what a graph IS
//! - [`palette`] — what colour a category gets
//! - [`chain_builder`] — building a positioned graph from chain stages
//! - [`routing_ids`] — the names of the split and merge routing nodes
//! - [`layout`] — placing existing nodes by edge topology
//! - [`validation`] — what makes a graph description ill-formed
//! - [`reorder`] — moving a dragged node among its column siblings
//! - [`anchors`] — the insert anchors on a laid-out chain's wires
```

- [ ] **Step 6: Run. Expect PASS.**

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --lib graph_view_model`

Expected: `test result: ok. 48 passed`

- [ ] **Step 7: Docs.** In `docs/gui/graph-view.md`, after the `validate_stages` row, add:

```markdown
| `insert_anchors(stages, nodes)` | one `GraphAnchor` per wire of `linear_chain_layout`'s output, at the wire midpoint: `id` (`stage:{i}` / `lane:{stage}:{lane}:{i}`), the `AnchorSlot` a block added or dropped there lands in (index in the ORIGINAL list, "insert before"), and `always_visible` for an empty segment (no block at either end) |
```

- [ ] **Step 8: Pre-push gate.** Run the same gate commands as Task 1, Step 11.

- [ ] **Step 9: Commit and push.**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328
git -C $S add crates/adapter-gui/src/graph_view_model/routing_ids.rs \
  crates/adapter-gui/src/graph_view_model/anchors.rs \
  crates/adapter-gui/src/graph_view_model/chain_builder.rs \
  crates/adapter-gui/src/graph_view_model/mod.rs \
  crates/adapter-gui/src/graph_view_model_anchor_tests.rs docs/gui/graph-view.md
git -C $S commit -m "feat(#328): one insert anchor per graph wire, carrying the slot a block lands in"
git -C $S fetch && git -C $S push
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 5 task 5 pushed: $(git -C $S rev-parse --short HEAD). insert_anchors + AnchorSlot + GraphAnchor, routing ids single-sourced; graph_view_model 48 tests green; workspace gate green."
```

---

## Task 6: Drop-anchor resolution (pure Rust)

**Files:**
- Create: `crates/adapter-gui/src/graph_view_model/drop_target.rs`
- Modify: `crates/adapter-gui/src/graph_view_model/mod.rs`
- Test: `crates/adapter-gui/src/graph_view_model_drop_tests.rs` (create)
- Docs: `docs/gui/graph-view.md` (Rust helpers table)

**Interfaces:**
- Consumes: `GraphAnchor`, `GraphNode.kind`, `GridMetrics`.
- Produces: `pub fn resolve_drop_anchor<'a>(nodes: &[GraphNode], anchors: &'a [GraphAnchor], dragged_id: &str, x: f32, y: f32, metrics: GridMetrics) -> Option<&'a GraphAnchor>`. This is what the Slint `pure callback resolve-drop-anchor` is wired to (Task 10, Part 6).

- [ ] **Step 1: Add the minimum surface.** Create `crates/adapter-gui/src/graph_view_model/drop_target.rs`:

```rust
//! Responsibility: resolves which insert anchor a dragged block lands on
//!
//! The canvas asks through a pure callback (the #787 `slot-at` pattern) on
//! every drag move, to light the target up, and emits `node-dropped` with
//! the answer on release — so the geometry lives here, once. Mapping the
//! anchor's slot onto a command belongs to the host.

use super::anchors::GraphAnchor;
use super::types::{GraphNode, GridMetrics};

/// The anchor a block `dragged_id` released at layout `(x, y)` lands on.
pub fn resolve_drop_anchor<'a>(
    _nodes: &[GraphNode],
    _anchors: &'a [GraphAnchor],
    _dragged_id: &str,
    _x: f32,
    _y: f32,
    _metrics: GridMetrics,
) -> Option<&'a GraphAnchor> {
    None
}
```

In `mod.rs`, add `mod drop_target;`, `pub use drop_target::resolve_drop_anchor;`, the doc line `//! - [`drop_target`] — which anchor a dragged block lands on`, and:

```rust
#[cfg(test)]
#[path = "../graph_view_model_drop_tests.rs"]
mod drop_tests;
```

- [ ] **Step 2: Write the failing tests.** Create `crates/adapter-gui/src/graph_view_model_drop_tests.rs`:

```rust
//! Tests for `graph_view_model::drop_target` (#328 §5.1): which insert
//! anchor a dragged block lands on.

use super::{
    insert_anchors, linear_chain_layout, resolve_drop_anchor, AnchorSlot, BlockBlueprint,
    ChainStage, GridMetrics, NodeCategory, NodeKind, ParallelEnd,
};

/// in → split → [a1] ∥ [b1] → mixer → out on the default grid:
/// in (80,200) · split (240,200) · a1 (400,140) · b1 (400,260) ·
/// mixer (560,200) · out (720,200). Wire midpoints: in→split (160,200),
/// split→a1 (320,170), split→b1 (320,230), a1→mixer (480,170),
/// b1→mixer (480,230), mixer→out (640,200).
fn resolve(dragged: &str, x: f32, y: f32) -> Option<AnchorSlot> {
    let block = |id: &str| BlockBlueprint::new(id, id.to_uppercase(), NodeCategory::Amp);
    let stages = [
        ChainStage::Single(
            BlockBlueprint::new("in", "In 1", NodeCategory::Input).with_kind(NodeKind::IoInput),
        ),
        ChainStage::Parallel {
            lanes: vec![vec![block("a1")], vec![block("b1")]],
            end: ParallelEnd::Merge,
        },
        ChainStage::Single(
            BlockBlueprint::new("out", "Out 1", NodeCategory::Output)
                .with_kind(NodeKind::IoOutput),
        ),
    ];
    let (nodes, _) = linear_chain_layout(&stages, GridMetrics::default());
    let anchors = insert_anchors(&stages, &nodes);
    resolve_drop_anchor(&nodes, &anchors, dragged, x, y, GridMetrics::default()).map(|a| a.slot)
}

fn lane(lane: usize, index: usize) -> AnchorSlot {
    AnchorSlot::Lane {
        stage: 1,
        lane,
        index,
    }
}

#[test]
fn a_drop_on_an_anchor_resolves_to_it() {
    assert_eq!(resolve("a1", 160.0, 200.0), Some(AnchorSlot::Stage { index: 1 }));
}

#[test]
fn a_drop_into_the_other_lane_resolves_to_that_lanes_slot() {
    // a1 (path A) dropped near the wire split → b1: first in path B.
    assert_eq!(resolve("a1", 322.0, 226.0), Some(lane(1, 0)));
}

#[test]
fn a_drop_on_the_blocks_own_wire_resolves_to_nothing() {
    // a1 → mixer is a1's own wire: landing there leaves the chain
    // unchanged, even though path B's end anchor (480, 230) is only 60px
    // away and must NOT catch the drop.
    assert_eq!(resolve("a1", 480.0, 170.0), None);
    assert_eq!(resolve("a1", 320.0, 170.0), None);
}

#[test]
fn only_a_block_can_be_dropped() {
    assert_eq!(resolve("__split_1", 480.0, 230.0), None, "the split node does not move");
    assert_eq!(resolve("in", 480.0, 230.0), None, "an I/O node does not move");
}

#[test]
fn a_drop_far_from_every_anchor_resolves_to_nothing() {
    assert_eq!(resolve("a1", 400.0, 400.0), None);
}

#[test]
fn an_unknown_block_resolves_to_nothing() {
    assert_eq!(resolve("ghost", 160.0, 200.0), None);
}
```

- [ ] **Step 3: Run. Expect FAIL.**

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --lib graph_view_model::drop_tests`

Expected:
```
---- graph_view_model::drop_tests::a_drop_on_an_anchor_resolves_to_it stdout ----
assertion `left == right` failed
  left: None
 right: Some(Stage { index: 1 })
---- graph_view_model::drop_tests::a_drop_into_the_other_lane_resolves_to_that_lanes_slot stdout ----
  left: None
 right: Some(Lane { stage: 1, lane: 1, index: 0 })
```

The four `None` tests pass: they are guards for the no-op cases.

- [ ] **Step 4: Implement.** In `drop_target.rs`, replace everything below the module doc comment (the two `use` lines and the stub) with:

```rust
use super::anchors::GraphAnchor;
use super::types::{GraphNode, GridMetrics, NodeKind};

/// The anchor a block `dragged_id` released at layout `(x, y)` lands on:
/// the nearest anchor closer than half a column.
///
/// `None` when the nearest anchor sits on the block's own wire (landing
/// there leaves the chain unchanged — it must not fall through to the next
/// nearest, which can be the other lane's), when the dragged node is not a
/// [`NodeKind::Block`] (I/O, split and mixer nodes do not move), when it is
/// unknown, or when no anchor is in reach.
pub fn resolve_drop_anchor<'a>(
    nodes: &[GraphNode],
    anchors: &'a [GraphAnchor],
    dragged_id: &str,
    x: f32,
    y: f32,
    metrics: GridMetrics,
) -> Option<&'a GraphAnchor> {
    let dragged = nodes.iter().find(|n| n.id == dragged_id)?;
    if dragged.kind != NodeKind::Block {
        return None;
    }
    let reach = metrics.column_spacing / 2.0;
    let (nearest, _) = anchors
        .iter()
        .map(|a| (a, (a.x - x).powi(2) + (a.y - y).powi(2)))
        .filter(|(_, distance_sq)| *distance_sq < reach * reach)
        .min_by(|(_, l), (_, r)| l.total_cmp(r))?;
    if nearest.from_id == dragged_id || nearest.to_id == dragged_id {
        return None;
    }
    Some(nearest)
}
```

- [ ] **Step 5: Run. Expect PASS.**

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --lib graph_view_model`

Expected: `test result: ok. 54 passed`

- [ ] **Step 6: Docs.** In `docs/gui/graph-view.md`, after the `insert_anchors` row, add:

```markdown
| `resolve_drop_anchor(nodes, anchors, dragged_id, x, y, metrics)` | the anchor a block dragged to layout `(x, y)` lands on: the nearest one within half a column. `None` when that nearest one is on the block's own wire (no move), when the dragged node is not a `Block`, or when nothing is in reach. The canvas asks through its `resolve-drop-anchor` pure callback |
```

- [ ] **Step 7: Pre-push gate.** Run the same gate commands as Task 1, Step 11.

- [ ] **Step 8: Commit and push.**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328
git -C $S add crates/adapter-gui/src/graph_view_model/drop_target.rs \
  crates/adapter-gui/src/graph_view_model/mod.rs \
  crates/adapter-gui/src/graph_view_model_drop_tests.rs docs/gui/graph-view.md
git -C $S commit -m "feat(#328): resolve the insert anchor a dragged block lands on; its own wire is a no-op"
git -C $S fetch && git -C $S push
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 5 task 6 pushed: $(git -C $S rev-parse --short HEAD). resolve_drop_anchor (nearest within half a column, own wire = none, blocks only); graph_view_model 54 tests green; workspace gate green."
```

---

## Task 7: `BlockTileStyle` — one source for a block tile's state colours

**Files:**
- Create: `crates/adapter-gui/ui/components/block_tile_style.slint`
- Modify: `crates/adapter-gui/ui/components/block_chip.slint`, lines 6-9 (import), 27-32, 55-71, 85 and 117
- Test: `crates/adapter-gui/tests/issue_328_graph_view_sources.rs`

**Interfaces:**
- Produces: `export global BlockTileStyle` with:
  - `out property <color> unavailable-tint`, `disabled-tint`, `dragging-tint`
  - `out property <float> unavailable-opacity`
  - `public pure function label-color(unavailable: bool, enabled: bool, accent: color) -> color`
  - `art-color(unavailable: bool, dragging: bool, enabled: bool, accent: color) -> color`
  - `marker-background(visible: bool, selected: bool) -> color`
  - `marker-border-width(visible: bool, selected: bool, neighbor: bool) -> length`
  - `marker-border-color(visible: bool, selected: bool, neighbor: bool, enabled: bool) -> color`

- [ ] **Step 1: Invoke the UI skills.** Invoke `claude-plugin:ux-ui`, `slint:slint` and `slint-best-practices`.

- [ ] **Step 2: Write the failing test.** Append to `issue_328_graph_view_sources.rs`:

```rust
/// Spec §5.1: the graph block card keeps parity with BlockChip. The state
/// colours (unavailable amber, disabled grey, drag grey, MIDI markers) live
/// in ONE global both tiles read.
#[test]
fn block_chip_paints_its_states_from_block_tile_style() {
    let chip = read_component("block_chip.slint");
    assert!(
        chip.contains("BlockTileStyle."),
        "block_chip.slint must read its state colours from BlockTileStyle \
         (#328: one source for BlockChip and the graph block card)"
    );
    for literal in ["#b07a3c", "#5c6678", "#8b95a5", "#f0a020"] {
        assert!(
            !chip.contains(literal),
            "block_chip.slint still hard-codes {literal}; it belongs in block_tile_style.slint"
        );
    }
}
```

- [ ] **Step 3: Run. Expect FAIL.**

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_sources`

Expected: `block_chip.slint must read its state colours from BlockTileStyle (#328: one source for BlockChip and the graph block card)`

- [ ] **Step 4: Create the global.** Create `crates/adapter-gui/ui/components/block_tile_style.slint`. The values are moved verbatim from `block_chip.slint`.

```slint
// Responsibility: holds the colours a block tile paints its states with.
// Single source of truth for the chain row's BlockChip and the graph view's
// block card (#328 §5.1 parity): a bypassed, unavailable, dragged or
// MIDI-marked block must read the same on both.

export global BlockTileStyle {
    // #606: a block whose model is not installed — amber, distinct from the
    // grey of a block the user switched off — and dimmed as a whole.
    out property <color> unavailable-tint: #b07a3c;
    out property <float> unavailable-opacity: 0.5;
    out property <color> disabled-tint: #5c6678;
    out property <color> dragging-tint: #8b95a5;

    // Type-label colour.
    public pure function label-color(unavailable: bool, enabled: bool, accent: color) -> color {
        return unavailable ? self.unavailable-tint : enabled ? accent : self.disabled-tint;
    }

    // Icon tint.
    public pure function art-color(unavailable: bool, dragging: bool, enabled: bool, accent: color) -> color {
        return unavailable ? self.unavailable-tint
            : dragging ? self.dragging-tint
            : enabled ? accent
            : self.disabled-tint;
    }

    // #591: the active block (strong) and the block the footswitch's
    // neighbor toggle would act on (light) are outlined only while a MIDI
    // command is recent; dimmer when the block is off.
    public pure function marker-background(visible: bool, selected: bool) -> color {
        return (visible && selected) ? #f0a02014 : transparent;
    }

    public pure function marker-border-width(visible: bool, selected: bool, neighbor: bool) -> length {
        return visible ? (selected ? 2px : neighbor ? 1px : 0px) : 0px;
    }

    public pure function marker-border-color(visible: bool, selected: bool, neighbor: bool, enabled: bool) -> color {
        return visible
            ? (selected
                ? (enabled ? #f0a020 : #f0a02066)
                : neighbor
                    ? (enabled ? #f0a0207a : #f0a02040)
                    : transparent)
            : transparent;
    }
}
```

- [ ] **Step 5: Make `BlockChip` read from the global.** This preserves behaviour: the same values move from one place to another. In `block_chip.slint`:

- After line 9 (`import { PowerSwitch } …`), add `import { BlockTileStyle } from "./block_tile_style.slint";`
- Delete line 32 (`property <color> unavailable-tint: #b07a3c;`). Lines 27-31 (the #606 comment and `property <bool> unavailable`) stay.
- Replace lines 55-71:

```slint
    // #591: the active-block (strong) and neighbor (light) markers only
    // show while a MIDI command is recent (`markers-visible`), then fade
    // out after 10s. Lit when the block is enabled, dimmer when disabled.
    background: (root.markers-visible && root.selected) ? #f0a02014 : transparent;
    border-width: root.markers-visible
        ? (root.selected ? 2px : root.neighbor ? 1px : 0px)
        : 0px;
    border-color: root.markers-visible
        ? (root.selected
            ? (root.block.enabled ? #f0a020 : #f0a02066)
            : root.neighbor
                ? (root.block.enabled ? #f0a0207a : #f0a02040)
                : transparent)
        : transparent;
    border-radius: 6px;
    // #606: dim the whole tile when its model is unavailable.
    opacity: root.unavailable ? 0.5 : 1.0;
```

with:

```slint
    // #591: the active-block (strong) and neighbor (light) markers only
    // show while a MIDI command is recent (`markers-visible`), then fade
    // out after 10s. Colours live in BlockTileStyle (#328), shared with the
    // graph view's block card.
    background: BlockTileStyle.marker-background(root.markers-visible, root.selected);
    border-width: BlockTileStyle.marker-border-width(root.markers-visible, root.selected, root.neighbor);
    border-color: BlockTileStyle.marker-border-color(root.markers-visible, root.selected, root.neighbor, root.block.enabled);
    border-radius: 6px;
    // #606: dim the whole tile when its model is unavailable.
    opacity: root.unavailable ? BlockTileStyle.unavailable-opacity : 1.0;
```

- Replace line 85 (`color: root.unavailable ? root.unavailable-tint : root.block.enabled ? root.accent-color : #5c6678;`) with `color: BlockTileStyle.label-color(root.unavailable, root.block.enabled, root.accent-color);`
- Replace line 117 (`tint: root.unavailable ? root.unavailable-tint : root.dragging ? #8b95a5 : root.block.enabled ? root.accent-color : #5c6678;`) with `tint: BlockTileStyle.art-color(root.unavailable, root.dragging, root.block.enabled, root.accent-color);`

- [ ] **Step 6: Run the new test and the chain-row tests.** Expect PASS. The chain-row tests are unchanged and prove the refactor preserves behaviour.

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_sources --test chain_row_height_grows_with_streams --test chain_row_meters_stack_upward --test chain_row_per_stream_full_row --test chain_row_stacks_stream_meters`

Expected: every binary reports `test result: ok`.

- [ ] **Step 7: Pre-push gate.** Run the same gate commands as Task 1, Step 11.

- [ ] **Step 8: Commit and push.**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328
git -C $S add crates/adapter-gui/ui/components/block_tile_style.slint \
  crates/adapter-gui/ui/components/block_chip.slint \
  crates/adapter-gui/tests/issue_328_graph_view_sources.rs
git -C $S commit -m "feat(#328): block tile state colours move to BlockTileStyle for BlockChip parity"
git -C $S fetch && git -C $S push
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 5 task 7 pushed: $(git -C $S rev-parse --short HEAD). BlockTileStyle global; BlockChip reads it (values moved verbatim); chain_row_* tests green unchanged; workspace gate green."
```

---

## Task 8: Kind-driven cards — every node is a clickable card, with `@tr` labels

**Files:**
- Modify: `crates/adapter-gui/ui/components/graph_view_types.slint`. `GraphNode` gains `kind`, `neighbor` and `block`, plus an import of `ChainBlockItem`.
- Modify: `crates/adapter-gui/ui/components/graph_view.slint` (whole file)
- Modify: `crates/adapter-gui/ui/components/graph_node_card.slint` (whole file)
- Create: `crates/adapter-gui/ui/assets/graph-split.svg` and `crates/adapter-gui/ui/assets/graph-mix.svg` — the strokes of today's `splitter.svg` / `mixer.svg` without their baked-in `<text>` ("SPLITTER" / "MIXER" in Arial: untranslated, about 5px on a card, and a font the Orange Pi may not have) and without their dark backing square, so the card can `colorize` them
- Modify: `crates/adapter-gui/translations/adapter-gui.pot` and the 9 `.po` files
- Modify: `crates/adapter-gui/src/graph_view_model_tests.rs`, the comment above `split_and_merge_use_routing_node_convention` only (originally lines 337-342; the Task 3 and Task 4 edits move it down, so find it by its text)
- Test: `crates/adapter-gui/tests/issue_328_graph_view_interaction.rs`, `crates/adapter-gui/tests/issue_328_graph_view_sources.rs`
- Docs: `docs/gui/graph-view.md:54-75,87,120-127`

**Interfaces:**
- Consumes: `NodeKind::as_str` slugs (Task 4), and `ChainBlockItem` from `ui/models.slint:46-77`.
- Produces:
  - Slint `GraphNode { …, kind: string, neighbor: bool, block: ChainBlockItem }`. In Rust these are `kind: SharedString`, `neighbor: bool` and `block: adapter_gui::ChainBlockItem`.
  - The `GraphView` node TouchArea id `GraphView::node-ta`.
  - `GraphView.node_width` and `node_height` default to `100px`.
  - The accessible label `@tr("accessible-graph-view")`.

- [ ] **Step 1: Invoke the UI skills.** Invoke `claude-plugin:ux-ui`, `slint:slint` and `slint-best-practices`.

- [ ] **Step 2: Add the minimum surface.** Add the fields and the id; rendering does not change yet. In `graph_view_types.slint`, add `import { ChainBlockItem } from "../models.slint";` below the header comment. Then, inside `GraphNode` after `selected: bool,`, add:

```slint
    // #328 §5.2: what the node IS — "block" | "io_input" | "io_output" |
    // "split" | "mixer" (graph_view_model::NodeKind::as_str). The kind picks
    // the card face; it replaced the `label == "" && category == "util"`
    // routing-dot convention.
    kind: string,
    // #591 parity with BlockChip: the block the footswitch's neighbor toggle
    // would act on (outlined lightly while MIDI markers show).
    neighbor: bool,
    // #328 parity with BlockChip: the block's own row item — type label,
    // icon or thumbnail, accent, unavailable flag and the tooltip data. The
    // host passes the same ChainBlockItem the chain row renders; empty for
    // the other kinds. The LED follows `bypass`, never `block.enabled`.
    block: ChainBlockItem,
```

In `graph_view.slint`, change the node loop's `TouchArea {` (the one inside `for node[idx] in root.nodes : Rectangle`, after `GraphNodeCard {…}`) to `node-ta := TouchArea {`.

- [ ] **Step 3: Write the failing tests.** Append to `issue_328_graph_view_interaction.rs`:

```rust
fn typed(id: &str, kind: &str, label: &str, x: f32, y: f32) -> GraphNode {
    let routing = kind == "split" || kind == "mixer";
    GraphNode {
        kind: kind.into(),
        category: if routing { "util" } else { "drive" }.into(),
        ..node(id, label, x, y)
    }
}

/// in → split → amp → mixer → out: one card per kind, the way
/// graph_view_model emits them (routing nodes: empty label, "util").
fn one_of_each_kind() -> Vec<GraphNode> {
    vec![
        typed("in", "io_input", "In 1", 80.0, 200.0),
        typed("__split_1", "split", "", 240.0, 200.0),
        typed("amp", "block", "Amp", 400.0, 200.0),
        typed("__merge_1", "mixer", "", 560.0, 200.0),
        typed("out", "io_output", "Out 1", 720.0, 200.0),
    ]
}

#[test]
fn every_node_kind_is_a_clickable_card() {
    let nodes = one_of_each_kind();
    let w = harness(nodes.clone());
    let clicked = recorder::<String>();
    let c = clicked.clone();
    w.on_node_clicked(move |id| c.borrow_mut().push(id.to_string()));

    for n in &nodes {
        click_at(&w, at(n.layout_x, n.layout_y));
    }

    assert_eq!(
        *clicked.borrow(),
        ["in", "__split_1", "amp", "__merge_1", "out"],
        "the split and the mixer must be clickable cards, not 8px routing dots (#328 §5.2)"
    );
}
```

Append to `issue_328_graph_view_sources.rs`:

```rust
/// Spec §5.2: the kind drives the card, not `label == ""`.
#[test]
fn the_graph_view_no_longer_picks_routing_dots_by_an_empty_label() {
    for file in ["graph_view.slint", "graph_node_card.slint"] {
        assert!(
            !read_component(file).contains("label == \"\""),
            "{file} still derives a node's face from an empty label; the face comes \
             from `node.kind` (#328 §5.2)"
        );
    }
}

/// Spec §5.2: the accessible label uses @tr.
#[test]
fn the_graph_views_accessible_labels_go_through_tr() {
    for file in ["graph_view.slint", "graph_node_card.slint"] {
        assert!(
            !read_component(file).contains("accessible-label: \""),
            "{file} has a raw accessible-label literal; use @tr (#328 §5.2)"
        );
    }
}

/// Spec §5.2: the split and mixer cards draw routing artwork with no text
/// baked into the SVG. The card prints the translated name; SVG text would
/// stay English, and it needs a font the Orange Pi may not have.
#[test]
fn the_routing_cards_draw_artwork_without_baked_in_text() {
    let card = read_component("graph_node_card.slint");
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/assets");
    for name in ["graph-split.svg", "graph-mix.svg"] {
        assert!(
            card.contains(name),
            "graph_node_card.slint must draw the split and mixer art from {name}"
        );
        let svg = std::fs::read_to_string(assets.join(name))
            .unwrap_or_else(|e| panic!("read {name}: {e}"));
        assert!(
            !svg.contains("<text"),
            "{name} bakes text into the artwork; the card prints the translated name"
        );
    }
}
```

- [ ] **Step 4: Run. Expect FAIL.**

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_interaction --test issue_328_graph_view_sources`

Expected:
```
---- every_node_kind_is_a_clickable_card stdout ----
assertion `left == right` failed: the split and the mixer must be clickable cards, not 8px routing dots (#328 §5.2)
  left: ["in", "amp", "out"]
 right: ["in", "__split_1", "amp", "__merge_1", "out"]
---- the_graph_view_no_longer_picks_routing_dots_by_an_empty_label stdout ----
graph_view.slint still derives a node's face from an empty label; the face comes from `node.kind` (#328 §5.2)
---- the_graph_views_accessible_labels_go_through_tr stdout ----
graph_view.slint has a raw accessible-label literal; use @tr (#328 §5.2)
---- the_routing_cards_draw_artwork_without_baked_in_text stdout ----
graph_node_card.slint must draw the split and mixer art from graph-split.svg
```

- [ ] **Step 5a: Create the text-free routing artwork.** The strokes are copied verbatim from `ui/assets/splitter.svg` and `ui/assets/mixer.svg`; the backing square and the `<text>` element are dropped, so the card's `colorize` paints the whole glyph.

Create `crates/adapter-gui/ui/assets/graph-split.svg`:

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256" fill="none">
<path d="M58 128H116" stroke="#E8E8E8" stroke-width="10" stroke-linecap="round"/>
<path d="M116 128L168 86" stroke="#E8E8E8" stroke-width="10" stroke-linecap="round"/>
<path d="M116 128L168 170" stroke="#E8E8E8" stroke-width="10" stroke-linecap="round"/>
<circle cx="180" cy="86" r="10" stroke="#E8E8E8" stroke-width="8"/>
<circle cx="180" cy="170" r="10" stroke="#E8E8E8" stroke-width="8"/>
</svg>
```

Create `crates/adapter-gui/ui/assets/graph-mix.svg`:

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256" fill="none">
<path d="M78 172V96M110 172V76M142 172V116M174 172V88" stroke="#E8E8E8" stroke-width="10" stroke-linecap="round"/>
<rect x="68" y="124" width="20" height="12" rx="6" fill="#E8E8E8"/>
<rect x="100" y="102" width="20" height="12" rx="6" fill="#E8E8E8"/>
<rect x="132" y="142" width="20" height="12" rx="6" fill="#E8E8E8"/>
<rect x="164" y="112" width="20" height="12" rx="6" fill="#E8E8E8"/>
</svg>
```

These are every non-text shape of the two originals except the backing `<rect x="16" y="16" …>` (checked against both files). `splitter.svg` and `mixer.svg` stay untouched: `ui/assets/preview.html` shows them.

- [ ] **Step 5b: Implement the card.** Replace `crates/adapter-gui/ui/components/graph_node_card.slint` with:

```slint
// Responsibility: draws one node card of the graph view.
// Node card of the graph view (#435). Pure visual: click and drag belong
// to the canvas's node TouchArea. `node.kind` picks the face (#328 §5.2) —
// the old empty-label routing dot is gone, so the split and the mixer are
// real, labelled, clickable cards.

import { GraphNode } from "graph_view_types.slint";
import { EffectTypeIcon } from "effect_type_icon.slint";
import { Theme } from "../theme.slint";

export component GraphNodeCard inherits Rectangle {
    in property <GraphNode> node;
    in property <bool> hovered;
    in property <bool> dragging;
    in property <length> card_width;
    in property <length> card_height;

    property <bool> is-io: root.node.kind == "io_input" || root.node.kind == "io_output";
    property <bool> is-routing: root.node.kind == "split" || root.node.kind == "mixer";
    // The split and the mixer carry no host label (graph_view_model keeps
    // it empty); their name is translated here.
    property <string> face-label: root.node.kind == "split" ? @tr("graph-node-split")
        : root.node.kind == "mixer" ? @tr("graph-node-mixer")
        : root.node.label;

    width: root.card_width;
    height: root.card_height;
    border-radius: 8px;
    border-width: root.node.selected ? 2px : 1px;
    border-color: root.node.selected
        ? #f5f7fb
        : root.node.border;
    background: root.node.bypass
        ? root.node.fill.with-alpha(0.35)
        : root.dragging
        ? root.node.fill.brighter(0.20)
        : root.hovered
        ? root.node.fill.brighter(0.10)
        : root.node.fill;
    opacity: root.node.bypass ? 0.7 : 1.0;
    drop-shadow-blur: root.dragging ? 12px : (root.hovered ? 6px : 2px);
    drop-shadow-color: #00000080;
    drop-shadow-offset-y: 2px;

    // I/O endpoint: its connector artwork above the endpoint names.
    if root.is-io : EffectTypeIcon {
        x: 0px;
        y: 8px;
        width: parent.width;
        height: max(0px, parent.height - 38px);
        icon-kind: root.node.kind == "io_input" ? "input" : "output";
    }

    // Split / mixer: the routing artwork above the translated name. The
    // SVGs carry no text of their own (graph-split.svg / graph-mix.svg).
    if root.is-routing : Image {
        x: (parent.width - self.width) / 2;
        y: 8px;
        width: max(0px, parent.height - 38px);
        height: max(0px, parent.height - 38px);
        source: root.node.kind == "split"
            ? @image-url("../assets/graph-split.svg")
            : @image-url("../assets/graph-mix.svg");
        image-fit: contain;
        colorize: #f5f7fb;
    }

    Text {
        x: 8px;
        y: root.is-io || root.is-routing ? parent.height - 30px : 0px;
        width: parent.width - 16px;
        height: root.is-io || root.is-routing ? 22px : parent.height;
        text: root.face-label;
        color: #f5f7fb;
        font-size: Theme.min-font;
        font-weight: 700;
        horizontal-alignment: center;
        vertical-alignment: center;
        overflow: elide;
    }

    accessible-role: button;
    accessible-label: root.face-label;
}
```

- [ ] **Step 6: Implement the canvas.** Replace `crates/adapter-gui/ui/components/graph_view.slint` with the file below. It is the old file with these changes: the header comment, the node size default, the `@tr` accessible label, the routing-dot loop removed, every node drawn as a card, and the `node-ta` id. Everything else is carried over verbatim, including the Task 2 wheel rule.

```slint
// Responsibility: renders a node canvas.
// GraphView — node-and-edge canvas with pan, zoom and per-node drag
// (#435), grown into the chain editor's canvas (#328). The host computes
// positions and node kinds; this component renders them.
//
// Coordinate system:
//
//   layout space   — coords on the input GraphNode (set by host)
//   viewport space — layout * zoom + pan (rendered position)
//
// All callbacks emit ids and coords in *layout* space.

import { GraphNode, GraphEdge, GraphEdgeGeometry, CategoryColor } from "graph_view_types.slint";
import { GraphNodeCard } from "graph_node_card.slint";
import { GraphWire } from "graph_wire.slint";

export { GraphNode, GraphEdge, GraphEdgeGeometry, CategoryColor }

// The actual GraphView. Hosts nodes + edges. Pan/zoom owned here.
export component GraphView inherits Rectangle {
    // ── Inputs ──────────────────────────────────────────────────────
    in property <[GraphNode]> nodes;
    in property <[GraphEdgeGeometry]> edges;

    // Viewport state. Two-way so the host can persist or programmatically
    // reset the view (e.g. "fit to content").
    in-out property <float> zoom: 1.0;
    in-out property <length> pan_x: 0px;
    in-out property <length> pan_y: 0px;

    // Visual constants — overridable by the host. 100×100 is the chain
    // row's BlockChip, the tile a block card keeps parity with (#328).
    in property <length> node_width: 100px;
    in property <length> node_height: 100px;
    in property <color> background_color: #11141a;
    in property <color> grid_color: #1a1f2a;
    in property <bool> show_grid: true;

    // When true the host computes positions via topological_layout and
    // re-tidies on drag end via reorder_for_drop. Documentational on the
    // Slint side — the algorithm is host-owned (project rule: no logic
    // in the UI layer).
    in property <bool> auto_layout: true;

    // Zoom limits (matches issue #435 acceptance criteria 0.3..3.0).
    in property <float> min_zoom: 0.3;
    in property <float> max_zoom: 3.0;

    // ── Outputs ─────────────────────────────────────────────────────
    callback node_clicked(string);
    callback node_double_clicked(string);
    // Emitted continuously while a node is being dragged. Coords are in
    // layout space — the host applies them back to its model so the
    // node sticks under the cursor on the next frame.
    callback node_dragged(string, length, length);
    // Fired once when the drag gesture ends (mouse up).
    callback node_drag_ended(string, length, length);
    callback viewport_changed(float, length, length);

    // ── Internal state ──────────────────────────────────────────────
    private property <string> hover_id: "";
    private property <string> drag_id: "";
    // Local mouse position (TouchArea space) captured at press. The drag
    // math is delta-based: new_layout = node.layout + (mouse - press)/zoom.
    // The node's own layout cancels out of that expression, so it tracks
    // the real cursor without feedback even though the TouchArea moves
    // with the node it drags.
    private property <length> press_x: 0px;
    private property <length> press_y: 0px;
    // Set once movement exceeds the threshold so mouse-up can tell a
    // click from a drag. Local delta collapses to ~0 once the node
    // catches the cursor, so the decision must latch on the first
    // significant move, not be re-derived at release.
    private property <bool> did_drag: false;
    // Click vs drag — threshold of 5px in viewport space (matches issue
    // #435 acceptance criteria).
    private property <length> click_threshold: 5px;
    // Background pan: anchor + initial pan captured on press.
    private property <length> pan_anchor_x: 0px;
    private property <length> pan_anchor_y: 0px;
    private property <length> pan_initial_x: 0px;
    private property <length> pan_initial_y: 0px;
    private property <bool> panning: false;

    background: root.background_color;
    clip: true;

    accessible-role: list;
    accessible-label: @tr("accessible-graph-view");

    // ── Pan / zoom on empty area ───────────────────────────────────
    // Declared FIRST so that node TouchAreas (later siblings) sit on
    // top of it for both rendering and event routing. The pan area is
    // the fallback — nodes intercept events that fall on them.
    pan_area := TouchArea {
        x: 0px;
        y: 0px;
        width: parent.width;
        height: parent.height;
        mouse-cursor: grab;
        pointer-event(event) => {
            if (event.kind == PointerEventKind.down) {
                root.panning = true;
                root.pan_anchor_x = self.mouse-x;
                root.pan_anchor_y = self.mouse-y;
                root.pan_initial_x = root.pan_x;
                root.pan_initial_y = root.pan_y;
            } else if (event.kind == PointerEventKind.up) {
                if (root.panning) {
                    root.panning = false;
                    root.viewport_changed(root.zoom, root.pan_x, root.pan_y);
                }
            }
        }
        moved => {
            if (root.panning) {
                root.pan_x = root.pan_initial_x + (self.mouse-x - root.pan_anchor_x);
                root.pan_y = root.pan_initial_y + (self.mouse-y - root.pan_anchor_y);
            }
        }
        scroll-event(event) => {
            // #328: the graph sits inside the chains list (spec §5.4), so a
            // plain wheel must reach the list and scroll it. Zoom needs the
            // platform shortcut modifier — Slint reports ⌘ on macOS and Ctrl
            // on Windows and Linux as `modifiers.control`.
            if (!event.modifiers.control) {
                return reject;
            }
            // Zoom around cursor.
            let old_zoom = root.zoom;
            let dz = event.delta-y > 0 ? 0.1 : -0.1;
            let target = old_zoom + dz;
            let clamped = target < root.min_zoom
                ? root.min_zoom
                : target > root.max_zoom ? root.max_zoom : target;
            if (clamped != old_zoom) {
                // Keep the layout-space point under the cursor stationary.
                let layout_x = (self.mouse-x - root.pan_x) / old_zoom;
                let layout_y = (self.mouse-y - root.pan_y) / old_zoom;
                root.zoom = clamped;
                root.pan_x = self.mouse-x - layout_x * clamped;
                root.pan_y = self.mouse-y - layout_y * clamped;
                root.viewport_changed(root.zoom, root.pan_x, root.pan_y);
            }
            accept
        }
    }

    // ── Optional grid backdrop ──────────────────────────────────────
    if root.show_grid : Rectangle {
        x: 0px;
        y: 0px;
        width: parent.width;
        height: parent.height;
        background: transparent;

        // Two crossing strips that hint at the grid origin. A real grid
        // (every N px) would need a custom Path with many MoveTo/LineTo
        // which costs more than its visual value at this stage — keep
        // the hint cheap until we know we need more.
        Rectangle {
            x: root.pan_x;
            y: 0px;
            width: 1px;
            height: parent.height;
            background: root.grid_color;
        }
        Rectangle {
            x: 0px;
            y: root.pan_y;
            width: parent.width;
            height: 1px;
            background: root.grid_color;
        }
    }

    // ── Edges (drawn under nodes) ──────────────────────────────────
    //
    // Each GraphWire fills the canvas with viewbox in pixel coords so
    // the absolute MoveTo/CubicTo lands at the right place. See the
    // note on GraphWire for why this matters.
    for edge in root.edges : GraphWire {
        canvas_width: root.width;
        canvas_height: root.height;
        from_x: root.pan_x + edge.from_x * root.zoom;
        from_y: root.pan_y + edge.from_y * root.zoom;
        to_x: root.pan_x + edge.to_x * root.zoom;
        to_y: root.pan_y + edge.to_y * root.zoom;
        stroke_thickness: 2px * root.zoom;
    }

    // ── Nodes ──────────────────────────────────────────────────────
    // Every node is a card, whatever its kind (#328 §5.2). The split and
    // the mixer used to be 8px routing dots picked by an empty label,
    // with a zero-size TouchArea — they could not be clicked. Position is
    // the card centre at (layout * zoom + pan), shifted by half the card
    // size so it lands geometrically centred.
    for node[idx] in root.nodes : Rectangle {
        property <length> w: root.node_width * root.zoom;
        property <length> h: root.node_height * root.zoom;

        x: root.pan_x + node.layout_x * root.zoom - self.w / 2;
        y: root.pan_y + node.layout_y * root.zoom - self.h / 2;
        width: self.w;
        height: self.h;

        GraphNodeCard {
            x: 0px;
            y: 0px;
            node: node;
            card_width: parent.width;
            card_height: parent.height;
            hovered: root.hover_id == node.id && root.drag_id == "";
            dragging: root.drag_id == node.id;
        }

        node-ta := TouchArea {
            x: 0px;
            y: 0px;
            width: parent.width;
            height: parent.height;
            mouse-cursor: pointer;
            pointer-event(event) => {
                if (event.kind == PointerEventKind.down) {
                    root.drag_id = node.id;
                    root.press_x = self.mouse-x;
                    root.press_y = self.mouse-y;
                    root.did_drag = false;
                } else if (event.kind == PointerEventKind.up) {
                    if (root.drag_id == node.id) {
                        if (root.did_drag) {
                            root.node_drag_ended(node.id, node.layout_x, node.layout_y);
                        } else {
                            root.node_clicked(node.id);
                        }
                    }
                    root.drag_id = "";
                }
            }
            moved => {
                if (self.pressed && root.drag_id == node.id) {
                    if (abs(self.mouse-x - root.press_x) > root.click_threshold
                     || abs(self.mouse-y - root.press_y) > root.click_threshold) {
                        root.did_drag = true;
                    }
                    // Delta-based: the node's current layout cancels out
                    // (see press_x note), so this tracks the cursor
                    // without drift even though the TouchArea moves with
                    // the node.
                    root.node_dragged(
                        node.id,
                        node.layout_x + (self.mouse-x - root.press_x) / root.zoom,
                        node.layout_y + (self.mouse-y - root.press_y) / root.zoom);
                }
            }
            changed has-hover => {
                if (self.has-hover) {
                    root.hover_id = node.id;
                } else if (root.hover_id == node.id) {
                    root.hover_id = "";
                }
            }
            double-clicked => {
                root.node_double_clicked(node.id);
            }
        }
    }
}
```

- [ ] **Step 7: Translations.** Add the three new keys to all ten catalogs. At the end of each file listed below, append a blank line and then, for each key, `msgid "<key>"` followed by `msgstr "<value>"`, one key per block. In the `.pot`, every `msgstr` is `""`.

| key | en_US | pt_BR | es_ES | de_DE | fr_FR | hi_IN | ja_JP | ko_KR | zh_CN |
|---|---|---|---|---|---|---|---|---|---|
| `accessible-graph-view` | Signal chain graph | Grafo da cadeia de sinal | Grafo de la cadena de señal | Signalketten-Graph | Graphe de la chaîne de signal | सिग्नल चेन ग्राफ़ | シグナルチェーンのグラフ | 신호 체인 그래프 | 信号链图 |
| `graph-node-split` | Split | Split | Split | Split | Split | स्प्लिट | スプリット | 스플릿 | 分路 |
| `graph-node-mixer` | Mixer | Mixer | Mixer | Mixer | Mixer | मिक्सर | ミキサー | 믹서 | 混音器 |

For example, `crates/adapter-gui/translations/en_US/LC_MESSAGES/adapter-gui.po` gains:

```
msgid "accessible-graph-view"
msgstr "Signal chain graph"

msgid "graph-node-split"
msgstr "Split"

msgid "graph-node-mixer"
msgstr "Mixer"
```

The files are `translations/adapter-gui.pot` and `translations/{de_DE,en_US,es_ES,fr_FR,hi_IN,ja_JP,ko_KR,pt_BR,zh_CN}/LC_MESSAGES/adapter-gui.po`. Brand-neutral domain words stay in English in Latin locales, the same convention the catalogs already use for "Looper" and "Insert".

- [ ] **Step 8: Update the stale test comment.** Change only the comment; the assertions stay. In `graph_view_model_tests.rs`, replace the comment above `split_and_merge_use_routing_node_convention` (originally lines 337-342; Tasks 3 and 4 moved it down, so find it by its text):

```rust
    // GraphView.slint relies on this convention to render split/merge as
    // a small routing dot instead of a full block card. If the layout
    // helper starts emitting labels or a non-Util category for split or
    // merge, the Slint side will draw empty grey rectangles where the
    // wires meet — exactly the regression we hit before this test
    // landed.
```

with:

```rust
    // The routing nodes carry no host label and the Util category: since
    // #328 the Slint card picks their face from `kind` and translates
    // their name itself, so a host label here would never be shown.
```

- [ ] **Step 9: Run. Expect PASS.**

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_interaction --test issue_328_graph_view_sources`

Then run: `nice -n 19 cargo test -p adapter-gui -j 2 --lib tr_key` (the four catalog guards, including `every_gui_tr_key_translated_in_every_locale`, which checks all nine locales)

Expected: `test result: ok. 5 passed`, `test result: ok. 5 passed` and `test result: ok. 4 passed`.

- [ ] **Step 10: Docs.** In `docs/gui/graph-view.md`:

Replace the `GraphNode` code block (lines 57-65) with:

```slint
struct GraphNode {
    id: string;
    label: string;          // endpoint names on I/O cards; empty on split/mixer
    category: string;       // "drive", "amp", "reverb", "util", ...
    fill: color;            // host-resolved from default_palette()
    border: color;
    layout_x: length;
    layout_y: length;
    bypass: bool;
    selected: bool;
    kind: string;           // "block" | "io_input" | "io_output" | "split" | "mixer"
    neighbor: bool;         // MIDI neighbor marker (parity with BlockChip)
    block: ChainBlockItem;  // the chain row's item for a block card; empty otherwise
}
```

Replace line 87 (the `node_width, node_height` row) with:

```markdown
| `node_width`, `node_height` | `in` | `length` | `100px` × `100px` | per-node card size in layout space — the chain row's BlockChip size |
```

After the structs subsection (after line 77), add:

```markdown
### Node kinds

`kind` picks the card face (#328): `block` — the block tile; `io_input` / `io_output` — connector artwork over the endpoint names (`label`); `split` / `mixer` — routing artwork (`ui/assets/graph-split.svg` / `graph-mix.svg`, text-free and colorized) over a translated name (`graph-node-split` / `graph-node-mixer`; the host leaves `label` empty). Every node is a clickable card: the earlier `label == "" && category == "util"` routing dot, which had no hit area, is gone. The canvas's accessible label is `@tr("accessible-graph-view")`.
```

Replace the "Category → colour mapping" section body (originally lines 122-127; the helper-table rows added in Tasks 3-6 moved it five lines down) with:

```markdown
The component owns no colours. The host resolves each node's `fill`/`border` from `default_palette()` (Rust, `graph_view_model/palette.rs`, the single source of truth) and writes them onto the node; they paint the I/O, split and mixer cards. Adding a category is one `NodeCategory` variant, its `as_str()` slug and one palette entry.
```

- [ ] **Step 11: Pre-push gate.** Run the same gate commands as Task 1, Step 11.

- [ ] **Step 12: Commit and push.**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328
git -C $S add crates/adapter-gui/ui/components/graph_view_types.slint \
  crates/adapter-gui/ui/components/graph_view.slint \
  crates/adapter-gui/ui/components/graph_node_card.slint \
  crates/adapter-gui/ui/assets/graph-split.svg \
  crates/adapter-gui/ui/assets/graph-mix.svg \
  crates/adapter-gui/translations/adapter-gui.pot \
  crates/adapter-gui/translations/de_DE/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/en_US/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/es_ES/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/fr_FR/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/hi_IN/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/ja_JP/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/ko_KR/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/pt_BR/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/zh_CN/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/src/graph_view_model_tests.rs \
  crates/adapter-gui/tests/issue_328_graph_view_interaction.rs \
  crates/adapter-gui/tests/issue_328_graph_view_sources.rs docs/gui/graph-view.md
git -C $S commit -m "feat(#328): graph node kinds pick the card; split and mixer are clickable cards"
git -C $S fetch && git -C $S push
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 5 task 8 pushed: $(git -C $S rev-parse --short HEAD). GraphNode.kind/neighbor/block, kind-driven GraphNodeCard, routing-dot hack removed, text-free split/mix artwork, @tr a11y + 3 keys in 9 catalogs; interaction 5 + sources 5 green; workspace gate green."
```

---

## Task 9: Block card parity with `BlockChip`, bypass/remove callbacks, tooltip

**Files:**
- Modify: `crates/adapter-gui/ui/components/graph_node_card.slint` (whole file)
- Modify: `crates/adapter-gui/ui/components/graph_view.slint` (imports, `markers_visible`, the two callbacks, `hover_index`, the node loop, the tooltip)
- Modify: `crates/adapter-gui/ui/components/graph_view_test_harness.slint` (`GraphViewHarness` forwards the new callbacks)
- Modify: the translations (2 keys × 10 files)
- Test: `crates/adapter-gui/tests/issue_328_graph_view_interaction.rs`, `crates/adapter-gui/tests/issue_328_graph_view_sources.rs`
- Docs: `docs/gui/graph-view.md` (callbacks table, a "Block card parity" section, the palette paragraph)

**BlockChip parity list** (`block_chip.slint:11-180`, `chain_row_blocks.slint:202-261`):

| Feature | Source in `BlockChip` | On the card |
|---|---|---|
| Type label | `block.type_label`, coloured `label-color(unavailable, enabled, accent)` | same, via `BlockTileStyle` |
| Icon or thumbnail | `EffectTypeIcon { icon-kind: block.icon_kind }`, or `Image { source: block.thumbnail }` when `has_thumbnail` | same |
| Enabled LED | `led-red.png` sprite 24×12, x `0px` on / `-12px` off | same, driven by `!node.bypass`; its hit zone fires `bypass-toggled` |
| Unavailable tint | amber label and icon, whole tile at `0.5` opacity | same |
| MIDI markers | `selected` / `neighbor` / `markers-visible` border and background | same, with `markers_visible` on the canvas |
| Tooltip | `BlockHoverTooltip`, drawn by the row after every chip, when `display_name != ""` and no drag is in flight | same, drawn by the canvas after every node |
| Selection | `selected` marker | same |
| Click / drag | the chip's own `hover-area` (`clicked`, `drag-moved`, `drag-finished`, 8px threshold) | the canvas's `node-ta` under the card (`node_clicked`, `node_dragged`, `node_drag_ended`, 5px threshold, #435) — not the card |
| Hover affordance | the artwork grows from 68px to 90px over the type label (`block_chip.slint:100-120`) | **not carried**: the card's hover affordance is the × appearing; the grow animation is left to Part 6's visual loop with the owner |
| Type label fades during a drag | `opacity: (dragging \|\| chain-dragging) ? 0 : 1` (`block_chip.slint:92`) | **not carried**: the canvas has no floating ghost for the label to clear; the dragged card's art turns `dragging-tint` grey instead |
| Remove (new, §5.2) | — | × at the top right, live only while hovered, fires `remove-requested` |

**Interfaces:**
- Produces:
  - `GraphView` gains `in property <bool> markers_visible`, `callback bypass-toggled(string)` and `callback remove-requested(string)`.
  - `GraphNodeCard` gains `callback bypass-toggled()`, `callback remove-requested()`, and the properties `markers-visible`, `zoom` and `backdrop`.
  - New element ids: `GraphNodeCard::bypass-ta` (`accessible-role: switch`, `accessible-checked: !bypass`) and `GraphNodeCard::remove-ta`.
  - The harness gets `set_markers_visible`, `on_bypass_toggled` and `on_remove_requested`.

- [ ] **Step 1: Invoke the UI skills.** Invoke `claude-plugin:ux-ui`, `slint:slint` and `slint-best-practices`.

- [ ] **Step 2: Add the minimum surface.** This declares the API and forwards it; nothing is rendered yet. In `graph_view.slint`, after `callback viewport_changed(float, length, length);`, add:

```slint
    // #328: a block card's LED was clicked — the host toggles its bypass.
    callback bypass-toggled(string);
    // #328: a block card's × was clicked — the host removes the block.
    callback remove-requested(string);
```

and after `in property <bool> show_grid: true;`, add:

```slint
    // #591 parity with BlockChip: MIDI markers show only while a MIDI
    // command is recent.
    in property <bool> markers_visible: false;
```

Replace the whole `GraphViewHarness` component in `graph_view_test_harness.slint`. `GraphViewScrollHarness` stays unchanged.

```slint
// The canvas alone, every callback forwarded.
export component GraphViewHarness inherits Window {
    width: 900px;
    height: 420px;
    in property <[GraphNode]> nodes;
    in property <[GraphEdgeGeometry]> edges;
    in property <bool> markers-visible: false;
    in-out property <float> zoom: 1.0;
    callback node-clicked(string);
    callback node-drag-ended(string, length, length);
    callback bypass-toggled(string);
    callback remove-requested(string);

    GraphView {
        x: 0px;
        y: 0px;
        width: 100%;
        height: 100%;
        nodes: root.nodes;
        edges: root.edges;
        markers_visible: root.markers-visible;
        zoom <=> root.zoom;
        node_clicked(id) => { root.node-clicked(id); }
        node_drag_ended(id, x, y) => { root.node-drag-ended(id, x, y); }
        bypass-toggled(id) => { root.bypass-toggled(id); }
        remove-requested(id) => { root.remove-requested(id); }
    }
}
```

- [ ] **Step 3: Write the failing tests.** In `issue_328_graph_view_interaction.rs`, replace the first `use` line with the following two lines:

```rust
use adapter_gui::{ChainBlockItem, GraphNode, GraphViewHarness, GraphViewScrollHarness};
use i_slint_backend_testing::ElementHandle;
```

and append:

```rust
/// A block card with a model: type label, icon and a tooltip name.
fn block_node(id: &str, label: &str, x: f32, y: f32) -> GraphNode {
    GraphNode {
        block: ChainBlockItem {
            type_label: "AMP".into(),
            icon_kind: "amp".into(),
            display_name: "Clean Amp".into(),
            ..Default::default()
        },
        ..typed(id, "block", label, x, y)
    }
}

fn center(el: &ElementHandle) -> LogicalPosition {
    let p = el.absolute_position();
    let s = el.size();
    at(p.x + s.width / 2.0, p.y + s.height / 2.0)
}

/// Every element with `id`, left to right on screen.
fn handles(w: &GraphViewHarness, id: &str) -> Vec<ElementHandle> {
    let mut found: Vec<ElementHandle> = ElementHandle::find_by_element_id(w, id).collect();
    found.sort_by(|a, b| a.absolute_position().x.total_cmp(&b.absolute_position().x));
    found
}

fn hover(w: &GraphViewHarness, p: LogicalPosition) {
    w.window()
        .dispatch_event(WindowEvent::PointerMoved { position: p });
}

#[test]
fn a_block_cards_led_shows_bypass_and_toggles_it_without_a_click() {
    let mut rev = block_node("rev", "Rev", 400.0, 200.0);
    rev.bypass = true;
    let w = harness(vec![block_node("amp", "Amp", 240.0, 200.0), rev]);
    let toggled = recorder::<String>();
    let clicked = recorder::<String>();
    let t = toggled.clone();
    w.on_bypass_toggled(move |id| t.borrow_mut().push(id.to_string()));
    let c = clicked.clone();
    w.on_node_clicked(move |id| c.borrow_mut().push(id.to_string()));

    let leds = handles(&w, "GraphNodeCard::bypass-ta");
    assert_eq!(
        leds.iter().map(|l| l.accessible_checked()).collect::<Vec<_>>(),
        [Some(true), Some(false)],
        "the LED reads on for a live block and off for a bypassed one"
    );

    click_at(&w, center(&leds[0]));
    assert_eq!(*toggled.borrow(), ["amp"]);
    assert!(
        clicked.borrow().is_empty(),
        "the LED toggles bypass; it must not also click the node"
    );

    click_at(&w, at(240.0, 200.0));
    assert_eq!(
        *clicked.borrow(),
        ["amp"],
        "the card body still clicks through to the node"
    );
}

#[test]
fn hovering_a_block_card_reveals_a_remove_button_that_fires_remove_requested() {
    let w = harness(vec![block_node("amp", "Amp", 240.0, 200.0)]);
    let removed = recorder::<String>();
    let clicked = recorder::<String>();
    let r = removed.clone();
    w.on_remove_requested(move |id| r.borrow_mut().push(id.to_string()));
    let c = clicked.clone();
    w.on_node_clicked(move |id| c.borrow_mut().push(id.to_string()));

    hover(&w, at(240.0, 200.0));
    let remove = handles(&w, "GraphNodeCard::remove-ta");
    assert_eq!(remove.len(), 1, "a block card carries one × button");
    let p = center(&remove[0]);
    hover(&w, p);
    click_at(&w, p);

    assert_eq!(*removed.borrow(), ["amp"]);
    assert!(clicked.borrow().is_empty(), "the × removes; it must not click the node");
}

#[test]
fn only_block_cards_carry_a_led_and_a_remove_button() {
    let mut nodes = one_of_each_kind();
    nodes[2] = block_node("amp", "Amp", 400.0, 200.0);
    let w = harness(nodes);
    assert_eq!(handles(&w, "GraphNodeCard::bypass-ta").len(), 1);
    assert_eq!(handles(&w, "GraphNodeCard::remove-ta").len(), 1);
}

#[test]
fn hovering_a_block_with_a_model_shows_its_tooltip() {
    let mut nodes = one_of_each_kind();
    nodes[2] = block_node("amp", "Amp", 400.0, 200.0);
    let w = harness(nodes);
    let tooltips = |w: &GraphViewHarness| {
        ElementHandle::find_by_element_type_name(w, "BlockHoverTooltip").count()
    };

    hover(&w, at(400.0, 200.0));
    assert_eq!(
        tooltips(&w),
        1,
        "hovering a block with a model shows the BlockChip tooltip"
    );

    hover(&w, at(80.0, 200.0));
    assert_eq!(tooltips(&w), 0, "an I/O node has no block tooltip");
}

#[test]
fn an_unavailable_block_card_reads_as_disabled() {
    let mut amp = block_node("amp", "Amp", 240.0, 200.0);
    amp.block.unavailable = true;
    let w = harness(vec![amp, block_node("rev", "Rev", 400.0, 200.0)]);
    let enabled = |label: &str| {
        ElementHandle::find_by_accessible_label(&w, label)
            .next()
            .and_then(|e| e.accessible_enabled())
    };
    assert_eq!(enabled("Amp"), Some(false), "an uninstalled model reads as disabled");
    assert_eq!(enabled("Rev"), Some(true));
}

#[test]
fn a_block_cards_led_and_remove_hit_zones_scale_with_the_zoom() {
    let w = harness(vec![block_node("amp", "Amp", 240.0, 200.0)]);
    w.set_zoom(0.5);
    let size = |id: &str| {
        handles(&w, id)
            .first()
            .map(|h| (h.size().width, h.size().height))
    };
    assert_eq!(
        size("GraphNodeCard::remove-ta"),
        Some((12.0, 12.0)),
        "at half zoom the × is half size; a fixed 24px one covers most of a small card"
    );
    assert_eq!(
        size("GraphNodeCard::bypass-ta"),
        Some((14.0, 10.0)),
        "at half zoom the LED switch is half size"
    );
}
```

Append to `issue_328_graph_view_sources.rs`:

```rust
/// Spec §5.1: the graph block card paints its states from the same
/// global as BlockChip.
#[test]
fn the_graph_block_card_paints_its_states_from_block_tile_style() {
    let card = read_component("graph_node_card.slint");
    assert!(
        card.contains("BlockTileStyle."),
        "graph_node_card.slint must paint a block's states from BlockTileStyle (#328 parity)"
    );
    for literal in ["#b07a3c", "#5c6678", "#8b95a5", "#f0a020"] {
        assert!(
            !card.contains(literal),
            "graph_node_card.slint hard-codes {literal}; read it from BlockTileStyle"
        );
    }
}
```

- [ ] **Step 4: Run. Expect FAIL.**

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_interaction --test issue_328_graph_view_sources`

Expected:
```
---- a_block_cards_led_shows_bypass_and_toggles_it_without_a_click stdout ----
assertion `left == right` failed: the LED reads on for a live block and off for a bypassed one
  left: []
 right: [Some(true), Some(false)]
---- hovering_a_block_card_reveals_a_remove_button_that_fires_remove_requested stdout ----
  left: 0
 right: 1: a block card carries one × button
---- hovering_a_block_with_a_model_shows_its_tooltip stdout ----
  left: 0
 right: 1: hovering a block with a model shows the BlockChip tooltip
---- an_unavailable_block_card_reads_as_disabled stdout ----
  left: None
 right: Some(false): an uninstalled model reads as disabled
---- a_block_cards_led_and_remove_hit_zones_scale_with_the_zoom stdout ----
  left: None
 right: Some((12.0, 12.0)): at half zoom the × is half size; a fixed 24px one covers most of a small card
---- the_graph_block_card_paints_its_states_from_block_tile_style stdout ----
graph_node_card.slint must paint a block's states from BlockTileStyle (#328 parity)
```

`left: None` for the unavailable card: the Task 8 card declares no `accessible-enabled`, so the generated accessor has no value to return.

- [ ] **Step 5: Implement the card.** Replace `crates/adapter-gui/ui/components/graph_node_card.slint` with:

```slint
// Responsibility: draws one node card of the graph view.
// Node card of the graph chain editor (#435, #328). `node.kind` picks the
// face: a "block" card keeps parity with the chain row's BlockChip (type
// label, icon or thumbnail, LED, unavailable tint, MIDI markers — every
// state colour read from BlockTileStyle so the two tiles never disagree);
// I/O cards show their connector over the endpoint names; the split and
// the mixer show their routing artwork over a translated name.
//
// Input: the canvas's node TouchArea is declared UNDER this card and owns
// click and drag. The card's only TouchAreas are a block's LED (bypass)
// and its × (remove), which therefore win on those two spots.

import { GraphNode } from "graph_view_types.slint";
import { EffectTypeIcon } from "effect_type_icon.slint";
import { BlockTileStyle } from "block_tile_style.slint";
import { Theme } from "../theme.slint";

export component GraphNodeCard inherits Rectangle {
    in property <GraphNode> node;
    in property <bool> hovered;
    in property <bool> dragging;
    // #591: MIDI markers only show while a MIDI command is recent.
    in property <bool> markers-visible: false;
    // Viewport zoom: block artwork scales with the card; text keeps
    // Theme.min-font (#954) and elides.
    in property <float> zoom: 1.0;
    // Opaque fill behind a block card so the wires that run centre to
    // centre stop at its edge (BlockChip itself sits on the row).
    in property <color> backdrop;
    in property <length> card_width;
    in property <length> card_height;
    callback bypass-toggled();
    callback remove-requested();

    property <bool> is-block: root.node.kind == "block";
    property <bool> is-io: root.node.kind == "io_input" || root.node.kind == "io_output";
    property <bool> is-routing: root.node.kind == "split" || root.node.kind == "mixer";
    property <bool> enabled: !root.node.bypass;
    property <bool> unavailable: root.is-block && root.node.block.unavailable;
    // BlockChip geometry (100×100 tile: 22px label, 6px gap, 68px art,
    // 12px LED 10px above the bottom), scaled by the zoom.
    property <length> label-h: max(22px, 22px * root.zoom);
    property <length> art-y: root.label-h + 6px * root.zoom;
    property <length> art-h: max(0px, root.height - root.art-y - 4px * root.zoom);
    property <length> led: 12px * root.zoom;
    // The split and the mixer carry no host label; their name is translated.
    property <string> face-label: root.node.kind == "split" ? @tr("graph-node-split")
        : root.node.kind == "mixer" ? @tr("graph-node-mixer")
        : root.node.label;

    width: root.card_width;
    height: root.card_height;
    border-radius: root.is-block ? 6px : 8px;
    background: root.is-block ? root.backdrop
        : root.node.bypass ? root.node.fill.with-alpha(0.35)
        : root.dragging ? root.node.fill.brighter(0.20)
        : root.hovered ? root.node.fill.brighter(0.10)
        : root.node.fill;
    border-width: root.is-block
        ? BlockTileStyle.marker-border-width(root.markers-visible, root.node.selected, root.node.neighbor)
        : (root.node.selected ? 2px : 1px);
    border-color: root.is-block
        ? BlockTileStyle.marker-border-color(root.markers-visible, root.node.selected, root.node.neighbor, root.enabled)
        : (root.node.selected ? #f5f7fb : root.node.border);
    opacity: root.unavailable ? BlockTileStyle.unavailable-opacity
        : (!root.is-block && root.node.bypass) ? 0.7
        : 1.0;
    drop-shadow-blur: root.is-block ? 0px : root.dragging ? 12px : (root.hovered ? 6px : 2px);
    drop-shadow-color: #00000080;
    drop-shadow-offset-y: root.is-block ? 0px : 2px;

    // ── Block: BlockChip parity ──────────────────────────────────────
    // #591 marker tint over the backdrop.
    if root.is-block : Rectangle {
        width: parent.width;
        height: parent.height;
        border-radius: 6px;
        background: BlockTileStyle.marker-background(root.markers-visible, root.node.selected);
    }

    if root.is-block : Text {
        x: 0px;
        y: 0px;
        width: parent.width;
        height: root.label-h;
        text: root.node.block.type-label;
        color: BlockTileStyle.label-color(root.unavailable, root.enabled, root.node.block.accent-color);
        font-size: Theme.min-font;
        font-weight: 700;
        letter-spacing: 1px;
        horizontal-alignment: center;
        vertical-alignment: center;
        overflow: elide;
    }

    if root.is-block && root.node.block.has-thumbnail : Image {
        x: 0px;
        y: root.art-y;
        width: parent.width;
        height: root.art-h;
        source: root.node.block.thumbnail;
        image-fit: contain;
    }

    if root.is-block && !root.node.block.has-thumbnail : EffectTypeIcon {
        x: 0px;
        y: root.art-y;
        width: parent.width;
        height: root.art-h;
        icon-kind: root.node.block.icon-kind;
        tint: BlockTileStyle.art-color(root.unavailable, root.dragging, root.enabled, root.node.block.accent-color);
    }

    // LED status (sprite 24×12 at zoom 1: 0px shows lit, -12px unlit), as
    // BlockChip draws it.
    if root.is-block : Rectangle {
        x: (parent.width - root.led) / 2;
        y: parent.height - 10px * root.zoom;
        width: root.led;
        height: root.led;
        clip: true;

        Image {
            source: @image-url("../assets/sprites/led-red.png");
            x: root.enabled ? 0px : -root.led;
            y: 0px;
            width: 2 * root.led;
            height: root.led;
            image-fit: fill;
            image-rendering: smooth;
        }
    }

    // The LED is the bypass switch (#328 §5.2 `bypass-toggled`). Its hit
    // zone scales with the card like the LED does: a fixed 28×20 would
    // swallow the bottom of a zoomed-out card.
    if root.is-block : bypass-ta := TouchArea {
        x: (parent.width - self.width) / 2;
        y: parent.height - 18px * root.zoom;
        width: 28px * root.zoom;
        height: 20px * root.zoom;
        mouse-cursor: pointer;
        clicked => { root.bypass-toggled(); }
        accessible-role: switch;
        accessible-checked: root.enabled;
        accessible-label: @tr("accessible-graph-node-bypass");
    }

    // × at the top right (#328 §5.2 `remove-requested`). Live only while the
    // card is hovered: a tap on a touch screen, which has no hover, can
    // never land on an invisible ×. Scaled with the card: a fixed 24×24
    // would cover most of a 30px card at the minimum zoom, and a click
    // meant to select the block would remove it.
    if root.is-block : remove-ta := TouchArea {
        x: parent.width - 26px * root.zoom;
        y: 2px * root.zoom;
        width: 24px * root.zoom;
        height: 24px * root.zoom;
        enabled: root.hovered || self.has-hover;
        mouse-cursor: pointer;
        clicked => { root.remove-requested(); }
        accessible-role: button;
        accessible-label: @tr("accessible-graph-node-remove");

        Image {
            x: 4px * root.zoom;
            y: 4px * root.zoom;
            width: 16px * root.zoom;
            height: 16px * root.zoom;
            source: @image-url("../assets/close.svg");
            colorize: remove-ta.has-hover ? #ffffff : #8a94a2;
            visible: root.hovered || remove-ta.has-hover;
        }
    }

    // ── I/O, split, mixer ────────────────────────────────────────────
    // I/O endpoint: its connector artwork above the endpoint names.
    if root.is-io : EffectTypeIcon {
        x: 0px;
        y: 8px;
        width: parent.width;
        height: max(0px, parent.height - 38px);
        icon-kind: root.node.kind == "io_input" ? "input" : "output";
    }

    // Split / mixer: the routing artwork above the translated name. The
    // SVGs carry no text of their own (graph-split.svg / graph-mix.svg).
    if root.is-routing : Image {
        x: (parent.width - self.width) / 2;
        y: 8px;
        width: max(0px, parent.height - 38px);
        height: max(0px, parent.height - 38px);
        source: root.node.kind == "split"
            ? @image-url("../assets/graph-split.svg")
            : @image-url("../assets/graph-mix.svg");
        image-fit: contain;
        colorize: #f5f7fb;
    }

    if !root.is-block : Text {
        x: 8px;
        y: root.is-io || root.is-routing ? parent.height - 30px : 0px;
        width: parent.width - 16px;
        height: root.is-io || root.is-routing ? 22px : parent.height;
        text: root.face-label;
        color: #f5f7fb;
        font-size: Theme.min-font;
        font-weight: 700;
        horizontal-alignment: center;
        vertical-alignment: center;
        overflow: elide;
    }

    accessible-role: button;
    accessible-label: root.face-label;
    accessible-enabled: !root.unavailable;
}
```

- [ ] **Step 6: Implement the canvas.** In `graph_view.slint`:

- After `import { GraphWire } from "graph_wire.slint";`, add `import { BlockHoverTooltip } from "block_hover_tooltip.slint";`
- After `private property <string> drag_id: "";`, add:

```slint
    // Index of the hovered node, for the block tooltip (-1 = none).
    private property <int> hover_index: -1;
```

- Replace everything from the line `    // ── Nodes ──────────────────────────────────────────────────────` to the end of the file with:

```slint
    // ── Nodes ──────────────────────────────────────────────────────
    // Every node is a card, whatever its kind (#328 §5.2). Position is
    // the card centre at (layout * zoom + pan), shifted by half the card
    // size so it lands geometrically centred.
    for node[idx] in root.nodes : Rectangle {
        property <length> w: root.node_width * root.zoom;
        property <length> h: root.node_height * root.zoom;

        x: root.pan_x + node.layout_x * root.zoom - self.w / 2;
        y: root.pan_y + node.layout_y * root.zoom - self.h / 2;
        width: self.w;
        height: self.h;

        // Declared BEFORE the card (#328): the card's own TouchAreas — a
        // block's LED and × — sit on top of this one and win on those two
        // spots. The rest of the card is plain visuals, so every other
        // pointer event falls through to here for click and drag.
        node-ta := TouchArea {
            x: 0px;
            y: 0px;
            width: parent.width;
            height: parent.height;
            mouse-cursor: pointer;
            pointer-event(event) => {
                if (event.kind == PointerEventKind.down) {
                    root.drag_id = node.id;
                    root.press_x = self.mouse-x;
                    root.press_y = self.mouse-y;
                    root.did_drag = false;
                } else if (event.kind == PointerEventKind.up) {
                    if (root.drag_id == node.id) {
                        if (root.did_drag) {
                            root.node_drag_ended(node.id, node.layout_x, node.layout_y);
                        } else {
                            root.node_clicked(node.id);
                        }
                    }
                    root.drag_id = "";
                }
            }
            moved => {
                if (self.pressed && root.drag_id == node.id) {
                    if (abs(self.mouse-x - root.press_x) > root.click_threshold
                     || abs(self.mouse-y - root.press_y) > root.click_threshold) {
                        root.did_drag = true;
                    }
                    // Delta-based: the node's current layout cancels out
                    // (see press_x note), so this tracks the cursor
                    // without drift even though the TouchArea moves with
                    // the node.
                    root.node_dragged(
                        node.id,
                        node.layout_x + (self.mouse-x - root.press_x) / root.zoom,
                        node.layout_y + (self.mouse-y - root.press_y) / root.zoom);
                }
            }
            changed has-hover => {
                if (self.has-hover) {
                    root.hover_id = node.id;
                    root.hover_index = idx;
                } else if (root.hover_id == node.id) {
                    root.hover_id = "";
                    root.hover_index = -1;
                }
            }
            double-clicked => {
                root.node_double_clicked(node.id);
            }
        }

        GraphNodeCard {
            x: 0px;
            y: 0px;
            node: node;
            zoom: root.zoom;
            backdrop: root.background_color;
            markers-visible: root.markers_visible;
            card_width: parent.width;
            card_height: parent.height;
            hovered: root.hover_id == node.id && root.drag_id == "";
            dragging: root.drag_id == node.id;
            bypass-toggled => { root.bypass-toggled(node.id); }
            remove-requested => { root.remove-requested(node.id); }
        }
    }

    // ── Block tooltip (#328 parity with BlockChip) ─────────────────
    // The chain row's hover card, declared after every node so it paints
    // on top. Only for a block with a model (display_name), never while a
    // drag is in flight.
    if root.hover_index >= 0
        && root.hover_index < root.nodes.length
        && root.nodes[root.hover_index].kind == "block"
        && root.nodes[root.hover_index].block.display_name != ""
        && root.drag_id == ""
        : BlockHoverTooltip {
        block: root.nodes[root.hover_index].block;
        anchor-x: root.pan_x + root.nodes[root.hover_index].layout_x * root.zoom - root.node_width * root.zoom / 2;
        anchor-y: root.pan_y + root.nodes[root.hover_index].layout_y * root.zoom - root.node_height * root.zoom / 2;
        chip-width: root.node_width * root.zoom;
        chip-height: root.node_height * root.zoom;
        container-width: root.width;
    }
}
```

- [ ] **Step 7: Translations.** Using the same format as Task 8, Step 7, append these two keys to all ten catalogs:

| key | en_US | pt_BR | es_ES | de_DE | fr_FR | hi_IN | ja_JP | ko_KR | zh_CN |
|---|---|---|---|---|---|---|---|---|---|
| `accessible-graph-node-bypass` | Turn the block on or off | Ligar ou desligar o bloco | Encender o apagar el bloque | Block ein- oder ausschalten | Activer ou désactiver le bloc | ब्लॉक चालू या बंद करें | ブロックのオン/オフ | 블록 켜기/끄기 | 开启或关闭模块 |
| `accessible-graph-node-remove` | Remove the block | Excluir o bloco | Eliminar el bloque | Block entfernen | Retirer le bloc | ब्लॉक हटाएँ | ブロックを削除 | 블록 제거 | 移除模块 |

- [ ] **Step 8: Run. Expect PASS.**

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_interaction --test issue_328_graph_view_sources`

Then run: `nice -n 19 cargo test -p adapter-gui -j 2 --lib tr_key` (the four catalog guards, including `every_gui_tr_key_translated_in_every_locale`, which checks all nine locales)

Expected: `test result: ok. 11 passed`, `test result: ok. 6 passed` and `test result: ok. 4 passed`. The Task 1 pins pass unchanged: the card centre now falls through to `node-ta`.

- [ ] **Step 9: Docs.** In `docs/gui/graph-view.md`:

In the Properties table, add the row:

```markdown
| `markers_visible` | `in` | `bool` | `false` | show the MIDI selected / neighbor markers on block cards (#591) |
```

In the Callbacks table, add these rows:

```markdown
| `bypass-toggled(string)` | node id | a block card's LED was clicked (#328) |
| `remove-requested(string)` | node id | a block card's × was clicked; the × is live only while the card is hovered, so a touch tap never removes (#328) |
```

After the "Node kinds" subsection, add:

```markdown
### Block card parity

A `block` card draws what the chain row's `BlockChip` draws, from the same `ChainBlockItem` (`node.block`): type label, icon or thumbnail, the LED (driven by `bypass`), the amber unavailable tint at half opacity, the MIDI selected / neighbor markers, and the `BlockHoverTooltip` on hover (drawn by the canvas after every node, only for a block with a `display_name`, never during a drag). State colours come from `BlockTileStyle` (`ui/components/block_tile_style.slint`), the global `BlockChip` reads too. The canvas's node TouchArea sits UNDER the card, so the LED and the × win on their spots and the rest of the card clicks and drags through.
```

At the end of the "Category → colour mapping" section, append: `Block cards do not use `fill`/`border`: they paint their states from `BlockTileStyle`, shared with `BlockChip`.`

- [ ] **Step 10: Pre-push gate.** Run the same gate commands as Task 1, Step 11.

- [ ] **Step 11: Commit and push.**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328
git -C $S add crates/adapter-gui/ui/components/graph_node_card.slint \
  crates/adapter-gui/ui/components/graph_view.slint \
  crates/adapter-gui/ui/components/graph_view_test_harness.slint \
  crates/adapter-gui/translations/adapter-gui.pot \
  crates/adapter-gui/translations/de_DE/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/en_US/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/es_ES/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/fr_FR/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/hi_IN/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/ja_JP/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/ko_KR/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/pt_BR/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/zh_CN/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/tests/issue_328_graph_view_interaction.rs \
  crates/adapter-gui/tests/issue_328_graph_view_sources.rs docs/gui/graph-view.md
git -C $S commit -m "feat(#328): graph block card matches BlockChip; LED toggles bypass, hover × removes"
git -C $S fetch && git -C $S push
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 5 task 9 pushed: $(git -C $S rev-parse --short HEAD). Block card parity (label/icon/LED/unavailable/markers/tooltip via BlockTileStyle), bypass-toggled + remove-requested, LED/× hit zones scale with zoom; interaction 11 + sources 6 green; workspace gate green."
```

---

## Task 10: "+" anchors, `add-requested`, and drag-drop to `node-dropped`

**Files:**
- Modify: `crates/adapter-gui/ui/components/graph_view_types.slint` (add `GraphAnchor`)
- Modify: `crates/adapter-gui/ui/components/graph_view.slint` (imports and re-export, `anchors`, the three callbacks, `drop_anchor`, the anchors loop, the node-ta handlers)
- Modify: `crates/adapter-gui/ui/components/block_insert_slot.slint:6-10,16,75` (`show-track`, accessible label)
- Modify: `crates/adapter-gui/ui/components/graph_view_test_harness.slint` (whole file)
- Modify: the translations (1 key × 10 files)
- Test: `crates/adapter-gui/tests/issue_328_graph_view_interaction.rs`
- Docs: `docs/gui/graph-view.md`

**Interfaces:**
- Consumes: `graph_view_model::{insert_anchors, resolve_drop_anchor, GraphAnchor, AnchorSlot}` (Tasks 5 and 6).
- Produces:
  - Slint `export struct GraphAnchor { id: string, layout_x: length, layout_y: length, always_visible: bool }`
  - `GraphView`: `in property <[GraphAnchor]> anchors`, `callback add-requested(string)`, `callback node-dropped(string, string)`, `pure callback resolve-drop-anchor(string, length, length) -> string`
  - `BlockInsertSlot`: `in property <bool> show-track: true`
  - The harness gets `set_anchors`, `on_add_requested`, `on_node_dropped` and `on_resolve_drop_anchor`.

- [ ] **Step 1: Invoke the UI skills.** Invoke `claude-plugin:ux-ui`, `slint:slint` and `slint-best-practices`.

- [ ] **Step 2: Add the minimum surface.** Declare the struct, the property and the callbacks, but render no anchors and drop nothing yet.

Append to `graph_view_types.slint`:

```slint
// #328 §5.1: one insert anchor — the "+" on a wire. Host-computed by
// graph_view_model::insert_anchors (wire midpoint, slot id).
export struct GraphAnchor {
    id: string,
    layout_x: length,
    layout_y: length,
    // An empty segment (no block at either end) always shows its "+".
    always_visible: bool,
}
```

In `graph_view.slint`:

- Change the types import to `import { GraphNode, GraphEdge, GraphEdgeGeometry, CategoryColor, GraphAnchor } from "graph_view_types.slint";`
- Change the re-export to `export { GraphNode, GraphEdge, GraphEdgeGeometry, CategoryColor, GraphAnchor }`
- After `in property <[GraphEdgeGeometry]> edges;`, add:

```slint
    // #328: the "+" anchors on the wires (graph_view_model::insert_anchors).
    in property <[GraphAnchor]> anchors;
```

- After the `remove-requested` callback, add:

```slint
    // #328: a "+" anchor was clicked — the host opens the add-block picker
    // for that anchor's slot.
    callback add-requested(string);
    // #328: a dragged block was released on an anchor (node id, anchor id).
    callback node-dropped(string, string);
    // #328: the host resolves which anchor a block dragged to layout (x, y)
    // lands on — "" for none (graph_view_model::resolve_drop_anchor; the
    // #787 `slot-at` pattern). Asked on every drag move to light the target
    // up, so the geometry lives in Rust exactly once.
    pure callback resolve-drop-anchor(string, length, length) -> string;
```

Replace the whole of `crates/adapter-gui/ui/components/graph_view_test_harness.slint`:

```slint
// Responsibility: hosts the graph view in test windows.
// #328 TEST-ONLY harness: hosts GraphView in real Windows so the headless
// interaction tests (i-slint-backend-testing) can dispatch real pointer,
// wheel and key events at it. Imported by app-window.slint purely so
// slint-build emits the Rust types; never shown in the running app.

import { GraphView, GraphNode, GraphEdgeGeometry, GraphAnchor } from "graph_view.slint";

// The canvas alone, every callback forwarded.
export component GraphViewHarness inherits Window {
    width: 900px;
    height: 420px;
    in property <[GraphNode]> nodes;
    in property <[GraphEdgeGeometry]> edges;
    in property <[GraphAnchor]> anchors;
    in property <bool> markers-visible: false;
    in-out property <float> zoom: 1.0;
    callback node-clicked(string);
    callback node-drag-ended(string, length, length);
    callback bypass-toggled(string);
    callback remove-requested(string);
    callback add-requested(string);
    callback node-dropped(string, string);
    pure callback resolve-drop-anchor(string, length, length) -> string;

    GraphView {
        x: 0px;
        y: 0px;
        width: 100%;
        height: 100%;
        nodes: root.nodes;
        edges: root.edges;
        anchors: root.anchors;
        markers_visible: root.markers-visible;
        zoom <=> root.zoom;
        node_clicked(id) => { root.node-clicked(id); }
        node_drag_ended(id, x, y) => { root.node-drag-ended(id, x, y); }
        bypass-toggled(id) => { root.bypass-toggled(id); }
        remove-requested(id) => { root.remove-requested(id); }
        add-requested(id) => { root.add-requested(id); }
        node-dropped(id, anchor) => { root.node-dropped(id, anchor); }
        resolve-drop-anchor(id, x, y) => { root.resolve-drop-anchor(id, x, y) }
    }
}

// The canvas inside a scroll area, the way the chains list hosts it
// (#328 §5.4): proves a plain wheel scrolls the list, not the graph.
export component GraphViewScrollHarness inherits Window {
    width: 900px;
    height: 400px;
    in property <[GraphNode]> nodes;
    in-out property <float> zoom: 1.0;
    out property <length> list-scroll-y: list.viewport-y;

    list := Flickable {
        x: 0px;
        y: 0px;
        width: 100%;
        height: 100%;
        viewport-height: 1200px;

        GraphView {
            x: 0px;
            y: 100px;
            width: 900px;
            height: 300px;
            nodes: root.nodes;
            zoom <=> root.zoom;
        }
    }
}
```

- [ ] **Step 3: Write the failing tests.** In `issue_328_graph_view_interaction.rs`, replace the `use` block with:

```rust
use adapter_gui::graph_view_model as model;
use adapter_gui::{
    ChainBlockItem, GraphAnchor, GraphEdgeGeometry, GraphNode, GraphViewHarness,
    GraphViewScrollHarness,
};
use i_slint_backend_testing::ElementHandle;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
```

and append:

```rust
/// in → split → [a1] ∥ [b1] → mixer → out, laid out by the real model on
/// the default grid: in (80,200) · split (240,200) · a1 (400,140) ·
/// b1 (400,260) · mixer (560,200) · out (720,200). Anchors sit on the wire
/// midpoints: split→b1 at (320,230), a1→mixer at (480,170).
fn split_mix_chain() -> (
    Vec<model::GraphNode>,
    Vec<model::GraphEdge>,
    Vec<model::GraphAnchor>,
) {
    let amp =
        |id: &str| model::BlockBlueprint::new(id, id.to_uppercase(), model::NodeCategory::Amp);
    let stages = [
        model::ChainStage::Single(
            model::BlockBlueprint::new("in", "In 1", model::NodeCategory::Input)
                .with_kind(model::NodeKind::IoInput),
        ),
        model::ChainStage::Parallel {
            lanes: vec![vec![amp("a1")], vec![amp("b1")]],
            end: model::ParallelEnd::Merge,
        },
        model::ChainStage::Single(
            model::BlockBlueprint::new("out", "Out 1", model::NodeCategory::Output)
                .with_kind(model::NodeKind::IoOutput),
        ),
    ];
    let (nodes, edges) = model::linear_chain_layout(&stages, model::GridMetrics::default());
    let anchors = model::insert_anchors(&stages, &nodes);
    (nodes, edges, anchors)
}

/// The graph as a host hands it to the canvas: the Rust model mapped field
/// by field onto the Slint structs, with the drop resolver wired to
/// `graph_view_model::resolve_drop_anchor`.
fn canvas_for(
    nodes: &[model::GraphNode],
    edges: &[model::GraphEdge],
    anchors: &[model::GraphAnchor],
) -> GraphViewHarness {
    let w = harness(
        nodes
            .iter()
            .map(|n| GraphNode {
                id: n.id.as_str().into(),
                label: n.label.as_str().into(),
                category: n.category.as_str().into(),
                kind: n.kind.as_str().into(),
                layout_x: n.x,
                layout_y: n.y,
                bypass: n.bypass,
                ..Default::default()
            })
            .collect(),
    );
    let pos = |id: &str| {
        nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| (n.x, n.y))
            .unwrap_or_default()
    };
    let geometry: Vec<GraphEdgeGeometry> = edges
        .iter()
        .map(|e| {
            let (from_x, from_y) = pos(&e.from_id);
            let (to_x, to_y) = pos(&e.to_id);
            GraphEdgeGeometry {
                from_id: e.from_id.as_str().into(),
                to_id: e.to_id.as_str().into(),
                from_x,
                from_y,
                to_x,
                to_y,
            }
        })
        .collect();
    w.set_edges(ModelRc::new(VecModel::from(geometry)));
    let slint_anchors: Vec<GraphAnchor> = anchors
        .iter()
        .map(|a| GraphAnchor {
            id: a.id.as_str().into(),
            layout_x: a.x,
            layout_y: a.y,
            always_visible: a.always_visible,
        })
        .collect();
    w.set_anchors(ModelRc::new(VecModel::from(slint_anchors)));
    let (nodes, anchors) = (nodes.to_vec(), anchors.to_vec());
    w.on_resolve_drop_anchor(move |id, x, y| {
        model::resolve_drop_anchor(&nodes, &anchors, &id, x, y, model::GridMetrics::default())
            .map(|a| SharedString::from(a.id.as_str()))
            .unwrap_or_default()
    });
    w
}

#[test]
fn clicking_a_wire_anchor_fires_add_requested_with_its_slot_id() {
    let (nodes, edges, anchors) = split_mix_chain();
    let w = canvas_for(&nodes, &edges, &anchors);
    let added = recorder::<String>();
    let a = added.clone();
    w.on_add_requested(move |id| a.borrow_mut().push(id.to_string()));

    assert_eq!(
        handles(&w, "BlockInsertSlot::hover-area").len(),
        anchors.len(),
        "one + per wire"
    );
    click_at(&w, at(320.0, 230.0));

    assert_eq!(
        *added.borrow(),
        ["lane:1:1:0"],
        "the + on split → b1 adds first in path B"
    );
}

#[test]
fn dragging_a_block_onto_the_other_lanes_anchor_fires_node_dropped() {
    let (nodes, edges, anchors) = split_mix_chain();
    let w = canvas_for(&nodes, &edges, &anchors);
    let dropped = recorder::<(String, String)>();
    let d = dropped.clone();
    w.on_node_dropped(move |id, anchor| d.borrow_mut().push((id.to_string(), anchor.to_string())));

    drag(&w, at(400.0, 140.0), at(320.0, 230.0));

    assert_eq!(
        *dropped.borrow(),
        [("a1".to_string(), "lane:1:1:0".to_string())],
        "path A's block dropped on split → b1 moves first into path B"
    );
}

#[test]
fn dropping_a_block_on_its_own_wire_fires_no_node_dropped() {
    let (nodes, edges, anchors) = split_mix_chain();
    let w = canvas_for(&nodes, &edges, &anchors);
    let dropped = recorder::<(String, String)>();
    let ended = recorder::<String>();
    let d = dropped.clone();
    w.on_node_dropped(move |id, anchor| d.borrow_mut().push((id.to_string(), anchor.to_string())));
    let e = ended.clone();
    w.on_node_drag_ended(move |id, _, _| e.borrow_mut().push(id.to_string()));

    drag(&w, at(400.0, 140.0), at(480.0, 170.0));

    assert!(
        dropped.borrow().is_empty(),
        "a1 → mixer is a1's own wire: no move, got {:?}",
        dropped.borrow()
    );
    assert_eq!(*ended.borrow(), ["a1"], "the drag itself still ends");
}
```

- [ ] **Step 4: Run. Expect FAIL.**

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_interaction`

Expected:
```
---- clicking_a_wire_anchor_fires_add_requested_with_its_slot_id stdout ----
assertion `left == right` failed: one + per wire
  left: 0
 right: 6
---- dragging_a_block_onto_the_other_lanes_anchor_fires_node_dropped stdout ----
assertion `left == right` failed: path A's block dropped on split → b1 moves first into path B
  left: []
 right: [("a1", "lane:1:1:0")]
```

`dropping_a_block_on_its_own_wire_fires_no_node_dropped` passes: it is a guard.

- [ ] **Step 5: Make `BlockInsertSlot` usable on a wire.** This preserves chain-row behaviour: the new property defaults to `true`. In `block_insert_slot.slint`:

- After line 9 (`in property <bool> suppress-hover-affordance: false;`), add:

```slint
    // #328: the chain row draws a straight track through the slot; on a
    // graph wire the wire itself is the track, so the graph turns it off.
    in property <bool> show-track: true;
```

- Change line 16 (`if !root.empty-slot : Rectangle {`) to `if !root.empty-slot && root.show-track : Rectangle {`
- After the `hover-area := TouchArea { … }` block (before the component's closing `}` at line 75), add:

```slint

    accessible-role: button;
    accessible-label: @tr("accessible-add-block-here");
```

- [ ] **Step 6: Implement the anchors and the drop.** In `graph_view.slint`:

- After `import { BlockHoverTooltip } from "block_hover_tooltip.slint";`, add `import { BlockInsertSlot } from "block_insert_slot.slint";`
- After `private property <int> hover_index: -1;`, add:

```slint
    // #328: the anchor the current drag would land on ("" = none).
    private property <string> drop_anchor: "";
```

- After the node loop (the `for node[idx] in root.nodes : Rectangle { … }` block) and before the `// ── Block tooltip` comment, add:

```slint

    // ── Insert anchors (#328 §5.1) ─────────────────────────────────
    // One "+" per wire, at the wire's midpoint (host-computed by
    // graph_view_model::insert_anchors). It is the chain row's
    // BlockInsertSlot without its straight track — the wire is the track.
    // Drawn OVER the nodes: the host keeps a dragged card under the cursor
    // (node_dragged), and that card would otherwise hide the lit "+" it is
    // about to land on. At zoom 1 an anchor sits inside the 60px gap
    // between two cards; it reaches a card's edge only below zoom ~0.53.
    // The tooltip, declared after, still paints on top.
    for anchor in root.anchors : BlockInsertSlot {
        x: root.pan_x + anchor.layout_x * root.zoom - self.width / 2;
        y: root.pan_y + anchor.layout_y * root.zoom - self.height / 2;
        show-track: false;
        empty-slot: anchor.always_visible;
        move-target: root.drag_id != "";
        active-drop-target: root.drop_anchor != "" && root.drop_anchor == anchor.id;
        clicked => { root.add-requested(anchor.id); }
    }
```

- In the node loop's `node-ta`, replace the `pointer-event(event) => { … }` and `moved => { … }` blocks with:

```slint
            pointer-event(event) => {
                if (event.kind == PointerEventKind.down) {
                    root.drag_id = node.id;
                    root.press_x = self.mouse-x;
                    root.press_y = self.mouse-y;
                    root.did_drag = false;
                    root.drop_anchor = "";
                } else if (event.kind == PointerEventKind.up) {
                    if (root.drag_id == node.id) {
                        if (root.did_drag) {
                            root.node_drag_ended(node.id, node.layout_x, node.layout_y);
                            // #328: released on an anchor → the host moves
                            // the block to that anchor's slot.
                            if (root.drop_anchor != "") {
                                root.node-dropped(node.id, root.drop_anchor);
                            }
                        } else {
                            root.node_clicked(node.id);
                        }
                    }
                    root.drag_id = "";
                    root.drop_anchor = "";
                }
            }
            moved => {
                if (self.pressed && root.drag_id == node.id) {
                    if (abs(self.mouse-x - root.press_x) > root.click_threshold
                     || abs(self.mouse-y - root.press_y) > root.click_threshold) {
                        root.did_drag = true;
                    }
                    // Delta-based: the node's current layout cancels out
                    // (see press_x note), so this tracks the cursor
                    // without drift even though the TouchArea moves with
                    // the node.
                    let drag_x = node.layout_x + (self.mouse-x - root.press_x) / root.zoom;
                    let drag_y = node.layout_y + (self.mouse-y - root.press_y) / root.zoom;
                    if (root.did_drag) {
                        // #328: the host names the anchor under the drag
                        // so it lights up before release.
                        root.drop_anchor = root.resolve-drop-anchor(node.id, drag_x, drag_y);
                    }
                    root.node_dragged(node.id, drag_x, drag_y);
                }
            }
```

- [ ] **Step 7: Translations.** Using the same format as Task 8, Step 7, append this key to all ten catalogs:

| key | en_US | pt_BR | es_ES | de_DE | fr_FR | hi_IN | ja_JP | ko_KR | zh_CN |
|---|---|---|---|---|---|---|---|---|---|
| `accessible-add-block-here` | Add a block here | Adicionar um bloco aqui | Añadir un bloque aquí | Hier einen Block hinzufügen | Ajouter un bloc ici | यहाँ ब्लॉक जोड़ें | ここにブロックを追加 | 여기에 블록 추가 | 在此添加模块 |

- [ ] **Step 8: Run. Expect PASS.** Also rerun the chain-row tests, because `BlockInsertSlot` is shared.

Run: `nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_interaction --test chain_row_height_grows_with_streams --test chain_row_per_stream_full_row`

Then run: `nice -n 19 cargo test -p adapter-gui -j 2 --lib tr_key` (the four catalog guards, including `every_gui_tr_key_translated_in_every_locale`, which checks all nine locales)

Expected: `test result: ok. 14 passed` for the interaction binary, and `ok` for every other binary.

- [ ] **Step 9: Docs.** In `docs/gui/graph-view.md`, append to the Structs code block:

```slint
struct GraphAnchor {        // one "+" on a wire (graph_view_model::insert_anchors)
    id: string;             // "stage:{i}" | "lane:{stage}:{lane}:{i}"
    layout_x: length;       // wire midpoint
    layout_y: length;
    always_visible: bool;   // empty segment: "+" shown without hover
}
```

In the Properties table, add:

```markdown
| `anchors` | `in` | `[GraphAnchor]` | — | the "+" insert anchors, one per wire, drawn as the chain row's `BlockInsertSlot` without its track, over the nodes so a dragged card never hides the lit target |
```

In the Callbacks table, add:

```markdown
| `add-requested(string)` | anchor id | a "+" was clicked — the host opens the add-block picker for that slot |
| `node-dropped(string, string)` | node id, anchor id | a dragged block was released on an anchor; fired after `node_drag_ended` |
| `resolve-drop-anchor(string, length, length) -> string` | node id, layout x, y | `pure` — the host answers which anchor a drag at (x, y) lands on (`""` = none) by calling `graph_view_model::resolve_drop_anchor`; asked on every drag move so the target lights up |
```

In "Interactivity contract", add the bullet:

```markdown
- **Drop on an anchor:** while a block is dragged the canvas asks `resolve-drop-anchor` on every move and highlights that "+"; releasing there fires `node-dropped(node, anchor)`. A drop on the block's own wire, or on nothing, fires no `node-dropped` (the drag still ends).
```

- [ ] **Step 10: Pre-push gate.** Run the same gate commands as Task 1, Step 11.

- [ ] **Step 11: Commit and push.**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328
git -C $S add crates/adapter-gui/ui/components/graph_view_types.slint \
  crates/adapter-gui/ui/components/graph_view.slint \
  crates/adapter-gui/ui/components/block_insert_slot.slint \
  crates/adapter-gui/ui/components/graph_view_test_harness.slint \
  crates/adapter-gui/translations/adapter-gui.pot \
  crates/adapter-gui/translations/de_DE/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/en_US/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/es_ES/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/fr_FR/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/hi_IN/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/ja_JP/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/ko_KR/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/pt_BR/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/translations/zh_CN/LC_MESSAGES/adapter-gui.po \
  crates/adapter-gui/tests/issue_328_graph_view_interaction.rs docs/gui/graph-view.md
git -C $S commit -m "feat(#328): + anchors on graph wires; dragging a block onto one fires node-dropped"
git -C $S fetch && git -C $S push
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 5 task 10 pushed: $(git -C $S rev-parse --short HEAD). GraphAnchor + anchors loop (BlockInsertSlot show-track=false), add-requested, node-dropped via pure resolve-drop-anchor → graph_view_model::resolve_drop_anchor, anchors drawn over the nodes; interaction 14 green; workspace gate green."
```

---

## Task 11: Render fixtures, PNG review, docs, full gate

**Files:**
- Create: `crates/adapter-gui/ui/components/_harness_graph_view.slint` (render-only, not compiled into the app, like `_harness_channel_picker.slint`)
- Docs: `docs/gui/graph-view.md`, the "## Tests" section (originally lines 154-172; the rows and sections added in Tasks 2-10 moved it down, so find it by its heading)

**Interfaces:**
- Produces: the render components `LinearChain`, `LinearChainZoomedOut`, `SplitMixChain` and `SplitYChain`, for `tools/slint-render`.

- [ ] **Step 1: Invoke the UI skills.** Invoke `claude-plugin:ux-ui`, `slint:slint` and `slint-best-practices`, and follow them for the review in Step 4.

- [ ] **Step 2: Write the render fixtures.** Coordinates follow `GridMetrics::default()`: origin (80, 200), columns 160 apart, lanes 120 apart. Every anchor sits at its wire's midpoint, exactly as `insert_anchors` places it. All literals of one array share the same fields.

Create `crates/adapter-gui/ui/components/_harness_graph_view.slint`:

```slint
// Responsibility: hosts the graph view for a render check.
// #328 render harness — the three chain shapes the graph chain editor draws
// (linear, Split → Mix, Y → A/B) plus the linear one zoomed out, with fake
// data on the default grid (graph_view_model::GridMetrics: origin 80,200 ·
// column 160 · lane 120; anchors on wire midpoints). Not compiled into the
// app; rendered headlessly with tools/slint-render. The interpreter has no
// translation catalog, so @tr text renders as its key.
import { GraphView } from "graph_view.slint";
import "../fonts/BebasNeue.ttf";

// IN → DRIVE (selected, MIDI marker) → AMP (unavailable, neighbor marker)
// → REVERB (bypassed) → OUT.
export component LinearChain inherits Window {
    in property <float> zoom: 1.0;
    width: 900px;
    height: 420px;
    background: #0a0d12;
    default-font-family: "Bebas Neue";

    GraphView {
        width: 100%;
        height: 100%;
        zoom: root.zoom;
        markers_visible: true;
        nodes: [
            { id: "in", kind: "io_input", label: "IN 1", category: "input", fill: #1e8aff, border: #1359a5, layout_x: 80px, layout_y: 200px, bypass: false, selected: false, neighbor: false, block: { type_label: "", icon_kind: "", accent_color: #000000, display_name: "", unavailable: false } },
            { id: "od", kind: "block", label: "OD", category: "drive", fill: #00000000, border: #00000000, layout_x: 240px, layout_y: 200px, bypass: false, selected: true, neighbor: false, block: { type_label: "DRIVE", icon_kind: "gain", accent_color: #d04a3a, display_name: "Tube Screamer", unavailable: false } },
            { id: "amp", kind: "block", label: "AMP", category: "amp", fill: #00000000, border: #00000000, layout_x: 400px, layout_y: 200px, bypass: false, selected: false, neighbor: true, block: { type_label: "AMP", icon_kind: "amp", accent_color: #4a7fd0, display_name: "Clean Amp", unavailable: true } },
            { id: "rev", kind: "block", label: "REV", category: "reverb", fill: #00000000, border: #00000000, layout_x: 560px, layout_y: 200px, bypass: true, selected: false, neighbor: false, block: { type_label: "REVERB", icon_kind: "reverb", accent_color: #3aa86a, display_name: "Hall", unavailable: false } },
            { id: "out", kind: "io_output", label: "OUT 1-2", category: "output", fill: #ff8a1e, border: #a55913, layout_x: 720px, layout_y: 200px, bypass: false, selected: false, neighbor: false, block: { type_label: "", icon_kind: "", accent_color: #000000, display_name: "", unavailable: false } },
        ];
        edges: [
            { from_id: "in", to_id: "od", from_x: 80px, from_y: 200px, to_x: 240px, to_y: 200px },
            { from_id: "od", to_id: "amp", from_x: 240px, from_y: 200px, to_x: 400px, to_y: 200px },
            { from_id: "amp", to_id: "rev", from_x: 400px, from_y: 200px, to_x: 560px, to_y: 200px },
            { from_id: "rev", to_id: "out", from_x: 560px, from_y: 200px, to_x: 720px, to_y: 200px },
        ];
        anchors: [
            { id: "stage:1", layout_x: 160px, layout_y: 200px, always_visible: false },
            { id: "stage:2", layout_x: 320px, layout_y: 200px, always_visible: false },
            { id: "stage:3", layout_x: 480px, layout_y: 200px, always_visible: false },
            { id: "stage:4", layout_x: 640px, layout_y: 200px, always_visible: false },
        ];
    }
}

export component LinearChainZoomedOut inherits LinearChain {
    zoom: 0.5;
}

// IN → COMP → SPLIT → [AMP A → CAB A] ∥ [AMP B] → MIXER → REVERB → OUT.
export component SplitMixChain inherits Window {
    width: 1300px;
    height: 420px;
    background: #0a0d12;
    default-font-family: "Bebas Neue";

    GraphView {
        width: 100%;
        height: 100%;
        nodes: [
            { id: "in", kind: "io_input", label: "IN 1", category: "input", fill: #1e8aff, border: #1359a5, layout_x: 80px, layout_y: 200px, bypass: false, block: { type_label: "", icon_kind: "", accent_color: #000000, display_name: "" } },
            { id: "comp", kind: "block", label: "COMP", category: "dynamics", fill: #00000000, border: #00000000, layout_x: 240px, layout_y: 200px, bypass: false, block: { type_label: "DYN", icon_kind: "dynamics", accent_color: #c69b3f, display_name: "Compressor" } },
            { id: "__split_1", kind: "split", label: "", category: "util", fill: #6a7483, border: #444b55, layout_x: 400px, layout_y: 200px, bypass: false, block: { type_label: "", icon_kind: "", accent_color: #000000, display_name: "" } },
            { id: "amp_a", kind: "block", label: "AMP A", category: "amp", fill: #00000000, border: #00000000, layout_x: 560px, layout_y: 140px, bypass: false, block: { type_label: "AMP", icon_kind: "amp", accent_color: #4a7fd0, display_name: "Clean Amp" } },
            { id: "cab_a", kind: "block", label: "CAB A", category: "amp", fill: #00000000, border: #00000000, layout_x: 720px, layout_y: 140px, bypass: false, block: { type_label: "CAB", icon_kind: "cab", accent_color: #4a7fd0, display_name: "2x12" } },
            { id: "amp_b", kind: "block", label: "AMP B", category: "amp", fill: #00000000, border: #00000000, layout_x: 560px, layout_y: 260px, bypass: false, block: { type_label: "AMP", icon_kind: "amp", accent_color: #4a7fd0, display_name: "Lead Amp" } },
            { id: "__merge_1", kind: "mixer", label: "", category: "util", fill: #6a7483, border: #444b55, layout_x: 880px, layout_y: 200px, bypass: false, block: { type_label: "", icon_kind: "", accent_color: #000000, display_name: "" } },
            { id: "rev", kind: "block", label: "REV", category: "reverb", fill: #00000000, border: #00000000, layout_x: 1040px, layout_y: 200px, bypass: false, block: { type_label: "REVERB", icon_kind: "reverb", accent_color: #3aa86a, display_name: "Hall" } },
            { id: "out", kind: "io_output", label: "OUT 1-2", category: "output", fill: #ff8a1e, border: #a55913, layout_x: 1200px, layout_y: 200px, bypass: false, block: { type_label: "", icon_kind: "", accent_color: #000000, display_name: "" } },
        ];
        edges: [
            { from_id: "in", to_id: "comp", from_x: 80px, from_y: 200px, to_x: 240px, to_y: 200px },
            { from_id: "comp", to_id: "__split_1", from_x: 240px, from_y: 200px, to_x: 400px, to_y: 200px },
            { from_id: "__split_1", to_id: "amp_a", from_x: 400px, from_y: 200px, to_x: 560px, to_y: 140px },
            { from_id: "amp_a", to_id: "cab_a", from_x: 560px, from_y: 140px, to_x: 720px, to_y: 140px },
            { from_id: "cab_a", to_id: "__merge_1", from_x: 720px, from_y: 140px, to_x: 880px, to_y: 200px },
            { from_id: "__split_1", to_id: "amp_b", from_x: 400px, from_y: 200px, to_x: 560px, to_y: 260px },
            { from_id: "amp_b", to_id: "__merge_1", from_x: 560px, from_y: 260px, to_x: 880px, to_y: 200px },
            { from_id: "__merge_1", to_id: "rev", from_x: 880px, from_y: 200px, to_x: 1040px, to_y: 200px },
            { from_id: "rev", to_id: "out", from_x: 1040px, from_y: 200px, to_x: 1200px, to_y: 200px },
        ];
        anchors: [
            { id: "stage:1", layout_x: 160px, layout_y: 200px, always_visible: false },
            { id: "stage:2", layout_x: 320px, layout_y: 200px, always_visible: false },
            { id: "lane:2:0:0", layout_x: 480px, layout_y: 170px, always_visible: false },
            { id: "lane:2:0:1", layout_x: 640px, layout_y: 140px, always_visible: false },
            { id: "lane:2:0:2", layout_x: 800px, layout_y: 170px, always_visible: false },
            { id: "lane:2:1:0", layout_x: 480px, layout_y: 230px, always_visible: false },
            { id: "lane:2:1:1", layout_x: 720px, layout_y: 230px, always_visible: false },
            { id: "stage:3", layout_x: 960px, layout_y: 200px, always_visible: false },
            { id: "stage:4", layout_x: 1120px, layout_y: 200px, always_visible: false },
        ];
    }
}

// IN → COMP → SPLIT → [AMP A → CAB A → OUT A] ∥ [OUT B]: path B is empty,
// so its "+" is always visible.
export component SplitYChain inherits Window {
    width: 1000px;
    height: 420px;
    background: #0a0d12;
    default-font-family: "Bebas Neue";

    GraphView {
        width: 100%;
        height: 100%;
        nodes: [
            { id: "in", kind: "io_input", label: "IN 1", category: "input", fill: #1e8aff, border: #1359a5, layout_x: 80px, layout_y: 200px, block: { type_label: "", icon_kind: "", accent_color: #000000, display_name: "" } },
            { id: "comp", kind: "block", label: "COMP", category: "dynamics", fill: #00000000, border: #00000000, layout_x: 240px, layout_y: 200px, block: { type_label: "DYN", icon_kind: "dynamics", accent_color: #c69b3f, display_name: "Compressor" } },
            { id: "__split_1", kind: "split", label: "", category: "util", fill: #6a7483, border: #444b55, layout_x: 400px, layout_y: 200px, block: { type_label: "", icon_kind: "", accent_color: #000000, display_name: "" } },
            { id: "amp_a", kind: "block", label: "AMP A", category: "amp", fill: #00000000, border: #00000000, layout_x: 560px, layout_y: 140px, block: { type_label: "AMP", icon_kind: "amp", accent_color: #4a7fd0, display_name: "Clean Amp" } },
            { id: "cab_a", kind: "block", label: "CAB A", category: "amp", fill: #00000000, border: #00000000, layout_x: 720px, layout_y: 140px, block: { type_label: "CAB", icon_kind: "cab", accent_color: #4a7fd0, display_name: "2x12" } },
            { id: "out_a", kind: "io_output", label: "OUT 1", category: "output", fill: #ff8a1e, border: #a55913, layout_x: 880px, layout_y: 140px, block: { type_label: "", icon_kind: "", accent_color: #000000, display_name: "" } },
            { id: "out_b", kind: "io_output", label: "OUT 2", category: "output", fill: #ff8a1e, border: #a55913, layout_x: 880px, layout_y: 260px, block: { type_label: "", icon_kind: "", accent_color: #000000, display_name: "" } },
        ];
        edges: [
            { from_id: "in", to_id: "comp", from_x: 80px, from_y: 200px, to_x: 240px, to_y: 200px },
            { from_id: "comp", to_id: "__split_1", from_x: 240px, from_y: 200px, to_x: 400px, to_y: 200px },
            { from_id: "__split_1", to_id: "amp_a", from_x: 400px, from_y: 200px, to_x: 560px, to_y: 140px },
            { from_id: "amp_a", to_id: "cab_a", from_x: 560px, from_y: 140px, to_x: 720px, to_y: 140px },
            { from_id: "cab_a", to_id: "out_a", from_x: 720px, from_y: 140px, to_x: 880px, to_y: 140px },
            { from_id: "__split_1", to_id: "out_b", from_x: 400px, from_y: 200px, to_x: 880px, to_y: 260px },
        ];
        anchors: [
            { id: "stage:1", layout_x: 160px, layout_y: 200px, always_visible: false },
            { id: "stage:2", layout_x: 320px, layout_y: 200px, always_visible: false },
            { id: "lane:2:0:0", layout_x: 480px, layout_y: 170px, always_visible: false },
            { id: "lane:2:0:1", layout_x: 640px, layout_y: 140px, always_visible: false },
            { id: "lane:2:0:2", layout_x: 800px, layout_y: 140px, always_visible: false },
            { id: "lane:2:1:0", layout_x: 640px, layout_y: 230px, always_visible: true },
        ];
    }
}
```

- [ ] **Step 3: Render.**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328
mkdir -p target/graph-view-png
F=crates/adapter-gui/ui/components/_harness_graph_view.slint
nice -n 19 cargo run --release -j 2 --manifest-path tools/slint-render/Cargo.toml -- $F LinearChain target/graph-view-png/linear.png 900 420
nice -n 19 cargo run --release -j 2 --manifest-path tools/slint-render/Cargo.toml -- $F LinearChainZoomedOut target/graph-view-png/linear-zoom-0.5.png 900 420
nice -n 19 cargo run --release -j 2 --manifest-path tools/slint-render/Cargo.toml -- $F SplitMixChain target/graph-view-png/split-mix.png 1300 420
nice -n 19 cargo run --release -j 2 --manifest-path tools/slint-render/Cargo.toml -- $F SplitYChain target/graph-view-png/split-y.png 1000 420
```

Expected: each command exits 0 and writes its PNG. Exit 1 means a compile error, which the tool prints to stderr.

- [ ] **Step 4: Read each PNG and self-critique before going further** (the `openrig-code-quality` UI rule). Open every PNG with the Read tool and check:

1. **linear.png**
   1. Five cards on one row, joined by wires. The wires stop at the card edges, because block cards have an opaque backdrop.
   2. The IN and OUT cards show the connector artwork above "IN 1" / "OUT 1-2".
   3. The DRIVE card shows a red type label, the gain icon, a lit LED, and an amber 2px marker border (selected, markers visible).
   4. The AMP card is at half opacity, with an amber type label and icon (unavailable) and a light 1px marker border (neighbor).
   5. The REVERB card shows a grey type label and icon and an unlit LED (bypassed).
   6. No "+" is visible at rest: no segment is empty.
2. **linear-zoom-0.5.png**: the cards and wires are halved, the labels elide instead of overflowing, and nothing overlaps.
3. **split-mix.png**
   1. The split card (`graph-split.svg`, light on the util fill, no baked-in text, with the `graph-node-split` key as its text) fans out to two lanes: AMP A → CAB A on top and AMP B below.
   2. Both lanes meet at the mixer card (`graph-mix.svg`), which is followed by REVERB and OUT on the centre row.
   3. Path B's wire runs from AMP B to the mixer without crossing any card.
4. **split-y.png**
   1. There is no mixer. OUT 1 and OUT 2 share the last column, with path A on top.
   2. Path B's wire runs from the split to OUT 2, and its "+" disc is visible at the wire midpoint (empty path).
5. **Every PNG**
   1. Clear hierarchy: the block type label is the strongest text.
   2. Semantic colours: bypass, unavailable and selection are distinguishable.
   3. The CRUD affordances exist: "+" to add, clicking a card to edit, the LED, and the × (hover only, proven by the Task 9 interaction test).
   4. No text is under 18px.
   5. The grid spacing is consistent.
   6. The always-visible "+" discs (drawn over the nodes) sit in the gaps between cards and touch no card at zoom 1.

If any check fails, fix the `.slint`, rerun `nice -n 19 cargo test -p adapter-gui -j 2 --test issue_328_graph_view_interaction --test issue_328_graph_view_sources` (the fix must not break a gesture), and render again. Go on only when every check holds.

- [ ] **Step 5: Docs.** In `docs/gui/graph-view.md`, replace the whole "## Tests" section (from its heading up to, not including, "## Future work") with:

```markdown
## Tests

Rust model — `crates/adapter-gui/src/graph_view_model_tests.rs`, `graph_view_model_anchor_tests.rs`, `graph_view_model_drop_tests.rs` (`cargo test -p adapter-gui --lib graph_view_model`):

- stage layout: singles; merge parallels (split + merge nodes, symmetric lanes, merge column); fan parallels (no merge node, one terminal per lane on a shared last column, path A above path B)
- `validate_stages` (nothing after a fan, no empty fan lane) and `validate_graph` (duplicate ids, dangling edges, self-loops; layout output always valid)
- node kinds (`as_str` slugs, a blueprint's kind reaches its node, split/mixer kinds)
- auto layout (ranks, lanes, reorder, fan terminals aligned)
- anchors (one per wire, slots, midpoints, empty segments visible)
- drop resolution (nearest anchor, cross-lane, own wire = none, blocks only, reach)

Slint — REAL pointer, wheel and key events through `i-slint-backend-testing` against `GraphViewHarness` / `GraphViewScrollHarness` (`ui/components/graph_view_test_harness.slint`), in `crates/adapter-gui/tests/issue_328_graph_view_interaction.rs`:

- click vs drag; every node kind is a clickable card
- a plain wheel scrolls the surrounding list, Cmd/Ctrl + wheel zooms
- the LED toggles bypass, the × removes (hover only), both hit zones scale with the zoom, the tooltip shows on hover, an unavailable block reads as disabled
- a "+" fires `add-requested`; a block dragged onto another lane's anchor fires `node-dropped`; a drop on its own wire fires nothing

Source pins — `tests/issue_328_graph_view_sources.rs`: the file split, no empty-label routing dots, translated accessible labels, text-free split/mixer artwork, block tile colours from `BlockTileStyle`.

Render — `ui/components/_harness_graph_view.slint` (not compiled into the app): `LinearChain`, `LinearChainZoomedOut`, `SplitMixChain`, `SplitYChain`, rendered with `tools/slint-render` (command in the `openrig-tooling` skill). The interpreter has no translation catalog, so `@tr` text renders as its key.
```

- [ ] **Step 6: Full gate.** Run every item before the push.

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328
cargo fmt --all -- --check
nice -n 19 cargo test --workspace -j 2 2>&1 | tee target/ppg-test.log | grep -E "^test result|FAILED|^error" | tail -60
grep -c '^warning' target/ppg-test.log    # expected: 0
grep -E "ignored" target/ppg-test.log | grep -v " 0 ignored"    # expected: empty
nice -n 19 cargo build --workspace -j 2 2>&1 | grep -c '^warning'    # expected: 0
VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates    # expected: VALIDATE PASSED
git -C /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328 status --porcelain crates/engine/src/volume_invariants_tests.rs    # expected: empty (Part 5 never touches it)
```

- [ ] **Step 7: Commit and push.**

```bash
S=/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-328
git -C $S add crates/adapter-gui/ui/components/_harness_graph_view.slint docs/gui/graph-view.md
git -C $S commit -m "feat(#328): render fixtures for linear, Split → Mix and Y → A/B graphs; graph view test docs"
git -C $S fetch && git -C $S push
gh issue comment 328 --repo jpfaria/OpenRig --body "Part 5 done: $(git -C $S rev-parse --short HEAD). GraphView component ready for the chain row (Part 6): kinds, BlockChip-parity cards, + anchors, drop resolution, Cmd/Ctrl zoom. PNGs reviewed (linear, zoom 0.5, split-mix, split-y). Full gate green: fmt, cargo test --workspace, build 0 warnings, validate.sh crates."
```

Part 5 needs no validation checklist from the owner: the component is not reachable in the running app until Part 6 puts it in `ChainRow`. The owner's visual loop happens there.
