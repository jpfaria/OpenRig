//! The chain row must stack per-stream meter bars VERTICALLY (one
//! below the other), not horizontally side-by-side. User feedback
//! after the first per-stream UI commit: "eu nao quero um do lado do
//! outro.. eu quero um embaixo do outro".
//!
//! Source-presence test: chain_row.slint's stream-meter loop must
//! advance `y` (vertical) by stream index, never `x`. The bar height
//! must be a `per-bar-height` derived from stream count.

use std::path::PathBuf;

fn slint() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui/pages/chain_row.slint");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn chain_row_stream_meter_loop_advances_y_not_x() {
    let src = slint();
    assert!(
        src.contains("per-bar-height"),
        "chain_row.slint must compute per-bar-height (vertical stacking)"
    );
    assert!(
        !src.contains("per-bar-width"),
        "chain_row.slint must NOT keep per-bar-width — the user asked \
         for vertical stacking; horizontal layout is the bug"
    );
    // The per-stream loop body for the INPUT mini-bars must offset y
    // by the stream index, not x.
    assert!(
        src.contains("y: 2px + i * (parent.per-bar-height + parent.bar-gap)")
            || src.contains("y: 2px + i * (parent.bar-gap + parent.per-bar-height)"),
        "stream loop must position each bar at increasing y, not x"
    );
}
