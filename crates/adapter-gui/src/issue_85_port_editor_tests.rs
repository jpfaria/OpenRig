//! #85 — drives the port editor the way the user does and checks the PROJECT,
//! not the callback: pick an E/S, pick an endpoint, press OK, and the mid
//! `Output` block must come out pointing there.
//!
//! Rendering proves layout only; this proves the clicks land (#749/#761).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Model, VecModel};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::ChannelMode;
use infra_filesystem::{IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, OutputBlock};
use project::chain::Chain;

use crate::port_wiring::{wire_port_window, PortWiringCtx};
use crate::project_ops::create_new_project_session;
use crate::state::{PortDraft, ProjectSession};
use crate::{AppWindow, ChainPortWindow};

fn endpoint(name: &str) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels: vec![0, 1],
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![],
            outputs: vec![endpoint("Main out")],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: vec![],
            outputs: vec![endpoint("Aux out")],
        },
    ]
}

/// A session whose single chain carries one unbound mid `Output` port.
fn session_with_port() -> Rc<RefCell<Option<ProjectSession>>> {
    let tmp = tempfile::TempDir::new().unwrap();
    let session = create_new_project_session(&tmp.path().join("config.yaml"));
    std::mem::forget(tmp);
    session.project.borrow_mut().chains = vec![Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![AudioBlock {
            id: BlockId("port".into()),
            enabled: true,
            kind: AudioBlockKind::Output(OutputBlock {
                model: "standard".into(),
                io: String::new(),
                endpoint: String::new(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    }];
    *session.io_bindings.borrow_mut() = registry();
    Rc::new(RefCell::new(Some(session)))
}

struct Harness {
    window: ChainPortWindow,
    session: Rc<RefCell<Option<ProjectSession>>>,
    _app: AppWindow,
}

impl Harness {
    fn new() -> Self {
        i_slint_backend_testing::init_no_event_loop();
        infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
        let app = AppWindow::new().unwrap();
        let window = ChainPortWindow::new().unwrap();
        let session = session_with_port();
        let binding_options = Rc::new(VecModel::from(Vec::<slint::SharedString>::new()));
        let endpoint_options = Rc::new(VecModel::from(Vec::<slint::SharedString>::new()));
        window.set_binding_options(slint::ModelRc::from(binding_options.clone()));
        window.set_endpoint_options(slint::ModelRc::from(endpoint_options.clone()));
        let port_draft = Rc::new(RefCell::new(Some(PortDraft {
            chain_index: 0,
            block_index: 0,
            is_input: false,
            io: String::new(),
            endpoint: String::new(),
            enabled: true,
        })));
        let ctx = PortWiringCtx {
            port_draft,
            project_session: session.clone(),
            project_chains: Rc::new(VecModel::default()),
            project_runtime: Rc::new(RefCell::new(None)),
            saved_project_snapshot: Rc::new(RefCell::new(None)),
            project_dirty: Rc::new(RefCell::new(false)),
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(Vec::new())),
            auto_save: false,
        };
        wire_port_window(&app, &window, ctx);
        Self {
            window,
            session,
            _app: app,
        }
    }

    /// Where the port block currently points.
    fn port_target(&self) -> (String, String) {
        let borrow = self.session.borrow();
        let session = borrow.as_ref().unwrap();
        let project = session.project.borrow();
        match &project.chains[0].blocks[0].kind {
            AudioBlockKind::Output(o) => (o.io.clone(), o.endpoint.clone()),
            other => panic!("expected an Output port, got {}", other.label()),
        }
    }
}

#[test]
fn picking_an_io_and_an_endpoint_and_pressing_ok_points_the_block_there() {
    let h = Harness::new();
    h.window.invoke_select_binding(1);
    h.window.invoke_save();

    assert_eq!(
        h.port_target(),
        ("aux".to_string(), "Aux out".to_string()),
        "#85: OK must write the picked E/S + endpoint onto the port block"
    );
}

#[test]
fn the_endpoint_list_follows_the_picked_io() {
    let h = Harness::new();

    h.window.invoke_select_binding(0);
    let first: Vec<String> = h
        .window
        .get_endpoint_options()
        .iter()
        .map(|s| s.to_string())
        .collect();
    h.window.invoke_select_binding(1);
    let second: Vec<String> = h
        .window
        .get_endpoint_options()
        .iter()
        .map(|s| s.to_string())
        .collect();

    assert_eq!(first, vec!["Main out".to_string()]);
    assert_eq!(second, vec!["Aux out".to_string()]);
}

#[test]
fn ok_without_a_pick_warns_instead_of_writing() {
    let h = Harness::new();
    h.window.invoke_save();

    assert!(
        h.window.get_show_endpoint_warning(),
        "pressing OK with nothing picked must warn"
    );
    assert!(
        h.port_target().0.is_empty(),
        "nothing may be written before a pick"
    );
}
