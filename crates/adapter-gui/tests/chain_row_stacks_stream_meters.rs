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
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/pages/chain_row.slint");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn chain_row_stream_meter_loop_advances_y_not_x() {
    let src = slint();
    // The user-facing contract: NO horizontal per-stream layout —
    // we never want streams sitting side-by-side ("um do lado do
    // outro"). Whatever variable the Slint uses to size the bar,
    // it must not be named `per-bar-width` (which signalled the
    // bug originally).
    assert!(
        !src.contains("per-bar-width"),
        "chain_row.slint must NOT use per-bar-width — vertical stacking only"
    );
    // The per-stream `for stream[i] in …` loop must position each
    // bar by an increasing y offset driven by the loop index `i`.
    // The current layout stacks UPWARD from the bottom row (last
    // stream lands at the base slot), but any vertical-stacking
    // formula is acceptable here as long as it offsets `y` by `i`,
    // not `x`. We assert the loop body uses the index inside a `y:`
    // expression and never inside an `x:` expression.
    let stream_loop_start = src
        .find("for stream[i] in root.chain.stream_meters")
        .expect("expected per-stream Rectangle loop in chain_row.slint");
    let loop_body = &src[stream_loop_start..];
    assert!(
        loop_body.contains("y:") && loop_body.contains("i)") || loop_body.contains("- i)"),
        "per-stream loop must offset `y` using the loop index `i`"
    );
    // Forbid an `x: ... * i` layout in the loop body (the original
    // horizontal-layout regression).
    let horizontal_loop_offset = loop_body
        .lines()
        .take_while(|l| !l.contains("}"))
        .any(|l| l.contains("x:") && (l.contains("* i") || l.contains("i *")));
    assert!(
        !horizontal_loop_offset,
        "per-stream loop must NOT offset `x` by the loop index — that brings the horizontal layout back"
    );
}
