//! Responsibility: proves a block added from the compact view lands in that window's block list
//!
//! #898 — the user clicks an insert slot in the compact chain view, picks a
//! block type, confirms the editor. The block IS created (it shows up in the
//! full-screen chain view and after reopening the compact window), but the
//! compact view the click came from never re-projects its `compact_blocks`
//! model, so the row the user is looking at stays unchanged.
//!
//! This drives the real wiring end to end — open the compact view, fire the
//! type-picker callback the way the UI does, then confirm the editor it
//! opens — and asserts on what the user sees: the compact window's own block
//! list. Same defect class as #667 (preset switch) and #614 ("dispatch alone
//! is dead"): the command runs, the project changes, the compact UI model is
//! never rebuilt.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, Model, Timer, VecModel};

use application::live_source::NoLiveSource;
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use crate::block_choose_type_callback::{self, BlockChooseTypeCallbackCtx};
use crate::block_insert_callbacks::{self, BlockInsertCallbacksCtx};
use crate::compact_chain_callbacks::{self, CompactChainCallbacksCtx};
use crate::project_ops::create_new_project_session;
use crate::state::ProjectSession;
use crate::{AppWindow, CompactChainViewWindow};

/// A session with one chain carrying a single gain block, so the compact view
/// has something to render and the assertion can watch the list grow.
fn session_with_one_block() -> Rc<RefCell<Option<ProjectSession>>> {
    let tmp = tempfile::TempDir::new().unwrap();
    let session = create_new_project_session(&tmp.path().join("config.yaml"));
    std::mem::forget(tmp);
    session.project.borrow_mut().chains = vec![Chain {
        id: ChainId("rig:input-1".into()),
        description: Some("Rig".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![AudioBlock {
            id: BlockId("gain-1".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: ParameterSet::default(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    }];
    Rc::new(RefCell::new(Some(session)))
}

struct Harness {
    _app: AppWindow,
    session: Rc<RefCell<Option<ProjectSession>>>,
    open_compact_window:
        Rc<RefCell<Option<(usize, slint::Weak<CompactChainViewWindow>)>>>,
    open_block_windows: Rc<RefCell<Vec<crate::state::BlockWindow>>>,
    block_editor_draft: Rc<RefCell<Option<crate::state::BlockEditorDraft>>>,
}

impl Harness {
    fn new() -> Self {
        i_slint_backend_testing::init_no_event_loop();
        infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());

        let app = AppWindow::new().unwrap();
        let session = session_with_one_block();
        let open_compact_window = Rc::new(RefCell::new(None));
        let open_block_windows = Rc::new(RefCell::new(Vec::new()));
        let block_editor_draft = Rc::new(RefCell::new(None));

        let project_chains = Rc::new(VecModel::default());
        let input_chain_devices = Rc::new(RefCell::new(Vec::new()));
        let output_chain_devices = Rc::new(RefCell::new(Vec::new()));
        let saved_project_snapshot = Rc::new(RefCell::new(None));
        let project_dirty = Rc::new(RefCell::new(false));
        let selected_block = Rc::new(RefCell::new(None));
        let inline_tab_state = Rc::new(RefCell::new(Default::default()));
        let block_model_options = Rc::new(VecModel::default());
        let filtered_block_model_options = Rc::new(VecModel::default());
        let block_model_option_labels = Rc::new(VecModel::default());
        let block_parameter_items = Rc::new(VecModel::default());
        let multi_slider_points = Rc::new(VecModel::default());
        let curve_editor_points = Rc::new(VecModel::default());
        let eq_band_curves = Rc::new(VecModel::default());

        compact_chain_callbacks::wire(
            &app,
            CompactChainCallbacksCtx {
                project_session: session.clone(),
                block_stream_reads: Rc::new(NoLiveSource),
                audio_taps: Rc::new(application::audio_taps::NoAudioTaps),
                project_chains: project_chains.clone(),
                input_chain_devices: input_chain_devices.clone(),
                output_chain_devices: output_chain_devices.clone(),
                saved_project_snapshot: saved_project_snapshot.clone(),
                project_dirty: project_dirty.clone(),
                toast_timer: Rc::new(Timer::default()),
                open_compact_window: open_compact_window.clone(),
                block_editor_draft: block_editor_draft.clone(),
                fullscreen: false,
                auto_save: false,
            },
        );

        // The compact view forwards the picked type to the main window, so the
        // insert + choose-type wiring has to be live for the flow to run.
        block_insert_callbacks::wire(
            &app,
            BlockInsertCallbacksCtx {
                inline_tab_state: inline_tab_state.clone(),
                selected_block: selected_block.clone(),
                block_editor_draft: block_editor_draft.clone(),
                block_type_options: Rc::new(VecModel::default()),
                block_model_options: block_model_options.clone(),
                filtered_block_model_options: filtered_block_model_options.clone(),
                block_model_option_labels: block_model_option_labels.clone(),
                block_parameter_items: block_parameter_items.clone(),
                multi_slider_points: multi_slider_points.clone(),
                curve_editor_points: curve_editor_points.clone(),
                eq_band_curves: eq_band_curves.clone(),
                project_session: session.clone(),
                project_chains: project_chains.clone(),
                saved_project_snapshot: saved_project_snapshot.clone(),
                project_dirty: project_dirty.clone(),
                input_chain_devices: input_chain_devices.clone(),
                output_chain_devices: output_chain_devices.clone(),
                block_editor_persist_timer: Rc::new(Timer::default()),
                auto_save: false,
            },
        );

        let insert_window = crate::ChainInsertWindow::new().unwrap();
        let port_window = crate::ChainPortWindow::new().unwrap();
        block_choose_type_callback::wire(
            &app,
            &insert_window,
            &port_window,
            BlockChooseTypeCallbackCtx {
                inline_tab_state,
                block_editor_draft: block_editor_draft.clone(),
                insert_draft: Rc::new(RefCell::new(None)),
                block_model_options,
                filtered_block_model_options,
                block_model_option_labels,
                block_parameter_items,
                multi_slider_points,
                curve_editor_points,
                eq_band_curves,
                project_session: session.clone(),
                project_chains,
                block_stream_reads: Rc::new(NoLiveSource),
                saved_project_snapshot,
                project_dirty,
                input_chain_devices,
                output_chain_devices,
                selected_block,
                open_block_windows: open_block_windows.clone(),
                plugin_info_window: Rc::new(RefCell::new(None)),
                port_draft: Rc::new(RefCell::new(None)),
                auto_save: false,
            },
        );

        Self {
            _app: app,
            session,
            open_compact_window,
            open_block_windows,
            block_editor_draft,
        }
    }

    fn compact_window(&self) -> CompactChainViewWindow {
        self.open_compact_window
            .borrow()
            .as_ref()
            .and_then(|(_, weak)| weak.upgrade())
            .expect("opening the compact chain view must create its window")
    }

    fn blocks_in_project(&self) -> usize {
        let borrow = self.session.borrow();
        let session = borrow.as_ref().unwrap();
        let proj = session.project.borrow();
        proj.chains[0].blocks.len()
    }
}

#[test]
fn adding_a_block_from_the_compact_view_shows_it_in_the_compact_view() {
    let h = Harness::new();
    h._app.invoke_open_compact_chain_view(0);
    let compact = h.compact_window();

    let before_rows = compact.get_compact_blocks().row_count();
    let before_blocks = h.blocks_in_project();

    // The user clicks the insert slot after the first block and picks the
    // first offered type — exactly what `compact_chain_view.slint` fires.
    compact.invoke_choose_block_type(0, 1, 0);

    // The add flow opens the per-block editor (#815); confirming it is what
    // creates the block.
    let editor = h
        .open_block_windows
        .borrow()
        .last()
        .map(|bw| bw.window.clone_strong())
        .expect("picking a block type must open the add editor");
    crate::BlockEditorBridge::get(&editor).invoke_save_block_drawer();

    assert_eq!(
        h.blocks_in_project(),
        before_blocks + 1,
        "sanity: confirming the editor must add the block to the chain"
    );
    assert_eq!(
        compact.get_compact_blocks().row_count(),
        before_rows + 1,
        "REGRESSION #898: the block was added to the project but the compact \
         view the user inserted it from still lists {before_rows} rows — the \
         new block only appears after reopening the window or switching to \
         the full-screen chain view"
    );

    let _ = h.block_editor_draft.borrow();
}
