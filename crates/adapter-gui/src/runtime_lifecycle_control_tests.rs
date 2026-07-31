//! #127 (Task 8): the GUI end of `RuntimeControl` — the handle a command
//! handler uses to reach THIS frontend's audio runtime.
//!
//! `application`'s tests prove the dispatcher calls the trait; these prove the
//! GUI's implementation of it is safe to hold: it sees the live project, it
//! does not keep the session alive, and it never wakes audio up on its own.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{BlockCommand, ChainCommand, Command};
use domain::ids::{BlockId, ChainId};
use infra_cpal::ProjectRuntimeController;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use super::{attach_runtime_control, GuiRuntimeControl, SessionHandle};
use crate::state::ProjectSession;

/// A stopped rig with one DISABLED chain: nothing here may open a device, so
/// the whole file runs headless.
fn stopped_session() -> ProjectSession {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("chain-127".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![AudioBlock {
                id: BlockId("blk-127".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "amp".into(),
                    model: "test_model".into(),
                    params: ParameterSet::default(),
                }),
            }],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    };
    ProjectSession::new(
        project,
        None,
        None,
        std::env::temp_dir().join("openrig-127-control-tests"),
    )
}

fn stopped_runtime() -> Rc<RefCell<Option<ProjectRuntimeController>>> {
    Rc::new(RefCell::new(None))
}

/// The control must read the LIVE project, not a copy taken at attach time —
/// a snapshot would make the sync helpers rebuild from stale blocks.
#[test]
fn the_mirrored_session_shares_the_live_project() {
    let session = stopped_session();
    let handle = SessionHandle::mirror(&session);

    session.project.borrow_mut().name = Some("renamed after attach".into());

    let mirrored = handle.session().expect("the session is still alive");
    assert!(
        Rc::ptr_eq(&mirrored.project, &session.project),
        "the control must hold the same project allocation, not a copy"
    );
    assert_eq!(
        mirrored.project.borrow().name.as_deref(),
        Some("renamed after attach"),
        "a mutation made after the attach must be visible to the control"
    );
}

/// The dispatcher OWNS the control, so the control may only hold the
/// dispatcher weakly. With an `Rc` this is a reference cycle: every project
/// switch would leak a whole session (project data, DI loop PCM).
#[test]
fn attaching_the_control_does_not_keep_the_session_alive() {
    let project_runtime = stopped_runtime();
    let session = stopped_session();
    let dispatcher = Rc::downgrade(&session.dispatcher);

    attach_runtime_control(&project_runtime, &session);
    assert!(
        dispatcher.upgrade().is_some(),
        "precondition: the session still holds its dispatcher"
    );

    drop(session);

    assert!(
        dispatcher.upgrade().is_none(),
        "the runtime control must not keep the dispatcher (and its session) \
         alive — that cycle leaks the project on every switch"
    );
}

/// Closing a project drops the session while the control may still be attached
/// to a dispatcher another handle keeps alive. Both entry points must then be
/// quiet no-ops, never a panic and never an error the user sees.
#[test]
fn a_control_whose_session_is_gone_is_a_silent_no_op() {
    let project_runtime = stopped_runtime();
    let control = {
        let session = stopped_session();
        GuiRuntimeControl {
            runtime: Rc::clone(&project_runtime),
            session: SessionHandle::mirror(&session),
        }
    };
    use application::runtime_control::RuntimeControl;

    control
        .set_block_enabled(
            &ChainId("chain-127".into()),
            &BlockId("blk-127".into()),
            false,
        )
        .expect("a closed project has no runtime to toggle — not an error");
    control
        .sync_chain(&ChainId("chain-127".into()))
        .expect("a closed project has no runtime to sync — not an error");
}

/// AUDIO SAFETY: a sync request for a DISABLED chain must never bring audio up.
/// `sync_chain` is reached from the command bus now, so an implementation that
/// reached for `ensure_runtime` would open devices behind the user's back —
/// on a stopped rig, and from any transport.
#[test]
fn syncing_a_disabled_chain_never_starts_the_audio_runtime() {
    let project_runtime = stopped_runtime();
    let session = stopped_session();
    attach_runtime_control(&project_runtime, &session);

    session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::SyncChainRuntime {
            chain: ChainId("chain-127".into()),
        }))
        .expect("SyncChainRuntime must succeed on a stopped rig");

    assert!(
        project_runtime.borrow().is_none(),
        "a sync request must not create a controller for a disabled chain"
    );
}

