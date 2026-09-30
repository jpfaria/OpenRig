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
    assert!(
        row.contains("visible: !ChainGraphBridge.graph-enabled;"),
        "the strip stays for touch"
    );
}

#[test]
fn the_row_height_follows_the_lane_count_in_graph_mode() {
    let row = read("ui/pages/chain_row.slint");
    assert!(row.contains("root.chain.graph_lanes"));
    let at = row.find("height: 106px").expect("ChainRow height formula");
    assert!(
        row[at..at + 80].contains("content-row-count"),
        "height must use content-row-count"
    );
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
    assert!(
        read("ui/pages/chain_row.slint").contains("markers-visible: root.midi-selection-active;")
    );
}
