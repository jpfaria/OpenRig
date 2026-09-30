//! Tests for `adapter-gui::graph_view_model`. Lifted out per project
//! convention — production `.rs` files keep `#[cfg(test)] #[path] mod tests;`
//! at the bottom; the body lives here.

use super::{
    linear_chain_layout, validate_graph, BlockBlueprint, ChainStage, GraphEdge, GraphNode,
    GridMetrics, NodeCategory, NodeKind, ParallelEnd,
};

fn block(id: &str, label: &str, category: NodeCategory) -> BlockBlueprint {
    BlockBlueprint::new(id, label, category)
}

fn find_node<'a>(nodes: &'a [GraphNode], id: &str) -> &'a GraphNode {
    nodes
        .iter()
        .find(|n| n.id == id)
        .unwrap_or_else(|| panic!("node {id} missing"))
}

mod node_category {
    use super::*;

    #[test]
    fn as_str_returns_stable_lowercase_slug() {
        assert_eq!(NodeCategory::Drive.as_str(), "drive");
        assert_eq!(NodeCategory::Amp.as_str(), "amp");
        assert_eq!(NodeCategory::Util.as_str(), "util");
    }
}

mod palette {
    use crate::graph_view_model::{default_palette, NodeCategory};

    #[test]
    fn default_palette_covers_every_category() {
        let pal = default_palette();
        for cat in [
            NodeCategory::Input,
            NodeCategory::Output,
            NodeCategory::Dynamics,
            NodeCategory::Drive,
            NodeCategory::Amp,
            NodeCategory::Modulation,
            NodeCategory::Time,
            NodeCategory::Reverb,
            NodeCategory::Eq,
            NodeCategory::Util,
            NodeCategory::Other,
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
                s.border,
                s.fill,
                s.category
            );
        }
    }
}

mod topological_rank {
    use super::*;
    use crate::graph_view_model::topological_layout;

    fn tn(id: &str) -> GraphNode {
        GraphNode {
            id: id.into(),
            label: id.into(),
            category: NodeCategory::Other,
            kind: NodeKind::Block,
            x: 0.0,
            y: 0.0,
            bypass: false,
        }
    }
    fn te(a: &str, b: &str) -> GraphEdge {
        GraphEdge {
            from_id: a.into(),
            to_id: b.into(),
        }
    }

    #[test]
    fn linear_chain_ranks_left_to_right() {
        let m = GridMetrics {
            origin_x: 0.0,
            origin_y: 0.0,
            column_spacing: 100.0,
            lane_spacing: 50.0,
        };
        let nodes = vec![tn("a"), tn("b"), tn("c")];
        let edges = vec![te("a", "b"), te("b", "c")];
        let out = topological_layout(&nodes, &edges, m);
        let get = |id| out.iter().find(|x| x.id == id).unwrap().x;
        assert_eq!(get("a"), 0.0);
        assert_eq!(get("b"), 100.0);
        assert_eq!(get("c"), 200.0);
    }

    #[test]
    fn diamond_longest_path_wins() {
        let m = GridMetrics {
            origin_x: 0.0,
            origin_y: 0.0,
            column_spacing: 100.0,
            lane_spacing: 50.0,
        };
        let nodes = vec![tn("a"), tn("b"), tn("d")];
        let edges = vec![te("a", "b"), te("b", "d"), te("a", "d")];
        let out = topological_layout(&nodes, &edges, m);
        let get = |id| out.iter().find(|x| x.id == id).unwrap().x;
        assert_eq!(get("d"), 200.0);
    }

    #[test]
    fn cycle_falls_back_to_input_order_no_panic() {
        let m = GridMetrics::default();
        let nodes = vec![tn("a"), tn("b")];
        let edges = vec![te("a", "b"), te("b", "a")];
        let out = topological_layout(&nodes, &edges, m);
        assert_eq!(out.len(), 2);
    }
}

mod topological_lane {
    use super::*;
    use crate::graph_view_model::topological_layout;