/// The same for the block toggle, which now applies its runtime effect from
/// the dispatcher: on a stopped rig it flips the project and stays quiet.
#[test]
fn toggling_a_block_on_a_stopped_rig_flips_the_project_and_starts_nothing() {
    let project_runtime = stopped_runtime();
    let session = stopped_session();
    attach_runtime_control(&project_runtime, &session);

    session
        .dispatcher
        .dispatch(Command::Block(BlockCommand::ToggleBlockEnabled {
            chain: ChainId("chain-127".into()),
            block: BlockId("blk-127".into()),
        }))
        .expect("ToggleBlockEnabled must succeed on a stopped rig");

    assert!(
        !session.project.borrow().chains[0].blocks[0].enabled,
        "the block must be disabled in the project"
    );
    assert!(
        project_runtime.borrow().is_none(),
        "a block toggle must not create a controller for a disabled chain"
    );
}

// ── Task 14: the teardown doors, driven from the bus ────────────────────────

/// A controller with no chains and no devices, reporting `rate` Hz — enough to
/// prove a teardown ran: the cell holds something the door has to take.
fn headless_runtime(rate: u32) -> Rc<RefCell<Option<ProjectRuntimeController>>> {
    Rc::new(RefCell::new(Some(
        ProjectRuntimeController::for_testing_with_sample_rate(
            engine::runtime::RuntimeGraph {
                chains: std::collections::HashMap::new(),
            },
            rate,
        ),
    )))
}

/// "Stop the rig" is a `Command` now, so a remote client can silence audio it
/// started. Before this the teardown was reachable only from the GUI's
/// back-to-launcher callback: an MCP client could enable a chain or play a DI
/// and had no way to stop it again.
#[test]
fn stopping_the_rig_from_the_bus_drops_this_frontends_controller() {
    let project_runtime = headless_runtime(44_100);
    let session = stopped_session();
    session.dispatcher.attach_engine_sr(44_100);
    attach_runtime_control(&project_runtime, &session);

    session
        .dispatcher
        .dispatch(Command::Project(
            application::command::ProjectCommand::StopProjectRuntime,
        ))
        .expect("stopping the rig is never an error");

    assert!(
        project_runtime.borrow().is_none(),
        "the stop must drop this frontend's controller — with it alive the \
         streams stay open and the rig keeps sounding"
    );
    assert_eq!(
        session.dispatcher.engine_sr(),
        application::local_dispatcher::REFERENCE_SAMPLE_RATE,
        "nothing is running, so the engine rate must be the reference again — \
         not the rate of the device that just went away"
    );
}

/// Deleting a chain is its own teardown, applied from the `RemoveChain`
/// handler. Over MCP the chain used to leave the project and keep sounding,
/// because the drop lived in the delete overlay's callback.
#[test]
fn deleting_a_chain_from_the_bus_tears_its_runtime_down() {
    let project_runtime = headless_runtime(44_100);
    let session = stopped_session();
    session.dispatcher.attach_engine_sr(44_100);
    attach_runtime_control(&project_runtime, &session);

    session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::RemoveChain {
            chain: ChainId("chain-127".into()),
        }))
        .expect("removing an existing chain must succeed");

    assert!(
        project_runtime.borrow().is_none(),
        "the deleted chain was the only one and nothing else is sounding, so \
         the controller must go with it — leaving it alive is what kept a dead \
         device's rate alive for every reader (#127/Task 11)"
    );
    assert_eq!(
        session.dispatcher.engine_sr(),
        application::local_dispatcher::REFERENCE_SAMPLE_RATE,
        "the delete must re-sync the engine rate like every other teardown"
    );
}

