//! #833 — the same physical input (device + channel) must NEVER be captured
//! by two enabled chains at the same time.
//!
//! #716 (model A) removed the per-block cross-chain channel-conflict check and
//! deferred it to activation, where endpoints are resolved from the per-machine
//! binding registry. It was never re-added there, so two chains bound to the
//! same device+channel could both be enabled — one guitar double-processed by
//! two runtimes.
//!
//! Contract asserted here (via the Command bus, so GUI / MCP / gRPC all get it):
//! enabling a chain whose input endpoints resolve to a (device, channel) pair
//! already claimed by another ENABLED chain returns `Err` and leaves the chain
//! disabled. Distinct channels, distinct devices, and disabled holders are not
//! conflicts.
//!
//! ## Test isolation
//!
//! Every test seeds its own `config.yaml` in a `TempDir` and attaches it with
//! `attach_io_config_path`. No `$HOME`, no global OS path.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{ChainCommand, Command};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_filesystem::FilesystemStorage;
use project::chain::Chain;
use project::project::Project;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A binding exposing a single mono input endpoint on `device` / `channel`.
fn input_binding(id: &str, device: &str, channel: usize) -> IoBinding {
    IoBinding {
        id: id.to_string(),
        name: id.to_string(),
        inputs: vec![IoEndpoint {
            name: format!("In {channel}"),
            device_id: DeviceId(device.to_string()),
            mode: ChannelMode::Mono,
            channels: vec![channel],
        }],
        outputs: vec![],
    }
}

/// Chain that carries `binding_ids` as its I/O selection (#716 model A).
fn chain_with_bindings(id: &str, enabled: bool, binding_ids: &[&str]) -> Chain {
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled,
        volume: 100.0,
        io_binding_ids: binding_ids.iter().map(|s| s.to_string()).collect(),
        blocks: vec![],
        di_output: None,
    }
}

/// Dispatcher over `chains`, with `bindings` seeded into a temp `config.yaml`.
fn dispatcher_with(
    chains: Vec<Chain>,
    bindings: Vec<IoBinding>,
) -> (
    LocalDispatcher,
    Rc<RefCell<Project>>,
    tempfile::TempDir, // keep alive for the duration of the test
) {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let cfg_path = tmp.path().join("config.yaml");
    FilesystemStorage::update_app_config_at(&cfg_path, |config| {
        config.io_bindings = bindings;
    })
    .expect("seed config.yaml");

    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains,
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_io_config_path(Some(cfg_path));
    (dispatcher, project, tmp)
}

fn toggle(
    dispatcher: &LocalDispatcher,
    chain: &str,
) -> anyhow::Result<Vec<application::event::Event>> {
    dispatcher.dispatch(Command::Chain(ChainCommand::ToggleChainEnabled {
        chain: ChainId(chain.to_string()),
    }))
}

fn enabled(project: &Rc<RefCell<Project>>, chain: &str) -> bool {
    project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id.0 == chain)
        .expect("chain present")
        .enabled
}

// ---------------------------------------------------------------------------
// Conflicts — must be rejected
// ---------------------------------------------------------------------------

#[test]
fn enabling_second_chain_on_same_device_channel_via_distinct_bindings_is_rejected() {
    // Two DIFFERENT bindings that both point at Scarlett channel 0. The
    // conflict is on the resolved (device, channel), not on the binding id.
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![
            chain_with_bindings("chain_a", true, &["io_a"]),
            chain_with_bindings("chain_b", false, &["io_b"]),
        ],
        vec![
            input_binding("io_a", "scarlett", 0),
            input_binding("io_b", "scarlett", 0),
        ],
    );

    let result = toggle(&dispatcher, "chain_b");

    assert!(
        result.is_err(),
        "enabling a second chain on scarlett ch0 must be rejected, got {result:?}"
    );
    assert!(
        !enabled(&project, "chain_b"),
        "chain_b must stay disabled after the rejected enable"
    );
    assert!(
        enabled(&project, "chain_a"),
        "the chain already holding the channel must stay enabled"
    );
}

#[test]
fn enabling_second_chain_sharing_the_same_binding_is_rejected() {
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![
            chain_with_bindings("chain_a", true, &["io_a"]),
            chain_with_bindings("chain_b", false, &["io_a"]),
        ],
        vec![input_binding("io_a", "scarlett", 0)],
    );

    let result = toggle(&dispatcher, "chain_b");

    assert!(
        result.is_err(),
        "two chains on the same binding must not both be enabled, got {result:?}"
    );
    assert!(!enabled(&project, "chain_b"), "chain_b must stay disabled");
}

#[test]
fn conflict_error_names_the_device_channel_and_the_holding_chain() {
    let (dispatcher, _project, _tmp) = dispatcher_with(
        vec![
            chain_with_bindings("chain_a", true, &["io_a"]),
            chain_with_bindings("chain_b", false, &["io_a"]),
        ],
        vec![input_binding("io_a", "scarlett", 0)],
    );

    let message = toggle(&dispatcher, "chain_b")
        .expect_err("must be rejected")
        .to_string();

    assert!(
        message.contains("scarlett"),
        "error must name the device, got: {message}"
    );
    assert!(
        message.contains("chain_a"),
        "error must name the chain already holding the channel, got: {message}"
    );
}

