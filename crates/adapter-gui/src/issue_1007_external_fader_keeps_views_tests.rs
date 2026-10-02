//! Responsibility: proves a fader moved from outside the GUI leaves the chain cards and compact rows in place
//!
//! #1007 — moving a fader on the SMC-Mixer (Mackie bridge → MCP
//! `set_mixer_fader` / `set_chain_mixer_fader`, or a MIDI CC bound to the
//! chain volume) makes the chain list and the open compact view jump up and
//! down while the fader travels. Every fader step is one command; its events
//! reach the shared external-event drain `apply_events_to_ui`, which used to
//! re-project the whole chain list (a model reset) and hand the compact view a
//! brand-new block model — tearing down and rebuilding every card on screen,
//! once per step.
//!
//! This drives the road an MCP command travels (`CommandBridge::submit`, then
//! `BridgeDrain::drain` on the GUI thread, then `apply_events_to_ui`) and
//! asserts on what the user sees: the chain card and the compact block rows
//! that were on screen before the fader moved are the same ones after it.

use std::cell::RefCell;
use std::rc::Rc;

use i_slint_backend_testing::ElementHandle;
use slint::{ComponentHandle, Model, ModelRc, Timer, VecModel};

use application::chain_factory::{build_default_chain, DefaultChainParams, EndpointSpec};
use application::command::{BlockCommand, ChainCommand, Command, MixerCommand};
use application::event::Event;
use application::live_source::NoLiveSource;
use domain::ids::ChainId;

use crate::chain_rig_nav_wiring::{apply_events_to_ui, ChainRigNavCtx};
use crate::compact_chain_callbacks::{self, CompactChainCallbacksCtx};
use crate::project_ops::create_new_project_session;
use crate::project_view::replace_project_chains;
use crate::runtime_analyzers::AnalyzerSessions;
use crate::runtime_lifecycle::RuntimeAttach;
use crate::state::ProjectSession;
use crate::{AppWindow, CompactChainViewWindow, ProjectChainsHarness};

/// The output strip the chain plays through (`dev`, channels 0 and 1).
const OUT_STRIP: &str = "out:0,1@dev";

/// A one-chain session with an amp, so the compact view has a block row.
fn session_with_a_chain() -> (Rc<RefCell<Option<ProjectSession>>>, ChainId) {
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
            model_id: "blackface_clean".into(),
            position: 1,
            path: None,
        }))
        .expect("AddBlock");
    (Rc::new(RefCell::new(Some(session))), chain_id)
}

struct Harness {
    app: AppWindow,
    /// The chain list, bound to the very model the drain refreshes.
    list: ProjectChainsHarness,
    session: Rc<RefCell<Option<ProjectSession>>>,
    chain: ChainId,
    nav_ctx: ChainRigNavCtx,
}

impl Harness {
    fn new() -> Self {
        i_slint_backend_testing::init_no_event_loop();
        infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());

        let app = AppWindow::new().unwrap();
        let (session, chain) = session_with_a_chain();

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
                looper_live: Rc::new(NoLiveSource),
                drums_live: Rc::new(NoLiveSource),
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

        let taps: Rc<dyn application::audio_taps::AudioTaps> =
            Rc::new(application::audio_taps::NoAudioTaps);
        let analyzers = AnalyzerSessions::new(&session, &taps);
        let runtime_attach = RuntimeAttach::new(&Rc::new(RefCell::new(None)), &analyzers);
        let nav_ctx = ChainRigNavCtx {
            project_session: session.clone(),
            project_chains: project_chains.clone(),
            runtime_attach,
            input_chain_devices,
            output_chain_devices,
            toast_timer: Rc::new(Timer::default()),
            saved_project_snapshot,
            project_dirty,
            open_compact_window,
        };

        // The chain list as the user sees it: seeded the way a project open
        // seeds it, then drawn from the shared model.
        {
            let borrow = session.borrow();
            let s = borrow.as_ref().unwrap();
            replace_project_chains(&project_chains, &s.project.borrow(), &[], &[], &[]);
        }
        let list = ProjectChainsHarness::new().unwrap();
        list.set_chains(ModelRc::from(project_chains));
        list.show().unwrap();

        // The compact view of that chain, open.
        app.invoke_open_compact_chain_view(0);

        Self {
            app,
            list,
            session,
            chain,
            nav_ctx,
        }
    }

    fn compact_window(&self) -> CompactChainViewWindow {
        self.nav_ctx
            .open_compact_window
            .borrow()
            .as_ref()
            .and_then(|(_, weak)| weak.upgrade())
            .expect("opening the compact chain view must create its window")
    }

    fn chain_cards(&self) -> Vec<ElementHandle> {
        ElementHandle::find_by_element_type_name(&self.list, "ChainRow").collect()
    }

    fn compact_rows(&self) -> Vec<ElementHandle> {
        ElementHandle::find_by_element_type_name(&self.compact_window(), "CompactBlockRow")
            .collect()
    }

    /// What an MCP tool call does: submit on the transport side, then the GUI
    /// thread's poll tick drains the bridge into the shared drain.
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

    /// Moves the fader through `steps` the way a surface does — one command
    /// per step — and asserts the cards on screen before the move are the
    /// very ones on screen after every step.
    fn assert_fader_keeps_views(&self, what: &str, steps: impl Iterator<Item = Command>) {
        let cards = self.chain_cards();
        let rows = self.compact_rows();
        assert!(!cards.is_empty(), "sanity: the chain list shows the chain");
        assert!(
            !rows.is_empty(),
            "sanity: the compact view shows the blocks"
        );

        for (step, cmd) in steps.enumerate() {
            self.send_from_mcp(cmd);
            // Walking the trees again makes Slint apply any pending model swap.
            let _ = self.chain_cards();
            let _ = self.compact_rows();
            assert!(
                cards.iter().all(ElementHandle::is_valid),
                "REGRESSION #1007: {what} step {step} tore down and rebuilt the \
                 chain card — the chain list jumps while the fader moves"
            );
            assert!(
                rows.iter().all(ElementHandle::is_valid),
                "REGRESSION #1007: {what} step {step} tore down and rebuilt the \
                 compact block rows — the compact view jumps while the fader moves"
            );
        }
    }
}

