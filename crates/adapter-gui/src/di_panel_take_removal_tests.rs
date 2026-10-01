//! The open DI panel drops a deleted take at once — it
//! shows a snapshot of the chain's list, so without this the row would sit
//! there, pointing at a file that is gone, until the panel is reopened.

use super::*;

fn list(sources: &[&str], take_rows: &[bool], selected: i32, playing: bool) -> DiPanelList {
    DiPanelList {
        sources: sources.iter().map(|s| s.to_string()).collect(),
        take_rows: take_rows.to_vec(),
        selected,
        playing,
    }
}

const SOURCES: [&str; 4] = ["funk", "riff.wav", "verse.wav", "Choose file…"];
const TAKES: [bool; 4] = [false, true, true, false];

#[test]
fn the_deleted_take_leaves_the_list() {
    let after = without_take(list(&SOURCES, &TAKES, -1, false), "riff.wav");

    assert_eq!(
        after,
        list(
            &["funk", "verse.wav", "Choose file…"],
            &[false, true, false],
            -1,
            false
        )
    );
}

#[test]
fn a_selection_below_the_deleted_row_follows_its_row_up() {
    let after = without_take(list(&SOURCES, &TAKES, 2, true), "riff.wav");

    assert_eq!((after.selected, after.playing), (1, true));
}

#[test]
fn a_selection_above_the_deleted_row_stays() {
    let after = without_take(list(&SOURCES, &TAKES, 0, true), "verse.wav");

    assert_eq!((after.selected, after.playing), (0, true));
}

#[test]
fn deleting_the_selected_take_clears_the_selection_and_stops() {
    // The dispatcher unloads and disarms it, so the panel must not keep
    // showing it as playing.
    let after = without_take(list(&SOURCES, &TAKES, 1, true), "riff.wav");

    assert_eq!((after.selected, after.playing), (-1, false));
}

#[test]
fn a_label_that_is_not_a_take_row_changes_nothing() {
    let before = list(&SOURCES, &TAKES, 0, true);

    assert_eq!(without_take(before.clone(), "funk"), before);
    assert_eq!(without_take(before.clone(), "ghost.wav"), before);
}
