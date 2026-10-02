//! Responsibility: proves a live edit in the block editor window reaches the open compact view
//!
//! #1023 — with the compact chain view open, the user opens a block's editor
//! window and turns a knob. The debounced persist writes the value into the
//! project and the audio follows, but the compact view keeps drawing the old
//! value: only the window's Save (#898) and the MCP/MIDI drain (#999)
//! re-project it.
//!
//! This builds the real editor window the compact view opens
//! (`block_editor_window_setup::create_and_wire`), fires its knob callback the
//! way the UI does, lets the 30 ms persist timer elapse, and asserts on what
//! the user sees: the compact window's own row for that block.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, Model, Timer, VecModel};

use application::chain_factory::{build_default_chain, DefaultChainParams, EndpointSpec};
use application::command::{BlockCommand, ChainCommand, Command};
use application::live_source::NoLiveSource;
use domain::ids::BlockId;
use project::block::AudioBlockKind;

use crate::block_editor::block_editor_data;
use crate::block_editor_window_setup::{self, BlockEditorWindowSetupCtx};
use crate::compact_chain_callbacks::{self, CompactChainCallbacksCtx};
use crate::project_ops::create_new_project_session;
use crate::state::ProjectSession;
use crate::{AppWindow, BlockEditorWindow, CompactBlockItem, CompactChainViewWindow};

const AMP_MODEL: &str = "blackface_clean";

fn session_with_an_amp() -> (Rc<RefCell<Option<ProjectSession>>>, BlockId) {
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
            chain: chain_id,
            kind: "amp".into(),
            model_id: AMP_MODEL.into(),
            position: 1,
            path: None,
        }))
        .expect("AddBlock");
    let amp = session.project.borrow().chains[0]
        .blocks
        .iter()
        .find(|b| matches!(b.kind, AudioBlockKind::Core(ref cb) if cb.model == AMP_MODEL))
        .map(|b| b.id.clone())
        .expect("amp block present");

    (Rc::new(RefCell::new(Some(session))), amp)
}

struct Harness {
    app: AppWindow,
    session: Rc<RefCell<Option<ProjectSession>>>,
    amp: BlockId,
    open_compact_window: Rc<RefCell<Option<(usize, slint::Weak<CompactChainViewWindow>)>>>,
    project_chains: Rc<VecModel<crate::ProjectChainItem>>,
}

impl Harness {
    fn new() -> Self {
        i_slint_backend_testing::init_no_event_loop();
        infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());

        let app = AppWindow::new().unwrap();
        let (session, amp) = session_with_an_amp();
        let open_compact_window = Rc::new(RefCell::new(None));
        let project_chains = Rc::new(VecModel::default());

        compact_chain_callbacks::wire(
            &app,
            CompactChainCallbacksCtx {
                project_session: session.clone(),
                block_stream_reads: Rc::new(NoLiveSource),
                looper_live: Rc::new(NoLiveSource),
                drums_live: Rc::new(NoLiveSource),
                audio_taps: Rc::new(application::audio_taps::NoAudioTaps),
                project_chains: project_chains.clone(),
                input_chain_devices: Rc::new(RefCell::new(Vec::new())),
                output_chain_devices: Rc::new(RefCell::new(Vec::new())),
                saved_project_snapshot: Rc::new(RefCell::new(None)),
                project_dirty: Rc::new(RefCell::new(false)),
                toast_timer: Rc::new(Timer::default()),
                open_compact_window: open_compact_window.clone(),
                block_editor_draft: Rc::new(RefCell::new(None)),
                fullscreen: false,
            },
        );

