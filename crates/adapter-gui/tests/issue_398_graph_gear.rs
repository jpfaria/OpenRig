//! #398 — the chains graph draws every block as its piece of gear (stomp box,
//! amp head, cab, rack unit, expression pedal), the input and output as
//! jacks, the split and mixer as the routing hub, on a dotted well with thin
//! wires coloured by lane.

use std::path::PathBuf;

fn ui(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn gear_art_draws_the_five_pieces_of_gear() {
    let src = ui("components/gear_art.slint");
    assert!(src.starts_with("// Responsibility: "));
    for piece in ["\"stomp\"", "\"head\"", "\"cab\"", "\"rack\"", "\"exp\""] {
        assert!(src.contains(piece), "gear art misses the {piece} piece");
    }
    assert!(src.contains("BrandLogo {"), "the gear prints its brand");
    assert!(
        src.contains("Theme.knob-option-font"),
        "text printed on the gear uses the knob-option size (#954)"
    );
}

#[test]
fn the_graph_card_draws_gear_jacks_and_hubs() {
    let card = ui("components/graph_node_card.slint");
    assert!(card.contains("GearArt {"), "a block node is its gear");
    assert!(card.contains("hub-graph-split.svg"));
    assert!(card.contains("hub-graph-mix.svg"));
    assert!(
        card.contains("@radial-gradient("),
        "input and output are drawn as jacks"
    );
    assert!(
        !card.contains("led-red.png"),
        "the old tile LED sprite is gone"
    );
    for svg in ["hub-graph-split.svg", "hub-graph-mix.svg"] {
        let art = ui(&format!("assets/{svg}"));
        assert!(!art.contains("<text"), "{svg} must not carry text");
    }
}

#[test]
fn the_graph_wires_are_thin_and_coloured_by_lane() {
    let view = ui("components/graph_view.slint");
    assert!(view.contains("Theme.wire-width * root.zoom"));
    assert!(view.contains("edge.path == 0 ? Theme.accent"));
    assert!(view.contains("Theme.dot"), "the canvas is a dotted well");
    let types = ui("components/graph_view_types.slint");
    assert!(types.contains("path: int,"), "an edge names its lane");
    let wire = ui("components/graph_wire.slint");
    assert!(wire.contains("in property <color> stroke_color"));
}

#[test]
fn the_graph_cards_fit_the_gear() {
    let row = ui("pages/chain_row_graph.slint");
    assert!(row.contains("node_width: 120px;"));
    assert!(row.contains("node_height: 84px;"));
    let theme = ui("theme.slint");
    assert!(theme.contains("out property <color> dot:"));
}
