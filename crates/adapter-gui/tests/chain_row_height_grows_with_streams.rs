//! Multi-stream chains overflow the chain row card — each extra
//! stream adds a 22 px row but `ChainRow.height` doesn't account for
//! it, so the meters bleed into the chain row below (user screenshot
//! 24 May 2026: "tem que dar espaço..").
//!
//! Source-presence test: the ChainRow's `height:` formula must
//! include a `stream_meters.length` term so the card grows with
//! stream count.

use std::path::PathBuf;

fn slint() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui/pages/chain_row.slint");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn chain_row_outer_height_includes_stream_count() {
    let src = slint();
    // The ChainRow's `height:` line (formula near the top of the
    // component) must reference `stream_meters.length` so the card
    // grows when a chain has multiple streams. Otherwise the per-
    // stream meter rows render outside the card and overlap the next
    // chain row.
    let height_section_idx = src
        .find("height: 106px")
        .expect("chain_row.slint should define the ChainRow height formula");
    let section = &src[height_section_idx..height_section_idx + 200];
    assert!(
        section.contains("stream_meters.length"),
        "ChainRow.height must include stream_meters.length term: {section:?}"
    );
}
