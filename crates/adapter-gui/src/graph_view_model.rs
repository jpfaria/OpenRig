//! Data model and layout helpers for the `GraphView` Slint component.
//!
//! Pure Rust types decoupled from Slint. Wiring code converts these into
//! Slint-generated structs before pushing to the UI. Kept here so the
//! layout logic can be unit-tested without spinning a UI.
//!
//! The component itself is **fully generic**: it knows nothing about
//! amps, drives, or signal-chain semantics. It only renders nodes
//! at given coordinates and edges between them. Domain-specific layout
//! (signal chain, project topology, etc.) is computed by helpers like
//! [`linear_chain_layout`] and [`parallel_paths_layout`].

use std::collections::HashMap;

/// Visual category of a node — used by the UI to colour the node panel.
///
/// Adding a new category does NOT require touching the component; the
/// UI layer (visual config) maps category → colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeCategory {
    /// Input source (mono/stereo/dual-mono).
    Input,
    /// Output sink.
    Output,
    /// Gate/compressor/dynamics.
    Dynamics,
    /// Drive/distortion/overdrive.
    Drive,
    /// Amplifier / preamp / cabinet.
    Amp,
    /// Modulation (chorus, phaser, flanger).
    Modulation,
    /// Time-based effects (delay, echo).
    Time,
    /// Reverb (room, hall, shimmer, plate).
    Reverb,
    /// Equalizer / filter.
    Eq,
    /// Utility (volume, splitter, merger, send).
    Util,
    /// Anything else.
    Other,
}

impl NodeCategory {
    /// Stable string identifier suitable for serialisation and for the
    /// Slint side to look up colours. Lower-case, no spaces.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
            Self::Dynamics => "dynamics",
            Self::Drive => "drive",
            Self::Amp => "amp",
            Self::Modulation => "modulation",
            Self::Time => "time",
            Self::Reverb => "reverb",
            Self::Eq => "eq",
            Self::Util => "util",
            Self::Other => "other",
        }
    }
}

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

/// A node in the graph view. Coordinates are in **layout space**
/// (logical pixels before zoom/pan). The component applies the
/// viewport transform when rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphNode {
    /// Stable identifier — must be unique within the graph. Used by edges
    /// and by callbacks (`node-clicked(id)`).
    pub id: String,
    /// Human-readable label shown on the node.
    pub label: String,
    /// Visual category — drives node colour.
    pub category: NodeCategory,
    /// X position in layout space.
    pub x: f32,
    /// Y position in layout space.
    pub y: f32,
    /// Whether the node represents a bypassed block. Dim in the UI.
    pub bypass: bool,
}

/// An edge between two nodes — represents signal flow.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdge {
    /// Source node id.
    pub from_id: String,
    /// Target node id.
    pub to_id: String,
}

/// Logical stage of a signal chain. The layout helpers below consume a
/// sequence of stages and produce positioned [`GraphNode`]s and
/// [`GraphEdge`]s.
#[derive(Debug, Clone, PartialEq)]
pub enum ChainStage {
    /// A single block — sits alone in one column.
    Single(BlockBlueprint),
    /// Parallel paths between an implicit split and merge. Each inner
    /// `Vec` is one path; all paths share the same column range.
    Parallel(Vec<Vec<BlockBlueprint>>),
}

/// Logical description of one block, without position. Position is
/// assigned by [`linear_chain_layout`].
#[derive(Debug, Clone, PartialEq)]
pub struct BlockBlueprint {
    pub id: String,
    pub label: String,
    pub category: NodeCategory,
    pub bypass: bool,
}

impl BlockBlueprint {
    pub fn new(id: impl Into<String>, label: impl Into<String>, category: NodeCategory) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            category,
            bypass: false,
        }
    }
}

/// Grid metrics used by [`linear_chain_layout`].
#[derive(Debug, Clone, Copy)]
pub struct GridMetrics {
    /// Horizontal distance between adjacent columns (centre to centre).
    pub column_spacing: f32,
    /// Vertical distance between parallel lanes (centre to centre).
    pub lane_spacing: f32,
    /// X coordinate of the first column's centre.
    pub origin_x: f32,
    /// Y coordinate of the central lane (single-path / merged blocks).
    pub origin_y: f32,
}

impl Default for GridMetrics {
    fn default() -> Self {
        Self {
            column_spacing: 160.0,
            lane_spacing: 120.0,
            origin_x: 80.0,
            origin_y: 200.0,
        }
    }
}

