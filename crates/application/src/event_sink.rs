//! Responsibility: fans an event batch out to whoever is listening.

use std::sync::mpsc::{self, Receiver, Sender};

use crate::event::Event;

/// Broadcast sink for fanned-out event batches (GUI- and MCP-originated).
///
/// Wired by [`crate::publishing_dispatcher::PublishingDispatcher`]; consumed
/// by the MCP server to emit notifications for *every* state change, no
/// matter which transport originated it.
#[derive(Clone)]
pub struct EventSink {
    tx: Sender<Vec<Event>>,
}

impl EventSink {
    /// Fan a non-empty event batch out to the stream. Never blocks.
    pub fn publish(&self, events: &[Event]) {
        if !events.is_empty() {
            let _ = self.tx.send(events.to_vec());
        }
    }
}

/// Receiver of fanned-out events, owned by the MCP server side.
pub struct EventStreamRx {
    rx: Receiver<Vec<Event>>,
}

impl EventStreamRx {
    /// Next available event batch, if any. Non-blocking.
    pub fn try_recv(&mut self) -> Option<Vec<Event>> {
        self.rx.try_recv().ok()
    }
}

/// Create a connected `(sink, receiver)` pair for event fan-out.
pub fn event_sink() -> (EventSink, EventStreamRx) {
    let (tx, rx) = mpsc::channel();
    (EventSink { tx }, EventStreamRx { rx })
}
