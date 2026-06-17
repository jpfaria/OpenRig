//! Wiring tests for the System / I/O bindings section (#716). No AppWindow
//! is constructed — tests drive the pure wiring functions and assert on
//! the Commands the dispatcher would receive.

use super::{build_create_command, build_update_with_output_endpoint, surface_delete_error};
use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};

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
