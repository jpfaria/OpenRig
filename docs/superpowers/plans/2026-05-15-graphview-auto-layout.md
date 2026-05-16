# GraphView Auto-Layout & Customizable Palette Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a topology-driven auto-layout, drag-to-reorder, and a host-customizable color palette to the GraphView component, all in the testable Rust model layer.

**Architecture:** `graph_view_model.rs` gains pure functions (`topological_layout`, `reorder_for_drop`, `default_palette`). `graph_view.slint` swaps its hardcoded category ternary for a host-supplied `[CategoryColor]` and adds an `auto_layout` flag. The demo wires the toggle. No system core, no chain model, no engine.

**Tech Stack:** Rust (std only), Slint, cargo test.

---

## File Structure

- Modify: `crates/adapter-gui/src/graph_view_model.rs` (~321 → ~480 lines, < 600 cap) — add `CategoryStyle`, `default_palette`, `topological_layout`, `reorder_for_drop`.
- Modify: `crates/adapter-gui/src/graph_view_model_tests.rs` — new test modules.
- Modify: `crates/adapter-gui/ui/components/graph_view.slint` — `CategoryColor` struct, `palette` + `auto_layout` props, palette lookup.
- Modify: `crates/adapter-gui/examples/graph_view_demo.rs` — supply default palette, auto toggle, call new fns.

All commands run from `.solvers/issue-435/`.

---

## Task 1: CategoryStyle + default_palette

**Files:**
- Modify: `crates/adapter-gui/src/graph_view_model.rs`
- Test: `crates/adapter-gui/src/graph_view_model_tests.rs`

- [ ] **Step 1: Write the failing test**

Add to `graph_view_model_tests.rs` after the `node_category` module:

```rust
mod palette {
    use super::*;
    use crate::graph_view_model::{default_palette, NodeCategory};

    #[test]
    fn default_palette_covers_every_category() {
        let pal = default_palette();
        for cat in [
            NodeCategory::Input, NodeCategory::Output, NodeCategory::Dynamics,
            NodeCategory::Drive, NodeCategory::Amp, NodeCategory::Modulation,
            NodeCategory::Time, NodeCategory::Reverb, NodeCategory::Eq,
            NodeCategory::Util, NodeCategory::Other,
        ] {
            assert!(
                pal.iter().any(|s| s.category == cat.as_str()),
                "missing palette entry for {}",
                cat.as_str()
            );
        }
    }

    #[test]
    fn default_palette_border_is_darker_than_fill() {
        for s in default_palette() {
            assert!(
                s.border <= s.fill,
                "border 0x{:06x} not darker than fill 0x{:06x} for {}",
                s.border, s.fill, s.category
            );
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p adapter-gui --lib palette:: 2>&1 | tail -5`
Expected: FAIL — `default_palette` not found.

- [ ] **Step 3: Write minimal implementation**

Add to `graph_view_model.rs` (after `NodeCategory` impl):

```rust
/// Visual style for a node category. The component is colour-agnostic;
/// the host supplies a palette (or uses [`default_palette`]). Colours are
/// `0xRRGGBB`. Single source of truth — the default lives here only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CategoryStyle {
    pub category: &'static str,
    pub fill: u32,
    pub border: u32,
}

fn darken(rgb: u32, factor: f32) -> u32 {
    let r = ((rgb >> 16) & 0xff) as f32 * (1.0 - factor);
    let g = ((rgb >> 8) & 0xff) as f32 * (1.0 - factor);
    let b = (rgb & 0xff) as f32 * (1.0 - factor);
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Default palette covering every [`NodeCategory`]. Host may override
/// fully; this is just sane defaults.
pub fn default_palette() -> Vec<CategoryStyle> {
    [
        (NodeCategory::Input, 0x1e8aff),
        (NodeCategory::Output, 0xff8a1e),
        (NodeCategory::Dynamics, 0xc69b3f),
        (NodeCategory::Drive, 0xd04a3a),
        (NodeCategory::Amp, 0x4a7fd0),
        (NodeCategory::Modulation, 0x8d4ad0),
        (NodeCategory::Time, 0x2d9b9b),
        (NodeCategory::Reverb, 0x3aa86a),
        (NodeCategory::Eq, 0xc8b400),
        (NodeCategory::Util, 0x6a7483),
        (NodeCategory::Other, 0x8a94a2),
    ]
    .into_iter()
    .map(|(cat, fill)| CategoryStyle {
        category: cat.as_str(),
        fill,
        border: darken(fill, 0.35),
    })
    .collect()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p adapter-gui --lib palette:: 2>&1 | tail -5`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/adapter-gui/src/graph_view_model.rs crates/adapter-gui/src/graph_view_model_tests.rs
