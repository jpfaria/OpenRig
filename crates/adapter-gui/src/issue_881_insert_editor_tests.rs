//! #881 — drives the insert editor the way the user does and checks the
//! PROJECT, not the callback: pick the E/S the external loop runs through,
//! press OK, and the `Insert` block must come out bound to it.
//!
//! Until now the window offered send/return device pickers that no longer exist
//! in the model (an insert carries one binding id since #716), and those pickers
//! were no-ops — so an insert added in the app could never be bound, and the
//! chain played nothing.
//!
//! Rendering proves layout only; this proves the clicks land (#749/#761).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Model, VecModel};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::ChannelMode;
use infra_filesystem::{IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;

use crate::insert_wiring::{open_insert_window, wire, InsertWiringCtx};
use crate::project_ops::create_new_project_session;
use crate::state::{InsertDraft, ProjectSession};
use crate::{AppWindow, ChainInsertWindow};

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
            inputs: vec![endpoint("Main in")],
            outputs: vec![endpoint("Main out")],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX LOOP".into(),
            inputs: vec![endpoint("Return")],
            outputs: vec![endpoint("Send")],
        },
    ]
}

/// A session whose single chain carries one unbound `Insert` block.
fn session_with_insert() -> Rc<RefCell<Option<ProjectSession>>> {
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
            id: BlockId("insert".into()),
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "external_loop".into(),
                io: String::new(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    }];
    *session.io_bindings.borrow_mut() = registry();
    Rc::new(RefCell::new(Some(session)))
}

struct Harness {
    window: ChainInsertWindow,
    session: Rc<RefCell<Option<ProjectSession>>>,
    _app: AppWindow,
}

impl Harness {
    fn new() -> Self {
        i_slint_backend_testing::init_no_event_loop();
        infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
        let app = AppWindow::new().unwrap();
        let window = ChainInsertWindow::new().unwrap();
        let session = session_with_insert();
        let insert_draft = Rc::new(RefCell::new(None));
        let ctx = InsertWiringCtx {
            insert_draft: insert_draft.clone(),
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(Vec::new())),
            project_session: session.clone(),
            project_chains: Rc::new(VecModel::default()),
            saved_project_snapshot: Rc::new(RefCell::new(None)),
            project_dirty: Rc::new(RefCell::new(false)),
            auto_save: false,
        };
        wire(&app, &window, ctx);
        // The app opens the editor for the block the user clicked.
        open_insert_window(
            &window,
            &insert_draft,
            InsertDraft {
                chain_index: 0,
                block_index: 0,
                io: String::new(),
            },
            &registry(),
            true,
        );
        Self {
            window,
            session,
            _app: app,
        }
    }

    /// The binding the insert block currently points at.
    fn insert_io(&self) -> String {
        let borrow = self.session.borrow();
        let session = borrow.as_ref().unwrap();
        let project = session.project.borrow();
        match &project.chains[0].blocks[0].kind {
            AudioBlockKind::Insert(ib) => ib.io.clone(),
            other => panic!("expected an Insert block, got {}", other.label()),
        }
    }
}

#[test]
fn picking_an_io_and_pressing_ok_binds_the_insert_loop() {
    let h = Harness::new();
    h.window.invoke_select_binding(1);
    h.window.invoke_save();

    assert_eq!(
        h.insert_io(),
        "fx",
        "#881: OK must write the picked E/S onto the insert block"
    );
}

#[test]
fn the_window_offers_every_binding_of_the_session() {
    let h = Harness::new();
    let options: Vec<String> = h
        .window
        .get_binding_options()
        .iter()
        .map(|s| s.to_string())
        .collect();

    assert_eq!(
        options,
        vec!["MAIN".to_string(), "FX LOOP".to_string()],
        "#881: the insert editor must list the E/S bindings to pick from"
    );
}

/// Reopening a bound insert must show WHERE it points — the editor that forgets
/// the current pick is how a binding silently gets rewritten.
#[test]
fn reopening_a_bound_insert_shows_its_binding() {
    let h = Harness::new();
    open_insert_window(
        &h.window,
        &Rc::new(RefCell::new(None)),
        InsertDraft {
            chain_index: 0,
            block_index: 0,
            io: "fx".into(),
        },
        &registry(),
        true,
    );

    assert_eq!(
        h.window.get_selected_binding_index(),
        1,
        "#881: the editor must open on the E/S the insert already points at"
    );
}

#[test]
fn ok_without_a_pick_warns_instead_of_writing() {
    let h = Harness::new();
    h.window.invoke_save();

    assert!(
        h.window.get_show_binding_warning(),
        "pressing OK with no E/S picked must warn"
    );
    assert!(
        h.insert_io().is_empty(),
        "nothing may be written before a pick"
    );
}
