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

// ── device dropdown is fed from the POPULATED source (the empty-dropdown bug) ──

/// Reproduces the on-screen bug: opening project settings enumerates devices
/// (the audio section shows them) but the I/O bindings device dropdowns stay
/// empty. The dropdown name model is built from the SHARED descriptor cache —
/// so opening settings must push the freshly enumerated descriptors into that
/// cache, and the name model built from it must be non-empty.
///
/// Before the fix `seed_device_caches` did not exist and the configure-project
/// path left the shared cache empty, so `device_list_models` produced an empty
/// name model even though enumeration had succeeded.
#[test]
fn opening_settings_seeds_device_dropdowns_from_enumerated_descriptors() {
    use std::cell::RefCell;
    use std::rc::Rc;

    // The shared caches the wiring reads from start empty (lazy enumeration).
    let input_cache: Rc<RefCell<Vec<AudioDeviceDescriptor>>> = Rc::new(RefCell::new(Vec::new()));
    let output_cache: Rc<RefCell<Vec<AudioDeviceDescriptor>>> = Rc::new(RefCell::new(Vec::new()));

    // Before settings opens the dropdown name model is empty (the bug symptom).
    let (_ids, names) = super::device_list_models(&input_cache.borrow());
    assert_eq!(
        slint::Model::row_count(names.as_ref()),
        0,
        "dropdown empty before enumeration"
    );

    // The configure-project path enumerated these descriptors.
    let fresh_input = vec![descriptor("hw:0,0", "Scarlett 2i2 In", 2)];
    let fresh_output = vec![descriptor("hw:0,1", "Scarlett 2i2 Out", 2)];

    // Opening settings must push them into the shared caches the wiring uses.
    super::seed_device_caches(&input_cache, &output_cache, &fresh_input, &fresh_output);

    assert_eq!(
        *input_cache.borrow(),
        fresh_input,
        "input cache must hold the enumerated descriptors"
    );
    assert_eq!(*output_cache.borrow(), fresh_output);

    // The dropdown name model built from the now-populated cache is non-empty.
    let (_ids, names) = super::device_list_models(&input_cache.borrow());
    assert_eq!(
        slint::Model::row_count(names.as_ref()),
        1,
        "dropdown must show the enumerated device after settings opens"
    );
    let name: slint::SharedString = slint::Model::row_data(names.as_ref(), 0).unwrap();
    assert_eq!(name.as_str(), "Scarlett 2i2 In");
}

// ── channel mode rule: mono = single-select, stereo/dual = multi ──────────────

