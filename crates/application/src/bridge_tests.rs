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
    let _rx = bridge.submit(Command::SaveProject);

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
        let _rx = bridge.submit(Command::SaveProject);
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
