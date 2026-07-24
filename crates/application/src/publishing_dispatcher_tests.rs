use super::*;
use crate::bridge::event_sink;
use crate::command::{Command, ProjectCommand};
use crate::local_dispatcher::LocalDispatcher;
use project::project::Project;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn publishes_every_dispatch_to_sink() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let inner = LocalDispatcher::new(project);
    let (sink, mut rx) = event_sink();
    let pd = PublishingDispatcher::new(inner, sink);

    let events = pd
        .dispatch(Command::Project(ProjectCommand::SaveProject))
        .unwrap();
    assert!(!events.is_empty());

    let pushed = rx.try_recv().expect("event batch fanned out");
    assert_eq!(pushed.len(), events.len());
}
