//! Issue #532: chain stream_meter row count must reflect the project's
//! input-entry count, not the engine's transient stream readings.
//!
//! Two user-observed symptoms in one screenshot:
//!
//! 1. After switching the preset on a chain, the chain's footer grows
//!    new INPUT/OUTPUT rows on every switch (6+ stacked rows on a chain
//!    that has a single XLR input).
//! 2. After toggling/activating a chain, sibling chains lose their
//!    INPUT/OUTPUT rows entirely.
//!
//! Both shapes point at the same logic gap in `start_meter_polling`:
//! the timer writes `stream_meters = per_stream_rows` straight from
//! the engine's per-stream readings (lines 417–453 of `meter_wiring.rs`).
//! If the engine transiently reports a different stream count than the
//! project's `InputBlock.entries.len()` sum (e.g. mid-rebuild after a
//! preset switch, or 0 while a sibling re-spawns), the row's
//! `stream_meters` model gets resized to the engine's number — which
//! makes the UI duplicate or vanish rows that should be stable.
//!
//! The correct invariant: the meter row count is owned by the PROJECT
//! state (one slot per input entry, min 1). The engine readings fill
//! those slots; missing readings become SILENT. The timer never grows
//! or shrinks the row past the project-derived count.
//!
//! These tests exercise a pure helper `rebuild_stream_meters_row`
//! that the timer must use to produce the row payload. Writing this
//! file first (against an API that does not exist yet) is the RED that
//! gates the implementation, per CLAUDE.md / docs/testing.md.

use super::meter_wiring::{rebuild_stream_meters_row, StreamMeterReading, METER_POLL_TICK_MS};
use crate::StreamMeter;
use engine::output_meter::SILENT_DBFS;

fn reading(in_dbfs: f32, out_dbfs: f32) -> StreamMeterReading {
    StreamMeterReading { in_dbfs, out_dbfs }
}

/// #715: the meter poll must not run faster than ~20 Hz. Its per-frame memory
/// traffic competes with the audio worker on the shared cache and evicts the
/// NAM weights → cold-cache inference → late buffer → crackle (reproduced in
/// engine/tests/issue_715_nam_cache_eviction). 30 Hz (33 ms) caused it; this
/// guards against regressing back to a fast meter refresh.
#[test]
fn meter_poll_is_not_faster_than_20hz() {
    assert!(
        METER_POLL_TICK_MS >= 50,
        "meter poll tick {METER_POLL_TICK_MS}ms is faster than 20 Hz — its memory \
         traffic evicts the audio worker's NAM weights (issue #715 crackle)"
    );
}

#[test]
fn row_length_equals_project_input_count_when_engine_matches() {
    // Steady state: project says 2 input entries, engine reports 2
    // streams. Row is 2 entries with the engine values.
    let readings = vec![reading(-6.0, -12.0), reading(-3.0, -9.0)];
    let row: Vec<StreamMeter> = rebuild_stream_meters_row(&readings, 2, 100.0, true);
    assert_eq!(row.len(), 2);
    assert!((row[0].in_dbfs - (-6.0)).abs() < 0.05);
    assert!((row[0].out_dbfs - (-12.0)).abs() < 0.05);
    assert!((row[1].in_dbfs - (-3.0)).abs() < 0.05);
    assert!((row[1].out_dbfs - (-9.0)).abs() < 0.05);
}

#[test]
fn row_length_stays_at_project_count_when_engine_reports_more_streams() {
    // User-observed bug A: after switching preset on the DEFAULT chain
    // the footer accumulates extra INPUT/OUTPUT rows (6 visible in the
    // screenshot for a chain with a single input). Engine reports
    // more streams than the project owns — the UI must NOT grow past
    // the project count, otherwise every preset switch leaks rows.
    let readings = vec![
        reading(-6.0, -12.0),
        reading(-6.0, -12.0),
        reading(-6.0, -12.0),
        reading(-6.0, -12.0),
        reading(-6.0, -12.0),
        reading(-6.0, -12.0),
    ];
    let row: Vec<StreamMeter> = rebuild_stream_meters_row(&readings, 1, 100.0, true);
    assert_eq!(
        row.len(),
        1,
        "row count must follow project input count (1), \
         not engine transient stream count (6) — issue #532 symptom A"
    );
}

