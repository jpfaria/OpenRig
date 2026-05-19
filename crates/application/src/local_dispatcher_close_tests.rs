//! Red-first (#436 E): `Command::CloseProject` despacha e emite
//! `Event::ProjectClosed`. Precedente `SaveProject` (teardown no
//! adapter, Command = intenção+evento).

use std::cell::RefCell;
use std::rc::Rc;

use project::project::Project;

use crate::command::Command;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

#[test]
fn close_project_emits_project_closed_event() {
    let disp = LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
    })));
    let events = disp
        .dispatch(Command::CloseProject)
        .expect("CloseProject deve ok");
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectClosed)),
        "esperava Event::ProjectClosed, veio {events:?}"
    );
}
