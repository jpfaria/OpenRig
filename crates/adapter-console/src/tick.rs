//! One turn of the console's event loop.
//!
//! The console's only heartbeat is this tick, so it owns the same duty the
//! GUI's 16 ms timer has: surface BOTH what the bridge drained and what
//! off-thread command work finished. Commands whose heavy part runs on its own
//! task (#693 — DI decode, offline render, the tone diagnosis) publish nothing
//! at dispatch time; their result exists only in `poll_async_results`. Without
//! draining it here those commands are accepted and then silently never
//! complete for any client on this transport.

use application::dispatcher::CommandDispatcher;
use application::event::Event;

/// Everything observable this turn: what the bridge drained, plus the
/// completions of off-thread command work.
pub fn tick(dispatcher: &dyn CommandDispatcher, from_bridge: Vec<Event>) -> Vec<Event> {
    let mut events = from_bridge;
    events.extend(dispatcher.poll_async_results());
    events
}

#[cfg(test)]
#[path = "tick_tests.rs"]
mod tests;
