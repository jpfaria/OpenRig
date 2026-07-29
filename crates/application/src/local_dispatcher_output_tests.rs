//! Red-first (#436 G): `SelectionCommand::SetOutputMuted` despacha e emite
//! `Event::OutputMutedChanged` — MCP/MIDI/GUI mutam a saída pela mesma
//! porta. Precedente `SaveProject` (efeito no adapter/runtime, Command
//! = intenção + evento).
//!
//! #127 (Task 7): the runtime effect is no longer the caller's job. The
//! dispatcher applies it through the attached `RuntimeControl`, so a
//! transport that only dispatches (MCP/gRPC) mutes the rig exactly like the
//! GUI does. The tests below assert the hook fires from the dispatcher.

use std::cell::RefCell;
use std::rc::Rc;

use domain::io_binding::IoBinding;
use project::project::Project;

use crate::command::{Command, IoBindingCommand, SelectionCommand};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::runtime_control::RuntimeControl;

fn dispatcher() -> LocalDispatcher {
    LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    })))
}

/// What the frontend's runtime was asked to do, in order.
#[derive(Debug, Clone, PartialEq)]
enum RuntimeCall {
    Muted(bool),
    Bindings(Vec<String>),
}

/// Stand-in for the frontend that hosts the audio runtime: records the calls
/// instead of touching real streams.
struct SpyRuntimeControl {
    calls: Rc<RefCell<Vec<RuntimeCall>>>,
}

impl RuntimeControl for SpyRuntimeControl {
    fn set_output_muted(&self, muted: bool) {
        self.calls.borrow_mut().push(RuntimeCall::Muted(muted));
    }

    fn set_io_bindings(&self, bindings: Vec<IoBinding>) {
        self.calls.borrow_mut().push(RuntimeCall::Bindings(
            bindings.into_iter().map(|b| b.id).collect(),
        ));
    }
}

/// A dispatcher with a spy runtime attached, plus the shared call log.
fn dispatcher_with_spy() -> (LocalDispatcher, Rc<RefCell<Vec<RuntimeCall>>>) {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let d = dispatcher();
    d.attach_runtime_control(Box::new(SpyRuntimeControl {
        calls: calls.clone(),
    }));
    (d, calls)
}

fn binding(id: &str) -> IoBinding {
    IoBinding {
        id: id.to_string(),
        name: id.to_string(),
        inputs: Vec::new(),
        outputs: Vec::new(),
    }
}

#[test]
fn set_output_muted_true_emits_event() {
    let events = dispatcher()
        .dispatch(Command::Selection(SelectionCommand::SetOutputMuted {
            muted: true,
        }))
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
        .dispatch(Command::Selection(SelectionCommand::SetOutputMuted {
            muted: false,
        }))
        .expect("SetOutputMuted deve ok");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::OutputMutedChanged { muted: false })),
        "esperava Event::OutputMutedChanged {{ muted: false }}, veio {events:?}"
    );
}

/// #127: dispatching the mute must (a) emit the event, (b) record the state
/// the dispatcher owns, and (c) reach the runtime THROUGH the dispatcher —
/// no `rt.set_output_muted` left in a UI callback.
#[test]
fn set_output_muted_applies_to_the_attached_runtime() {
    let (d, calls) = dispatcher_with_spy();

    let events = d
        .dispatch(Command::Selection(SelectionCommand::SetOutputMuted {
            muted: true,
        }))
        .expect("SetOutputMuted deve ok");

    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::OutputMutedChanged { muted: true })),
        "esperava Event::OutputMutedChanged {{ muted: true }}, veio {events:?}"
    );
    assert!(
        d.selection_state
            .read()
            .expect("selection state")
            .output_muted,
        "dispatcher must record the mute state it just applied"
    );
    assert_eq!(
        *calls.borrow(),
        vec![RuntimeCall::Muted(true)],
        "the runtime must be muted by the dispatcher, not by the UI afterwards"
    );
}

/// Unmuting travels the same way — the runtime hears the release from the
/// dispatcher, so the tuner's auto-mute is cleared for every transport.
#[test]
fn set_output_unmuted_applies_to_the_attached_runtime() {
    let (d, calls) = dispatcher_with_spy();

    d.dispatch(Command::Selection(SelectionCommand::SetOutputMuted {
        muted: true,
    }))
    .expect("mute deve ok");
    d.dispatch(Command::Selection(SelectionCommand::SetOutputMuted {
        muted: false,
    }))
    .expect("unmute deve ok");

    assert!(
        !d.selection_state
            .read()
            .expect("selection state")
            .output_muted,
        "dispatcher must record the released mute"
    );
    assert_eq!(
        *calls.borrow(),
        vec![RuntimeCall::Muted(true), RuntimeCall::Muted(false)],
        "both edges must reach the runtime through the dispatcher"
    );
}

/// #127: pushing the per-machine I/O binding registry into the live runtime is
/// a Command too — the GUI used to call `controller.set_io_bindings` itself.
#[test]
fn set_io_bindings_applies_to_the_attached_runtime() {
    let (d, calls) = dispatcher_with_spy();

    let events = d
        .dispatch(Command::IoBinding(IoBindingCommand::SetIoBindings {
            bindings: vec![binding("focusrite"), binding("orange-pi")],
        }))
        .expect("SetIoBindings deve ok");

    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::IoBindingRegistryChanged)),
        "esperava Event::IoBindingRegistryChanged, veio {events:?}"
    );
    assert_eq!(
        *calls.borrow(),
        vec![RuntimeCall::Bindings(vec![
            "focusrite".to_string(),
            "orange-pi".to_string(),
        ])],
        "the registry must reach the runtime through the dispatcher, in order"
    );
}

/// With no frontend runtime attached (MCP-only process, headless tests) the
/// command still succeeds and still reports the state change — it just has no
/// runtime to touch.
#[test]
fn runtime_commands_are_no_ops_without_an_attached_runtime() {
    let d = dispatcher();

    let muted = d
        .dispatch(Command::Selection(SelectionCommand::SetOutputMuted {
            muted: true,
        }))
        .expect("SetOutputMuted sem runtime deve ok");
    let bound = d
        .dispatch(Command::IoBinding(IoBindingCommand::SetIoBindings {
            bindings: vec![binding("focusrite")],
        }))
        .expect("SetIoBindings sem runtime deve ok");

    assert!(muted
        .iter()
        .any(|e| matches!(e, Event::OutputMutedChanged { muted: true })));
    assert!(bound
        .iter()
        .any(|e| matches!(e, Event::IoBindingRegistryChanged)));
}