#[test]
fn conflict_on_any_shared_channel_of_a_multi_channel_endpoint_is_rejected() {
    // A stereo endpoint (ch 0+1) overlaps a mono endpoint on ch 1.
    let stereo = IoBinding {
        id: "io_stereo".to_string(),
        name: "io_stereo".to_string(),
        inputs: vec![IoEndpoint {
            name: "Stereo In".to_string(),
            device_id: DeviceId("scarlett".to_string()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
        outputs: vec![],
    };
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![
            chain_with_bindings("chain_a", true, &["io_stereo"]),
            chain_with_bindings("chain_b", false, &["io_mono"]),
        ],
        vec![stereo, input_binding("io_mono", "scarlett", 1)],
    );

    let result = toggle(&dispatcher, "chain_b");

    assert!(
        result.is_err(),
        "ch1 is already inside the enabled chain's stereo pair, got {result:?}"
    );
    assert!(!enabled(&project, "chain_b"), "chain_b must stay disabled");
}

// ---------------------------------------------------------------------------
// Non-conflicts — must stay allowed
// ---------------------------------------------------------------------------

#[test]
fn enabling_second_chain_on_a_different_channel_is_allowed() {
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![
            chain_with_bindings("chain_a", true, &["io_a"]),
            chain_with_bindings("chain_b", false, &["io_b"]),
        ],
        vec![
            input_binding("io_a", "scarlett", 0),
            input_binding("io_b", "scarlett", 1),
        ],
    );

    assert!(
        toggle(&dispatcher, "chain_b").is_ok(),
        "distinct channels on the same device are independent streams"
    );
    assert!(enabled(&project, "chain_b"), "chain_b must be enabled");
}

#[test]
fn enabling_second_chain_on_same_channel_of_another_device_is_allowed() {
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![
            chain_with_bindings("chain_a", true, &["io_a"]),
            chain_with_bindings("chain_b", false, &["io_b"]),
        ],
        vec![
            input_binding("io_a", "scarlett", 0),
            input_binding("io_b", "clarett", 0),
        ],
    );

    assert!(
        toggle(&dispatcher, "chain_b").is_ok(),
        "channel 0 of a different device is a different physical input"
    );
    assert!(enabled(&project, "chain_b"), "chain_b must be enabled");
}

#[test]
fn enabling_is_allowed_when_the_chain_holding_the_channel_is_disabled() {
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![
            chain_with_bindings("chain_a", false, &["io_a"]),
            chain_with_bindings("chain_b", false, &["io_a"]),
        ],
        vec![input_binding("io_a", "scarlett", 0)],
    );

    assert!(
        toggle(&dispatcher, "chain_b").is_ok(),
        "only ENABLED chains hold a channel"
    );
    assert!(enabled(&project, "chain_b"), "chain_b must be enabled");
}

// ---------------------------------------------------------------------------
// The other doors into "enabled" — same rule, no exception
// ---------------------------------------------------------------------------

#[test]
fn adding_an_already_enabled_chain_on_a_taken_channel_is_rejected() {
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![chain_with_bindings("chain_a", true, &["io_a"])],
        vec![
            input_binding("io_a", "scarlett", 0),
            input_binding("io_b", "scarlett", 0),
        ],
    );

    let result = dispatcher.dispatch(Command::Chain(ChainCommand::AddChain {
        chain: chain_with_bindings("chain_b", true, &["io_b"]),
    }));

    assert!(
        result.is_err(),
        "adding an enabled chain on a taken channel must be rejected, got {result:?}"
    );
    assert_eq!(
        project.borrow().chains.len(),
        1,
        "the rejected chain must not land in the project"
    );
}

#[test]
fn saving_an_enabled_chain_onto_a_taken_channel_is_rejected() {
    // SaveChain preserves the existing `enabled` flag, so the risk is the
    // reverse of AddChain: an ALREADY enabled chain re-saved with I/O that
    // lands on a capture point another enabled chain holds.
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![
            chain_with_bindings("chain_a", true, &["io_a"]),
            chain_with_bindings("chain_b", true, &["io_free"]),
        ],
        vec![
            input_binding("io_a", "scarlett", 0),
            input_binding("io_free", "scarlett", 1),
            input_binding("io_b", "scarlett", 0),
        ],
    );

    let result = dispatcher.dispatch(Command::Chain(ChainCommand::SaveChain {
        chain: chain_with_bindings("chain_b", true, &["io_b"]),
    }));

    assert!(
        result.is_err(),
        "SaveChain must not move an enabled chain onto a taken channel, got {result:?}"
    );
    assert_eq!(
        project.borrow().chains[1].io_binding_ids,
        vec!["io_free".to_string()],
        "the rejected save must not be applied"
    );
}