        Self {
            app,
            session,
            amp,
            open_compact_window,
            project_chains,
        }
    }

    fn compact_window(&self) -> CompactChainViewWindow {
        self.open_compact_window
            .borrow()
            .as_ref()
            .and_then(|(_, weak)| weak.upgrade())
            .expect("opening the compact chain view must create its window")
    }

    /// The detached editor window for the amp, built the way the compact
    /// view's block click builds it.
    fn open_amp_editor(&self) -> BlockEditorWindow {
        let (block_index, editor_data) = {
            let borrow = self.session.borrow();
            let proj = borrow.as_ref().unwrap().project.borrow();
            let blocks = &proj.chains[0].blocks;
            let index = blocks.iter().position(|b| b.id == self.amp).unwrap();
            (
                index,
                block_editor_data(&blocks[index]).expect("editor data"),
            )
        };
        let (win, _stream_timer) = block_editor_window_setup::create_and_wire(
            self.app.as_weak(),
            BlockEditorWindowSetupCtx {
                chain_index: 0,
                block_index: Some(block_index),
                before_index: block_index,
                instrument: "electric_guitar".into(),
                effect_type: editor_data.effect_type.clone(),
                model_id: editor_data.model_id.clone(),
                enabled: editor_data.enabled,
                editor_data,
                block_id: Some(self.amp.clone()),
                path: None,
                project_session: self.session.clone(),
                project_chains: self.project_chains.clone(),
                block_stream_reads: Rc::new(NoLiveSource),
                saved_project_snapshot: Rc::new(RefCell::new(None)),
                project_dirty: Rc::new(RefCell::new(false)),
                input_chain_devices: Rc::new(RefCell::new(Vec::new())),
                output_chain_devices: Rc::new(RefCell::new(Vec::new())),
                selected_block: Rc::new(RefCell::new(None)),
                open_block_windows: Rc::new(RefCell::new(Vec::new())),
                plugin_info_window: Rc::new(RefCell::new(None)),
                open_compact_window: self.open_compact_window.clone(),
            },
        )
        .expect("editor window");
        win
    }

    fn amp_gain(&self) -> f32 {
        let borrow = self.session.borrow();
        let proj = borrow.as_ref().unwrap().project.borrow();
        let block = proj.chains[0]
            .blocks
            .iter()
            .find(|b| b.id == self.amp)
            .unwrap();
        match &block.kind {
            AudioBlockKind::Core(cb) => cb.params.get_f32("gain").expect("gain"),
            _ => unreachable!("the amp is a core block"),
        }
    }

    fn amp_row(&self) -> CompactBlockItem {
        let rows = self.compact_window().get_compact_blocks();
        (0..rows.row_count())
            .filter_map(|i| rows.row_data(i))
            .find(|it| it.block_id.as_str() == self.amp.0)
            .expect("amp row in the compact list")
    }
}

/// Every numeric value the compact row carries for `path`.
fn shown_numbers(row: &CompactBlockItem, path: &str) -> Vec<f32> {
    let mut out = Vec::new();
    for it in row.parameter_items.iter().filter(|it| it.path == path) {
        out.push(it.numeric_value);
    }
    for line in row.parameter_lines.iter() {
        for it in line.cells.iter().filter(|it| it.path == path) {
            out.push(it.numeric_value);
        }
    }
    for k in row.knob_overlays.iter().filter(|k| k.path == path) {
        out.push(k.value);
    }
    for line in row.overlay_lines.iter() {
        for k in line.knobs.iter().filter(|k| k.path == path) {
            out.push(k.value);
        }
    }
    out
}

#[test]
fn a_knob_turned_in_the_block_window_shows_in_the_open_compact_view() {
    let h = Harness::new();
    h.app.invoke_open_compact_chain_view(0);
    assert!(
        !shown_numbers(&h.amp_row(), "gain").is_empty(),
        "sanity: the compact row shows the amp's gain"
    );

    let before = h.amp_gain();
    let wanted = if (before - 7.0).abs() > 0.5 { 7.0 } else { 3.0 };
    let editor = h.open_amp_editor();
    crate::BlockEditorBridge::get(&editor)
        .invoke_update_block_parameter_number("gain".into(), wanted);
    // The block window debounces its persist by 30 ms.
    i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(50));

    let now = h.amp_gain();
    assert!(
        (now - wanted).abs() < 1e-3,
        "sanity: the block window's edit must reach the project ({before} → {now})"
    );
    let shown = shown_numbers(&h.amp_row(), "gain");
    assert!(
        shown.iter().all(|v| (v - now).abs() < 1e-3),
        "REGRESSION #1023: the project's gain is {now} after the block window \
         edit, but the open compact view still shows {shown:?}"
    );
}
