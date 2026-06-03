//! User screenshot 24 May 2026: "eh so ter uma linha dessa para cada
//! guitarra porra..". The whole INPUT-bar + OUTPUT-bar row must repeat
//! per stream (one full row per source), not little stacked sub-bars
//! inside a single shared row.
//!
//! Source-presence test: chain_row.slint's stream-meter loop wraps
//! BOTH the INPUT and OUTPUT rectangles per iteration, and uses the
//! per-stream `stream.in_dbfs` / `stream.out_dbfs` for each row's bar
//! AND dB text (not the chain-level aggregate).

use std::path::PathBuf;

fn slint() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/pages/chain_row.slint");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn each_stream_renders_full_row_with_per_stream_db_text() {
    let src = slint();
    // The dB text on the right of each bar must read the per-stream
    // value, otherwise multi-stream chains all show the same number.
    assert!(
        src.contains("stream.in_dbfs <= -119.0") && src.contains("stream.out_dbfs <= -119.0"),
        "stream-meter loop must use per-stream dB text \
         (`stream.in_dbfs` / `stream.out_dbfs`), not the chain aggregate"
    );
    // No remaining reference to the aggregate scalars inside the
    // meter-row area (they used to drive the dB text):
    let bottom_section_start = src
        .find("Per-stream INPUT + OUTPUT bars")
        .or_else(|| src.find("Per-stream: one full row per stream"))
        .expect("chain_row.slint must contain the per-stream rendering block");
    let bottom_section = &src[bottom_section_start..];
    assert!(
        !bottom_section.contains("root.chain.meter_in_dbfs")
            && !bottom_section.contains("root.chain.meter_out_dbfs"),
        "the per-stream rendering must NOT read the chain-level \
         aggregate (`root.chain.meter_in_dbfs` / `meter_out_dbfs`)"
    );
    // The old per-bar-height tuning is gone — each stream is a full
    // 16 px tall bar inside its own row.
    assert!(
        !bottom_section.contains("per-bar-height"),
        "drop the per-bar-height squeeze trick — each stream gets a \
         full-height bar in its own row"
    );
}
