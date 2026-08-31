//! #913 — the metronome's read seam, on its own.
//!
//! The click is an independent pipeline (invariant #4): its reading depends on
//! no chain, no project row and no analyzer session. What must hold is that the
//! seam answers safely with no runtime attached — the wiring reads the beat on
//! every tick, including before a project is open, and a seam that unwrapped
//! there would take the window down at launch.

use super::metronome_live_source;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn with_no_runtime_there_is_no_beat_to_report() {
    let runtime = Rc::new(RefCell::new(None));
    let source = metronome_live_source(&runtime);
    assert!(source.metronome().is_none());
}

#[test]
fn the_seam_shares_the_runtime_handle_instead_of_copying_it() {
    let runtime = Rc::new(RefCell::new(None));
    let before = Rc::strong_count(&runtime);
    let source = metronome_live_source(&runtime);
    assert_eq!(
        Rc::strong_count(&runtime),
        before + 1,
        "the seam must hold the SAME handle the app allocated"
    );
    drop(source);
    assert_eq!(Rc::strong_count(&runtime), before);
}

#[test]
fn the_seam_can_be_read_repeatedly_without_a_runtime() {
    let runtime = Rc::new(RefCell::new(None));
    let source = metronome_live_source(&runtime);
    for _ in 0..3 {
        assert!(source.metronome().is_none());
    }
}
