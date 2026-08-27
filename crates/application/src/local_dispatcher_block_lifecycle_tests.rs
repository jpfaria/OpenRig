//! #127 (Task 8), red-first: `BlockCommand::ToggleBlockEnabled` must reach the
//! audio runtime from the DISPATCHER, not from the UI callback that happened to
//! dispatch it.
//!
//! Before this, `adapter-gui` dispatched the command and THEN called
//! `sync_block_toggle` on the audio controller itself. The same command issued
//! over MCP/gRPC flipped `block.enabled` in the project and left the pedal
//! sounding — the exact gap the "every state change is born a `Command`" law
//! exists to close.
//!
//! The toggle must stay a LIVE operation (#522): the assertions below pin that
//! the dispatcher asks for the in-place block toggle and nothing else — no
//! chain sync, no rebuild, no restart.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use crate::command::{BlockCommand, Command};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::runtime_control::RuntimeControl;

/// What the frontend's runtime was asked to do, in order.
#[derive(Debug, Clone, PartialEq)]
enum RuntimeCall {
    /// The #522 in-place toggle: chain, block, new state.
    BlockEnabled(String, String, bool),
    /// A whole-chain re-sync (rebuild / pause / drop).
    SyncChain(String),
}

/// Stand-in for the frontend that hosts the audio runtime: records the calls
/// instead of touching real streams.
struct SpyRuntimeControl {
    calls: Rc<RefCell<Vec<RuntimeCall>>>,
}

impl RuntimeControl for SpyRuntimeControl {
    fn set_block_enabled(
        &self,
        chain: &ChainId,
        block: &BlockId,
        enabled: bool,
    ) -> anyhow::Result<()> {
        self.calls.borrow_mut().push(RuntimeCall::BlockEnabled(
            chain.0.clone(),
            block.0.clone(),
            enabled,
        ));
        Ok(())
    }

    fn sync_chain(&self, chain: &ChainId) -> anyhow::Result<()> {
        self.calls
            .borrow_mut()
            .push(RuntimeCall::SyncChain(chain.0.clone()));
        Ok(())
    }
}

/// A runtime whose block toggle fails, standing in for "the fast path declined
/// AND the fallback rebuild errored".
struct FailingRuntimeControl;

impl RuntimeControl for FailingRuntimeControl {
    fn set_block_enabled(
        &self,
        _chain: &ChainId,
        _block: &BlockId,
        _enabled: bool,
    ) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("runtime not started"))
    }
}

fn block(id: &str, enabled: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "amp".to_string(),
            model: "test_model".to_string(),
            params: ParameterSet::default(),
        }),
    }
}

fn chain(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

/// Two chains on purpose: every assertion below can prove the OTHER stream's
/// runtime was left alone (isolation is a `CLAUDE.md` LAW, not a nicety).
fn two_chain_project() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![
            chain("chain_a", vec![block("a_drive", true)]),
            chain("chain_b", vec![block("b_drive", true)]),
        ],
        midi: None,
    }))
}

fn dispatcher_with_spy() -> (LocalDispatcher, Rc<RefCell<Vec<RuntimeCall>>>) {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let d = LocalDispatcher::new(two_chain_project());
    d.attach_runtime_control(Rc::new(SpyRuntimeControl {
        calls: Rc::clone(&calls),
    }));
    (d, calls)
}

fn toggle(chain: &str, block: &str) -> Command {
    Command::Block(BlockCommand::ToggleBlockEnabled {
        chain: ChainId(chain.to_string()),
        block: BlockId(block.to_string()),
    })
}

#[test]
fn toggling_a_block_off_applies_it_to_the_attached_runtime() {
    let (dispatcher, calls) = dispatcher_with_spy();

    let events = dispatcher
        .dispatch(toggle("chain_a", "a_drive"))
        .expect("ToggleBlockEnabled must succeed");

    assert!(
        matches!(
            &events[0],
            Event::BlockEnabledChanged { chain, block, enabled }
                if chain.0 == "chain_a" && block.0 == "a_drive" && !*enabled
        ),
        "expected BlockEnabledChanged(false), got {:?}",
        events[0]
    );
    assert_eq!(
        *calls.borrow(),
        vec![RuntimeCall::BlockEnabled(
            "chain_a".to_string(),
            "a_drive".to_string(),
            false
        )],
        "the runtime must be toggled by the dispatcher, not by the UI afterwards"
    );
}

#[test]
fn both_edges_reach_the_runtime_through_the_dispatcher() {
    let (dispatcher, calls) = dispatcher_with_spy();

    dispatcher
        .dispatch(toggle("chain_a", "a_drive"))
        .expect("disable must succeed");
    dispatcher
        .dispatch(toggle("chain_a", "a_drive"))
        .expect("re-enable must succeed");

    assert_eq!(
        *calls.borrow(),
        vec![
            RuntimeCall::BlockEnabled("chain_a".to_string(), "a_drive".to_string(), false),
            RuntimeCall::BlockEnabled("chain_a".to_string(), "a_drive".to_string(), true),
        ],
        "both edges must reach the runtime, in order"
    );
}