#[test]
fn row_length_stays_at_project_count_when_engine_reports_zero_streams() {
    // User-observed bug B: after activating one chain, sibling chains
    // lose their INPUT/OUTPUT rows entirely. The engine transiently
    // reports zero streams for the sibling while the controller is
    // rebuilt; the UI must NOT collapse the row — it must keep
    // project-derived slots showing SILENT.
    let row: Vec<StreamMeter> = rebuild_stream_meters_row(&[], 1, 100.0, true);
    assert_eq!(
        row.len(),
        1,
        "row count must stay at project input count (1) when engine \
         reports nothing — issue #532 symptom B"
    );
    assert_eq!(
        row[0].in_dbfs, SILENT_DBFS,
        "missing reading slot must read SILENT"
    );
    assert_eq!(
        row[0].out_dbfs, SILENT_DBFS,
        "missing reading slot must read SILENT"
    );
}

#[test]
fn row_pads_missing_slots_with_silent_when_engine_reports_fewer_streams() {
    // Multi-input chain: project has 3 input entries, engine has
    // only finished spinning up 1 stream so far. The active stream
    // shows its reading; the remaining slots stay SILENT so the row
    // stays at the project-derived 3 entries.
    let readings = vec![reading(-6.0, -12.0)];
    let row: Vec<StreamMeter> = rebuild_stream_meters_row(&readings, 3, 100.0, true);
    assert_eq!(row.len(), 3, "row length follows project input count");
    assert!((row[0].in_dbfs - (-6.0)).abs() < 0.05);
    assert_eq!(row[1].in_dbfs, SILENT_DBFS);
    assert_eq!(row[2].in_dbfs, SILENT_DBFS);
    assert_eq!(row[1].out_dbfs, SILENT_DBFS);
    assert_eq!(row[2].out_dbfs, SILENT_DBFS);
}

#[test]
fn row_min_length_is_one_when_project_input_count_is_zero() {
    // Defensive: project with no input entries yet still gets a single
    // SILENT slot so the UI never shows "no meter" for a chain card
    // that's mid-construction. Mirrors the `.max(1)` clamp in
    // `replace_project_chains`.
    let row: Vec<StreamMeter> = rebuild_stream_meters_row(&[], 0, 100.0, true);
    assert_eq!(
        row.len(),
        1,
        "min 1 silent slot mirrors replace_project_chains clamp"
    );
    assert_eq!(row[0].in_dbfs, SILENT_DBFS);
    assert_eq!(row[0].out_dbfs, SILENT_DBFS);
}

#[test]
fn disabled_chain_yields_no_meter_rows() {
    // #750: the per-stream graph is a LIVE surface — a disabled chain must
    // render ZERO rows even while a stale tap still reports readings and the
    // project resolves several inputs. Otherwise the timer re-grows the footer
    // a tick after the chain is switched off (the `.max(1)` clamp would keep a
    // phantom row), so the graph "sticks" on a disabled chain.
    let readings = vec![reading(-6.0, -12.0), reading(-3.0, -9.0)];
    let row: Vec<StreamMeter> = rebuild_stream_meters_row(&readings, 4, 100.0, false);
    assert!(
        row.is_empty(),
        "disabled chain must produce no meter rows, got {}",
        row.len()
    );
}

#[test]
fn row_applies_chain_volume_to_output_only() {
    // Same volume-compensation rule the timer already enforces: the
    // OUTPUT reading is scaled by `20·log10(volume_pct/100)` because
    // the stream_tap reads BEFORE the audio callback applies the
    // chain volume slider. The INPUT reading is untouched. Issue #496
    // — must be preserved here.
    let readings = vec![reading(-6.0, -12.0)];
    let row: Vec<StreamMeter> = rebuild_stream_meters_row(&readings, 1, 200.0, true);
    assert!(
        (row[0].in_dbfs - (-6.0)).abs() < 0.05,
        "input is not scaled"
    );
    // -12 dBFS + 20·log10(2.0) = -12 + 6 = -6 dBFS
    assert!(
        (row[0].out_dbfs - (-6.0)).abs() < 0.1,
        "output must compensate for chain volume (200% → +6 dB); got {}",
        row[0].out_dbfs
    );
}
