//! Wiring tests for the System / I/O bindings section (#716). No AppWindow
//! is constructed — tests drive the pure wiring functions and assert on
//! the Commands the dispatcher would receive.

use super::{build_create_command, build_update_with_output_endpoint, surface_delete_error};
use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_cpal::AudioDeviceDescriptor;

// ── reproject_reflects_created_binding (the real "+" bug, headless) ───────────

/// The list-model the section renders MUST gain a row after a binding is added
/// to the config. This reproduces the on-screen bug ("+" did nothing) at the
/// model layer — no AppWindow needed (LAW 1: state is the source of truth).
#[test]
fn reproject_reflects_created_binding() {
    use infra_filesystem::AppConfig;

    let mut cfg = AppConfig::default();
    assert_eq!(super::project_bindings(&cfg).len(), 0, "starts empty");

    // What the create closure does to state:
    cfg.io_bindings.push(make_binding("main", "Main Rig"));

    let models = super::project_bindings(&cfg);
    let names = super::binding_names(&cfg);
    assert_eq!(models.len(), 1, "list model must show the created binding");
    assert_eq!(names.len(), 1);
    assert_eq!(models[0].id.as_str(), "main");
    assert_eq!(names[0].as_str(), "Main Rig");
    // The nested endpoint models project too (the fixture has one input).
    assert_eq!(slint::Model::row_count(&models[0].inputs), 1);
}

/// The exact on-screen action: clicking "+" with the name field EMPTY must
/// still create a visible, renamable binding ("I/O 1") — the previous build
/// did nothing because the button was guarded on a non-empty name.
#[test]
fn create_with_empty_name_yields_default_named_row() {
    use infra_filesystem::AppConfig;
    use std::cell::RefCell;
    use std::rc::Rc;

    let cfg = Rc::new(RefCell::new(AppConfig::default()));

    // user clicks "+" with the field empty
    let display = super::binding_display_name("", &cfg);
    assert_eq!(display, "I/O 1");
    cfg.borrow_mut().io_bindings.push(IoBinding {
        id: super::make_id(&display),
        name: display.clone(),
        inputs: vec![],
        outputs: vec![],
    });

    let names = super::binding_names(&cfg.borrow());
    assert_eq!(names.len(), 1, "+ with empty name must add a row");
    assert_eq!(names[0].as_str(), "I/O 1");
}

fn descriptor(id: &str, name: &str, channels: usize) -> AudioDeviceDescriptor {
    AudioDeviceDescriptor {
        id: id.into(),
        name: name.into(),
        channels,
    }
}

// ── channel_items_for_device (device → channel checkboxes) ────────────────────

/// Selecting a device must yield exactly one channel option per physical
/// channel the device reports (a 2-channel device → 2 ChannelOptionItems),
/// derived ONLY from the enumerated descriptor — no hardcoded channel count.
#[test]
fn selecting_two_channel_device_yields_two_channel_options() {
    let devices = vec![descriptor("A", "Iface A", 2), descriptor("B", "Iface B", 8)];

    let items = super::channel_items_for_device("A", &devices, &[]);

    assert_eq!(items.len(), 2, "2-channel device must produce 2 options");
    assert_eq!(items[0].index, 0);
    assert_eq!(items[1].index, 1);
    assert!(!items[0].selected, "nothing selected yet");

    // A different device with a different channel count derives its own count.
    let items_b = super::channel_items_for_device("B", &devices, &[0, 1]);
    assert_eq!(items_b.len(), 8, "8-channel device must produce 8 options");
    assert!(items_b[0].selected, "channel 0 selected");
    assert!(items_b[1].selected, "channel 1 selected");
    assert!(!items_b[2].selected, "channel 2 not selected");
}

/// An unknown device id yields no options (no fallback to a default device).
#[test]
fn channel_items_for_unknown_device_is_empty() {
    let devices = vec![descriptor("A", "Iface A", 2)];
    let items = super::channel_items_for_device("ghost", &devices, &[]);
    assert!(items.is_empty(), "unknown device must not invent channels");
}

// ── add_input_endpoint structured (device + channels + mode) ──────────────────

