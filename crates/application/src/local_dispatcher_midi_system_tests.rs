//! Tests for `handle_midi_system` (issue #792 split — each file with its test).
//!
//! System-side MIDI commands: none touch the project, each only records intent
//! via an `Event`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::command::{Command, MidiCommand};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use project::project::Project;

fn empty_project_rc() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }))
}

#[test]
fn save_midi_devices_emits_event_without_mutating_project() {
    let project = empty_project_rc();
    let before = project.borrow().clone();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatcher
        .dispatch(Command::Midi(MidiCommand::SaveMidiDevices {
            devices: vec![],
        }))
        .unwrap();

    assert_eq!(events, vec![Event::MidiDevicesSaved]);
    assert_eq!(
        project.borrow().chains.len(),
        before.chains.len(),
        "system command must not touch project chains"
    );
    assert_eq!(
        project.borrow().device_settings.len(),
        before.device_settings.len(),
        "system command must not touch project device_settings"
    );
    assert_eq!(
        project.borrow().name,
        before.name,
        "system command must not touch project name"
    );
}

#[test]
fn start_and_stop_midi_learn_emit_events() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    assert_eq!(
        dispatcher
            .dispatch(Command::Midi(MidiCommand::StartMidiLearn))
            .unwrap(),
        vec![Event::MidiLearnStarted]
    );
    assert_eq!(
        dispatcher
            .dispatch(Command::Midi(MidiCommand::StopMidiLearn))
            .unwrap(),
        vec![Event::MidiLearnStopped]
    );
}

#[test]
fn publish_midi_event_passthrough_emits_midi_event_received() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let source = project::midi::Source::Cc {
        channel: 1,
        controller: 7,
    };
    let events = dispatcher
        .dispatch(Command::Midi(MidiCommand::PublishMidiEvent {
            source: source.clone(),
        }))
        .unwrap();
    assert_eq!(events, vec![Event::MidiEventReceived { source }]);
}