    fn tn(id: &str) -> GraphNode {
        GraphNode {
            id: id.into(),
            label: id.into(),
            category: NodeCategory::Other,
            kind: NodeKind::Block,
            x: 0.0,
            y: 0.0,
            bypass: false,
        }
    }
    fn te(a: &str, b: &str) -> GraphEdge {
        GraphEdge {
            from_id: a.into(),
            to_id: b.into(),
        }
    }

    #[test]
    fn parallel_siblings_get_symmetric_lanes() {
        let m = GridMetrics {
            origin_x: 0.0,
            origin_y: 100.0,
            column_spacing: 100.0,
            lane_spacing: 40.0,
        };
        let nodes = vec![tn("s"), tn("p"), tn("q"), tn("m")];
        let edges = vec![te("s", "p"), te("s", "q"), te("p", "m"), te("q", "m")];
        let out = topological_layout(&nodes, &edges, m);
        let y = |id| out.iter().find(|x| x.id == id).unwrap().y;
        assert_eq!(y("s"), 100.0);
        assert_eq!(y("m"), 100.0);
        let mut ys = [y("p"), y("q")];
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(ys, [80.0, 120.0]);
    }

    #[test]
    fn single_node_per_rank_stays_on_centre_lane() {
        let m = GridMetrics {
            origin_x: 0.0,
            origin_y: 50.0,
            column_spacing: 100.0,
            lane_spacing: 40.0,
        };
        let nodes = vec![tn("a"), tn("b")];
        let edges = vec![te("a", "b")];
        let out = topological_layout(&nodes, &edges, m);
        assert_eq!(out.iter().find(|x| x.id == "a").unwrap().y, 50.0);
        assert_eq!(out.iter().find(|x| x.id == "b").unwrap().y, 50.0);
    }
}

mod reorder {
    use super::*;
    use crate::graph_view_model::{reorder_for_drop, topological_layout};

    fn tn(id: &str) -> GraphNode {
        GraphNode {
            id: id.into(),
            label: id.into(),
            category: NodeCategory::Other,
            kind: NodeKind::Block,
            x: 0.0,
            y: 0.0,
            bypass: false,
        }
    }
    fn te(a: &str, b: &str) -> GraphEdge {
        GraphEdge {
            from_id: a.into(),
            to_id: b.into(),
        }
    }

    #[test]
    fn dropping_swaps_sibling_lane_order() {
        let m = GridMetrics {
            origin_x: 0.0,
            origin_y: 0.0,
            column_spacing: 100.0,
            lane_spacing: 40.0,
        };
        let nodes = vec![tn("s"), tn("p"), tn("q"), tn("g")];
        let edges = vec![te("s", "p"), te("s", "q"), te("p", "g"), te("q", "g")];
        let base = topological_layout(&nodes, &edges, m);
        let p_y = base.iter().find(|x| x.id == "p").unwrap().y;
        let q_y = base.iter().find(|x| x.id == "q").unwrap().y;
        let after = reorder_for_drop(&nodes, &edges, "p", q_y, m);
        let p_y2 = after.iter().find(|x| x.id == "p").unwrap().y;
        let q_y2 = after.iter().find(|x| x.id == "q").unwrap().y;
        assert_ne!((p_y, q_y), (p_y2, q_y2), "lane order must change");
        assert_eq!(p_y2, q_y, "p takes q's old lane");
        assert_eq!(q_y2, p_y, "q takes p's old lane");
    }

    #[test]
    fn dropping_in_place_is_idempotent() {
        let m = GridMetrics::default();
        let nodes = vec![tn("a"), tn("b")];
        let edges = vec![te("a", "b")];
        let base = topological_layout(&nodes, &edges, m);
        let a = base.iter().find(|x| x.id == "a").unwrap();
        let after = reorder_for_drop(&nodes, &edges, "a", a.y, m);
        assert_eq!(after, base);
    }

    #[test]
    fn unknown_id_returns_clean_layout_no_panic() {
        let m = GridMetrics::default();
        let nodes = vec![tn("a"), tn("b")];
        let edges = vec![te("a", "b")];
        let after = reorder_for_drop(&nodes, &edges, "zzz", 0.0, m);
        assert_eq!(after, topological_layout(&nodes, &edges, m));
    }
}