git commit -m "feat(435): CategoryStyle + default_palette (single-source colours)"
```

---

## Task 2: topological_layout — rank (column) assignment

**Files:**
- Modify: `crates/adapter-gui/src/graph_view_model.rs`
- Test: `crates/adapter-gui/src/graph_view_model_tests.rs`

- [ ] **Step 1: Write the failing test**

```rust
mod topological_rank {
    use super::*;
    use crate::graph_view_model::{topological_layout, GraphEdge};

    fn n(id: &str) -> GraphNode {
        GraphNode { id: id.into(), label: id.into(),
            category: NodeCategory::Other, x: 0.0, y: 0.0, bypass: false }
    }
    fn e(a: &str, b: &str) -> GraphEdge {
        GraphEdge { from_id: a.into(), to_id: b.into() }
    }

    #[test]
    fn linear_chain_ranks_left_to_right() {
        let m = GridMetrics { origin_x: 0.0, origin_y: 0.0,
            column_spacing: 100.0, lane_spacing: 50.0 };
        let nodes = vec![n("a"), n("b"), n("c")];
        let edges = vec![e("a", "b"), e("b", "c")];
        let out = topological_layout(&nodes, &edges, m);
        let get = |id| out.iter().find(|x| x.id == id).unwrap().x;
        assert_eq!(get("a"), 0.0);
        assert_eq!(get("b"), 100.0);
        assert_eq!(get("c"), 200.0);
    }

    #[test]
    fn diamond_longest_path_wins() {
        // a -> b -> d ; a -> d  : d must be at rank 2 (via b), not 1.
        let m = GridMetrics { origin_x: 0.0, origin_y: 0.0,
            column_spacing: 100.0, lane_spacing: 50.0 };
        let nodes = vec![n("a"), n("b"), n("d")];
        let edges = vec![e("a", "b"), e("b", "d"), e("a", "d")];
        let out = topological_layout(&nodes, &edges, m);
        let get = |id| out.iter().find(|x| x.id == id).unwrap().x;
        assert_eq!(get("d"), 200.0);
    }

