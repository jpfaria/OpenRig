//! `CommandDispatcher` decorator: dispatches via an inner `LocalDispatcher`,
//! then fans every resulting event batch out to an [`EventSink`]. This is the
//! single point where every frontend's command path becomes observable by
//! transports (MCP notifications). GUI- and MCP-originated commands both flow
//! through the one dispatcher a frontend holds, so wrapping it here captures
//! every state change with no per-call-site instrumentation.

use anyhow::Result;

use crate::bridge::EventSink;
use crate::command::Command;
use crate::dispatcher::{CommandDispatcher, EventStream};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

pub struct PublishingDispatcher {
    inner: LocalDispatcher,
    sink: EventSink,
}

impl PublishingDispatcher {
    pub fn new(inner: LocalDispatcher, sink: EventSink) -> Self {
        Self { inner, sink }
    }
}

impl CommandDispatcher for PublishingDispatcher {
    fn dispatch(&self, cmd: Command) -> Result<Vec<Event>> {
        // #693 diagnostic: name any command that holds the dispatching
        // (frontend) thread past one frame, so real-session stalls are
        // self-reporting instead of guessed at.
        let label = {
            let full = format!("{cmd:?}");
            full.split([' ', '{']).next().unwrap_or("?").to_string()
        };
        let t0 = std::time::Instant::now();
        let events = self.inner.dispatch(cmd)?;
        let elapsed = t0.elapsed();
        if elapsed > std::time::Duration::from_millis(50) {
            log::warn!("[ui-stall] Command::{label} held the dispatching thread for {elapsed:?}");
        }
        self.sink.publish(&events);
        // #693: refresh the read snapshot after every state change so
        // transports serve reads API-style on their own thread instead
        // of hopping to this (frontend) one.
        self.inner.publish_state_snapshot();
        Ok(events)
    }

    fn subscribe(&self) -> EventStream {
        self.inner.subscribe()
    }

    /// #693: async completions flow through the same fan-out a
    /// synchronous dispatch gets — transports see them as events.
    fn poll_async_results(&self) -> Vec<Event> {
        let events = self.inner.poll_async_results();
        if !events.is_empty() {
            self.sink.publish(&events);
            self.inner.publish_state_snapshot();
        }
        events
    }
}

#[cfg(test)]
#[path = "publishing_dispatcher_tests.rs"]
mod tests;