mod linear_layout_single_stage {
    use super::*;

    #[test]
    fn empty_input_produces_empty_output() {
        let (nodes, edges) = linear_chain_layout(&[], GridMetrics::default());
        assert!(nodes.is_empty());
        assert!(edges.is_empty());
    }

    #[test]
    fn single_block_yields_one_node_no_edges() {
        let stages = [ChainStage::Single(block("a", "A", NodeCategory::Drive))];
        let (nodes, edges) = linear_chain_layout(&stages, GridMetrics::default());

        assert_eq!(nodes.len(), 1);
        assert_eq!(edges.len(), 0);
        let only = &nodes[0];
        assert_eq!(only.id, "a");
        assert_eq!(only.label, "A");
        assert_eq!(only.category, NodeCategory::Drive);
    }

    #[test]
    fn two_singles_are_connected_left_to_right() {
        let stages = [
            ChainStage::Single(block("a", "A", NodeCategory::Drive)),
            ChainStage::Single(block("b", "B", NodeCategory::Amp)),
        ];
        let (nodes, edges) = linear_chain_layout(&stages, GridMetrics::default());

        assert_eq!(nodes.len(), 2);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from_id, "a");
        assert_eq!(edges[0].to_id, "b");
    }

    #[test]
    fn singles_increment_column_by_one_each() {
        let metrics = GridMetrics {
            origin_x: 0.0,
            origin_y: 0.0,
            column_spacing: 100.0,
            lane_spacing: 50.0,
        };
        let stages = [
            ChainStage::Single(block("a", "A", NodeCategory::Drive)),
            ChainStage::Single(block("b", "B", NodeCategory::Amp)),
            ChainStage::Single(block("c", "C", NodeCategory::Reverb)),
        ];
        let (nodes, _) = linear_chain_layout(&stages, metrics);

        assert_eq!(nodes[0].x, 0.0);
        assert_eq!(nodes[1].x, 100.0);
        assert_eq!(nodes[2].x, 200.0);
        for n in &nodes {
            assert_eq!(n.y, 0.0, "singles stay on centre lane");
        }
    }
}

mod linear_layout_parallel_stage {
    use super::*;

    #[test]
    fn parallel_split_inserts_split_and_merge_nodes() {
        let stages = [ChainStage::Parallel {
            lanes: vec![
                vec![block("l", "L", NodeCategory::Amp)],
                vec![block("r", "R", NodeCategory::Amp)],
            ],
            end: ParallelEnd::Merge,
        }];
        let (nodes, _) = linear_chain_layout(&stages, GridMetrics::default());

        assert!(
            nodes.iter().any(|n| n.id.starts_with("__split_")),
            "split node present"
        );
        assert!(
            nodes.iter().any(|n| n.id.starts_with("__merge_")),
            "merge node present"
        );
    }

    // The routing nodes carry no host label and the Util category: since
    // #328 the Slint card picks their face from `kind` and translates
    // their name itself, so a host label here would never be shown.
    #[test]
    fn split_and_merge_use_routing_node_convention() {
        let stages = [ChainStage::Parallel {
            lanes: vec![
                vec![block("l", "L", NodeCategory::Amp)],
                vec![block("r", "R", NodeCategory::Amp)],
            ],
            end: ParallelEnd::Merge,
        }];
        let (nodes, _) = linear_chain_layout(&stages, GridMetrics::default());

        let split = find_node(&nodes, "__split_1");
        let merge = find_node(&nodes, "__merge_1");

        for routing in [split, merge] {
            assert_eq!(
                routing.label, "",
                "routing node {} must have empty label",
                routing.id
            );
            assert_eq!(
                routing.category,
                NodeCategory::Util,
                "routing node {} must be Util category",
                routing.id
            );
        }
    }

