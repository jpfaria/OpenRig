# GraphView — node-and-edge canvas with pan/zoom/drag

**Status:** introduced in #435.
**Source:** `crates/adapter-gui/ui/components/graph_view.slint` + `crates/adapter-gui/src/graph_view_model.rs`.
**Demo:** `cargo run -p graphview-demo` (own crate — keeps adapter-gui free of demo build weight).

A reusable Slint component for rendering a directed graph with full interactivity. Built first as a standalone primitive — integration with the existing chain UI (`secondary_windows_chain.slint`, `chain_chips.slint`) is a separate effort and not part of this component.

The visual language follows pedalboards in the **Helix / Quad Cortex / Mooer GE1000** family: grid-aligned nodes, explicit Bézier wires, parallel paths drawn on dedicated lanes. Not force-directed — signal chains are too structured for spring physics to give a stable result across reopens.

## When to use it

| You want… | Component? |
|---|---|
| Single signal chain with up to ~30 blocks, branched paths | **Yes** |
| Project topology (inputs → chains → outputs) | **Yes** (once #436 lands and you can feed it the project graph) |
| Arbitrary graph with hundreds of nodes, force-directed | Out of scope. Different layout algo, different perf budget. |
| Static diagram for docs | Overkill. Render to SVG offline. |

## Architecture

```
                ┌─ Rust ───────────────────────────────────────┐
                │                                              │
  domain  ───►  │  graph_view_model.rs                         │
   data         │   ChainStage[]  → linear_chain_layout()      │
                │   → (Vec<GraphNode>, Vec<GraphEdge>)         │
                │   → validate_graph()                         │
                │                                              │
                │  wiring code (per use site)                  │
                │   converts pure Rust → Slint structs         │
                │   resolves GraphEdge → GraphEdgeGeometry     │
                └──────────────────────────────────────────────┘
                                       │
                                       ▼ ModelRc<...>
                ┌─ Slint ──────────────────────────────────────┐
                │  GraphView component                         │
                │   renders nodes + Bezier wires               │
                │   handles pan/zoom/drag/click                │
                │   emits callbacks with layout-space coords   │
                └──────────────────────────────────────────────┘
```

**Two coordinate systems:**

| Space | Where | Computed how |
|---|---|---|
| Layout space | `GraphNode.layout_x/y`, `GraphEdgeGeometry.from/to_x/y`, all callback args | `linear_chain_layout()` (or by the host) |
| Viewport space | `x: pan_x + layout_x * zoom` inside Slint | applied per frame by the component |

The host **never** computes viewport coords — only emits layout coords. The component applies the transform.

## Slint API

### Structs

```slint
struct GraphNode {
    id: string;
    label: string;
    category: string;     // "drive", "amp", "reverb", "util", ...
    layout_x: length;
    layout_y: length;
    bypass: bool;
    selected: bool;
}

struct GraphEdgeGeometry {
    from_id: string;
    to_id: string;
    from_x: length;
    from_y: length;
    to_x: length;
    to_y: length;
}
```

`GraphEdgeGeometry` is the resolved form of an edge — coordinates are already looked up from the node list so Slint doesn't search per frame.

### Properties

| Property | Direction | Type | Default | Purpose |
|---|---|---|---|---|
| `nodes` | `in` | `[GraphNode]` | — | nodes to render |
| `edges` | `in` | `[GraphEdgeGeometry]` | — | resolved edges to render |
| `zoom` | `in-out` | `float` | `1.0` | viewport scale, clamped to `[min_zoom, max_zoom]` |
| `pan_x`, `pan_y` | `in-out` | `length` | `0px` | viewport offset |
| `node_width`, `node_height` | `in` | `length` | `96px` × `56px` | per-node card size in layout space |
| `background_color` | `in` | `color` | `#11141a` | canvas background |
| `grid_color` | `in` | `color` | `#1a1f2a` | grid hint colour |
| `show_grid` | `in` | `bool` | `true` | render origin-cross grid hint |
| `min_zoom`, `max_zoom` | `in` | `float` | `0.3`, `3.0` | zoom limits |

### Callbacks

| Callback | Args | When |
|---|---|---|
| `node_clicked(string)` | node id | press + release within 5 px (no drag) |
| `node_double_clicked(string)` | node id | double click on a node |
| `node_dragged(string, length, length)` | id, new layout-space x, y | continuously while drag in progress |
| `node_drag_ended(string, length, length)` | id, layout x, y | on mouse up after drag |
| `viewport_changed(float, length, length)` | zoom, pan_x, pan_y | after pan release or wheel zoom step |

The host receives layout-space coords. To persist a moved node, write them back into `nodes` — the Slint side reads positions reactively.

## Rust helpers (`graph_view_model`)

| Item | What it does |
|---|---|
| `GraphNode` | pure Rust mirror of the Slint struct |
| `GraphEdge` | source/target id pair (no geometry) |
| `NodeCategory` | enum of visual categories — `as_str()` produces the slug the Slint side expects |
| `BlockBlueprint` | one block in a logical chain — id, label, category, bypass |
| `ChainStage` | `Single(...)` or `Parallel(Vec<Vec<...>>)` |
| `GridMetrics` | column/lane spacing + origin |
| `linear_chain_layout(stages, metrics)` | builds positioned nodes + edges, inserts split/merge utility nodes for parallel stages |
| `validate_graph(nodes, edges)` | returns error strings (empty = valid). Catches duplicate ids, dangling edges, self-loops. |

`linear_chain_layout` is pure — same input, same output. Used in tests + at runtime to compute positions from a logical chain description. Splits and merges are auto-generated with id prefix `__split_N` / `__merge_N`.

## Category → colour mapping

Centralised in the Slint `CategoryPalette` component (single source of truth). Adding a new category:

1. Add a variant to `NodeCategory` and its `as_str()` slug.
2. Add one ternary arm in `CategoryPalette` in `graph_view.slint`.

No other file touches. This is the **only authorised** brand-of-conditional in this component (an exception explicit in `slint-best-practices`: Slint cannot select assets/colours via runtime strings without a ternary chain).

## Interactivity contract

- **Pan:** drag on empty canvas. Cursor turns `grab`. Released → fires `viewport_changed`.
- **Zoom:** scroll wheel anywhere over the canvas. Zooms around the cursor (the point under the cursor stays fixed in layout space). Clamped to `[min_zoom, max_zoom]`. Fires `viewport_changed`.
- **Drag a node:** press on a node card, move beyond 5 px. Fires `node_dragged` continuously, `node_drag_ended` on release.
- **Click vs drag:** total displacement < 5 px in viewport space → `node_clicked`. Threshold is a `private property` so it can be retuned without changing the API.
- **Double-click:** fires `node_double_clicked` — host opens the block editor.

## What this component is NOT responsible for

| Concern | Where it lives instead |
|---|---|
| Persisting moved nodes | host (write back into `nodes` on `node_drag_ended`) |
| Block editor | existing `BlockEditorPanel` — host wires `node_double_clicked` to it |
| Real-time audio meters on nodes | future `GraphNodeMeter` overlay component |
| Edge routing avoidance (no crossings) | future — current Bézier is naive |
| Touch gestures (pinch-to-zoom) | future — scroll-wheel only for now |

## Invariants

- **Audio thread:** untouched. This is pure UI. ✅ invariant #4/#8 of `CLAUDE.md`.
- **Allocations per frame:** none — `for` loops bind to the model, no `set_*` in render path.
- **Cross-platform:** no `cfg`-gated logic, no hardcoded paths. ✅
- **No mock audio:** this component doesn't process audio. N/A.

## Tests

Layout helper has 17 tests in `crates/adapter-gui/src/graph_view_model_tests.rs` covering:

- Empty input → empty output
- Single block → 1 node, 0 edges
- Sequential blocks connected left-to-right
- Column spacing math (origin + N × column_spacing)
- Parallel stage inserts split + merge nodes
- Symmetric lane offsets (N paths → offsets `-N/2..N/2`)
- Split connects to each path's first block
- Each path's last block connects to merge
- Merge column accounts for longest path
- Singles after parallel continue past merge
- Empty parallel stage is a no-op
- `validate_graph` reports duplicate ids, dangling edges, self-loops
- The output of `linear_chain_layout` is always valid

Visual behaviour (drag, zoom, click thresholds) is validated by running the demo example. There is no automated UI test harness for Slint at this time — that's a project-wide gap, not specific to this component.

## Demo

```bash
cargo run -p graphview-demo
```

Opens a window with a hardcoded chain (Noise Gate → Compressor → Overdrive → split → 2× Vox AC30 → 2× delays → merge → Shimmer Reverb → Output). Click a node to select, double-click to log "would open editor", drag to reposition, scroll to zoom, drag empty space to pan.

## Future work

Captured here so the next contributor doesn't reinvent it:

- **Routing avoidance:** Bézier wires currently cross when paths zig-zag. Use a layered routing algorithm (Manhattan or orthogonal-with-bends).
- **Keyboard navigation:** Tab/arrow to move focus between nodes, Enter for click, Shift+Enter for double-click. `petgraph` can supply BFS/DFS over the graph if we add it as a dep.
- **Persist viewport** between sessions per chain.
- **Meter overlays** on amp/dynamics nodes for live signal level.
- **Multi-select** with drag-rectangle for bulk operations.
- **Snap-to-grid** during drag.
