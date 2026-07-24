//! Red-first (#436 H): `SelectionCommand::SetTunerEnabled` /
//! `SetSpectrumEnabled` despacham e emitem
//! `Event::TunerEnabledChanged` / `SpectrumEnabledChanged`. Precedente
//! `SaveProject` (build/teardown no adapter, Command = intenção+evento).

use std::cell::RefCell;
use std::rc::Rc;

use project::project::Project;

use crate::command::{Command, SelectionCommand};
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
fn set_tuner_enabled_emits_event() {
    let events = dispatcher()
        .dispatch(Command::Selection(SelectionCommand::SetTunerEnabled {
            enabled: true,
        }))
        .expect("SetTunerEnabled deve ok");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TunerEnabledChanged { enabled: true })),
        "esperava Event::TunerEnabledChanged {{ enabled: true }}, veio {events:?}"
    );
}

#[test]
fn set_spectrum_enabled_emits_event() {
    let events = dispatcher()
        .dispatch(Command::Selection(SelectionCommand::SetSpectrumEnabled {
            enabled: false,
        }))
        .expect("SetSpectrumEnabled deve ok");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::SpectrumEnabledChanged { enabled: false })),
        "esperava Event::SpectrumEnabledChanged {{ enabled: false }}, veio {events:?}"
    );
}
