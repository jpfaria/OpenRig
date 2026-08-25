//! Responsibility: places existing nodes by the topology of their edges
//!
//! Column = longest path from a source (in-degree 0). Lane assignment
//! spreads nodes that share a column symmetrically around `origin_y`,
//! ordered by the barycenter of their predecessors so parallel paths stay
//! readable. Panic-free: a cycle or malformed graph falls back to
//! input-order ranking on the centre lane.

use std::collections::HashMap;

use super::types::{GraphEdge, GraphNode, GridMetrics};

/// Index adjacency built from edges: `(out, indeg)` keyed by node index.
/// Edges whose endpoints are not both in `node_idx` are ignored.
fn build_adjacency(
    node_idx: &HashMap<&str, usize>,
    n: usize,
    edges: &[GraphEdge],
) -> (Vec<Vec<usize>>, Vec<usize>) {
    let mut outs: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    for ed in edges {
        if let (Some(&f), Some(&t)) = (
            node_idx.get(ed.from_id.as_str()),
            node_idx.get(ed.to_id.as_str()),
        ) {
            outs[f].push(t);
            indeg[t] += 1;
        }
    }
    (outs, indeg)
}

/// Longest-path rank per node index (column). `None` if the graph has a
/// cycle — caller falls back to input order.
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
    (processed == n).then_some(rank)
}

/// Lane offset per node index. Nodes sharing a rank are ordered by the
/// barycenter of their predecessors (already-placed earlier ranks) with
/// input index as tiebreak, then spread symmetrically around 0.
fn assign_lanes(nodes: &[GraphNode], edges: &[GraphEdge], rank: &[usize]) -> Vec<f32> {
    let n = nodes.len();
    let node_idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, x)| (x.id.as_str(), i))
        .collect();
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
    for ed in edges {
        if let (Some(&f), Some(&t)) = (
            node_idx.get(ed.from_id.as_str()),
            node_idx.get(ed.to_id.as_str()),
        ) {
            preds[t].push(f);
        }
    }
    let max_rank = rank.iter().copied().max().unwrap_or(0);
    let mut by_rank: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (i, &r) in rank.iter().enumerate() {
        by_rank[r].push(i);
    }
    let mut lane = vec![0f32; n];
    for group in by_rank.iter() {
        let mut g = group.clone();
        let bary = |i: usize, lane: &[f32]| -> f32 {
            if preds[i].is_empty() {
                i as f32
            } else {
                preds[i].iter().map(|&p| lane[p]).sum::<f32>() / preds[i].len() as f32
            }
        };
        g.sort_by(|&a, &b| {
            bary(a, &lane)
                .partial_cmp(&bary(b, &lane))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(&b))
        });
        let k = g.len() as f32;
        for (slot, i) in g.into_iter().enumerate() {
            lane[i] = slot as f32 - (k - 1.0) / 2.0;
        }
    }
    lane
}

/// Auto-layout from graph topology.
pub fn topological_layout(
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    metrics: GridMetrics,
) -> Vec<GraphNode> {
    let ids: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let Some(rank) = longest_path_ranks(&ids, edges) else {
        return nodes
            .iter()
            .enumerate()
            .map(|(i, src)| GraphNode {
                x: metrics.origin_x + i as f32 * metrics.column_spacing,
                y: metrics.origin_y,
                ..src.clone()
            })
            .collect();
    };
    let lane = assign_lanes(nodes, edges, &rank);
    nodes
        .iter()
        .enumerate()
        .map(|(i, src)| GraphNode {
            x: metrics.origin_x + rank[i] as f32 * metrics.column_spacing,
            y: metrics.origin_y + lane[i] * metrics.lane_spacing,
            ..src.clone()
        })
        .collect()
}
