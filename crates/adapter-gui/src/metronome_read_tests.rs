//! #913 — reading the metronome the dispatcher owns.
//!
//! The lamp timer compares this once a frame, so a tempo or timbre set from
//! MCP or a MIDI CC follows on screen without any event reaching the window.
//! With no project open the answer must be "nothing" — a default snapshot
//! would draw a metronome that does not exist.

use super::metronome_snapshot;
use crate::state::ProjectSession;
use application::command::{Command, MetronomeCommand};
use project::project::Project;
use std::cell::RefCell;
use std::rc::Rc;

fn session() -> Rc<RefCell<Option<ProjectSession>>> {
    Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-913-metronome-tests"),
    ))))
}

#[test]
fn with_no_project_open_there_is_no_metronome_to_read() {
    let none: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    assert!(metronome_snapshot(&none).is_none());
}

#[test]
fn an_open_project_answers_with_its_metronome() {
    let snapshot = metronome_snapshot(&session()).expect("a project has a metronome");
    assert!(
        !snapshot.running,
        "the app always boots silent — POWER is not persisted"
    );
    assert!(snapshot.settings.bpm > 0.0);
}

#[test]
fn a_tempo_set_on_the_bus_shows_up_in_the_next_read() {
    // This is exactly the path that makes an MCP or MIDI tempo change follow
    // on screen: nothing pushes an event at the window, the timer just reads.
    let session = session();
    {
        let borrowed = session.borrow();
        borrowed
            .as_ref()
            .unwrap()
            .dispatcher
            .dispatch(Command::Metronome(MetronomeCommand::SetMetronomeBpm {
                bpm: 132.0,
            }))
            .expect("set tempo");
    }
    assert_eq!(
        metronome_snapshot(&session).expect("snapshot").settings.bpm,
        132.0
    );
}

#[test]
fn reading_twice_without_a_change_gives_the_same_answer() {
    // The timer only re-renders when the snapshot DIFFERS, so a read that
    // varied on its own would repaint the knobs every frame.
    let session = session();
    assert_eq!(
        metronome_snapshot(&session).map(|s| s.settings.bpm),
        metronome_snapshot(&session).map(|s| s.settings.bpm)
    );
}
