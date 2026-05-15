//! Tests for `adapter-gui::graph_view_model`. Lifted out per project
//! convention — production `.rs` files keep `#[cfg(test)] #[path] mod tests;`
//! at the bottom; the body lives here.

use super::{
    linear_chain_layout, validate_graph, BlockBlueprint, ChainStage, GraphEdge, GraphNode,
    GridMetrics, NodeCategory,
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
    use super::*;
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
        let stages = [ChainStage::Parallel(vec![
            vec![block("l", "L", NodeCategory::Amp)],
            vec![block("r", "R", NodeCategory::Amp)],
        ])];
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

    // GraphView.slint relies on this convention to render split/merge as
    // a small routing dot instead of a full block card. If the layout
    // helper starts emitting labels or a non-Util category for split or
    // merge, the Slint side will draw empty grey rectangles where the
    // wires meet — exactly the regression we hit before this test
    // landed.
    #[test]
    fn split_and_merge_use_routing_node_convention() {
        let stages = [ChainStage::Parallel(vec![
            vec![block("l", "L", NodeCategory::Amp)],
            vec![block("r", "R", NodeCategory::Amp)],
        ])];
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
        let stages = [ChainStage::Parallel(vec![
            vec![block("l", "L", NodeCategory::Amp)],
            vec![block("r", "R", NodeCategory::Amp)],
        ])];
        let (nodes, _) = linear_chain_layout(&stages, metrics);

        let l = find_node(&nodes, "l");
        let r = find_node(&nodes, "r");

        // Two paths → lane offsets are -0.5 and +0.5 around the centre.
        assert_eq!(l.y, -40.0);
        assert_eq!(r.y, 40.0);
    }

    #[test]
    fn split_connects_to_each_path_first_block() {
        let stages = [ChainStage::Parallel(vec![
            vec![block("l", "L", NodeCategory::Amp)],
            vec![block("r", "R", NodeCategory::Amp)],
        ])];
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
        let stages = [ChainStage::Parallel(vec![
            vec![block("l", "L", NodeCategory::Amp)],
            vec![block("r", "R", NodeCategory::Amp)],
        ])];
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
        let stages = [ChainStage::Parallel(vec![
            vec![
                block("l1", "L1", NodeCategory::Amp),
                block("l2", "L2", NodeCategory::Time),
            ],
            vec![block("r", "R", NodeCategory::Amp)],
        ])];
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
            ChainStage::Parallel(vec![
                vec![block("l", "L", NodeCategory::Amp)],
                vec![block("r", "R", NodeCategory::Amp)],
            ]),
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
            ChainStage::Parallel(vec![]),
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
                x: 0.0,
                y: 0.0,
                bypass: false,
            },
            GraphNode {
                id: "a".into(),
                label: "A duplicate".into(),
                category: NodeCategory::Drive,
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
            ChainStage::Parallel(vec![
                vec![
                    block("amp_l", "Amp L", NodeCategory::Amp),
                    block("dly_l", "Delay L", NodeCategory::Time),
                ],
                vec![
                    block("amp_r", "Amp R", NodeCategory::Amp),
                    block("dly_r", "Delay R", NodeCategory::Time),
                ],
            ]),
            ChainStage::Single(block("rev", "Rev", NodeCategory::Reverb)),
        ];
        let (nodes, edges) = linear_chain_layout(&stages, GridMetrics::default());
        let errs = validate_graph(&nodes, &edges);
        assert!(errs.is_empty(), "layout produced invalid graph: {errs:?}");
    }
}
