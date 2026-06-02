//! Task 7 — RED-FIRST tests for the per-chain DI loop source list builder
//! and the selected-string → DiLoopSource mapper.
//!
//! Both functions are pure (no I/O, no AppWindow). They live in
//! `adapter_gui::di_loop_ui_sources`.
//!
//! ## build_di_loop_sources(bundled_ids)
//! Returns a Vec<String> where:
//!   - Each bundled id appears as a display entry (first elements).
//!   - A "Choose file…" sentinel appears LAST.
//!
//! ## parse_di_loop_source(selected, bundled_ids)
//! Maps a selected entry string back to `DiLoopSource`:
//!   - A bundled id → `Some(DiLoopSource::Bundled(id))`
//!   - "Choose file…" sentinel → `None` (caller opens the file picker)
//!   - Unknown string → `None`

use application::di_loader::DiLoopSource;

use adapter_gui::di_loop_ui_sources::{build_di_loop_sources, parse_di_loop_source, CHOOSE_FILE_SENTINEL};

// ── build_di_loop_sources ────────────────────────────────────────────────────

#[test]
fn build_sources_with_bundled_ids_appends_sentinel_last() {
    let ids = vec!["dry_guitar_1", "dry_guitar_2"];
    let sources = build_di_loop_sources(&ids);

    assert_eq!(sources.len(), 3, "2 bundled + sentinel");
    assert_eq!(sources[0], "dry_guitar_1");
    assert_eq!(sources[1], "dry_guitar_2");
    assert_eq!(sources[2], CHOOSE_FILE_SENTINEL);
}

#[test]
fn build_sources_with_no_bundled_ids_returns_only_sentinel() {
    let sources = build_di_loop_sources(&[]);
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0], CHOOSE_FILE_SENTINEL);
}

// ── parse_di_loop_source ─────────────────────────────────────────────────────

#[test]
fn parse_bundled_id_returns_bundled_source() {
    let ids = vec!["dry_guitar_1", "dry_guitar_2"];
    let result = parse_di_loop_source("dry_guitar_1", &ids);
    assert!(
        matches!(result, Some(DiLoopSource::Bundled(ref id)) if id == "dry_guitar_1"),
        "expected Some(Bundled(dry_guitar_1)), got {result:?}"
    );
}

#[test]
fn parse_sentinel_returns_none() {
    let ids = vec!["dry_guitar_1"];
    let result = parse_di_loop_source(CHOOSE_FILE_SENTINEL, &ids);
    assert!(result.is_none(), "sentinel must return None (caller opens file picker)");
}

#[test]
fn parse_unknown_string_returns_none() {
    let ids = vec!["dry_guitar_1"];
    let result = parse_di_loop_source("not_a_real_id", &ids);
    assert!(result.is_none(), "unknown string must return None");
}

#[test]
fn parse_empty_string_returns_none() {
    let result = parse_di_loop_source("", &[]);
    assert!(result.is_none());
}
