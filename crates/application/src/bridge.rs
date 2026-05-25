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
    qtx: Sender<QueryRequest>,
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

/// Read-only state a transport can request. Resolved on the frontend thread
/// (which owns the `!Send` `Project`); serialization is done by domain code,
/// never re-derived in the adapter.
#[derive(Clone, Debug)]
pub enum QueryKind {
    /// Whole project as YAML.
    ProjectYaml,
    /// Available audio devices, one per line.
    Devices,
    /// Human-readable chain/block ID listing (for `midi-map.yaml` authors
    /// and the MCP `openrig://ids` resource). See [`crate::query::list_ids`].
    Ids,
    /// Per-chain input/output peak meters (`(chain_id, in_dbfs, out_dbfs)`,
    /// one record per line). Same numbers the GUI's IN/OUT bars read —
    /// every transport gets the same view (`openrig-code-quality` lei).
    ChainMeters,
}

struct QueryRequest {
    kind: QueryKind,
    reply: oneshot::Sender<Result<String, String>>,
}

impl CommandBridge {
    /// Queue a read-only query. Resolves once the frontend services it.
    pub fn query(&self, kind: QueryKind) -> oneshot::Receiver<Result<String, String>> {
        let (reply, rx) = oneshot::channel();
        let _ = self.qtx.send(QueryRequest { kind, reply });
        rx
    }
}

/// Receiver side, owned by the frontend thread.
pub struct BridgeDrain {
    rx: Receiver<BridgeRequest>,
    qrx: Receiver<QueryRequest>,
}

impl BridgeDrain {
    /// Dispatch up to `cap` queued commands on the calling (frontend) thread.
    /// Returns the events every dispatched command produced, in order, so the
    /// caller (the GUI's MIDI/MCP drain timer) can run the same screen/runtime
    /// refresh a GUI click does — a footswitch must move the screen too.
    /// Non-blocking; safe to call every tick. Empty result ⇒ nothing changed.
    pub fn drain(&self, dispatcher: &dyn CommandDispatcher, cap: usize) -> Vec<Event> {
        let mut events = Vec::new();
        let mut handled = 0;
        while handled < cap {
            match self.rx.try_recv() {
                Ok(req) => {
                    let outcome = dispatcher.dispatch(req.cmd).map_err(|e| e.to_string());
                    if let Ok(produced) = &outcome {
                        events.extend(produced.iter().cloned());
                    }
                    let _ = req.reply.send(outcome);
                    handled += 1;
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        events
    }

    /// Service queued read-only queries on the calling (frontend) thread.
    /// `resolver` runs with the frontend's `Project` access and returns the
    /// serialized payload (or an error message) for each [`QueryKind`].
    pub fn serve_queries<F>(&self, resolver: F, cap: usize) -> usize
    where
        F: Fn(&QueryKind) -> Result<String, String>,
    {
        let mut handled = 0;
        while handled < cap {
            match self.qrx.try_recv() {
                Ok(req) => {
                    let _ = req.reply.send(resolver(&req.kind));
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
    let (qtx, qrx) = mpsc::channel();
    (CommandBridge { tx, qtx }, BridgeDrain { rx, qrx })
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
            midi: None,
        }))
    }

    #[test]
    fn drain_dispatches_request_and_replies() {
        let dispatcher = LocalDispatcher::new(test_project());
        let (bridge, drain) = channel();

        let mut reply_rx = bridge.submit(Command::SaveProject);

        // Nothing dispatched until the frontend drains.
        assert!(reply_rx.try_recv().unwrap().is_none());

        let drained = drain.drain(&dispatcher, 16);
        assert_eq!(drained.len(), 1);

        let events = reply_rx
            .try_recv()
            .unwrap()
            .expect("reply present after drain")
            .expect("dispatch ok");
        assert!(!events.is_empty());
    }

    #[test]
    fn drain_returns_dispatched_events_so_midi_path_can_refresh() {
        // A footswitch-originated command goes through the same drain as the
        // GUI. The drain must hand back what was dispatched so the MIDI/MCP
        // timer can run the same screen/runtime refresh the GUI does — today
        // it only returns a count and the events are lost.
        let dispatcher = LocalDispatcher::new(test_project());
        let (bridge, drain) = channel();
        let _ = bridge.submit(Command::SaveProject);

        let events = drain.drain(&dispatcher, 16);

        assert!(
            events.iter().any(|e| matches!(e, Event::ProjectSaved)),
            "drain must surface dispatched events for the caller to react: {events:?}"
        );
    }

    #[test]
    fn drain_respects_cap() {
        let dispatcher = LocalDispatcher::new(test_project());
        let (bridge, drain) = channel();
        for _ in 0..5 {
            let _ = bridge.submit(Command::SaveProject);
        }
        // SaveProject yields exactly one ProjectSaved, so the event count
        // tracks the command count: cap=2 ⇒ 2 handled, then the remaining 3.
        assert_eq!(drain.drain(&dispatcher, 2).len(), 2);
        assert_eq!(drain.drain(&dispatcher, 10).len(), 3);
    }

    #[test]
    fn query_chain_meters_variant_round_trips_through_bridge() {
        // Pin the contract: a transport (MCP/gRPC/...) submits
        // `QueryKind::ChainMeters` and the frontend resolver replies
        // with a TSV-shaped payload. The resolver in the running app
        // is what actually fills the values; the bridge's contract is
        // just that the variant is plumbed through and the reply is
        // delivered to the caller.
        let (bridge, drain) = channel();
        let mut rx = bridge.query(QueryKind::ChainMeters);
        let served = drain.serve_queries(
            |kind| match kind {
                QueryKind::ChainMeters => {
                    Ok("rig:input-1\t-12.3\t-9.8\nrig:input-2\t-30.0\t-25.5\n".to_string())
                }
                _ => Err("unexpected".into()),
            },
            8,
        );
        assert_eq!(served, 1, "the meters query was actually served");
        let payload: String = rx
            .try_recv()
            .expect("channel alive")
            .expect("reply delivered")
            .expect("ok payload");
        for line in payload.trim_end().split('\n') {
            let cols: Vec<&str> = line.split('\t').collect();
            assert_eq!(cols.len(), 3, "tsv shape: chain_id, in_dbfs, out_dbfs");
            cols[1].parse::<f32>().expect("in_dbfs is a finite float");
            cols[2].parse::<f32>().expect("out_dbfs is a finite float");
        }
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
