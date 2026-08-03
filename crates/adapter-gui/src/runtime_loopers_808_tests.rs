//! #808 through the bus, for the looper: which looper commands may wake audio.
//!
//! A loop is recorded and played on the controller's store, which exists only
//! while a controller does — and the looper, like the DI, must work with NO
//! chain enabled. The UI callback used to run `ensure_runtime` before EVERY
//! looper command, so the same command over MCP/MIDI found no controller and
//! did nothing at all. The rule that replaces it is narrower on purpose:
//!
//! * an ADD may wake audio — the panel arms REC only against a live store, so
//!   a looper added with nothing running is a looper the user cannot record
//!   into;
//! * a Record / Play / PlayStop may wake audio — they are requests to hear
//!   something;
//! * nothing else may. Stopping, clearing, undoing, turning a knob or picking
//!   an endpoint is never a reason to open the machine's audio devices, from
//!   any transport.
//!
//! Headless: one DISABLED chain with no I/O binding, so nothing here opens a
//! device.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, LooperAction, LooperCommand, LooperParam};
use domain::ids::ChainId;
use infra_cpal::ProjectRuntimeController;
use project::chain::{Chain, EndpointRef, LooperConfig, LooperSpeed};
use project::project::Project;

use crate::runtime_lifecycle::attach_runtime_control;
use crate::state::ProjectSession;

const UID: u64 = 1;

fn chain_id() -> ChainId {
    ChainId("chain-808-looper".into())
}

/// A stopped rig whose one (disabled) chain already carries a looper.
fn stopped_session(loopers: Vec<LooperConfig>) -> ProjectSession {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: chain_id(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
            loopers,
        }],
        midi: None,
    };
    ProjectSession::new(
        project,
        None,
        None,
        std::env::temp_dir().join("openrig-808-looper-tests"),
    )
}

fn stopped_runtime() -> Rc<RefCell<Option<ProjectRuntimeController>>> {
    Rc::new(RefCell::new(None))
}

#[test]
fn recording_starts_the_audio_runtime_with_no_chain_enabled() {
    let project_runtime = stopped_runtime();
    let session = stopped_session(vec![LooperConfig::new(UID)]);
    attach_runtime_control(
        &project_runtime,
        &crate::runtime_analyzers::AnalyzerSessions::detached(),
        &session,
    );

    assert!(
        project_runtime.borrow().is_none(),
        "precondition: no chain is enabled, so no controller exists yet"
    );

    session
        .dispatcher
        .dispatch(Command::Looper(LooperCommand::SetChainLooperTransport {
            chain: chain_id(),
            looper: UID,
            action: LooperAction::Record,
        }))
        .expect("recording on a stopped rig is not an error");

    assert!(
        project_runtime.borrow().is_some(),
        "#808: pressing RECORD must create the runtime controller even though \
         no chain is enabled — otherwise the loop records nothing until some \
         chain toggle happens to create one"
    );
}

#[test]
fn adding_a_looper_starts_the_audio_runtime() {
    let project_runtime = stopped_runtime();
    let session = stopped_session(vec![]);
    attach_runtime_control(
        &project_runtime,
        &crate::runtime_analyzers::AnalyzerSessions::detached(),
        &session,
    );

    session
        .dispatcher
        .dispatch(Command::Looper(LooperCommand::AddChainLooper {
            chain: chain_id(),
        }))
        .expect("adding a looper on a stopped rig is not an error");

    assert!(
        project_runtime.borrow().is_some(),
        "the panel arms REC only against a live store, so a looper added with \
         nothing running would be one the user cannot record into"
    );
}

/// The other side of the rule, and the one that keeps audio from starting
/// behind the user's back: everything that is not an add or a transport START
/// leaves a stopped rig stopped.
#[test]
fn no_other_looper_command_starts_the_audio_runtime() {
    let project_runtime = stopped_runtime();
    let session = stopped_session(vec![LooperConfig::new(UID)]);
    attach_runtime_control(
        &project_runtime,
        &crate::runtime_analyzers::AnalyzerSessions::detached(),
        &session,
    );

    for action in [
        LooperAction::Stop,
        LooperAction::Clear,
        LooperAction::Undo,
        LooperAction::Redo,
    ] {
        session
            .dispatcher
            .dispatch(Command::Looper(LooperCommand::SetChainLooperTransport {
                chain: chain_id(),
                looper: UID,
                action,
            }))
            .unwrap_or_else(|e| panic!("{action:?} on a stopped rig is not an error: {e}"));
    }
    for param in [
        LooperParam::Mix(0.5),
        LooperParam::Decay(0.5),
        LooperParam::Speed(LooperSpeed::Double),
        LooperParam::Reverse(true),
    ] {
        session
            .dispatcher
            .dispatch(Command::Looper(LooperCommand::SetChainLooperParam {
                chain: chain_id(),
                looper: UID,
                param,
            }))
            .expect("a knob edit on a stopped rig is not an error");
    }
    let endpoint = Some(EndpointRef {
        binding_id: "io-1".into(),
        endpoint: "Main Out".into(),
    });
    session
        .dispatcher
        .dispatch(Command::Looper(LooperCommand::SetChainLooperInput {
            chain: chain_id(),
            looper: UID,
            input: endpoint.clone(),
        }))
        .expect("picking the record input on a stopped rig is not an error");
    session
        .dispatcher
        .dispatch(Command::Looper(LooperCommand::SetChainLooperOutput {
            chain: chain_id(),
            looper: UID,
            output: endpoint,
        }))
        .expect("picking the play output on a stopped rig is not an error");
    session
        .dispatcher
        .dispatch(Command::Looper(LooperCommand::RemoveChainLooper {
            chain: chain_id(),
            looper: UID,
        }))
        .expect("removing a looper on a stopped rig is not an error");

    assert!(
        project_runtime.borrow().is_none(),
        "none of stop / clear / undo / redo / the knobs / the endpoint picks / \
         a delete may open the machine's audio devices — audio starting behind \
         the user's back, from any transport, is the failure mode"
    );
}
