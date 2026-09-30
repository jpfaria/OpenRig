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
