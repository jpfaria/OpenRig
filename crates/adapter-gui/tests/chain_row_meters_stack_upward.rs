//! Per-stream meter rows must fit INSIDE the chain card.
//!
//! Bug: `y: root.height - 22px + i * 22px` stacks downward from the
//! original meter slot. With the chain card grown by `(N-1)*22px`, the
//! last meter row (i = N-1) lands at `root.height - 22 + (N-1)*22 =
//! root.height + (N-2)*22`, i.e. (N-2)*22 px PAST the card bottom for
//! N>1. The rows visibly overlap the next chain card.
//!
//! Fix shape: stack the rows UPWARD from the original slot so the last
//! row (i = N-1) lands exactly at `root.height - 22px` (original
//! position) and previous rows are above it inside the grown card.
//!
//! Source-presence test: the y formula must subtract by
//! `(stream_meters.length - 1 - i)` (stacks upward), not add by `i`.

use std::path::PathBuf;

fn slint() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/pages/chain_row.slint");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn stream_meter_rows_stack_upward_inside_card() {
    let src = slint();
    assert!(
        src.contains("root.chain.stream_meters.length - 1 - i"),
        "stream-meter row y formula must subtract `(length - 1 - i) * 22px` \
         so the last row lands at the original meter position and the rest \
         stack above it INSIDE the grown card"
    );
    assert!(
        !src.contains("y: root.height - 22px + i * 22px"),
        "drop the downward-stack formula (`+ i * 22px`); it pushes rows \
         past the card bottom"
    );
}
