//! A chain whose audio interface is not available must not switch on.
//!
//! Enabling a chain whose binding points at a device that is not present
//! would leave it looking on with no stream behind it. It is refused through
//! the Command bus (GUI / MCP / gRPC alike), the chain stays off, and the
//! error names the missing interface so the toast tells the user what to fix.
//!
//! Device presence comes from the frontend that owns an audio host through the
//! `DevicePresence` port; here a fake answers, so no real device is touched.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{ChainCommand, Command};
use application::device_presence::DevicePresence;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_filesystem::FilesystemStorage;
use project::binding_discovery::PortDirection;
use project::chain::Chain;
use project::project::Project;

/// Fake presence: only the listed device ids exist.
struct PresentDevices(Vec<&'static str>);

impl DevicePresence for PresentDevices {
    fn is_present(&self, device: &DeviceId, _direction: PortDirection) -> bool {
        self.0.contains(&device.0.as_str())
    }
}

fn endpoint(device: &str, channel: usize) -> IoEndpoint {
    IoEndpoint {
        name: format!("Ch {channel}"),
        device_id: DeviceId(device.to_string()),
        mode: ChannelMode::Mono,
        channels: vec![channel],
    }
}

/// A binding named `name` with one input on `in_device` and one output on
/// `out_device`.
fn binding(id: &str, name: &str, in_device: &str, out_device: &str) -> IoBinding {
    IoBinding {
        id: id.to_string(),
        name: name.to_string(),
        inputs: vec![endpoint(in_device, 0)],
        outputs: vec![endpoint(out_device, 0)],
    }
}

fn chain(id: &str, binding_ids: &[&str]) -> Chain {
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: binding_ids.iter().map(|s| s.to_string()).collect(),
        blocks: vec![],
        loopers: vec![],
        di_output: None,
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

fn dispatcher_with(
    chains: Vec<Chain>,
    bindings: Vec<IoBinding>,
    present: Option<Vec<&'static str>>,
) -> (LocalDispatcher, Rc<RefCell<Project>>, tempfile::TempDir) {
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
    if let Some(present) = present {
        dispatcher.attach_device_presence(Rc::new(PresentDevices(present)));
    }
    (dispatcher, project, tmp)
}

fn toggle(dispatcher: &LocalDispatcher, chain: &str) -> anyhow::Result<()> {
    dispatcher
        .dispatch(Command::Chain(ChainCommand::ToggleChainEnabled {
            chain: ChainId(chain.to_string()),
        }))
        .map(|_| ())
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

#[test]
fn enabling_a_chain_whose_input_interface_is_absent_is_refused_and_names_it() {
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![chain("digital", &["hd8"])],
        vec![binding("hd8", "Quantum HD 8", "hd8-uid", "hd8-uid")],
        Some(vec![]),
    );

    let err = toggle(&dispatcher, "digital").expect_err("absent interface must refuse");

    assert!(
        !enabled(&project, "digital"),
        "the chain must stay off when its interface is absent"
    );
    assert!(
        err.to_string().contains("Quantum HD 8"),
        "the error must name the missing interface, got: {err}"
    );
}

#[test]
fn enabling_a_chain_whose_output_interface_is_absent_is_refused() {
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![chain("digital", &["split"])],
        vec![binding("split", "Split rig", "in-uid", "out-uid")],
        Some(vec!["in-uid"]),
    );

    assert!(toggle(&dispatcher, "digital").is_err());
    assert!(!enabled(&project, "digital"));
}

#[test]
fn enabling_a_chain_whose_interfaces_are_present_succeeds() {
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![chain("digital", &["hd8"])],
        vec![binding("hd8", "Quantum HD 8", "hd8-uid", "hd8-uid")],
        Some(vec!["hd8-uid"]),
    );

    toggle(&dispatcher, "digital").expect("present interface enables");

    assert!(enabled(&project, "digital"));
}

#[test]
fn disabling_a_chain_never_checks_its_interface() {
    let mut on = chain("digital", &["hd8"]);
    on.enabled = true;
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![on],
        vec![binding("hd8", "Quantum HD 8", "hd8-uid", "hd8-uid")],
        Some(vec![]),
    );

    toggle(&dispatcher, "digital").expect("switching off is always allowed");

    assert!(!enabled(&project, "digital"));
}

#[test]
fn a_dispatcher_with_no_presence_source_does_not_block_enabling() {
    // MCP-only / tests: no frontend hosts an audio host, so there is nothing
    // to ask — the toggle behaves as before.
    let (dispatcher, project, _tmp) = dispatcher_with(
        vec![chain("digital", &["hd8"])],
        vec![binding("hd8", "Quantum HD 8", "hd8-uid", "hd8-uid")],
        None,
    );

    toggle(&dispatcher, "digital").expect("no presence source, no check");

    assert!(enabled(&project, "digital"));
}
