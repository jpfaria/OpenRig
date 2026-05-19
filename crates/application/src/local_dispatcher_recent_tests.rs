//! Red-first (#436 F): `Command::RemoveRecentProject` despacha e emite
//! `Event::RecentProjectRemoved { index }` — MCP/MIDI/GUI pela mesma
//! porta. Precedente `SaveProject` (persistência no adapter).

use std::cell::RefCell;
use std::rc::Rc;

use project::project::Project;

use crate::command::Command;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

fn dispatcher() -> LocalDispatcher {
    LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
    })))
}

#[test]
fn remove_recent_project_emits_event_with_index() {
    let events = dispatcher()
        .dispatch(Command::RemoveRecentProject { index: 3 })
        .expect("RemoveRecentProject deve ok");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::RecentProjectRemoved { index: 3 })),
        "esperava Event::RecentProjectRemoved {{ index: 3 }}, veio {events:?}"
    );
}