    #[test]
    fn cycle_falls_back_to_input_order_no_panic() {
        let m = GridMetrics::default();
        let nodes = vec![n("a"), n("b")];
        let edges = vec![e("a", "b"), e("b", "a")];
        let out = topological_layout(&nodes, &edges, m); // must not panic
        assert_eq!(out.len(), 2);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p adapter-gui --lib topological_rank:: 2>&1 | tail -5`
Expected: FAIL — `topological_layout` not found.

- [ ] **Step 3: Write minimal implementation**

Add to `graph_view_model.rs`:

```rust
/// Auto-layout from graph topology. Column = longest path from a source
/// (in-degree 0). Lane assignment is added in a later step; for now all
/// nodes sit on the centre lane. Panic-free: a cycle or malformed graph
/// falls back to input-order ranking.
pub fn topological_layout(
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    metrics: GridMetrics,
) -> Vec<GraphNode> {
    let ids: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let mut indeg: HashMap<&str, usize> = ids.iter().map(|i| (*i, 0)).collect();
    let mut outs: HashMap<&str, Vec<&str>> = ids.iter().map(|i| (*i, vec![])).collect();
    for ed in edges {
        let (f, t) = (ed.from_id.as_str(), ed.to_id.as_str());
        if indeg.contains_key(f) && indeg.contains_key(t) {
            *indeg.get_mut(t).unwrap() += 1;
            outs.get_mut(f).unwrap().push(t);
        }
    }
    let mut rank: HashMap<&str, usize> = ids.iter().map(|i| (*i, 0)).collect();
    let mut queue: Vec<&str> =
        ids.iter().filter(|i| indeg[*i] == 0).copied().collect();
    let mut processed = 0usize;
    let mut indeg_w = indeg.clone();
    let mut head = 0;
    while head < queue.len() {
        let u = queue[head];
        head += 1;
        processed += 1;
        for &v in &outs[u] {
            let nr = rank[u] + 1;
            if nr > rank[v] {
                *rank.get_mut(v).unwrap() = nr;
            }
            let d = indeg_w.get_mut(v).unwrap();
            *d -= 1;
            if *d == 0 {
                queue.push(v);
            }
        }
    }
    if processed < ids.len() {
        // Cycle / malformed: degrade to input order, never panic.
        return nodes
            .iter()
            .enumerate()
            .map(|(i, src)| GraphNode {
                x: metrics.origin_x + i as f32 * metrics.column_spacing,
                y: metrics.origin_y,
                ..src.clone()
            })
            .collect();
    }
    nodes
        .iter()
        .map(|src| GraphNode {
            x: metrics.origin_x + rank[src.id.as_str()] as f32 * metrics.column_spacing,
            y: metrics.origin_y,
            ..src.clone()
        })
        .collect()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p adapter-gui --lib topological_rank:: 2>&1 | tail -5`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/adapter-gui/src/graph_view_model.rs crates/adapter-gui/src/graph_view_model_tests.rs
git commit -m "feat(435): topological_layout rank (longest-path columns, cycle-safe)"
```

---

## Task 3: topological_layout — lane (barycenter) assignment

**Files:**
- Modify: `crates/adapter-gui/src/graph_view_model.rs`
- Test: `crates/adapter-gui/src/graph_view_model_tests.rs`

- [ ] **Step 1: Write the failing test**

```rust
mod topological_lane {
    use super::*;
    use crate::graph_view_model::{topological_layout, GraphEdge};

    fn n(id: &str) -> GraphNode {
        GraphNode { id: id.into(), label: id.into(),
            category: NodeCategory::Other, x: 0.0, y: 0.0, bypass: false }
    }
    fn e(a: &str, b: &str) -> GraphEdge {
        GraphEdge { from_id: a.into(), to_id: b.into() }
    }

    #[test]
    fn parallel_siblings_get_symmetric_lanes() {
        // s -> p ; s -> q ; p -> m ; q -> m  : p and q share rank 1,
        // symmetric around origin_y.
        let m = GridMetrics { origin_x: 0.0, origin_y: 100.0,
            column_spacing: 100.0, lane_spacing: 40.0 };
        let nodes = vec![n("s"), n("p"), n("q"), n("m")];
        let edges = vec![e("s","p"), e("s","q"), e("p","m"), e("q","m")];
        let out = topological_layout(&nodes, &edges, m);
        let y = |id| out.iter().find(|x| x.id == id).unwrap().y;
        assert_eq!(y("s"), 100.0);
        assert_eq!(y("m"), 100.0);
        // Two siblings → offsets -0.5 / +0.5 → y = 80 / 120.
        let mut ys = [y("p"), y("q")];
        ys.sort_by(|a,b| a.partial_cmp(b).unwrap());
        assert_eq!(ys, [80.0, 120.0]);
    }

    #[test]
    fn single_node_per_rank_stays_on_centre_lane() {
        let m = GridMetrics { origin_x: 0.0, origin_y: 50.0,
            column_spacing: 100.0, lane_spacing: 40.0 };
        let nodes = vec![n("a"), n("b")];
        let edges = vec![e("a","b")];
        let out = topological_layout(&nodes, &edges, m);
        assert_eq!(out.iter().find(|x| x.id=="a").unwrap().y, 50.0);
        assert_eq!(out.iter().find(|x| x.id=="b").unwrap().y, 50.0);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p adapter-gui --lib topological_lane:: 2>&1 | tail -5`
Expected: FAIL — both `p`/`q` currently at `origin_y` (no lane logic yet).

- [ ] **Step 3: Write minimal implementation**

Replace the final `nodes.iter().map(...)` block (the success path) in `topological_layout` with lane assignment:

```rust
    // Group node indices by rank.
    let max_rank = ids.iter().map(|i| rank[*i]).max().unwrap_or(0);
    let mut by_rank: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (idx, src) in nodes.iter().enumerate() {
        by_rank[rank[src.id.as_str()]].push(idx);
    }
    // lane_offset per node id, computed rank by rank left→right.
    let mut lane: HashMap<&str, f32> = HashMap::new();
    let id_of = |idx: usize| nodes[idx].id.as_str();
    let preds: HashMap<&str, Vec<&str>> = {
        let mut m: HashMap<&str, Vec<&str>> =
            ids.iter().map(|i| (*i, vec![])).collect();
        for ed in edges {
            let (f, t) = (ed.from_id.as_str(), ed.to_id.as_str());
            if m.contains_key(t) && m.contains_key(f) {
                m.get_mut(t).unwrap().push(f);
            }
        }
        m
    };
    for r in 0..by_rank.len() {
        let mut group = by_rank[r].clone();
        // Barycenter = mean predecessor lane; rank 0 uses input index.
        let bary = |idx: usize| -> f32 {
            let id = id_of(idx);
            let ps = &preds[id];
            let known: Vec<f32> =
                ps.iter().filter_map(|p| lane.get(*p).copied()).collect();
            if known.is_empty() {
                idx as f32
            } else {
                known.iter().sum::<f32>() / known.len() as f32
            }
        };
        group.sort_by(|&a, &b| {
            bary(a)
                .partial_cmp(&bary(b))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(&b))
        });
        let k = group.len() as f32;
        for (slot, idx) in group.into_iter().enumerate() {
            let offset = slot as f32 - (k - 1.0) / 2.0;
            lane.insert(id_of(idx), offset);
        }
    }
    nodes
        .iter()
        .map(|src| GraphNode {
            x: metrics.origin_x
                + rank[src.id.as_str()] as f32 * metrics.column_spacing,
            y: metrics.origin_y
                + lane[src.id.as_str()] * metrics.lane_spacing,
            ..src.clone()
        })
        .collect()
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p adapter-gui --lib topological_lane:: topological_rank:: 2>&1 | tail -5`
Expected: PASS (5 tests, no regression in rank tests).

- [ ] **Step 5: Commit**

```bash
git add crates/adapter-gui/src/graph_view_model.rs crates/adapter-gui/src/graph_view_model_tests.rs
git commit -m "feat(435): topological_layout lane assignment (barycenter, symmetric)"
```

---

## Task 4: reorder_for_drop

**Files:**
- Modify: `crates/adapter-gui/src/graph_view_model.rs`
- Test: `crates/adapter-gui/src/graph_view_model_tests.rs`

- [ ] **Step 1: Write the failing test**

```rust
mod reorder {
    use super::*;
    use crate::graph_view_model::{reorder_for_drop, topological_layout, GraphEdge};

    fn n(id: &str) -> GraphNode {
        GraphNode { id: id.into(), label: id.into(),
            category: NodeCategory::Other, x: 0.0, y: 0.0, bypass: false }
    }
    fn e(a: &str, b: &str) -> GraphEdge {
        GraphEdge { from_id: a.into(), to_id: b.into() }
    }

    #[test]
    fn dropping_swaps_sibling_lane_order() {
        let m = GridMetrics { origin_x: 0.0, origin_y: 0.0,
            column_spacing: 100.0, lane_spacing: 40.0 };
        let nodes = vec![n("s"), n("p"), n("q"), n("g")];
        let edges = vec![e("s","p"), e("s","q"), e("p","g"), e("q","g")];
        let base = topological_layout(&nodes, &edges, m);
        let p_y = base.iter().find(|x| x.id=="p").unwrap().y;
        let q_y = base.iter().find(|x| x.id=="q").unwrap().y;
        // Drag p to q's lane: drop at q's y.
        let after = reorder_for_drop(&nodes, &edges, "p",
            base.iter().find(|x| x.id=="p").unwrap().x, q_y, m);
        let p_y2 = after.iter().find(|x| x.id=="p").unwrap().y;
        let q_y2 = after.iter().find(|x| x.id=="q").unwrap().y;
        assert_ne!((p_y, q_y), (p_y2, q_y2), "lane order must change");
        assert_eq!(p_y2, q_y, "p takes q's old lane");
        assert_eq!(q_y2, p_y, "q takes p's old lane");
    }

    #[test]
    fn dropping_in_place_is_idempotent() {
        let m = GridMetrics::default();
        let nodes = vec![n("a"), n("b")];
        let edges = vec![e("a","b")];
        let base = topological_layout(&nodes, &edges, m);
        let a = base.iter().find(|x| x.id=="a").unwrap();
        let after = reorder_for_drop(&nodes, &edges, "a", a.x, a.y, m);
        assert_eq!(after, base);
    }

    #[test]
    fn unknown_id_returns_clean_layout_no_panic() {
        let m = GridMetrics::default();
        let nodes = vec![n("a"), n("b")];
        let edges = vec![e("a","b")];
        let after = reorder_for_drop(&nodes, &edges, "zzz", 0.0, 0.0, m);
        assert_eq!(after, topological_layout(&nodes, &edges, m));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p adapter-gui --lib reorder:: 2>&1 | tail -5`
Expected: FAIL — `reorder_for_drop` not found.

- [ ] **Step 3: Write minimal implementation**

Add to `graph_view_model.rs`:

```rust
/// Re-tidy after a drag in auto mode. The dragged node keeps its
/// edge-derived column; `drop_y` only reorders it among its column
/// siblings. Returns a fresh [`topological_layout`]. Panic-free: an
/// unknown id yields the clean layout unchanged.
pub fn reorder_for_drop(
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    dragged_id: &str,
    _drop_x: f32,
    drop_y: f32,
    metrics: GridMetrics,
) -> Vec<GraphNode> {
    let laid = topological_layout(nodes, edges, metrics);
    let Some(dragged) = laid.iter().find(|n| n.id == dragged_id) else {
        return laid;
    };
    // Column siblings = same x as the dragged node.
    let mut siblings: Vec<&GraphNode> =
        laid.iter().filter(|n| n.x == dragged.x).collect();
    if siblings.len() < 2 {
        return laid;
    }
    siblings.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
    let order: Vec<&str> = siblings.iter().map(|n| n.id.as_str()).collect();
    let cur = order.iter().position(|id| *id == dragged_id).unwrap();
    // Target slot = sibling whose lane is closest to drop_y.
    let mut target = 0usize;
    let mut best = f32::MAX;
    for (i, s) in siblings.iter().enumerate() {
        let d = (s.y - drop_y).abs();
        if d < best {
            best = d;
            target = i;
        }
    }
    if target == cur {
        return laid;
    }
    // Rebuild the input order with the dragged node moved to `target`
    // among its siblings; topological_layout's tiebreak (input index)
    // then yields the new lane order.
    let mut new_sib_ids: Vec<&str> =
        order.iter().filter(|id| **id != dragged_id).copied().collect();
    new_sib_ids.insert(target.min(new_sib_ids.len()), dragged_id);
    let sib_set: std::collections::HashSet<&str> =
        order.iter().copied().collect();
    let mut feed: Vec<GraphNode> = Vec::with_capacity(nodes.len());
    let mut sib_iter = new_sib_ids.iter();
    for src in nodes {
        if sib_set.contains(src.id.as_str()) {
            let next = sib_iter.next().unwrap();
            feed.push(nodes.iter().find(|n| n.id == *next).unwrap().clone());
        } else {
            feed.push(src.clone());
        }
    }
    topological_layout(&feed, edges, metrics)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p adapter-gui --lib reorder:: 2>&1 | tail -5`
Expected: PASS (3 tests).

- [ ] **Step 5: Run full model suite (no regression)**

Run: `cargo test -p adapter-gui --lib graph_view_model 2>&1 | tail -3`
Expected: all PASS, 0 failed, 0 ignored.

- [ ] **Step 6: Commit**

```bash
git add crates/adapter-gui/src/graph_view_model.rs crates/adapter-gui/src/graph_view_model_tests.rs
git commit -m "feat(435): reorder_for_drop — drag re-tidies sibling lane order"
```

---

## Task 5: Slint — CategoryColor struct, palette property, lookup

**Files:**
- Modify: `crates/adapter-gui/ui/components/graph_view.slint`

- [ ] **Step 1: Add the struct and property**

After the existing `export struct GraphEdgeGeometry { ... }` block, add:

```slint
export struct CategoryColor {
    category: string,
    fill: color,
    border: color,
}
```

- [ ] **Step 2: Replace the hardcoded CategoryPalette body**

Replace the whole `component CategoryPalette { ... }` with:

```slint
// Looks the node's category up in the host-supplied palette. The
// component owns NO colours — only a single neutral fallback so an
// unmapped category still renders. Single source of truth lives in the
// Rust host (default_palette()).
component CategoryPalette {
    in property <string> category;
    in property <[CategoryColor]> palette;
    out property <color> fill: root.resolve().fill;
    out property <color> border: root.resolve().border;

    pure function resolve() -> CategoryColor {
        for c in root.palette {
            if c.category == root.category {
                return c;
            }
        }
        return { category: root.category, fill: #8a94a2, border: #5c636e };
    }
}
```

- [ ] **Step 3: Thread the palette to the card**

In `GraphView`, add the input property near the other `in property`s
(after `in property <bool> show_grid: true;`):

```slint
    // Host-supplied category → colour map. Empty = every node uses the
    // neutral fallback. See default_palette() in graph_view_model.rs.
    in property <[CategoryColor]> palette;
```

In `GraphNodeCard`, add a forwarding property at the top of its
`in property` list:

```slint
    in property <[CategoryColor]> palette;
```

Change the `palette := CategoryPalette { category: root.node.category; }`
line inside `GraphNodeCard` to:

```slint
    palette := CategoryPalette {
        category: root.node.category;
        palette: root.palette;
    }
```

In the node `for` loop where `GraphNodeCard` is instantiated, pass it:

```slint
        GraphNodeCard {
            x: 0px;
            y: 0px;
            node: node;
            palette: root.palette;
            card_width: parent.width;
            card_height: parent.height;
            hovered: root.hover_id == node.id && root.drag_id == "";
            dragging: root.drag_id == node.id;
        }
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p adapter-gui --example graph_view_demo 2>&1 | tail -3`
Expected: `Finished` (the demo doesn't set `palette` yet → all nodes
fall back to neutral grey; fixed in Task 7).

- [ ] **Step 5: Commit**

```bash
git add crates/adapter-gui/ui/components/graph_view.slint
git commit -m "feat(435): host-customizable palette (CategoryColor, no hardcoded colours)"
```

---

## Task 6: Slint — auto_layout flag

**Files:**
- Modify: `crates/adapter-gui/ui/components/graph_view.slint`

- [ ] **Step 1: Add the property**

In `GraphView`, right after the `palette` property from Task 5, add:

```slint
    // When true the host computes positions via topological_layout and
    // re-tidies on drag end via reorder_for_drop. Documentational on the
    // Slint side — the algorithm is host-owned (project rule: no logic
    // in the UI layer).
    in property <bool> auto_layout: true;
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p adapter-gui --example graph_view_demo 2>&1 | tail -3`
Expected: `Finished`.

- [ ] **Step 3: Commit**

```bash
git add crates/adapter-gui/ui/components/graph_view.slint
git commit -m "feat(435): auto_layout flag on GraphView"
```

---

## Task 7: Demo — wire palette, auto toggle, topological layout

**Files:**
- Modify: `crates/adapter-gui/examples/graph_view_demo.rs`

- [ ] **Step 1: Import the new helper and convert palette**

In the `use adapter_gui::graph_view_model::{...}` line, add
`default_palette` and `topological_layout`, `reorder_for_drop`.

In the `slint::slint!` `DemoWindow`, add to the exported component a
checkbox + the palette/auto props. Replace the `VerticalLayout { ... }`
header `Rectangle` with one that also holds a toggle:

```slint
            Rectangle {
                height: 36px;
                background: #161b25;
                HorizontalLayout {
                    padding-left: 16px;
                    spacing: 16px;
                    Text {
                        text: "GraphView demo — drag a node, scroll zoom, drag empty area pan";
                        color: #c9d3e2;
                        font-size: 11px;
                        vertical-alignment: center;
                    }
                    auto_box := CheckBox {
                        text: "auto-layout";
                        checked: true;
                    }
                }
            }
```

Add `import { CheckBox } from "std-widgets.slint";` at the top of the
`slint::slint!` block (first line inside the macro), and expose:

```slint
        in property <[CategoryColor]> palette <=> graph.palette;
        out property <bool> auto <=> auto_box.checked;
```

Also add `palette: root.palette;` and `auto-layout: root.auto;` to the
`graph := GraphView { ... }` instantiation, and
`import { ... CategoryColor }` to the existing import line from
`graph_view.slint`.

- [ ] **Step 2: Build the palette and apply auto layout at startup**

Replace the `let (slint_nodes, slint_edges) = into_slint_models(...)`
section's downstream up to `window.set_edges(...)` with:

```rust
    let (nodes, edges) = demo_chain();
    let errors = adapter_gui::graph_view_model::validate_graph(&nodes, &edges);
    assert!(errors.is_empty(), "demo graph is invalid: {errors:?}");

    let metrics = GridMetrics::default();
    let laid = topological_layout(&nodes, &edges, metrics);
    let (slint_nodes, slint_edges) = into_slint_models(laid, edges.clone());

    let palette: Vec<CategoryColor> = default_palette()
        .into_iter()
        .map(|s| CategoryColor {
            category: s.category.into(),
            fill: slint::Color::from_rgb_u8(
                ((s.fill >> 16) & 0xff) as u8,
                ((s.fill >> 8) & 0xff) as u8,
                (s.fill & 0xff) as u8,
            ),
            border: slint::Color::from_rgb_u8(
                ((s.border >> 16) & 0xff) as u8,
                ((s.border >> 8) & 0xff) as u8,
                (s.border & 0xff) as u8,
            ),
        })
        .collect();

    let node_model = std::rc::Rc::new(slint::VecModel::from(slint_nodes));
    let edge_model = std::rc::Rc::new(slint::VecModel::from(slint_edges));

    let window = DemoWindow::new()?;
    window.set_palette(slint::ModelRc::new(slint::VecModel::from(palette)));
    window.set_nodes(node_model.clone().into());
    window.set_edges(edge_model.clone().into());
```

Add `CategoryColor` to the `slint::slint!` re-exported imports and to
the Rust `use` of generated types if needed (the `slint!` macro
generates `CategoryColor`; reference it as in the demo's existing
`GraphNode`/`GraphEdgeGeometry` pattern).

- [ ] **Step 3: Re-tidy on drag end when auto is on**

Replace the `window.on_node_drag_ended(...)` closure with one that
re-runs the layout:

```rust
    let nodes_dl = node_model.clone();
    let edges_dl = edge_model.clone();
    let win_weak = window.as_weak();
    let base_nodes = nodes.clone();
    let base_edges = edges.clone();
    window.on_node_drag_ended(move |id, x, y| {
        log::info!("drag end: {id} -> ({x}, {y})");
        let Some(w) = win_weak.upgrade() else { return };
        if !w.get_auto() {
            return;
        }
        let relaid = reorder_for_drop(
            &base_nodes, &base_edges, id.as_str(), x, y, GridMetrics::default(),
        );
        let coords: std::collections::HashMap<String, (f32, f32)> =
            relaid.iter().map(|n| (n.id.clone(), (n.x, n.y))).collect();
        for i in 0..nodes_dl.row_count() {
            let mut nd = nodes_dl.row_data(i).unwrap();
            if let Some((nx, ny)) = coords.get(nd.id.as_str()) {
                nd.layout_x = *nx;
                nd.layout_y = *ny;
                nodes_dl.set_row_data(i, nd);
            }
        }
        for i in 0..edges_dl.row_count() {
            let mut ed = edges_dl.row_data(i).unwrap();
            if let Some((fx, fy)) = coords.get(ed.from_id.as_str()) {
                ed.from_x = *fx; ed.from_y = *fy;
            }
            if let Some((tx, ty)) = coords.get(ed.to_id.as_str()) {
                ed.to_x = *tx; ed.to_y = *ty;
            }
            edges_dl.set_row_data(i, ed);
        }
    });
```

Ensure `use slint::Model;` is present (it is) for `row_count`/`row_data`.

- [ ] **Step 4: Build**

Run: `cargo build -p adapter-gui --example graph_view_demo 2>&1 | tail -3`
Expected: `Finished`.

- [ ] **Step 5: Run model + fmt + clippy gate**

Run:
```bash
cargo test -p adapter-gui --lib graph_view_model 2>&1 | tail -3
cargo fmt --all -- --check && echo "fmt ok"
cargo clippy -p adapter-gui --example graph_view_demo 2>&1 | tail -3
```
Expected: tests PASS, fmt ok, no new clippy errors.

- [ ] **Step 6: Commit**

```bash
git add crates/adapter-gui/examples/graph_view_demo.rs
git commit -m "feat(435): demo wires palette + auto-layout toggle + reorder on drop"
```

---

## Task 8: Visual validation + push + issue comment

**Files:** none (verification only)

- [ ] **Step 1: Push the branch**

```bash
git push 2>&1 | tail -2
```

- [ ] **Step 2: Comment on the issue with the checkout block**

```bash
gh issue comment 435 --body "Auto-layout + palette customizável implementados. Branch atualizada.

\`\`\`bash
git checkout feature/issue-435 && git pull
cargo run -p adapter-gui --example graph_view_demo
\`\`\`

Testa: nasce organizado (topológico). Arrasta um nó com 'auto-layout' ligado → solta → re-organiza tidy. Desliga o checkbox → drag livre como antes. Cores vêm de default_palette() (host pode trocar 100%). Screenshot se algo estiver torto."
```

- [ ] **Step 3: Wait for user visual confirmation**

The component render cannot be unit-tested (project rule). Do not mark
the feature done until the user confirms the demo looks right.

---

## Self-Review

- **Spec coverage:** topological_layout (Tasks 2-3), reorder_for_drop (Task 4), auto_layout flag (Task 6), customizable palette (Tasks 1,5,7), default_palette single-source (Task 1), panic-free/cycle (Tasks 2,4), tests pure-Rust (Tasks 1-4), demo toggle (Task 7), visual validation (Task 8). All spec sections covered.
- **Placeholder scan:** none — every code step is complete.
- **Type consistency:** `CategoryStyle` (Rust, `&'static str` + `u32`) vs `CategoryColor` (Slint, `string` + `color`); demo Task 7 converts between them explicitly. `topological_layout`/`reorder_for_drop` signatures match across Tasks 2-4 and 7. `GridMetrics`, `GraphNode`, `GraphEdge` reuse existing fields.
- **Note:** `reorder_for_drop` takes `_drop_x` unused (column is edge-fixed per spec) — intentional, documented.
