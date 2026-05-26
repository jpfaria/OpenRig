//! Red-first (#436 G): `Command::SetOutputMuted` despacha e emite
//! `Event::OutputMutedChanged` — MCP/MIDI/GUI mutam a saída pela mesma
//! porta. Precedente `SaveProject` (efeito no adapter/runtime, Command
//! = intenção + evento).

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
        midi: None,
    })))
}

#[test]
fn set_output_muted_true_emits_event() {
    let events = dispatcher()
        .dispatch(Command::SetOutputMuted { muted: true })
        .expect("SetOutputMuted deve ok");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::OutputMutedChanged { muted: true })),
        "esperava Event::OutputMutedChanged {{ muted: true }}, veio {events:?}"
    );
}

#[test]
fn set_output_muted_false_emits_event() {
    let events = dispatcher()
        .dispatch(Command::SetOutputMuted { muted: false })
        .expect("SetOutputMuted deve ok");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::OutputMutedChanged { muted: false })),
        "esperava Event::OutputMutedChanged {{ muted: false }}, veio {events:?}"
    );
}
