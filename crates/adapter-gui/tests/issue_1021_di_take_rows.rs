//! The DI picker knows which of its entries are saved takes
//! (deletable) — never a bundled loop, a hand-picked file outside the library,
//! or the "Choose file…" sentinel.

use std::path::PathBuf;

use adapter_gui::di_loop_ui_sources::{di_loop_take_rows, CHOOSE_FILE_SENTINEL};

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn only_the_entries_of_the_take_library_are_takes() {
    let sources = s(&["funk", "riff.wav", "my-own.wav", CHOOSE_FILE_SENTINEL]);
    let takes = vec![PathBuf::from("/lib/looper-takes/riff.wav")];

    assert_eq!(
        di_loop_take_rows(&sources, &takes),
        vec![false, true, false, false]
    );
}

#[test]
fn an_empty_library_marks_nothing() {
    let sources = s(&["funk", CHOOSE_FILE_SENTINEL]);
    assert_eq!(di_loop_take_rows(&sources, &[]), vec![false, false]);
}