/// Adding an input endpoint with device "A" + channels [0,1] + Stereo builds
/// `Command::UpdateIoBinding` whose binding gains the structured endpoint, and
/// the endpoint survives the YAML serialization round-trip that
/// `save_app_config`/`load_app_config` perform (persistence proof WITHOUT
/// touching the shared HOME-derived config path — see memory: concurrent
/// solver sessions clobber the real config.yaml when HOME is swapped).
#[test]
fn add_input_endpoint_structured_appends_and_persists() {
    use infra_filesystem::AppConfig;

    let mut config = AppConfig::default();
    config.io_bindings.push(make_binding("main", "Main Rig"));

    // What the structured add-input callback feeds in: device id + 0-based
    // channel indices + mode — no free text.
    let ep = super::build_input_endpoint("In 1", "A", vec![0, 1], ChannelMode::Stereo);
    assert_eq!(ep.device_id, DeviceId("A".to_string()));
    assert_eq!(ep.channels, vec![0, 1]);
    assert_eq!(ep.mode, ChannelMode::Stereo);

    let binding = config.io_bindings[0].clone();
    let cmd = super::build_update_with_input_endpoint(binding, ep.clone());
    use application::command::Command;
    match cmd {
        Command::UpdateIoBinding { binding: b } => {
            assert_eq!(b.inputs.len(), 2, "existing input + new input");
            assert_eq!(b.inputs[1].device_id, DeviceId("A".to_string()));
            assert_eq!(b.inputs[1].channels, vec![0, 1]);
            assert_eq!(b.inputs[1].mode, ChannelMode::Stereo);
            config.io_bindings[0] = b;
        }
        other => panic!("expected UpdateIoBinding, got {other:?}"),
    }

    // The exact serialization the on-disk save performs (serde_yaml).
    let raw = serde_yaml::to_string(&config).expect("serialize config");
    let reloaded: AppConfig = serde_yaml::from_str(&raw).expect("reload config");
    assert_eq!(
        reloaded.io_bindings[0].inputs.len(),
        2,
        "endpoint must survive a round-trip through config persistence"
    );
    assert_eq!(
        reloaded.io_bindings[0].inputs[1].device_id,
        DeviceId("A".to_string())
    );
    assert_eq!(reloaded.io_bindings[0].inputs[1].channels, vec![0, 1]);
}

// ── add_output_endpoint structured (mono/stereo only) ─────────────────────────

/// Output endpoints are limited to mono/stereo; the structured builder forwards
/// the chosen mode and channels verbatim.
#[test]
fn add_output_endpoint_structured_appends_mono() {
    let ep = super::build_output_endpoint("Out 1", "B", vec![0], ChannelMode::Mono);
    assert_eq!(ep.mode, ChannelMode::Mono);
    assert_eq!(ep.channels, vec![0]);

    let binding = make_binding("main", "Main Rig");
    let cmd = super::build_update_with_output_endpoint(binding, ep);
    use application::command::Command;
    match cmd {
        Command::UpdateIoBinding { binding: b } => {
            assert_eq!(b.outputs.len(), 1);
            assert_eq!(b.outputs[0].device_id, DeviceId("B".to_string()));
            assert_eq!(b.outputs[0].mode, ChannelMode::Mono);
        }
        other => panic!("expected UpdateIoBinding, got {other:?}"),
    }
}

// ── remove_endpoint drops it ──────────────────────────────────────────────────

/// Removing an endpoint by name builds `Command::UpdateIoBinding` with that
/// endpoint dropped from the matching list (input vs output).
#[test]
fn remove_input_endpoint_drops_it() {
    let binding = make_binding("main", "Main Rig");
    assert_eq!(binding.inputs.len(), 1, "fixture has one input");

    let cmd = super::build_update_removing_endpoint(binding, "Guitar In", true);
    use application::command::Command;
    match cmd {
        Command::UpdateIoBinding { binding: b } => {
            assert_eq!(b.inputs.len(), 0, "named input endpoint must be removed");
        }
        other => panic!("expected UpdateIoBinding, got {other:?}"),
    }
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
