//! Responsibility: proves a scene switched from the compact view re-projects that window's block list
//!
//! #966 — the user switches SCENE from inside the compact chain view. The
//! audio follows (the project's blocks are re-projected from the rig), but
//! the compact window keeps rendering the previous scene: a block the new
//! scene bypasses still shows enabled.
//!
//! Same defect class as #667 (preset switch), #898 (insert) and #614
//! ("dispatch alone is dead"): the command runs, the project changes, and
//! the compact window's own `compact_blocks` model is never rebuilt. The
//! sibling `on_switch_chain_preset` closure re-projects; the scene one does
//! not.
//!
//! The assertion is on what the user sees — the compact window's block list —
//! after driving the same callback `compact_chain_view.slint` fires.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Model, ModelRc, Timer, VecModel};

use application::chain_factory::{build_default_chain, DefaultChainParams, EndpointSpec};
use application::command::{
    BlockCommand, ChainCommand, Command, ProjectCommand, RigNavKind, SelectionCommand,
};
use domain::ids::BlockId;
use project::block::AudioBlockKind;

use crate::compact_block_view::build_compact_blocks;
use crate::project_ops::create_new_project_session;
use crate::runtime_lifecycle::RuntimeAttach;
use crate::runtime_analyzers::AnalyzerSessions;
use crate::state::ProjectSession;
use crate::{AppWindow, CompactChainViewWindow, ProjectChainItem};

/// A one-chain session carrying a gate block, with TWO scenes: scene 1 leaves
/// the gate enabled, scene 2 bypasses it. Scene 2 is the active one, so the
/// switch under test is "back to scene 1" — the gate must come back enabled.
fn session_with_two_scenes() -> (Rc<RefCell<Option<ProjectSession>>>, BlockId) {
    let tmp = tempfile::TempDir::new().unwrap();
    let session = create_new_project_session(&tmp.path().join("config.yaml"));
    std::mem::forget(tmp);

    let chain = build_default_chain(DefaultChainParams {
        project: &session.project.borrow(),
        instrument: "electric_guitar",
        description: Some("Chain".into()),
        input: EndpointSpec {
            device_id: Some("dev"),
            channels: vec![0],
            io: String::new(),
            endpoint: String::new(),
        },
        output: EndpointSpec {
            device_id: Some("dev"),
            channels: vec![0, 1],
            io: String::new(),
            endpoint: String::new(),
        },
    });
    session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::SaveChain { chain }))
        .expect("SaveChain");
    let chain_id = session.project.borrow().chains[0].id.clone();
    session
        .dispatcher
        .dispatch(Command::Block(BlockCommand::AddBlock {
            chain: chain_id.clone(),
            kind: "dynamics".into(),
            model_id: "gate_basic".into(),
            position: 1,
        }))
        .expect("AddBlock");
    let gate = session
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id == chain_id)
        .and_then(|c| {
            c.blocks
                .iter()
                .find(|b| matches!(b.kind, AudioBlockKind::Core(ref cb) if cb.effect_type == "dynamics"))
                .map(|b| b.id.clone())
        })
        .expect("gate block present");
    session
        .dispatcher
        .dispatch(Command::Project(ProjectCommand::CaptureRigEdits))
        .expect("capture scene 1");

    // Scene 2 (added by the sentinel, becomes active) bypasses the gate.
    session
        .dispatcher
        .dispatch(Command::Selection(SelectionCommand::ApplyRigNav {
            chain: chain_id.clone(),
            kind: RigNavKind::Scene(-1),
        }))
        .expect("add scene 2");
    session
        .dispatcher
        .dispatch(Command::Block(BlockCommand::ToggleBlockEnabled {
            chain: chain_id,
            block: gate.clone(),
        }))
        .expect("bypass the gate on scene 2");
    session
        .dispatcher
        .dispatch(Command::Project(ProjectCommand::CaptureRigEdits))
        .expect("capture scene 2");

    (Rc::new(RefCell::new(Some(session))), gate)
}

struct Harness {
    app: AppWindow,
    compact: CompactChainViewWindow,
    session: Rc<RefCell<Option<ProjectSession>>>,
    gate: BlockId,
}

impl Harness {
    fn new() -> Self {
        i_slint_backend_testing::init_no_event_loop();
        infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());

        let app = AppWindow::new().unwrap();
        let (session, gate) = session_with_two_scenes();

        let taps: Rc<dyn application::audio_taps::AudioTaps> =
            Rc::new(application::audio_taps::NoAudioTaps);
        let analyzers = AnalyzerSessions::new(&session, &taps);
        let runtime_attach = RuntimeAttach::new(&Rc::new(RefCell::new(None)), &analyzers);
        let project_chains: Rc<VecModel<ProjectChainItem>> = Rc::new(VecModel::default());

        // The main window owns the scene switch (the compact view forwards to
        // it), so its rig-nav wiring has to be live for the flow to run.
        crate::chain_rig_nav_wiring::wire(
            &app,
            crate::chain_rig_nav_wiring::ChainRigNavCtx {
                project_session: session.clone(),
                project_chains,
                runtime_attach,
                input_chain_devices: Rc::new(RefCell::new(Vec::new())),
                output_chain_devices: Rc::new(RefCell::new(Vec::new())),
                toast_timer: Rc::new(Timer::default()),
                saved_project_snapshot: Rc::new(RefCell::new(None)),
                project_dirty: Rc::new(RefCell::new(false)),
                auto_save: false,
            },
        );

        let compact = CompactChainViewWindow::new().unwrap();
        crate::compact_chain_header_wiring::wire(&app, &compact, 0, &session);
        // The window opens showing the active scene — seed its model the way
        // `open_compact_chain_view` does.
        let blocks = {
            let borrow = session.borrow();
            let s = borrow.as_ref().unwrap();
            let blocks = build_compact_blocks(&s.project.borrow(), 0, &s.io_bindings.borrow());
            blocks
        };
        compact.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));

        Self {
            app,
            compact,
            session,
            gate,
        }
    }

    /// Is the gate enabled in the live project (what the audio follows)?
    fn gate_enabled_in_project(&self) -> bool {
        let borrow = self.session.borrow();
        let s = borrow.as_ref().unwrap();
        let proj = s.project.borrow();
        proj.chains[0]
            .blocks
            .iter()
            .find(|b| b.id == self.gate)
            .expect("gate still in the chain")
            .enabled
    }

    /// Is the gate enabled in the compact window's own list (what the user sees)?
    fn gate_enabled_in_compact_view(&self) -> bool {
        let rows = self.compact.get_compact_blocks();
        (0..rows.row_count())
            .filter_map(|i| rows.row_data(i))
            .find(|it| it.block_id.as_str() == self.gate.0)
            .expect("gate row in the compact list")
            .enabled
    }
}

#[test]
fn switching_scene_from_the_compact_view_updates_that_window_block_list() {
    let h = Harness::new();
    assert!(
        !h.gate_enabled_in_compact_view(),
        "sanity: the window opens on scene 2, which bypasses the gate"
    );

    // Exactly what `compact_chain_view.slint` fires when the user taps a
    // scene button in the compact header.
    h.compact.invoke_switch_chain_scene(1);

    assert!(
        h.gate_enabled_in_project(),
        "sanity: the scene switch must re-project the chain — scene 1 leaves \
         the gate enabled"
    );
    assert!(
        h.gate_enabled_in_compact_view(),
        "REGRESSION #966: the scene switch changed the audio but the compact \
         view the user tapped in still shows the gate bypassed — its \
         `compact_blocks` model was never rebuilt (same class as #667/#898)"
    );
    let _ = &h.app;
}
