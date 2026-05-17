//! `Send` bridge between an async transport (MCP/gRPC) and the `!Send`
//! `LocalDispatcher`. The transport thread `submit`s a `Command`; the
//! frontend thread `drain`s and dispatches on its own thread, replying
//! over a `futures` oneshot. No tokio runtime is pulled into this crate.

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use anyhow::Result;
use futures::channel::oneshot;

use crate::command::Command;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;

/// Result of one dispatched command: `Ok(events)` or a stringified error
/// (the bridge crosses a thread boundary; the transport's serialization
/// layer wants an owned, `Send` payload, not `anyhow::Error`).
pub type DispatchOutcome = Result<Vec<Event>, String>;

struct BridgeRequest {
    cmd: Command,
    reply: oneshot::Sender<DispatchOutcome>,
}

/// Cloneable, `Send` handle held by the transport (MCP server thread).
#[derive(Clone)]
pub struct CommandBridge {
    tx: Sender<BridgeRequest>,
}

impl CommandBridge {
    /// Queue a command. Returns a oneshot receiver that resolves once the
    /// frontend drains and dispatches it. Never blocks.
    pub fn submit(&self, cmd: Command) -> oneshot::Receiver<DispatchOutcome> {
        let (reply, rx) = oneshot::channel();
        // If the frontend is gone the receiver simply never resolves; the
        // transport layer applies its own request timeout.
        let _ = self.tx.send(BridgeRequest { cmd, reply });
        rx
    }
}

/// Receiver side, owned by the frontend thread.
pub struct BridgeDrain {
    rx: Receiver<BridgeRequest>,
}

impl BridgeDrain {
    /// Dispatch up to `cap` queued commands on the calling (frontend) thread.
    /// Returns how many were handled. Non-blocking; safe to call every tick.
    pub fn drain(&self, dispatcher: &dyn CommandDispatcher, cap: usize) -> usize {
        let mut handled = 0;
        while handled < cap {
            match self.rx.try_recv() {
                Ok(req) => {
                    let outcome = dispatcher.dispatch(req.cmd).map_err(|e| e.to_string());
                    let _ = req.reply.send(outcome);
                    handled += 1;
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        handled
    }
}

/// Create a connected `(transport handle, frontend drain)` pair.
pub fn channel() -> (CommandBridge, BridgeDrain) {
    let (tx, rx) = mpsc::channel();
    (CommandBridge { tx }, BridgeDrain { rx })
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::local_dispatcher::LocalDispatcher;
    use project::project::Project;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn test_project() -> Rc<RefCell<Project>> {
        Rc::new(RefCell::new(Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
        }))
    }

    #[test]
    fn drain_dispatches_request_and_replies() {
        let dispatcher = LocalDispatcher::new(test_project());
        let (bridge, drain) = channel();

        let mut reply_rx = bridge.submit(Command::SaveProject);

        // Nothing dispatched until the frontend drains.
        assert!(reply_rx.try_recv().unwrap().is_none());

        let handled = drain.drain(&dispatcher, 16);
        assert_eq!(handled, 1);

        let events = reply_rx
            .try_recv()
            .unwrap()
            .expect("reply present after drain")
            .expect("dispatch ok");
        assert!(!events.is_empty());
    }

    #[test]
    fn drain_respects_cap() {
        let dispatcher = LocalDispatcher::new(test_project());
        let (bridge, drain) = channel();
        for _ in 0..5 {
            let _ = bridge.submit(Command::SaveProject);
        }
        assert_eq!(drain.drain(&dispatcher, 2), 2);
        assert_eq!(drain.drain(&dispatcher, 10), 3);
    }

    #[test]
    fn event_sink_fans_out_non_empty_batches() {
        let (sink, mut rx) = event_sink();
        sink.publish(&[]);
        assert!(rx.try_recv().is_none());
        sink.publish(&[Event::ProjectSaved]);
        assert_eq!(rx.try_recv().unwrap().len(), 1);
    }
}
