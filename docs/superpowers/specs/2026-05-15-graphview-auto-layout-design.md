# GraphView — Auto-Layout & Customizable Palette

**Date:** 2026-05-15
**Issue:** #435 (component-level increment)
**Scope:** GraphView component only — its Rust half (`graph_view_model.rs`) and Slint half (`graph_view.slint`). **No system core, no chain model, no engine, no other issues.**

## Goal

The GraphView component must be *malleable but always tidy*. Today nodes sit
wherever the host puts them and free-drag leaves the graph messy. Add:

1. A topology-driven auto-layout the component computes itself from the edges.
2. Drag in auto mode that **reorders** a node and recomputes the tidy grid.
3. Fully host-customizable node colors (no hardcoded palette in the component).

All logic lives in the testable Rust model layer. Slint stays a pure renderer
(project rule: "tela não tem regra de negócio").

## Non-Goals

- Changing the chain data model, routing engine, or any non-component code.
- Editing graph topology by drag (creating/removing edges/splits). Edges are
  owned by the host and immutable from the component's perspective.
- Persisting user-edited positions (auto mode recomputes; free mode unchanged).
- Sugiyama-grade crossing minimization (single barycenter pass only).

## Architecture

### 1. Topological auto-layout — `graph_view_model.rs`

`pub fn topological_layout(nodes: &[GraphNode], edges: &[GraphEdge], metrics: GridMetrics) -> Vec<GraphNode>`

- Build incoming/outgoing adjacency from `edges`.
- **Column (rank)** = longest path from any source (in-degree 0), via Kahn
  topological order: `rank[v] = max(rank[pred] + 1)`, sources at rank 0.
- **Cycle / malformed safety:** if a cycle is detected (should never happen for
  a signal chain) or the graph is invalid per `validate_graph`, fall back to
  input-order ranking. Never panic — preserves the existing panic-free contract.
- **Lane (y)** per rank: order the nodes sharing a rank by the *barycenter* =
  mean lane of their predecessors (single left→right pass). Tiebreak by original
  input index for determinism. Center each rank's lane band symmetrically around
  `metrics.origin_y` (same convention as today's ±0.5 offsets).
- Returns positioned `GraphNode`s: `x = origin_x + rank*column_spacing`,
  `y = origin_y + lane_offset*lane_spacing`.

### 2. Drag-reorder on drop — `graph_view_model.rs`

`pub fn reorder_for_drop(nodes: &[GraphNode], edges: &[GraphEdge], dragged_id: &str, drop_x: f32, drop_y: f32, metrics: GridMetrics) -> Vec<GraphNode>`

- Map `drop_x` → nearest rank, `drop_y` → target slot among that rank's nodes.
- Re-key the dragged node's ordering tiebreak so a re-run of
  `topological_layout` places it at the dropped slot. Edges unchanged.
- Because rank is edge-derived and edges are immutable, dropping a node onto a
  different column does **not** move it to that column — it snaps back to its
  topological column at the dropped *lane order*. This is intentional: the graph
  stays a faithful, tidy picture of the actual topology; the malleable part is
  the vertical ordering of siblings, not the wiring.
- Returns the freshly laid-out node list. Idempotent: re-dropping in place is a
  no-op.

### 3. Config flag — `graph_view.slint`

- `in property <bool> auto_layout: true;` on `GraphView` (documentational on the
  Slint side; the host owns the algorithm).
- Demo exposes a checkbox toggling it. When on: host runs `topological_layout`
  initially and `reorder_for_drop` on `node_drag_ended`; `node_dragged` keeps
  the free-follow visual feedback, snap happens on drop. When off: free drag as
  it works today.

### 4. Customizable palette — `graph_view.slint` + `graph_view_model.rs`

- New Slint struct `CategoryColor { category: string, fill: color, border: color }`.
- `in property <[CategoryColor]> palette;` on `GraphView`.
- `CategoryPalette` looks the category up in `root.palette`; on miss, falls back
  to ONE neutral default constant (no per-category ternary tree — single source
  of truth).
- Rust helper `pub fn default_palette() -> Vec<(/*category*/&str,/*fill*/u32,/*border*/u32)>`
  (or equivalent typed struct) in the model layer: the default lives **once** in
  Rust, host gets sane defaults and can fully override.

## Data Flow

```
host nodes+edges ──> topological_layout() ──> positioned nodes ──> Slint render
        ▲                                                              │
        │                                                       node_drag_ended
        └──────────── reorder_for_drop() ◀─────────────────────────────┘
host palette (or default_palette()) ──> GraphView.palette ──> CategoryPalette lookup
```

## Testing

Pure Rust unit tests in `graph_view_model_tests.rs` (no Slint UI tests):

- **Rank:** linear chain → ranks `0..n`; diamond split/merge → correct ranks;
  longest path wins when paths differ in length.
- **Lane:** parallel siblings get distinct symmetric lanes; barycenter reduces
  an obvious crossing; deterministic under tiebreak.
- **reorder_for_drop:** dropping changes lane order; idempotent; never panics on
  empty / duplicate / cyclic input (reuse `validate_graph`).
- **palette:** `default_palette()` covers every `NodeCategory` variant; lookup
  fallback returns the neutral default.

Visual validation via the demo + user screenshot (component render can't be
unit-tested; project convention).

## Constraints / Invariants

- Pure UI. No audio thread, no DSP, no per-frame allocation (layout computed on
  drop, not per frame; demo model already mutated in place).
- File caps: `graph_view_model.rs` ~321 → ~470 lines (< 600 Rust cap OK);
  `graph_view.slint` near the 500 cap — split if it crosses during impl.
- Zero coupling: component still knows nothing about amps/drives/chains. Palette
  and topology are generic graph concepts.

## Risk

- Barycenter lane heuristic is the only non-trivial algorithm; bounded to a
  single pass, fully unit-tested, deterministic. Acceptable.
