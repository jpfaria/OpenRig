//! Wiring tests for the System / I/O bindings section (#716). No AppWindow
//! is constructed — tests drive the pure wiring functions and assert on
//! the Commands the dispatcher would receive.

use super::{build_create_command, build_update_with_output_endpoint, surface_delete_error};
use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};

// ── reproject_reflects_created_binding (the real "+" bug, headless) ───────────

/// The list-model the section renders MUST gain a row after a binding is added
/// to the config. This reproduces the on-screen bug ("+" did nothing) at the
/// model layer — no AppWindow needed (LAW 1: state is the source of truth).
#[test]
fn reproject_reflects_created_binding() {
    use infra_filesystem::AppConfig;
    use slint::{Model, SharedString, VecModel};
    use std::cell::RefCell;
    use std::rc::Rc;

    let cfg = Rc::new(RefCell::new(AppConfig::default()));
    let id_model: Rc<VecModel<SharedString>> = Rc::new(VecModel::default());
    let name_model: Rc<VecModel<SharedString>> = Rc::new(VecModel::default());

    super::reproject(&id_model, &name_model, &cfg);
    assert_eq!(name_model.row_count(), 0, "starts empty");

    // What the create closure does to state:
    cfg.borrow_mut().io_bindings.push(make_binding("main", "Main Rig"));
    super::reproject(&id_model, &name_model, &cfg);

    assert_eq!(
        name_model.row_count(),
        1,
        "list model must show the created binding"
    );
    assert_eq!(id_model.row_count(), 1);
    assert_eq!(name_model.row_data(0).unwrap().as_str(), "Main Rig");
    assert_eq!(id_model.row_data(0).unwrap().as_str(), "main");
}

/// The exact on-screen action: clicking "+" with the name field EMPTY must
/// still create a visible, renamable binding ("I/O 1") — the previous build
/// did nothing because the button was guarded on a non-empty name.
#[test]
fn create_with_empty_name_yields_default_named_row() {
    use infra_filesystem::AppConfig;
    use slint::{Model, SharedString, VecModel};
    use std::cell::RefCell;
    use std::rc::Rc;

    let cfg = Rc::new(RefCell::new(AppConfig::default()));
    let id_model: Rc<VecModel<SharedString>> = Rc::new(VecModel::default());
    let name_model: Rc<VecModel<SharedString>> = Rc::new(VecModel::default());

    // user clicks "+" with the field empty
    let display = super::binding_display_name("", &cfg);
    assert_eq!(display, "I/O 1");
    cfg.borrow_mut().io_bindings.push(IoBinding {
        id: super::make_id(&display),
        name: display.clone(),
        inputs: vec![],
        outputs: vec![],
    });
    super::reproject(&id_model, &name_model, &cfg);

    assert_eq!(name_model.row_count(), 1, "+ with empty name must add a row");
    assert_eq!(name_model.row_data(0).unwrap().as_str(), "I/O 1");
}

fn make_binding(id: &str, name: &str) -> IoBinding {
    IoBinding {
        id: id.to_string(),
        name: name.to_string(),
        inputs: vec![IoEndpoint {
            name: "Guitar In".to_string(),
            device_id: DeviceId("hw:0,0".to_string()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![],
    }
}

fn make_output_endpoint(name: &str) -> IoEndpoint {
    IoEndpoint {
        name: name.to_string(),
        device_id: DeviceId("hw:0,1".to_string()),
        mode: ChannelMode::Stereo,
        channels: vec![0, 1],
    }
}

// ── create_binding_event_dispatches_create ───────────────────────────────────

/// Creating a binding builds a `Command::CreateIoBinding` carrying the binding.
#[test]
fn create_binding_event_dispatches_create() {
    let binding = make_binding("main", "Main Rig");
    let cmd = build_create_command(binding.clone());

    use application::command::Command;
    match cmd {
        Command::CreateIoBinding { binding: b } => {
            assert_eq!(b.id, "main");
            assert_eq!(b.name, "Main Rig");
        }
        other => panic!("expected CreateIoBinding, got {other:?}"),
    }
}

// ── add_output_endpoint_event_dispatches_update ──────────────────────────────

/// Adding an output endpoint to an existing binding builds
/// `Command::UpdateIoBinding` with the endpoint appended to `outputs`.
#[test]
fn add_output_endpoint_event_dispatches_update() {
    let binding = make_binding("main", "Main Rig");
    let new_ep = make_output_endpoint("Amp Out");

    let cmd = build_update_with_output_endpoint(binding, new_ep.clone());

    use application::command::Command;
    match cmd {
        Command::UpdateIoBinding { binding: b } => {
            assert_eq!(b.id, "main");
            assert_eq!(b.outputs.len(), 1);
            assert_eq!(b.outputs[0].name, "Amp Out");
            assert_eq!(b.outputs[0].mode, ChannelMode::Stereo);
        }
        other => panic!("expected UpdateIoBinding, got {other:?}"),
    }
}

// ── delete_referenced_binding_surfaces_error ─────────────────────────────────

/// When the dispatcher rejects a delete because the binding is referenced by
/// a chain, `surface_delete_error` produces a non-empty error string and does
/// NOT clear the binding list.
#[test]
fn delete_referenced_binding_surfaces_error() {
    let err = anyhow::anyhow!("cannot delete binding 'main': referenced by chain 'chain-1'");
    let mut list = vec![make_binding("main", "Main Rig")];

    let msg = surface_delete_error(&err, &mut list);

    // The error string is non-empty and contains the reject reason.
    assert!(!msg.is_empty(), "error message must not be empty");
    assert!(
        msg.contains("chain-1") || msg.contains("main"),
        "error message must mention the chain or binding: {msg}"
    );
    // The list is UNCHANGED — delete was rejected.
    assert_eq!(list.len(), 1, "binding list must be unchanged after reject");
}
