//! Responsibility: proves a parameter set from outside the GUI reaches the open compact view
//!
//! #999 — with the compact chain view open, an MCP client sets a block
//! parameter (`set_block_parameter_number` / `set_block_parameter_bool`). The
//! command lands in the project and the audio follows, but the compact window
//! keeps drawing the old knob until it is closed and reopened.
//!
//! This drives the exact road an MCP command travels — `CommandBridge::submit`
//! on the transport side, `BridgeDrain::drain` on the GUI thread, then the
//! shared external-event drain `apply_events_to_ui` (the MIDI footswitch timer
//! lands in the same drain) — and asserts on what the user sees: every value
//! the compact window's own row carries for that parameter.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Model, Timer, VecModel};

use application::chain_factory::{build_default_chain, DefaultChainParams, EndpointSpec};
use application::command::{BlockCommand, ChainCommand, Command};
use application::live_source::NoLiveSource;
use domain::ids::{BlockId, ChainId};
use project::block::AudioBlockKind;

use crate::chain_rig_nav_wiring::{apply_events_to_ui, ChainRigNavCtx};
use crate::compact_chain_callbacks::{self, CompactChainCallbacksCtx};
use crate::project_ops::create_new_project_session;
use crate::runtime_analyzers::AnalyzerSessions;
use crate::runtime_lifecycle::RuntimeAttach;
use crate::state::ProjectSession;
use crate::{AppWindow, CompactBlockItem, CompactChainViewWindow};

const AMP_MODEL: &str = "blackface_clean";

/// A one-chain session carrying a native amp added the way every frontend
/// adds one, so its parameters are seeded like the user's.
fn session_with_an_amp() -> (Rc<RefCell<Option<ProjectSession>>>, ChainId, BlockId) {
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
            kind: "amp".into(),
            model_id: AMP_MODEL.into(),
            position: 1,
            path: None,
        }))
        .expect("AddBlock");
    let amp = session
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id == chain_id)
        .and_then(|c| {
            c.blocks
                .iter()
                .find(|b| matches!(b.kind, AudioBlockKind::Core(ref cb) if cb.model == AMP_MODEL))
                .map(|b| b.id.clone())
        })
        .expect("amp block present");

    (Rc::new(RefCell::new(Some(session))), chain_id, amp)
}

struct Harness {
    app: AppWindow,
    session: Rc<RefCell<Option<ProjectSession>>>,
    chain: ChainId,
    amp: BlockId,
    nav_ctx: ChainRigNavCtx,
    open_compact_window: Rc<RefCell<Option<(usize, slint::Weak<CompactChainViewWindow>)>>>,
}

impl Harness {
    fn new() -> Self {
        i_slint_backend_testing::init_no_event_loop();
        infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());

        let app = AppWindow::new().unwrap();
        let (session, chain, amp) = session_with_an_amp();