/// Build a positioned graph from a sequence of [`ChainStage`]s.
///
/// Layout strategy:
///
/// - [`ChainStage::Single`] blocks sit on the central lane and advance
///   the column cursor by one.
/// - [`ChainStage::Parallel`] places each inner path on its own lane
///   (above/below the centre, distributed symmetrically) and reserves
///   columns equal to the longest path. Split/merge utility nodes are
///   inserted automatically so the result is a connected DAG.
///
/// Returns the (nodes, edges) pair ready to push to the Slint side. IDs
/// must be unique across the whole input — duplicates produce undefined
/// behaviour at the UI level (the panic-free contract is kept here, but
/// the UI may render only one of the duplicates).
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
            ChainStage::Parallel(paths) if paths.is_empty() => {
                // No-op — nothing to render, no column consumed.
            }
            ChainStage::Parallel(paths) => {
                split_counter += 1;
                let split_id = format!("__split_{split_counter}");
                let merge_id = format!("__merge_{split_counter}");

                let longest = paths.iter().map(Vec::len).max().unwrap_or(0);
                let split_col = col;
                let merge_col = col + longest + 1;

                // Split node sits at split_col on the centre lane.
                nodes.push(GraphNode {
                    id: split_id.clone(),
                    label: String::new(),
                    category: NodeCategory::Util,
                    x: metrics.origin_x + split_col as f32 * metrics.column_spacing,
                    y: metrics.origin_y,
                    bypass: false,
                });
                if let Some(prev) = prev_tail.take() {
                    edges.push(GraphEdge {
                        from_id: prev,
                        to_id: split_id.clone(),
                    });
                }

                // Each path occupies its own lane. With N paths, lanes
                // are -N/2..N/2 around the centre; 2 paths → -0.5 / +0.5.
                let n_paths = paths.len() as f32;
                for (lane_idx, path) in paths.iter().enumerate() {
                    let lane_offset = lane_idx as f32 - (n_paths - 1.0) / 2.0;
                    let mut last_in_lane = split_id.clone();
                    for (block_idx, block) in path.iter().enumerate() {
                        let node = position_block_lane(
                            block,
                            split_col + 1 + block_idx,
                            lane_offset,
                            &metrics,
                        );
                        edges.push(GraphEdge {
                            from_id: last_in_lane,
                            to_id: node.id.clone(),
                        });
                        last_in_lane = node.id.clone();
                        nodes.push(node);
                    }
                    edges.push(GraphEdge {
                        from_id: last_in_lane,
                        to_id: merge_id.clone(),
                    });
                }

                // Merge node sits at merge_col on the centre lane.
                nodes.push(GraphNode {
                    id: merge_id.clone(),
                    label: String::new(),
                    category: NodeCategory::Util,
                    x: metrics.origin_x + merge_col as f32 * metrics.column_spacing,
                    y: metrics.origin_y,
                    bypass: false,
                });
                prev_tail = Some(merge_id);
                col = merge_col + 1;
            }
        }
    }

    (nodes, edges)
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

/// Validate that the (nodes, edges) pair is a well-formed graph:
///
/// - every node id is unique,
/// - every edge references existing node ids,
/// - no node references itself.
///
/// Returns a list of error messages — empty means valid. Useful for
/// guarding the layout output before it reaches the UI.
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

/// Auto-layout from graph topology. Column = longest path from a source
/// (in-degree 0). Lane assignment spreads nodes that share a column
/// symmetrically around `origin_y`, ordered by the barycenter of their
/// predecessors so parallel paths stay readable. Panic-free: a cycle or
/// malformed graph falls back to input-order ranking on the centre lane.
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
    let mut queue: Vec<&str> = ids.iter().filter(|i| indeg[*i] == 0).copied().collect();
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

    // Group node indices by rank, then assign lanes left→right using the
    // barycenter (mean lane of predecessors) with input index as tiebreak.
    let max_rank = ids.iter().map(|i| rank[*i]).max().unwrap_or(0);
    let mut by_rank: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (idx, src) in nodes.iter().enumerate() {
        by_rank[rank[src.id.as_str()]].push(idx);
    }
    let id_of = |idx: usize| nodes[idx].id.as_str();
    let preds: HashMap<&str, Vec<&str>> = {
        let mut m: HashMap<&str, Vec<&str>> = ids.iter().map(|i| (*i, vec![])).collect();
        for ed in edges {
            let (f, t) = (ed.from_id.as_str(), ed.to_id.as_str());
            if m.contains_key(t) && m.contains_key(f) {
                m.get_mut(t).unwrap().push(f);
            }
        }
        m
    };
    let mut lane: HashMap<&str, f32> = HashMap::new();
    for group in by_rank.iter() {
        let mut group = group.clone();
        let bary = |idx: usize| -> f32 {
            let id = id_of(idx);
            let known: Vec<f32> = preds[id]
                .iter()
                .filter_map(|p| lane.get(*p).copied())
                .collect();
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
            lane.insert(id_of(idx), slot as f32 - (k - 1.0) / 2.0);
        }
    }
    nodes
        .iter()
        .map(|src| GraphNode {
            x: metrics.origin_x + rank[src.id.as_str()] as f32 * metrics.column_spacing,
            y: metrics.origin_y + lane[src.id.as_str()] * metrics.lane_spacing,
            ..src.clone()
        })
        .collect()
}

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
    let dragged_x = dragged.x;
    let mut siblings: Vec<&GraphNode> = laid.iter().filter(|n| n.x == dragged_x).collect();
    if siblings.len() < 2 {
        return laid;
    }
    siblings.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    let order: Vec<&str> = siblings.iter().map(|n| n.id.as_str()).collect();
    let cur = order.iter().position(|id| *id == dragged_id).unwrap();
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
    let mut new_sib_ids: Vec<&str> = order
        .iter()
        .filter(|id| **id != dragged_id)
        .copied()
        .collect();
    new_sib_ids.insert(target.min(new_sib_ids.len()), dragged_id);
    let sib_set: std::collections::HashSet<&str> = order.iter().copied().collect();
    let mut sib_iter = new_sib_ids.iter();
    let feed: Vec<GraphNode> = nodes
        .iter()
        .map(|src| {
            if sib_set.contains(src.id.as_str()) {
                let next = *sib_iter.next().unwrap();
                nodes.iter().find(|n| n.id == next).unwrap().clone()
            } else {
                src.clone()
            }
        })
        .collect();
    topological_layout(&feed, edges, metrics)
}

#[cfg(test)]
#[path = "graph_view_model_tests.rs"]
mod tests;
