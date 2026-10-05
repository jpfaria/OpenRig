use super::filter_picks;
use crate::drums_view::DrumPick;

fn choice(key: &str, label: &str) -> DrumPick {
    DrumPick {
        key: key.into(),
        label: label.into(),
        header: false,
    }
}

fn header(label: &str) -> DrumPick {
    DrumPick {
        key: String::new(),
        label: label.into(),
        header: true,
    }
}

fn grooves() -> Vec<DrumPick> {
    vec![
        header("blues"),
        choice("blues-01", "Blues 1 (shuffle)"),
        header("rock"),
        choice("rock-01", "Rock 1"),
        choice("rock-02", "Rock 2 (shuffle)"),
        header("jazz"),
        choice("jazz-01", "Jazz 1"),
    ]
}

fn labels(picks: Vec<DrumPick>) -> Vec<String> {
    picks.into_iter().map(|p| p.label).collect()
}

#[test]
fn an_empty_query_keeps_every_row() {
    assert_eq!(filter_picks(&grooves(), "  "), grooves());
}

#[test]
fn a_query_matches_labels_ignoring_case() {
    let kits = vec![
        choice("black-pearl", "Black Pearl"),
        choice("red", "Red Zeppelin"),
    ];
    assert_eq!(labels(filter_picks(&kits, "ZEP")), vec!["Red Zeppelin"]);
}

#[test]
fn a_matching_groove_keeps_its_genre_header() {
    assert_eq!(
        labels(filter_picks(&grooves(), "shuffle")),
        vec!["blues", "Blues 1 (shuffle)", "rock", "Rock 2 (shuffle)"]
    );
}

#[test]
fn a_query_naming_a_genre_keeps_the_whole_genre() {
    assert_eq!(
        labels(filter_picks(&grooves(), "Rock")),
        vec!["rock", "Rock 1", "Rock 2 (shuffle)"]
    );
}

#[test]
fn a_query_that_matches_nothing_leaves_no_rows() {
    assert!(filter_picks(&grooves(), "polka").is_empty());
}
