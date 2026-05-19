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
        let events = self.inner.dispatch(cmd)?;
        self.sink.publish(&events);
        Ok(events)
    }

    fn subscribe(&self) -> EventStream {
        self.inner.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::event_sink;
    use crate::command::Command;
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
        }));
        let inner = LocalDispatcher::new(project);
        let (sink, mut rx) = event_sink();
        let pd = PublishingDispatcher::new(inner, sink);

        let events = pd.dispatch(Command::SaveProject).unwrap();
        assert!(!events.is_empty());

        let pushed = rx.try_recv().expect("event batch fanned out");
        assert_eq!(pushed.len(), events.len());
    }
}