    #[test]
    fn parallel_paths_sit_on_symmetric_lanes() {
        let metrics = GridMetrics {
            origin_x: 0.0,
            origin_y: 0.0,
            column_spacing: 100.0,
            lane_spacing: 80.0,
        };
        let stages = [ChainStage::Parallel {
            lanes: vec![
                vec![block("l", "L", NodeCategory::Amp)],
                vec![block("r", "R", NodeCategory::Amp)],
            ],
            end: ParallelEnd::Merge,
        }];
        let (nodes, _) = linear_chain_layout(&stages, metrics);

        let l = find_node(&nodes, "l");
        let r = find_node(&nodes, "r");

        // Two paths → lane offsets are -0.5 and +0.5 around the centre.
        assert_eq!(l.y, -40.0);
        assert_eq!(r.y, 40.0);
    }

    #[test]
    fn split_connects_to_each_path_first_block() {
        let stages = [ChainStage::Parallel {
            lanes: vec![
                vec![block("l", "L", NodeCategory::Amp)],
                vec![block("r", "R", NodeCategory::Amp)],
            ],
            end: ParallelEnd::Merge,
        }];
        let (_, edges) = linear_chain_layout(&stages, GridMetrics::default());

        let split_id = "__split_1";
        let split_edges: Vec<&GraphEdge> = edges.iter().filter(|e| e.from_id == split_id).collect();
        assert_eq!(split_edges.len(), 2);
        let targets: Vec<&str> = split_edges.iter().map(|e| e.to_id.as_str()).collect();
        assert!(targets.contains(&"l"));
        assert!(targets.contains(&"r"));
    }

    #[test]
    fn each_path_last_block_connects_to_merge() {
        let stages = [ChainStage::Parallel {
            lanes: vec![
                vec![block("l", "L", NodeCategory::Amp)],
                vec![block("r", "R", NodeCategory::Amp)],
            ],
            end: ParallelEnd::Merge,
        }];
        let (_, edges) = linear_chain_layout(&stages, GridMetrics::default());

        let merge_id = "__merge_1";
        let merge_edges: Vec<&GraphEdge> = edges.iter().filter(|e| e.to_id == merge_id).collect();
        assert_eq!(merge_edges.len(), 2);
        let sources: Vec<&str> = merge_edges.iter().map(|e| e.from_id.as_str()).collect();
        assert!(sources.contains(&"l"));
        assert!(sources.contains(&"r"));
    }

    #[test]
    fn merge_column_accounts_for_longest_path() {
        let metrics = GridMetrics {
            origin_x: 0.0,
            origin_y: 0.0,
            column_spacing: 100.0,
            lane_spacing: 50.0,
        };
        let stages = [ChainStage::Parallel {
            lanes: vec![
                vec![
                    block("l1", "L1", NodeCategory::Amp),
                    block("l2", "L2", NodeCategory::Time),
                ],
                vec![block("r", "R", NodeCategory::Amp)],
            ],
            end: ParallelEnd::Merge,
        }];
        let (nodes, _) = linear_chain_layout(&stages, metrics);

        let merge = find_node(&nodes, "__merge_1");
        // Split at col 0, longest path = 2 blocks → merge at col 3.
        assert_eq!(merge.x, 300.0);
    }

    #[test]
    fn single_after_parallel_continues_past_merge_column() {
        let metrics = GridMetrics {
            origin_x: 0.0,
            origin_y: 0.0,
            column_spacing: 100.0,
            lane_spacing: 50.0,
        };
        let stages = [
            ChainStage::Parallel {
                lanes: vec![
                    vec![block("l", "L", NodeCategory::Amp)],
                    vec![block("r", "R", NodeCategory::Amp)],
                ],
                end: ParallelEnd::Merge,
            },
            ChainStage::Single(block("rev", "Rev", NodeCategory::Reverb)),
        ];
        let (nodes, _) = linear_chain_layout(&stages, metrics);

        let merge = find_node(&nodes, "__merge_1");
        let rev = find_node(&nodes, "rev");
        assert!(
            rev.x > merge.x,
            "reverb must sit after merge column (got rev.x={}, merge.x={})",
            rev.x,
            merge.x,
        );
    }

