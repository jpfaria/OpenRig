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
