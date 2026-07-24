//! Phase 4 integration red-first test (issue #548): end-to-end through
//! the new pipeline: raw IncomingMessage + active profiles +
//! SelectionState → dispatcher receives the expected `Command`s.
//!
//! The function under test (`dispatch_midi_message`) is the connector
//! the daemon (real MIDI input) and any test harness call. Pure logic
//! around it — collects every hit, builds the slot command, calls
//! `dispatcher.dispatch` once per hit.

use std::cell::RefCell;

use adapter_midi::pipeline::dispatch_midi_message;
use adapter_midi::profile::parse_profile_yaml;
use adapter_midi::slots::IncomingMessage;
use anyhow::Result;
use application::command::{ChainCommand, Command, SelectionCommand};
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use application::SelectionState;

/// Spy dispatcher that records every command it sees.
struct SpyDispatcher {
    seen: RefCell<Vec<Command>>,
}

impl SpyDispatcher {
    fn new() -> Self {
        Self {
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl CommandDispatcher for SpyDispatcher {
    fn dispatch(&self, cmd: Command) -> Result<Vec<Event>> {
        self.seen.borrow_mut().push(cmd);
        Ok(vec![])
    }
}

#[test]
fn integration_pc_in_chocolate_profile_dispatches_prev_preset_on_active_chain() {
    let profile = parse_profile_yaml(
        r#"
name: "Chocolate"
source: "FootCtrlPlus"
bindings:
  - when: { kind: ProgramChange, channel: 1, program: 0 }
    do: prev_preset
"#,
    )
    .unwrap();

    let sel = SelectionState {
        active_chain: Some("rig:guitar".to_string()),
        ..Default::default()
    };
    let dispatcher = SpyDispatcher::new();

    let msg = IncomingMessage::ProgramChange {
        channel: 1,
        program: 0,
    };
    dispatch_midi_message(
        &[&profile],
        "FootCtrlPlus Bluetooth",
        &msg,
        &sel,
        &dispatcher,
    );

    let seen = dispatcher.seen.borrow();
    assert_eq!(seen.len(), 1);
    assert!(matches!(
        seen[0],
        Command::Selection(SelectionCommand::ApplyRigNav { .. })
    ));
}

#[test]
fn integration_filters_by_source_so_other_devices_do_not_fire() {
    let profile = parse_profile_yaml(
        r#"
name: "Chocolate"
source: "FootCtrlPlus"
bindings:
  - when: { kind: ProgramChange, channel: 1, program: 0 }
    do: prev_preset
"#,
    )
    .unwrap();
    let sel = SelectionState {
        active_chain: Some("g".to_string()),
        ..Default::default()
    };
    let dispatcher = SpyDispatcher::new();

    let msg = IncomingMessage::ProgramChange {
        channel: 1,
        program: 0,
    };
    dispatch_midi_message(&[&profile], "IAC Driver", &msg, &sel, &dispatcher);

    assert!(
        dispatcher.seen.borrow().is_empty(),
        "different port should not trigger"
    );
}

#[test]
fn integration_two_profiles_both_fire() {
    let a = parse_profile_yaml(
        r#"
name: "A"
bindings:
  - when: { kind: ProgramChange, channel: 1, program: 0 }
    do: prev_preset
"#,
    )
    .unwrap();
    let b = parse_profile_yaml(
        r#"
name: "B"
bindings:
  - when: { kind: ProgramChange, channel: 1, program: 0 }
    do: next_preset
"#,
    )
    .unwrap();
    let sel = SelectionState {
        active_chain: Some("g".to_string()),
        ..Default::default()
    };
    let dispatcher = SpyDispatcher::new();

    dispatch_midi_message(
        &[&a, &b],
        "any",
        &IncomingMessage::ProgramChange {
            channel: 1,
            program: 0,
        },
        &sel,
        &dispatcher,
    );

    assert_eq!(dispatcher.seen.borrow().len(), 2);
}

#[test]
fn integration_cc_chain_volume_dispatches_scaled_value() {
    let profile = parse_profile_yaml(
        r#"
name: "Knob"
bindings:
  - when: { kind: ControlChange, channel: 1, controller: 7 }
    do: chain_volume
"#,
    )
    .unwrap();
    let sel = SelectionState {
        active_chain: Some("g".to_string()),
        ..Default::default()
    };
    let dispatcher = SpyDispatcher::new();

    dispatch_midi_message(
        &[&profile],
        "any",
        &IncomingMessage::ControlChange {
            channel: 1,
            controller: 7,
            value: 127,
        },
        &sel,
        &dispatcher,
    );

    let seen = dispatcher.seen.borrow();
    assert_eq!(seen.len(), 1);
    match &seen[0] {
        Command::Chain(ChainCommand::SetChainVolume { value, .. }) => {
            assert!((value - 1.0).abs() < 1e-6)
        }
        other => panic!("expected SetChainVolume, got {other:?}"),
    }
}

#[test]
fn integration_slot_without_active_chain_dispatches_nothing() {
    let profile = parse_profile_yaml(
        r#"
name: "Needs chain"
bindings:
  - when: { kind: ProgramChange, channel: 1, program: 0 }
    do: prev_preset
"#,
    )
    .unwrap();
    let sel = SelectionState::default(); // no active chain
    let dispatcher = SpyDispatcher::new();

    dispatch_midi_message(
        &[&profile],
        "any",
        &IncomingMessage::ProgramChange {
            channel: 1,
            program: 0,
        },
        &sel,
        &dispatcher,
    );

    assert!(dispatcher.seen.borrow().is_empty());
}