/// Invariant #4 / LAW: a block toggle is a LIVE, in-place operation on ONE
/// stream. It must never ask for a chain sync (which resolves devices and can
/// rebuild the stream), and must never name another stream's chain.
#[test]
fn a_block_toggle_never_rebuilds_and_never_names_another_stream() {
    let (dispatcher, calls) = dispatcher_with_spy();

    dispatcher
        .dispatch(toggle("chain_b", "b_drive"))
        .expect("ToggleBlockEnabled must succeed");

    let calls = calls.borrow();
    assert!(
        !calls.iter().any(|c| matches!(c, RuntimeCall::SyncChain(_))),
        "a live block toggle must not trigger a chain sync/rebuild, got {calls:?}"
    );
    assert!(
        calls
            .iter()
            .all(|c| matches!(c, RuntimeCall::BlockEnabled(chain, _, _) if chain == "chain_b")),
        "only the toggled block's own chain may be touched, got {calls:?}"
    );
}

#[test]
fn the_toggle_still_succeeds_when_no_runtime_is_attached() {
    let dispatcher = LocalDispatcher::new(two_chain_project());

    let events = dispatcher
        .dispatch(toggle("chain_a", "a_drive"))
        .expect("a transport with no audio runtime must still dispatch");

    assert!(
        matches!(
            &events[0],
            Event::BlockEnabledChanged { enabled: false, .. }
        ),
        "the event is reported with or without a runtime, got {:?}",
        events[0]
    );
}

/// A frontend that has NOT yet handed its runtime over (project opened, never
/// synced) must not have the operation silently evaporate: before #127 this
/// path cold-started the chain's runtime through `sync_live_chain_runtime`.
/// The dispatcher cannot cold-start anything itself, so it says so — and the
/// event it emits is the one the frontend's drain already turns into that same
/// sync sequence. Swallowing it would mean a toggle that used to produce audio
/// now does nothing.
#[test]
fn an_unapplied_toggle_asks_the_frontend_to_sync_the_chain() {
    let dispatcher = LocalDispatcher::new(two_chain_project());

    let events = dispatcher
        .dispatch(toggle("chain_a", "a_drive"))
        .expect("a transport with no audio runtime must still dispatch");

    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainRuntimeSyncNeeded { chain } if chain.0 == "chain_a"
        )),
        "with no runtime attached the toggle must REQUEST the sync (the \
         cold-start path), not swallow it — got {events:?}"
    );
}

/// The mirror image: when the runtime DID take the live toggle there is nothing
/// to ask for. Emitting the request anyway would make the frontend's drain
/// rebuild the chain right after the cheap in-place toggle (#740).
#[test]
fn an_applied_toggle_asks_for_nothing() {
    let (dispatcher, _calls) = dispatcher_with_spy();

    let events = dispatcher
        .dispatch(toggle("chain_a", "a_drive"))
        .expect("ToggleBlockEnabled must succeed");

    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::ChainRuntimeSyncNeeded { .. })),
        "the runtime already took the toggle — asking for a sync would rebuild \
         the chain for nothing: {events:?}"
    );
}

/// The GUI used to surface a failed `sync_block_toggle` as a status error. Now
/// that the dispatcher owns the call, the failure has to come back out of
/// `dispatch` — swallowing it would hide a dead runtime from every transport.
#[test]
fn a_runtime_failure_surfaces_as_a_dispatch_error() {
    let dispatcher = LocalDispatcher::new(two_chain_project());
    dispatcher.attach_runtime_control(Rc::new(FailingRuntimeControl));

    let error = dispatcher
        .dispatch(toggle("chain_a", "a_drive"))
        .expect_err("a runtime failure must not be swallowed");

    assert!(
        error.to_string().contains("runtime not started"),
        "expected the runtime's own error, got {error}"
    );
}

// ── #881: routing blocks are topology, not processors ────────────────────────

/// A mid-chain `Insert` — routing metadata, never a processor node.
fn insert_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Insert(project::block::InsertBlock {
            model: "external_loop".to_string(),
            io: "fx".to_string(),
        }),
    }
}

fn dispatcher_with_insert() -> (LocalDispatcher, Rc<RefCell<Vec<RuntimeCall>>>) {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![
            chain(
                "chain_a",
                vec![block("a_drive", true), insert_block("a_insert")],
            ),
            chain("chain_b", vec![block("b_drive", true)]),
        ],
        midi: None,
    }));
    let d = LocalDispatcher::new(project);
    d.attach_runtime_control(Rc::new(SpyRuntimeControl {
        calls: Rc::clone(&calls),
    }));
    (d, calls)
}

/// #881 — an `Insert` splits the chain into segments; it has no live node to
/// fade. The #522 in-place toggle finds nothing, posts "block '…' not found in
/// any input runtime of the chain" from the audio thread and changes nothing
/// audible. Only a rebuild re-splits the chain, so that is what the toggle of a
/// routing block must ask for — scoped to its own chain.
#[test]
fn toggling_an_insert_asks_for_its_chain_rebuild() {
    let (dispatcher, calls) = dispatcher_with_insert();

    dispatcher
        .dispatch(toggle("chain_a", "a_insert"))
        .expect("toggling an insert must succeed");

    assert_eq!(
        *calls.borrow(),
        vec![RuntimeCall::SyncChain("chain_a".to_string())],
        "an insert is a segment boundary: rebuild its chain, never the in-place \
         block toggle that cannot apply it"
    );
}