fn fader_travel() -> impl Iterator<Item = f32> {
    [-12.0_f32, -9.5, -7.0, -4.5, -2.0].into_iter()
}

#[test]
fn a_global_fader_moved_over_mcp_keeps_the_cards_in_place() {
    let h = Harness::new();
    h.assert_fader_keeps_views(
        "set_mixer_fader",
        fader_travel().map(|gain_db| {
            Command::Mixer(MixerCommand::SetMixerFader {
                strip: OUT_STRIP.into(),
                gain_db,
            })
        }),
    );
}

#[test]
fn a_chain_di_fader_moved_over_mcp_keeps_the_cards_in_place() {
    let h = Harness::new();
    let chain = h.chain.clone();
    h.assert_fader_keeps_views(
        "set_chain_di_fader",
        fader_travel().map(move |gain_db| {
            Command::Mixer(MixerCommand::SetChainDiFader {
                chain: chain.clone(),
                gain_db,
            })
        }),
    );
}

#[test]
fn a_chain_mixer_fader_event_keeps_the_cards_in_place() {
    // `set_chain_mixer_fader` needs an I/O binding the chain plays through;
    // the drain only ever sees the events it produces, so feed those.
    let h = Harness::new();
    let cards = h.chain_cards();
    let rows = h.compact_rows();
    assert!(!cards.is_empty() && !rows.is_empty(), "sanity: views drawn");

    for gain_db in fader_travel() {
        apply_events_to_ui(
            &h.app,
            &h.nav_ctx,
            &[
                Event::ChainMixerStripChanged {
                    chain: h.chain.clone(),
                    strip: OUT_STRIP.into(),
                    gain_db,
                    muted: false,
                },
                Event::ProjectMutated,
            ],
        );
        let _ = h.chain_cards();
        let _ = h.compact_rows();
        assert!(
            cards.iter().all(ElementHandle::is_valid),
            "REGRESSION #1007: a chain mixer fader step rebuilt the chain card"
        );
        assert!(
            rows.iter().all(ElementHandle::is_valid),
            "REGRESSION #1007: a chain mixer fader step rebuilt the compact rows"
        );
    }
}

#[test]
fn a_chain_volume_moved_over_mcp_keeps_the_cards_and_shows_the_new_volume() {
    let h = Harness::new();
    let chain = h.chain.clone();
    h.assert_fader_keeps_views(
        "set_chain_volume",
        [60.0_f32, 70.0, 80.0, 90.0].into_iter().map(move |value| {
            Command::Chain(ChainCommand::SetChainVolume {
                chain: chain.clone(),
                value,
            })
        }),
    );
    assert_eq!(
        h.nav_ctx.project_chains.row_data(0).map(|row| row.volume),
        Some(90),
        "the chain card's volume must follow a volume moved from outside the GUI"
    );
}

#[test]
fn a_non_fader_edit_in_the_same_batch_still_redraws_the_views() {
    // Guard on the other side: a fader step drained in the same tick as a
    // structural edit must not swallow that edit's redraw.
    let h = Harness::new();
    let card_blocks = || {
        h.nav_ctx
            .project_chains
            .row_data(0)
            .map(|row| row.blocks.row_count())
            .unwrap_or_default()
    };
    let compact_blocks = || h.compact_window().get_compact_blocks().row_count();
    let (card_before, compact_before) = (card_blocks(), compact_blocks());

    let events = {
        let borrow = h.session.borrow();
        let s = borrow.as_ref().unwrap();
        let mut events = s
            .dispatcher
            .dispatch(Command::Mixer(MixerCommand::SetMixerFader {
                strip: OUT_STRIP.into(),
                gain_db: -3.0,
            }))
            .expect("fader");
        events.extend(
            s.dispatcher
                .dispatch(Command::Block(BlockCommand::AddBlock {
                    chain: h.chain.clone(),
                    kind: "gain".into(),
                    model_id: "volume".into(),
                    position: 1,
                    path: None,
                }))
                .expect("add block"),
        );
        events
    };
    apply_events_to_ui(&h.app, &h.nav_ctx, &events);

    assert!(
        card_blocks() > card_before,
        "the block added next to a fader step must show on the chain card"
    );
    assert!(
        compact_blocks() > compact_before,
        "the block added next to a fader step must show in the compact view"
    );
}
