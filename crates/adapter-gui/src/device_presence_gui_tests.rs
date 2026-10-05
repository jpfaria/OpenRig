//! The desktop session asks the real audio host before switching a chain on:
//! a chain bound to a device the host does not list stays off.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{ChainCommand, Command};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::{ChainId, DeviceId};
use infra_filesystem::{ChannelMode, FilesystemStorage, IoBinding, IoEndpoint};
use project::chain::Chain;
use project::project::Project;

use super::attach_device_presence;
use crate::state::ProjectSession;

const MISSING: &str = "openrig-1069-no-such-device";

fn endpoint() -> IoEndpoint {
    IoEndpoint {
        name: "Ch 1".into(),
        device_id: DeviceId(MISSING.into()),
        mode: ChannelMode::Mono,
        channels: vec![0],
    }
}

fn project() -> Project {
    Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("digital".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec!["gone".into()],
            blocks: vec![],
            di_output: None,
            loopers: vec![],
            disabled_endpoints: Default::default(),
            mix: Default::default(),
        }],
        midi: None,
    }
}

#[test]
fn a_chain_on_a_device_the_host_does_not_list_stays_off() {
    // Never touch the machine's real config.yaml.
    let home = tempfile::tempdir().expect("temp config dir");
    let cfg = home.path().join("config.yaml");
    FilesystemStorage::update_app_config_at(&cfg, |config| {
        config.io_bindings = vec![IoBinding {
            id: "gone".into(),
            name: "Unplugged interface".into(),
            inputs: vec![endpoint()],
            outputs: vec![endpoint()],
        }];
    })
    .expect("seed config.yaml");
    let shared = Rc::new(RefCell::new(project()));
    let dispatcher = Rc::new(LocalDispatcher::new(Rc::clone(&shared)));
    dispatcher.attach_io_config_path(Some(cfg));
    let session = ProjectSession::with_dispatcher(
        project(),
        dispatcher as Rc<dyn CommandDispatcher>,
        None,
        None,
        home.path().join("presets"),
    );
    attach_device_presence(&session);

    let err = session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::ToggleChainEnabled {
            chain: ChainId("digital".into()),
        }))
        .expect_err("a missing interface must refuse");

    assert!(
        err.to_string().contains("Unplugged interface"),
        "got: {err}"
    );
    assert!(!shared.borrow().chains[0].enabled);
}