    #[test]
    fn empty_parallel_stage_is_skipped() {
        let stages = [
            ChainStage::Single(block("a", "A", NodeCategory::Drive)),
            ChainStage::Parallel {
                lanes: vec![],
                end: ParallelEnd::Merge,
            },
            ChainStage::Single(block("b", "B", NodeCategory::Amp)),
        ];
        let (nodes, edges) = linear_chain_layout(&stages, GridMetrics::default());

        assert_eq!(nodes.len(), 2);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from_id, "a");
        assert_eq!(edges[0].to_id, "b");
    }
}

mod validate_graph_invariants {
    use super::*;

    #[test]
    fn empty_graph_is_valid() {
        let errs = validate_graph(&[], &[]);
        assert!(errs.is_empty());
    }

    #[test]
    fn duplicate_node_id_is_reported() {
        let nodes = vec![
            GraphNode {
                id: "a".into(),
                label: "A".into(),
                category: NodeCategory::Drive,
                kind: NodeKind::Block,
                x: 0.0,
                y: 0.0,
                bypass: false,
            },
            GraphNode {
                id: "a".into(),
                label: "A duplicate".into(),
                category: NodeCategory::Drive,
                kind: NodeKind::Block,
                x: 0.0,
                y: 0.0,
                bypass: false,
            },
        ];
        let errs = validate_graph(&nodes, &[]);
        assert!(
            errs.iter().any(|e| e.contains("duplicate")),
            "got: {errs:?}"
        );
    }

    #[test]
    fn edge_to_missing_node_is_reported() {
        let nodes = vec![GraphNode {
            id: "a".into(),
            label: "A".into(),
            category: NodeCategory::Drive,
            kind: NodeKind::Block,
            x: 0.0,
            y: 0.0,
            bypass: false,
        }];
        let edges = vec![GraphEdge {
            from_id: "a".into(),
            to_id: "ghost".into(),
        }];
        let errs = validate_graph(&nodes, &edges);
        assert!(errs.iter().any(|e| e.contains("ghost")), "got: {errs:?}");
    }

    #[test]
    fn self_loop_is_reported() {
        let nodes = vec![GraphNode {
            id: "a".into(),
            label: "A".into(),
            category: NodeCategory::Drive,
            kind: NodeKind::Block,
            x: 0.0,
            y: 0.0,
            bypass: false,
        }];
        let edges = vec![GraphEdge {
            from_id: "a".into(),
            to_id: "a".into(),
        }];
        let errs = validate_graph(&nodes, &edges);
        assert!(
            errs.iter().any(|e| e.contains("self-loop")),
            "got: {errs:?}"
        );
    }

    #[test]
    fn layout_output_is_always_valid() {
        let stages = [
            ChainStage::Single(block("noise", "Noise", NodeCategory::Dynamics)),
            ChainStage::Single(block("comp", "Comp", NodeCategory::Dynamics)),
            ChainStage::Single(block("od", "OD", NodeCategory::Drive)),
            ChainStage::Parallel {
                lanes: vec![
                    vec![
                        block("amp_l", "Amp L", NodeCategory::Amp),
                        block("dly_l", "Delay L", NodeCategory::Time),
                    ],
                    vec![
                        block("amp_r", "Amp R", NodeCategory::Amp),
                        block("dly_r", "Delay R", NodeCategory::Time),
                    ],
                ],
                end: ParallelEnd::Merge,
            },
            ChainStage::Single(block("rev", "Rev", NodeCategory::Reverb)),
        ];
        let (nodes, edges) = linear_chain_layout(&stages, GridMetrics::default());
        let errs = validate_graph(&nodes, &edges);
        assert!(errs.is_empty(), "layout produced invalid graph: {errs:?}");
    }
}

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
        assert!(
            errs.is_empty(),
            "fan layout produced invalid graph: {errs:?}"
        );
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
            errs.iter()
                .any(|e| e.contains("fan lane 1 of stage 0 is empty")),
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