#[test]
fn saving_a_new_enabled_chain_on_a_taken_channel_is_rejected() {
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![chain_with_bindings("chain_a", true, &["io_a"])],
        vec![
            input_binding("io_a", "scarlett", 0),
            input_binding("io_b", "scarlett", 0),
        ],
    );

    let result = dispatcher.dispatch(Command::Chain(ChainCommand::SaveChain {
        chain: chain_with_bindings("chain_new", true, &["io_b"]),
    }));

    assert!(
        result.is_err(),
        "creating an enabled chain on a taken channel must be rejected, got {result:?}"
    );
    assert_eq!(
        project.borrow().chains.len(),
        1,
        "the rejected chain must not land in the project"
    );
}

#[test]
fn configuring_a_chain_onto_a_taken_channel_while_enabled_is_rejected() {
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![
            chain_with_bindings("chain_a", true, &["io_a"]),
            chain_with_bindings("chain_b", true, &["io_free"]),
        ],
        vec![
            input_binding("io_a", "scarlett", 0),
            input_binding("io_free", "scarlett", 1),
            input_binding("io_b", "scarlett", 0),
        ],
    );

    let result = dispatcher.dispatch(Command::Chain(ChainCommand::ConfigureChain {
        chain: chain_with_bindings("chain_b", true, &["io_b"]),
    }));

    assert!(
        result.is_err(),
        "re-pointing an enabled chain onto a taken channel must be rejected, got {result:?}"
    );
    assert_eq!(
        project.borrow().chains[1].io_binding_ids,
        vec!["io_free".to_string()],
        "the rejected reconfiguration must not be applied"
    );
}

#[test]
fn re_binding_an_enabled_chain_onto_a_taken_channel_is_rejected() {
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![
            chain_with_bindings("chain_a", true, &["io_a"]),
            chain_with_bindings("chain_b", true, &["io_free"]),
        ],
        vec![
            input_binding("io_a", "scarlett", 0),
            input_binding("io_free", "scarlett", 1),
        ],
    );

    let result = dispatcher.dispatch(Command::Chain(ChainCommand::SetChainIoBindings {
        chain: ChainId("chain_b".to_string()),
        binding_ids: vec!["io_a".to_string()],
    }));

    assert!(
        result.is_err(),
        "an enabled chain must not be re-bound onto a taken channel, got {result:?}"
    );
    assert_eq!(
        project.borrow().chains[1].io_binding_ids,
        vec!["io_free".to_string()],
        "the rejected binding change must not be applied"
    );
}

#[test]
fn re_binding_a_disabled_chain_onto_a_taken_channel_is_allowed() {
    // A disabled chain captures nothing; the guard fires when it is enabled.
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![
            chain_with_bindings("chain_a", true, &["io_a"]),
            chain_with_bindings("chain_b", false, &["io_free"]),
        ],
        vec![
            input_binding("io_a", "scarlett", 0),
            input_binding("io_free", "scarlett", 1),
        ],
    );

    assert!(
        dispatcher
            .dispatch(Command::Chain(ChainCommand::SetChainIoBindings {
                chain: ChainId("chain_b".to_string()),
                binding_ids: vec!["io_a".to_string()],
            }))
            .is_ok(),
        "binding a disabled chain is free"
    );
    assert!(
        toggle(&dispatcher, "chain_b").is_err(),
        "…but enabling it afterwards must be rejected"
    );
    assert!(!enabled(&project, "chain_b"), "chain_b must stay disabled");
}

#[test]
fn loading_a_project_with_two_enabled_chains_on_one_channel_disables_the_extra() {
    // Hand-edited YAML (or a project saved before this guard existed) must not
    // come up with one input live in two chains.
    let (dispatcher, project, _tmp) = dispatcher_with(
        Vec::new(),
        vec![
            input_binding("io_a", "scarlett", 0),
            input_binding("io_b", "scarlett", 0),
        ],
    );

    let loaded = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![
            chain_with_bindings("chain_a", true, &["io_a"]),
            chain_with_bindings("chain_b", true, &["io_b"]),
        ],
        midi: None,
    };
    dispatcher
        .dispatch(Command::Project(
            application::command::ProjectCommand::LoadProject {
                project: loaded,
                path: std::path::PathBuf::from("project.openrig"),
            },
        ))
        .expect("load must succeed");

    assert!(
        enabled(&project, "chain_a"),
        "the first chain keeps the channel"
    );
    assert!(
        !enabled(&project, "chain_b"),
        "the second chain on the same channel must come up disabled"
    );
}

#[test]
fn disabling_a_chain_is_never_rejected() {
    // Toggling OFF must never hit the conflict guard, even from a project that
    // somehow already holds a duplicate (hand-edited YAML).
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![
            chain_with_bindings("chain_a", true, &["io_a"]),
            chain_with_bindings("chain_b", true, &["io_a"]),
        ],
        vec![input_binding("io_a", "scarlett", 0)],
    );

    assert!(
        toggle(&dispatcher, "chain_b").is_ok(),
        "disabling must always be allowed"
    );
    assert!(!enabled(&project, "chain_b"), "chain_b must be disabled");
}
