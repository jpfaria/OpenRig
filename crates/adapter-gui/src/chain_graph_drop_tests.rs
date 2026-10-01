//! #328 (spec §5.1, §7 "drop-target resolution") — the row answers GraphView's
//! `resolve-drop-anchor` from the chain's own layout.

use super::*;
use crate::chain_graph_fixtures_tests::mix_chain;

// mix_chain on the row grid: a1 (446, 50) · a2 (578, 50) · b1 (446, 158) ·
// mixer (710, 104). Wire midpoints: a1 → a2 (512, 50) = "path:sp:0:1",
// b1 → mixer (578, 131) = "path:sp:1:1". Reach: half a column, 66 px.

#[test]
fn a_lane_a_card_dropped_by_the_end_of_lane_b_lands_there() {
    assert_eq!(
        drop_anchor_id(&mix_chain(), "a1", 578.0, 135.0),
        "path:sp:1:1"
    );
}

#[test]
fn a_card_on_its_own_wire_or_out_of_reach_lands_nowhere() {
    assert_eq!(
        drop_anchor_id(&mix_chain(), "a1", 512.0, 50.0),
        "",
        "a1 → a2 is a1's own wire"
    );
    assert_eq!(drop_anchor_id(&mix_chain(), "a1", 2000.0, 2000.0), "");
}

#[test]
fn a_split_card_dropped_past_post_lands_there() {
    // post → output midpoint: "top:4".
    assert_eq!(
        drop_anchor_id(&mix_chain(), "__split_sp", 908.0, 104.0),
        "top:4"
    );
}

#[test]
fn a_split_card_on_its_own_place_or_paths_lands_nowhere() {
    assert_eq!(
        drop_anchor_id(&mix_chain(), "__split_sp", 776.0, 104.0),
        "",
        "mixer → post is the gap right after the split"
    );
    assert_eq!(
        drop_anchor_id(&mix_chain(), "__split_sp", 578.0, 131.0),
        "",
        "its own path B"
    );
}

#[test]
fn an_io_card_does_not_move() {
    assert_eq!(drop_anchor_id(&mix_chain(), "__io_input", 908.0, 104.0), "");
}
