//! Tests for the preset bank dropdown search/filter (issue #659).

use super::{filter_preset_labels_indexed, preset_label_matches};

fn labels() -> Vec<String> {
    [
        "CPM 22 - Um Minuto",
        "Pitty - Na Sua Estante",
        "Foo Fighters - Everlong",
        "John Mayer - Gravity (Lead)",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[test]
fn empty_query_returns_all_labels_with_original_slots() {
    let result = filter_preset_labels_indexed(&labels(), "");
    assert_eq!(
        result,
        vec![
            (0, "CPM 22 - Um Minuto".to_string()),
            (1, "Pitty - Na Sua Estante".to_string()),
            (2, "Foo Fighters - Everlong".to_string()),
            (3, "John Mayer - Gravity (Lead)".to_string()),
        ]
    );
}

#[test]
fn whitespace_only_query_is_treated_as_empty() {
    assert_eq!(filter_preset_labels_indexed(&labels(), "   ").len(), 4);
}

#[test]
fn match_preserves_original_slot_not_filtered_position() {
    // "everlong" only matches slot 2. The kept pair MUST carry slot 2,
    // never 0 — selecting it dispatches Preset(2), and a remapped index
    // would switch the wrong preset.
    let result = filter_preset_labels_indexed(&labels(), "everlong");
    assert_eq!(result, vec![(2, "Foo Fighters - Everlong".to_string())]);
}

#[test]
fn match_is_case_insensitive() {
    let result = filter_preset_labels_indexed(&labels(), "PITTY");
    assert_eq!(result, vec![(1, "Pitty - Na Sua Estante".to_string())]);
}

#[test]
fn query_matches_substring_anywhere_in_label() {
    // "gravity" sits mid-label, after the artist name.
    let result = filter_preset_labels_indexed(&labels(), "gravity");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, 3);
}

#[test]
fn no_match_returns_empty() {
    assert!(filter_preset_labels_indexed(&labels(), "metallica").is_empty());
}

#[test]
fn multiple_matches_keep_relative_order_and_slots() {
    // "e" matches slots 1, 2 and 3 (not 0: "CPM 22 - Um Minuto" has none).
    // The kept rows must stay in original order carrying their real slots.
    let result = filter_preset_labels_indexed(&labels(), "e");
    let slots: Vec<usize> = result.iter().map(|(s, _)| *s).collect();
    assert_eq!(slots, vec![1, 2, 3]);
}

#[test]
fn predicate_empty_query_matches_everything() {
    assert!(preset_label_matches("anything", ""));
}

#[test]
fn predicate_requires_lowercased_query_to_match() {
    // The predicate assumes the caller already lowercased the query.
    assert!(preset_label_matches("Foo Fighters", "foo"));
    assert!(!preset_label_matches("Foo Fighters", "xyz"));
}
