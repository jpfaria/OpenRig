//! Responsibility: builds a positioned graph from a chain's stages
//!
//! Layout strategy (#328, recursive):
//!
//! - [`super::types::ChainStage::Single`] blocks sit on the centre of the
//!   row band they are placed in and advance the column cursor by one.
//! - `Parallel` starts with an auto-generated split node on its band's
//!   centre. Its lanes stack top to bottom, each taking as many rows as its
//!   own stages need ([`stage_extent`]), the stack centred on the band. With
//!   [`ParallelEnd::Merge`] the lanes meet again at an auto-generated merge
//!   node on the column after the widest lane, and the next stage continues
//!   from it. With [`ParallelEnd::Fan`] there is no merge node: each lane's
//!   last single blueprint is its terminal, and the terminals line up on the
//!   widest lane's last column.

use super::routing_ids::{merge_node_id, split_node_id};
use super::types::{
    BlockBlueprint, ChainStage, GraphEdge, GraphNode, GridMetrics, NodeCategory, NodeKind,
    ParallelEnd,
};

/// Columns and rows a stage list takes: a single is 1×1; a parallel stage
/// is its split (and merge) column plus its widest lane, by the sum of its
/// lanes' rows (at least one per lane); a list is its stages side by side.
pub fn stage_extent(stages: &[ChainStage]) -> (usize, usize) {
    stages.iter().fold((0, 1), |(cols, rows), stage| {
        let (c, r) = single_extent(stage);
        (cols + c, rows.max(r))
    })
}

fn single_extent(stage: &ChainStage) -> (usize, usize) {
    match stage {
        ChainStage::Single(_) => (1, 1),
        ChainStage::Parallel { lanes, .. } if lanes.is_empty() => (0, 1),
        ChainStage::Parallel { lanes, end, .. } => {
            let widest = lanes.iter().map(|l| stage_extent(l).0).max().unwrap_or(0);
            let rows = lanes.iter().map(|l| stage_extent(l).1).sum();
            let routing = match end {
                ParallelEnd::Merge => 2,
                ParallelEnd::Fan => 1,
            };
            (routing + widest, rows)
        }
    }
}

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
    let mut out = Placed {
        nodes: Vec::new(),
        edges: Vec::new(),
        metrics,
    };
    out.place(stages, 0, 0.0, None, None);
    (out.nodes, out.edges)
}

struct Placed {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    metrics: GridMetrics,
}

impl Placed {
    /// Place `stages` from column `col` on the band centred `centre` lanes
    /// from the origin row, wiring the first one from `tail`. With
    /// `terminal_col`, a trailing single blueprint is a Fan terminal and
    /// sits on that column. Returns the next free column and the tail.
    fn place(
        &mut self,
        stages: &[ChainStage],
        mut col: usize,
        centre: f32,
        mut tail: Option<String>,
        terminal_col: Option<usize>,
    ) -> (usize, Option<String>) {
        for (at, stage) in stages.iter().enumerate() {
            match stage {
                ChainStage::Single(block) => {
                    let is_terminal = at + 1 == stages.len();
                    let column = match terminal_col {
                        Some(terminal) if is_terminal => terminal,
                        _ => col,
                    };
                    let node = self.block_node(block, column, centre);
                    self.wire(tail.take(), &node.id);
                    tail = Some(node.id.clone());
                    self.nodes.push(node);
                    col += 1;
                }
                ChainStage::Parallel { lanes, .. } if lanes.is_empty() => {}
                ChainStage::Parallel {
                    split_id,
                    lanes,
                    end,
                } => {
                    let (next, next_tail) =
                        self.place_parallel(split_id, lanes, *end, col, centre, tail.take());
                    col = next;
                    tail = next_tail;
                }
            }
        }
        (col, tail)
    }

    fn place_parallel(
        &mut self,
        split_id: &str,
        lanes: &[Vec<ChainStage>],
        end: ParallelEnd,
        col: usize,
        centre: f32,
        tail: Option<String>,
    ) -> (usize, Option<String>) {
        let split_node = split_node_id(split_id);
        let merge_node = merge_node_id(split_id);
        let extents: Vec<(usize, usize)> = lanes.iter().map(|l| stage_extent(l)).collect();
        let widest = extents.iter().map(|e| e.0).max().unwrap_or(0);
        let total_rows: usize = extents.iter().map(|e| e.1).sum();

        let split = self.routing_node(&split_node, NodeKind::Split, col, centre);
        self.nodes.push(split);
        self.wire(tail, &split_node);

        let terminal_col = (end == ParallelEnd::Fan).then_some(col + widest);
        let mut row_top = centre - (total_rows as f32 - 1.0) / 2.0;
        for (lane, (_, rows)) in lanes.iter().zip(&extents) {
            let lane_centre = row_top + (*rows as f32 - 1.0) / 2.0;
            row_top += *rows as f32;
            let (_, lane_tail) = self.place(
                lane,
                col + 1,
                lane_centre,
                Some(split_node.clone()),
                terminal_col,
            );
            if end == ParallelEnd::Merge {
                if let Some(from) = lane_tail {
                    self.wire(Some(from), &merge_node);
                }
            }
        }

        match end {
            ParallelEnd::Merge => {
                let merge_col = col + widest + 1;
                let merge = self.routing_node(&merge_node, NodeKind::Mixer, merge_col, centre);
                self.nodes.push(merge);
                (merge_col + 1, Some(merge_node))
            }
            ParallelEnd::Fan => (col + widest + 1, None),
        }
    }

    fn wire(&mut self, from: Option<String>, to: &str) {
        if let Some(from_id) = from {
            self.edges.push(GraphEdge {
                from_id,
                to_id: to.to_string(),
            });
        }
    }

    /// An auto-generated split or merge node: no label, `Util` category.
    /// Its card is picked by `kind`; the label stays empty — the Slint card
    /// translates the name.
    fn routing_node(&self, id: &str, kind: NodeKind, col: usize, centre: f32) -> GraphNode {
        GraphNode {
            id: id.to_string(),
            label: String::new(),
            category: NodeCategory::Util,
            kind,
            x: self.x(col),
            y: self.y(centre),
            bypass: false,
        }
    }

    fn block_node(&self, block: &BlockBlueprint, col: usize, centre: f32) -> GraphNode {
        GraphNode {
            id: block.id.clone(),
            label: block.label.clone(),
            category: block.category,
            kind: block.kind,
            x: self.x(col),
            y: self.y(centre),
            bypass: block.bypass,
        }
    }

    fn x(&self, col: usize) -> f32 {
        self.metrics.origin_x + col as f32 * self.metrics.column_spacing
    }

    fn y(&self, centre: f32) -> f32 {
        self.metrics.origin_y + centre * self.metrics.lane_spacing
    }
}