        let project_chains = Rc::new(VecModel::default());
        let input_chain_devices = Rc::new(RefCell::new(Vec::new()));
        let output_chain_devices = Rc::new(RefCell::new(Vec::new()));
        let saved_project_snapshot = Rc::new(RefCell::new(None));
        let project_dirty = Rc::new(RefCell::new(false));
        let open_compact_window = Rc::new(RefCell::new(None));

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
                block_editor_draft: Rc::new(RefCell::new(None)),
                fullscreen: false,
            },
        );

        // The context the MCP poll timer hands the external-event drain.
        let taps: Rc<dyn application::audio_taps::AudioTaps> =
            Rc::new(application::audio_taps::NoAudioTaps);
        let analyzers = AnalyzerSessions::new(&session, &taps);
        let runtime_attach = RuntimeAttach::new(&Rc::new(RefCell::new(None)), &analyzers);
        let nav_ctx = ChainRigNavCtx {
            project_session: session.clone(),
            project_chains,
            runtime_attach,
            input_chain_devices,
            output_chain_devices,
            toast_timer: Rc::new(Timer::default()),
            saved_project_snapshot,
            project_dirty,
            open_compact_window: open_compact_window.clone(),
        };

        Self {
            app,
            session,
            chain,
            amp,
            nav_ctx,
            open_compact_window,
        }
    }

    fn compact_window(&self) -> CompactChainViewWindow {
        self.open_compact_window
            .borrow()
            .as_ref()
            .and_then(|(_, weak)| weak.upgrade())
            .expect("opening the compact chain view must create its window")
    }

    /// Exactly what an MCP tool call does: submit on the transport side, then
    /// the GUI thread's poll tick drains the bridge and feeds the events to
    /// the shared external-event drain.
    fn send_from_mcp(&self, cmd: Command) {
        let (bridge, drain) = application::bridge::channel();
        let _reply = bridge.submit(cmd);
        let events = {
            let borrow = self.session.borrow();
            let session = borrow.as_ref().unwrap();
            drain.drain(session.dispatcher.as_ref(), 32)
        };
        assert!(
            !events.is_empty(),
            "sanity: the command must produce events"
        );
        apply_events_to_ui(&self.app, &self.nav_ctx, &events);
    }

    fn amp_params(&self) -> project::param::ParameterSet {
        let borrow = self.session.borrow();
        let s = borrow.as_ref().unwrap();
        let proj = s.project.borrow();
        let block = proj.chains[0]
            .blocks
            .iter()
            .find(|b| b.id == self.amp)
            .expect("amp still in the chain");
        match &block.kind {
            AudioBlockKind::Core(cb) => cb.params.clone(),
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

/// Every numeric value the compact row carries for `path`: the full item list
/// a save reads, the strip cells it draws, and the curated knob overlays.
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

/// Every bool the compact row carries for `path`.
fn shown_bools(row: &CompactBlockItem, path: &str) -> Vec<bool> {
    let mut out = Vec::new();
    for it in row.parameter_items.iter().filter(|it| it.path == path) {
        out.push(it.bool_value);
    }
    for line in row.parameter_lines.iter() {
        for it in line.cells.iter().filter(|it| it.path == path) {
            out.push(it.bool_value);
        }
    }
    out
}

#[test]
fn a_number_set_over_mcp_shows_in_the_open_compact_view() {
    let h = Harness::new();
    h.app.invoke_open_compact_chain_view(0);

    let before = h.amp_params().get_f32("gain").expect("gain seeded");
    assert!(
        !shown_numbers(&h.amp_row(), "gain").is_empty(),
        "sanity: the compact row shows the amp's gain"
    );
    let wanted = if (before - 7.0).abs() > 0.5 { 7.0 } else { 3.0 };

    h.send_from_mcp(Command::Block(BlockCommand::SetBlockParameterNumber {
        chain: h.chain.clone(),
        block: h.amp.clone(),
        path: "gain".into(),
        value: wanted as f64,
    }));

    let now = h.amp_params().get_f32("gain").expect("gain still set");
    assert!(
        (now - before).abs() > 0.5,
        "sanity: the MCP write must reach the project ({before} → {now})"
    );
    let shown = shown_numbers(&h.amp_row(), "gain");
    assert!(
        shown.iter().all(|v| (v - now).abs() < 1e-3),
        "REGRESSION #999: the project's gain is {now} after the MCP write, but \
         the open compact view still shows {shown:?} — it only refreshes on \
         reopen"
    );
}

#[test]
fn a_bool_set_over_mcp_shows_in_the_open_compact_view() {
    let h = Harness::new();
    h.app.invoke_open_compact_chain_view(0);

    let before = h.amp_params().get_bool("bright").expect("bright seeded");
    assert!(
        !shown_bools(&h.amp_row(), "bright").is_empty(),
        "sanity: the compact row shows the amp's bright switch"
    );

    h.send_from_mcp(Command::Block(BlockCommand::SetBlockParameterBool {
        chain: h.chain.clone(),
        block: h.amp.clone(),
        path: "bright".into(),
        value: !before,
    }));

    assert_eq!(
        h.amp_params().get_bool("bright"),
        Some(!before),
        "sanity: the MCP write must reach the project"
    );
    let shown = shown_bools(&h.amp_row(), "bright");
    assert!(
        shown.iter().all(|v| *v == !before),
        "REGRESSION #999: the project's bright is {} after the MCP write, but \
         the open compact view still shows {shown:?}",
        !before
    );
}
