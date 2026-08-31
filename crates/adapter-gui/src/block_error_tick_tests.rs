//! #913 — the block-error tick.
//!
//! It runs every 200 ms for the life of the app, so the common case is
//! "nothing to say". What must hold: the off-thread rebuilds are installed on
//! EVERY tick, not only when there is an error to report (#672 — a chain
//! rebuilt off-thread only goes live when this tick swaps it in), and an error
//! storm surfaces one message rather than a wall of toasts.

use super::error_tick;
use application::live_source::{BlockErrorReading, LiveSource};
use application::runtime_control::RuntimeControl;
use domain::ids::{BlockId, ChainId};
use std::cell::RefCell;

/// `BlockErrorReading` is not `Clone` — the queue is drained, never copied —
/// so the fake holds the messages and builds a fresh reading per call.
#[derive(Default)]
struct Errors(Option<Vec<&'static str>>);
impl LiveSource for Errors {
    fn block_errors(&self) -> Option<Vec<BlockErrorReading>> {
        self.0
            .as_ref()
            .map(|messages| messages.iter().map(|m| error(m)).collect())
    }
}

#[derive(Default)]
struct Worker {
    applied: RefCell<usize>,
}
impl RuntimeControl for Worker {
    fn apply_finished_rebuilds(&self) -> usize {
        *self.applied.borrow_mut() += 1;
        0
    }
}

fn error(message: &str) -> BlockErrorReading {
    BlockErrorReading {
        chain: ChainId("chain:0".into()),
        block: BlockId("gain".into()),
        message: message.to_string(),
    }
}

#[test]
fn a_quiet_tick_reports_nothing() {
    assert_eq!(
        error_tick(&Errors(Some(Vec::new())), &Worker::default()),
        None
    );
}

#[test]
fn a_frontend_that_hosts_no_audio_reports_nothing() {
    assert_eq!(error_tick(&Errors(None), &Worker::default()), None);
}

#[test]
fn an_error_is_surfaced_by_its_message() {
    assert_eq!(
        error_tick(
            &Errors(Some(vec!["NAM model failed to load"])),
            &Worker::default()
        ),
        Some("NAM model failed to load".to_string())
    );
}

#[test]
fn an_error_storm_surfaces_one_message_not_a_wall_of_toasts() {
    let storm = Errors(Some(vec!["first", "second", "third"]));
    assert_eq!(
        error_tick(&storm, &Worker::default()),
        Some("first".to_string())
    );
}

#[test]
fn the_finished_rebuilds_are_installed_on_a_quiet_tick_too() {
    // #672: a chain rebuilt off-thread only goes live when this tick swaps it
    // in. Doing that only when an error shows up would leave the edit silent.
    let worker = Worker::default();
    error_tick(&Errors(Some(Vec::new())), &worker);
    error_tick(&Errors(None), &worker);
    assert_eq!(*worker.applied.borrow(), 2);
}

#[test]
fn the_rebuilds_are_installed_before_the_errors_are_read() {
    let worker = Worker::default();
    error_tick(&Errors(Some(vec!["boom"])), &worker);
    assert_eq!(
        *worker.applied.borrow(),
        1,
        "an early return on the error path would strand the rebuild"
    );
}
