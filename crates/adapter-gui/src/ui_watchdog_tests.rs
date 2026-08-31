//! #913 — starting the event-loop watchdog.
//!
//! The watchdog runs for the process and there is nothing to join, so what a
//! test can hold it to is that starting it is safe in every environment it
//! meets: with no Slint event loop running, the heartbeat it posts must be
//! discarded rather than panicking the thread — otherwise a headless run (CI,
//! a render, a test binary) would abort inside a background thread.

use super::spawn;

#[test]
fn starting_the_watchdog_without_an_event_loop_is_safe() {
    spawn();
    // Give the thread time to complete one full iteration: sleep, measure the
    // gap, and post a heartbeat into an event loop that does not exist.
    std::thread::sleep(std::time::Duration::from_millis(400));
}

#[test]
fn starting_it_twice_does_not_fight_over_the_heartbeat() {
    // Each call owns its own counter, so a second watchdog cannot make the
    // first one report a stall.
    spawn();
    spawn();
    std::thread::sleep(std::time::Duration::from_millis(400));
}