/// In mono the channel picker is a radio group: selecting a second channel must
/// deselect the first (exactly one channel allowed). In stereo/dual_mono it is a
/// checkbox set: two channels can be selected at once. Reproduces the bug where
/// `toggle_channel` blindly set the row regardless of mode, letting a mono
/// endpoint accumulate multiple channels.
#[test]
fn mono_channel_toggle_is_single_select() {
    use domain::io_binding::ChannelMode;

    fn ch(index: i32, selected: bool) -> crate::ChannelOptionItem {
        crate::ChannelOptionItem {
            index,
            label: format!("Ch {}", index + 1).into(),
            selected,
            available: true,
        }
    }
    let items = vec![ch(0, false), ch(1, false)];

    // Mono: select ch0, then ch1 → only ch1 stays selected.
    let after0 = super::apply_channel_toggle(&items, 0, true, ChannelMode::Mono);
    assert!(
        after0[0].selected && !after0[1].selected,
        "mono: ch0 selected"
    );
    let after1 = super::apply_channel_toggle(&after0, 1, true, ChannelMode::Mono);
    assert!(
        !after1[0].selected && after1[1].selected,
        "mono: selecting ch1 must deselect ch0 (max 1)"
    );

    // Stereo: both can be selected at once.
    let s0 = super::apply_channel_toggle(&items, 0, true, ChannelMode::Stereo);
    let s1 = super::apply_channel_toggle(&s0, 1, true, ChannelMode::Stereo);
    assert!(
        s1[0].selected && s1[1].selected,
        "stereo: both channels selectable"
    );

    // DualMono: both can be selected at once too.
    let d0 = super::apply_channel_toggle(&items, 0, true, ChannelMode::DualMono);
    let d1 = super::apply_channel_toggle(&d0, 1, true, ChannelMode::DualMono);
    assert!(
        d1[0].selected && d1[1].selected,
        "dual_mono: both channels selectable"
    );
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
/// `IoBindingCommand::UpdateIoBinding` whose binding gains the structured endpoint, and
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
    use application::command::{Command, IoBindingCommand};
    match cmd {
        Command::IoBinding(IoBindingCommand::UpdateIoBinding { binding: b }) => {
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

// ── edit endpoint: prefill the add-form, then replace on save (Bug 3) ─────────

/// Clicking the pencil on an existing endpoint must prefill the add-form with
/// that endpoint's device, mode and selected channels. `endpoint_prefill`
/// resolves the device index in the side's device list and rebuilds the channel
/// options with the endpoint's channels pre-selected.
#[test]
fn edit_endpoint_prefills_device_mode_and_channels() {
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};

    let devices = vec![descriptor("A", "Iface A", 2), descriptor("B", "Iface B", 4)];
    let binding = IoBinding {
        id: "main".into(),
        name: "Main".into(),
        inputs: vec![IoEndpoint {
            name: "In 1".into(),
            device_id: DeviceId("B".into()),
            mode: ChannelMode::Stereo,
            channels: vec![2, 3],
        }],
        outputs: vec![],
    };

    let prefill = super::endpoint_prefill(&binding, "In 1", true, &devices)
        .expect("endpoint exists → prefill");

    assert_eq!(prefill.device_index, 1, "device B is index 1 in the list");
    assert_eq!(prefill.mode, ChannelMode::Stereo);
    // 4-channel device → 4 options, with 2 and 3 pre-selected.
    assert_eq!(prefill.channel_items.len(), 4);
    assert!(prefill.channel_items[2].selected, "ch2 was on the endpoint");
    assert!(prefill.channel_items[3].selected, "ch3 was on the endpoint");
    assert!(!prefill.channel_items[0].selected);
}

/// Saving an edit replaces the original endpoint in place (no duplicate, no
/// reorder of the others): remove the old name + insert the new endpoint.
#[test]
fn editing_endpoint_replaces_in_place() {
    use application::command::{Command, IoBindingCommand};
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};

    let binding = IoBinding {
        id: "main".into(),
        name: "Main".into(),
        inputs: vec![
            IoEndpoint {
                name: "In 1".into(),
                device_id: DeviceId("A".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            },
            IoEndpoint {
                name: "In 2".into(),
                device_id: DeviceId("A".into()),
                mode: ChannelMode::Mono,
                channels: vec![1],
            },
        ],
        outputs: vec![],
    };

    // Edit "In 1" → switch it to device B, stereo, channels [0,1].
    let new_ep = super::build_input_endpoint("In 1", "B", vec![0, 1], ChannelMode::Stereo);
    let cmd = super::build_update_replacing_endpoint(binding, "In 1", new_ep, true);

    match cmd {
        Command::IoBinding(IoBindingCommand::UpdateIoBinding { binding: b }) => {
            assert_eq!(b.inputs.len(), 2, "replace, not append");
            // "In 1" is updated in place (still first), "In 2" untouched.
            assert_eq!(b.inputs[0].name, "In 1");
            assert_eq!(b.inputs[0].device_id, DeviceId("B".into()));
            assert_eq!(b.inputs[0].mode, ChannelMode::Stereo);
            assert_eq!(b.inputs[0].channels, vec![0, 1]);
            assert_eq!(b.inputs[1].name, "In 2", "other endpoint unchanged");
        }
        other => panic!("expected UpdateIoBinding, got {other:?}"),
    }
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
    use application::command::{Command, IoBindingCommand};
    match cmd {
        Command::IoBinding(IoBindingCommand::UpdateIoBinding { binding: b }) => {
            assert_eq!(b.outputs.len(), 1);
            assert_eq!(b.outputs[0].device_id, DeviceId("B".to_string()));
            assert_eq!(b.outputs[0].mode, ChannelMode::Mono);
        }
        other => panic!("expected UpdateIoBinding, got {other:?}"),
    }
}

// ── remove_endpoint drops it ──────────────────────────────────────────────────

/// Removing an endpoint by name builds `IoBindingCommand::UpdateIoBinding` with that
/// endpoint dropped from the matching list (input vs output).
#[test]
fn remove_input_endpoint_drops_it() {
    let binding = make_binding("main", "Main Rig");
    assert_eq!(binding.inputs.len(), 1, "fixture has one input");

    let cmd = super::build_update_removing_endpoint(binding, "Guitar In", true);
    use application::command::{Command, IoBindingCommand};
    match cmd {
        Command::IoBinding(IoBindingCommand::UpdateIoBinding { binding: b }) => {
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

/// Creating a binding builds an `IoBindingCommand::CreateIoBinding` carrying the binding.
#[test]
fn create_binding_event_dispatches_create() {
    let binding = make_binding("main", "Main Rig");
    let cmd = build_create_command(binding.clone());

    use application::command::{Command, IoBindingCommand};
    match cmd {
        Command::IoBinding(IoBindingCommand::CreateIoBinding { binding: b }) => {
            assert_eq!(b.id, "main");
            assert_eq!(b.name, "Main Rig");
        }
        other => panic!("expected CreateIoBinding, got {other:?}"),
    }
}

// ── add_output_endpoint_event_dispatches_update ──────────────────────────────

/// Adding an output endpoint to an existing binding builds
/// `IoBindingCommand::UpdateIoBinding` with the endpoint appended to `outputs`.
#[test]
fn add_output_endpoint_event_dispatches_update() {
    let binding = make_binding("main", "Main Rig");
    let new_ep = make_output_endpoint("Amp Out");

    let cmd = build_update_with_output_endpoint(binding, new_ep.clone());

    use application::command::{Command, IoBindingCommand};
    match cmd {
        Command::IoBinding(IoBindingCommand::UpdateIoBinding { binding: b }) => {
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
