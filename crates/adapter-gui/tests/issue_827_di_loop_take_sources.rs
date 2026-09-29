//! #827, red-first: saved looper takes are DI loop sources on every chain.
//!
//! The DI picker lists the bundled loops, then every take in the app-wide
//! library (by file name), then the file the user picked by hand (when it is
//! not one of those), then "Choose file…". Picking a take is a
//! `DiLoopSource::File` pointing into the library — the same source a hand
//! picked file is, so the DI plays it with no second code path.

use std::path::PathBuf;

use adapter_gui::di_loop_ui_sources::{
    build_di_loop_sources_with_takes, di_loop_selected_index, parse_di_loop_source_with_takes,
    CHOOSE_FILE_SENTINEL,
};
use application::di_loader::DiLoopSource;

fn takes() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/lib/looper-takes/riff.wav"),
        PathBuf::from("/lib/looper-takes/verse.wav"),
    ]
}

#[test]
fn the_picker_lists_bundled_loops_then_saved_takes_then_choose_file() {
    let sources = build_di_loop_sources_with_takes(&["strat"], &takes(), None);
    assert_eq!(
        sources,
        vec!["strat", "riff.wav", "verse.wav", CHOOSE_FILE_SENTINEL]
    );
}

#[test]
fn a_hand_picked_file_is_listed_after_the_takes() {
    let loaded = DiLoopSource::File(PathBuf::from("/music/ambience.wav"));
    let sources = build_di_loop_sources_with_takes(&[], &takes(), Some(&loaded));
    assert_eq!(
        sources,
        vec![
            "riff.wav",
            "verse.wav",
            "ambience.wav",
            CHOOSE_FILE_SENTINEL
        ]
    );
}

#[test]
fn a_loaded_take_is_not_listed_twice_and_is_the_selected_row() {
    let loaded = DiLoopSource::File(PathBuf::from("/lib/looper-takes/verse.wav"));
    let sources = build_di_loop_sources_with_takes(&["strat"], &takes(), Some(&loaded));
    assert_eq!(
        sources,
        vec!["strat", "riff.wav", "verse.wav", CHOOSE_FILE_SENTINEL]
    );
    assert_eq!(di_loop_selected_index(&sources, &loaded), 2);
}

#[test]
fn picking_a_take_selects_its_file_in_the_library() {
    assert_eq!(
        parse_di_loop_source_with_takes("verse.wav", &["strat"], &takes()),
        Some(DiLoopSource::File(PathBuf::from(
            "/lib/looper-takes/verse.wav"
        )))
    );
}

#[test]
fn picking_a_bundled_loop_still_selects_the_bundled_loop() {
    assert_eq!(
        parse_di_loop_source_with_takes("strat", &["strat"], &takes()),
        Some(DiLoopSource::Bundled("strat".into()))
    );
}

#[test]
fn choose_file_and_unknown_labels_select_nothing() {
    assert_eq!(
        parse_di_loop_source_with_takes(CHOOSE_FILE_SENTINEL, &["strat"], &takes()),
        None
    );
    assert_eq!(
        parse_di_loop_source_with_takes("gone.wav", &["strat"], &takes()),
        None
    );
}