/// AUDIO SAFETY, and the mirror of `syncing_a_disabled_chain_never_starts_the
/// _audio_runtime`: the whole-graph rebuild follows what is RUNNING. On a
/// stopped rig there is nothing to rebuild, and opening the machine's devices
/// because a client saved a buffer size would be audio starting behind the
/// user's back.
///
/// Driven on the door itself rather than by dispatching `SaveAudioSettings`:
/// that handler persists `config.yaml`, and a test must never write the user's
/// real one (#701).
#[test]
fn the_whole_graph_rebuild_never_starts_the_audio_runtime() {
    use application::runtime_control::RuntimeControl;

    let project_runtime = stopped_runtime();
    let session = stopped_session();
    let control = GuiRuntimeControl {
        runtime: Rc::clone(&project_runtime),
        session: SessionHandle::mirror(&session),
    };

    control
        .sync_project()
        .expect("rebuilding a stopped rig is not an error");

    assert!(
        project_runtime.borrow().is_none(),
        "a device-settings rebuild must not create a controller — it follows \
         the running graph, it does not start one"
    );
}

// ── #808: arming the DI is the ONE door that may start the audio runtime ────

/// Write a tiny mono WAV and load it as `chain`'s DI source, waiting for the
/// off-thread decode (#693) to land. Returns the tempdir, which must outlive
/// the load.
fn load_di_source(session: &ProjectSession, chain: &ChainId) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("di.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(&wav, spec).expect("WavWriter::create");
    for _ in 0..64 {
        writer.write_sample(0.25f32).expect("write_sample");
    }
    writer.finalize().expect("finalize");

    session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::SetChainDiLoopSource {
            chain: chain.clone(),
            source: application::di_loader::DiLoopSource::File(wav),
        }))
        .expect("SetChainDiLoopSource must succeed");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while session.dispatcher.di_loop_for_chain(chain).is_none()
        && std::time::Instant::now() < deadline
    {
        let _ = session.dispatcher.poll_async_results();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        session.dispatcher.di_loop_for_chain(chain).is_some(),
        "precondition: the DI source must be decoded before the play"
    );
    dir
}

/// #808 through the bus: the DI is an independent pipeline, so pressing Play
/// with NO chain enabled must bring the runtime up. It used to be the UI
/// callback that ran `ensure_runtime` first, which is why the same play over
/// MCP found no controller and was a silent no-op.
#[test]
fn playing_the_di_starts_the_audio_runtime_with_no_chain_enabled() {
    let project_runtime = stopped_runtime();
    let session = stopped_session();
    let chain = ChainId("chain-127".into());
    let _dir = load_di_source(&session, &chain);
    attach_runtime_control(&project_runtime, &session);

    assert!(
        project_runtime.borrow().is_none(),
        "precondition: no chain is enabled, so no controller exists yet"
    );

    // The arm itself may fail on a machine with no reachable output endpoint —
    // what #808 is about is that the runtime EXISTS to be armed.
    let _ = session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::SetChainDiLoopEnabled {
            chain: chain.clone(),
            enabled: true,
        }));

    assert!(
        project_runtime.borrow().is_some(),
        "#808: arming the DI must create the runtime controller even though no \
         chain is enabled — otherwise the play is a silent no-op until some \
         chain toggle happens to create one"
    );
}

/// The other side of the same rule: STOPPING the DI, and picking its output,
/// are not requests to hear anything. Neither may open a device on a stopped
/// rig — that would be audio starting behind the user's back, from any
/// transport.
#[test]
fn stopping_the_di_or_picking_its_output_starts_nothing() {
    let project_runtime = stopped_runtime();
    let session = stopped_session();
    let chain = ChainId("chain-127".into());
    let _dir = load_di_source(&session, &chain);
    attach_runtime_control(&project_runtime, &session);

    session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::SetChainDiLoopEnabled {
            chain: chain.clone(),
            enabled: false,
        }))
        .expect("stopping the DI on a stopped rig is not an error");
    session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::SetChainDiLoopOutput {
            chain,
            output: project::chain::DiOutputRef {
                binding_id: "io-1".into(),
                endpoint: "Main Out".into(),
            },
        }))
        .expect("picking the DI output on a stopped rig is not an error");

    assert!(
        project_runtime.borrow().is_none(),
        "neither a stop nor an output pick may start the audio runtime"
    );
}
