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
    d.attach_runtime_control(Rc::new(SpyRuntimeControl {
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
        .expect("SetOutputMuted must succeed");

    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::OutputMutedChanged { muted: true })),
        "expected Event::OutputMutedChanged {{ muted: true }}, got {events:?}"
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
    .expect("mute must succeed");
    d.dispatch(Command::Selection(SelectionCommand::SetOutputMuted {
        muted: false,
    }))
    .expect("unmute must succeed");

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
/// The command installs the registry the DISPATCHER owns, so it carries no
/// arbitrary payload that nothing durably records.
#[test]
fn set_io_bindings_applies_to_the_attached_runtime() {
    let (d, calls) = dispatcher_with_spy();
    let registry = Rc::new(RefCell::new(vec![
        binding("focusrite"),
        binding("orange-pi"),
    ]));
    d.attach_io_bindings(registry);

    let events = d
        .dispatch(Command::IoBinding(IoBindingCommand::SetIoBindings))
        .expect("SetIoBindings must succeed");

    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::IoBindingRegistryChanged)),
        "expected Event::IoBindingRegistryChanged, got {events:?}"
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

/// #127 (parity): a registry edit issued OFF the GUI must survive the next
/// chain sync.
///
/// The sync path re-installs `session.io_bindings` into the controller on every
/// sync (`runtime_lifecycle.rs`, #716). That handle is the registry attached
/// here, so a `CreateIoBinding` that only reached `config.yaml` would be wiped
/// out moments later: the command would report success and then evaporate,
/// which is exactly the gap the Command-parity law exists to prevent.
#[test]
fn a_registry_edit_issued_off_the_gui_survives_the_next_runtime_sync() {
    let (d, calls) = dispatcher_with_spy();
    let home = tempfile::tempdir().expect("temp config dir");
    d.attach_io_config_path(Some(home.path().join("config.yaml")));
    let registry = Rc::new(RefCell::new(Vec::new()));
    d.attach_io_bindings(registry.clone());

    // A non-GUI transport (MCP/gRPC) creates a binding, then installs the
    // registry into the live runtime.
    d.dispatch(Command::IoBinding(IoBindingCommand::CreateIoBinding {
        binding: binding("focusrite"),
    }))
    .expect("CreateIoBinding must succeed");
    d.dispatch(Command::IoBinding(IoBindingCommand::SetIoBindings))
        .expect("SetIoBindings must succeed");

    assert_eq!(
        *calls.borrow(),
        vec![RuntimeCall::Bindings(vec!["focusrite".to_string()])],
        "the runtime must receive the binding the non-GUI caller just created"
    );
    assert_eq!(
        registry
            .borrow()
            .iter()
            .map(|b| b.id.as_str())
            .collect::<Vec<_>>(),
        vec!["focusrite"],
        "the registry the next chain sync re-installs must carry the new \
         binding, or the command evaporates at the next sync"
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
        .expect("SetOutputMuted with no runtime must succeed");
    let bound = d
        .dispatch(Command::IoBinding(IoBindingCommand::SetIoBindings))
        .expect("SetIoBindings with no runtime must succeed");

    assert!(muted
        .iter()
        .any(|e| matches!(e, Event::OutputMutedChanged { muted: true })));
    assert!(bound
        .iter()
        .any(|e| matches!(e, Event::IoBindingRegistryChanged)));
}

/// #127: `AddIoEndpoint` must give the new endpoint the SAME auto-assigned name
/// in both copies of the registry.
///
/// The name is sequential ("In N"), so deriving it from each copy's own length
/// hands the two copies different names the moment they disagree — a latent
/// divergence that a rename or an endpoint removal on one side is enough to
/// trigger. Derive it once, apply it to both.
#[test]
fn an_added_endpoint_is_named_once_and_applied_to_both_copies() {
    use domain::io_binding::{ChannelMode, IoEndpoint};

    let home = tempfile::tempdir().expect("temp config dir");
    let config_path = home.path().join("config.yaml");
    let d = dispatcher();
    d.attach_io_config_path(Some(config_path.clone()));

    // The two copies disagree: the in-memory registry already carries an input,
    // the persisted config does not.
    let existing = IoEndpoint {
        name: "In 1".to_string(),
        device_id: domain::ids::DeviceId("dev".to_string()),
        mode: ChannelMode::Mono,
        channels: vec![0],
    };
    let mut seeded = binding("focusrite");
    seeded.inputs.push(existing);
    infra_filesystem::FilesystemStorage::update_app_config_at(&config_path, |config| {
        config.io_bindings.push(binding("focusrite"));
    })
    .expect("seed the persisted registry");
    let registry = Rc::new(RefCell::new(vec![seeded]));
    d.attach_io_bindings(registry.clone());

    d.dispatch(Command::IoBinding(IoBindingCommand::AddIoEndpoint {
        binding_id: "focusrite".to_string(),
        is_input: true,
        device_id: "dev".to_string(),
        channels: vec![1],
        mode: ChannelMode::Mono,
    }))
    .expect("AddIoEndpoint must succeed");
    crate::persist_worker::flush();

    let in_memory = registry.borrow()[0]
        .inputs
        .last()
        .expect("added")
        .name
        .clone();
    let persisted = infra_filesystem::FilesystemStorage::load_app_config_at(&config_path)
        .expect("load persisted config")
        .io_bindings[0]
        .inputs
        .last()
        .expect("added")
        .name
        .clone();
    assert_eq!(
        in_memory, persisted,
        "the endpoint must carry one name, not one per copy"
    );
}
