//! #826 — the editor's callbacks, driven on a REAL `AppWindow`.
//!
//! `issue_826_looper_editor_interaction` clicks the Slint harness and proves
//! the controls fire; it says nothing about the Rust side they fire INTO.
//! These drive `wire_looper_editor_callbacks` itself: what each callback asks
//! the bus for, and what it writes back into the editor's global.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, LooperAction, LooperCommand};
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use application::live_source::LiveSource;
use application::looper_edit::{LoopEdit, LoopEditReading};
use domain::ids::ChainId;
use engine::{LooperState, LooperStatus};
use project::chain::{Chain, LooperConfig};
use project::project::Project;
use slint::ComponentHandle;

use super::{wire_looper_editor_callbacks, EditorDirtyCtx};
use crate::state::ProjectSession;
use crate::{AppWindow, LoopEditKind as LoopEditKind_slint, LooperEditor};

const CHAIN: &str = "rig:in";

/// A bus that answers every command with success and remembers what it was
/// asked — the callbacks' whole job is WHICH command they dispatch. An edit
/// shrinks the loop the way the store would, so the outcome the editor reports
/// is read from a length that really moved.
#[derive(Default)]
struct SpyDispatcher {
    seen: RefCell<Vec<Command>>,
    selection: std::sync::Arc<std::sync::RwLock<application::SelectionState>>,
    applies: RefCell<Option<Rc<FakeLive>>>,
}

impl CommandDispatcher for SpyDispatcher {
    fn dispatch(&self, cmd: Command) -> anyhow::Result<Vec<Event>> {
        if matches!(
            cmd,
            Command::Looper(LooperCommand::EditChainLooperAudio { .. })
        ) {
            if let Some(live) = self.applies.borrow().as_ref() {
                let mut len = live.len_frames.borrow_mut();
                *len /= 2;
            }
        }
        self.seen.borrow_mut().push(cmd);
        Ok(vec![])
    }

    fn selection_state(&self) -> std::sync::Arc<std::sync::RwLock<application::SelectionState>> {
        std::sync::Arc::clone(&self.selection)
    }
}

/// A loop of `len_frames`, playing or not, with whatever history depth.
struct FakeLive {
    len_frames: RefCell<usize>,
    playing: bool,
    can_undo: bool,
    position: usize,
}

impl Default for FakeLive {
    fn default() -> Self {
        Self {
            len_frames: RefCell::new(48_000),
            playing: false,
            can_undo: true,
            position: 12_000,
        }
    }
}

impl LiveSource for FakeLive {
    fn chain_loop_edit(
        &self,
        _chain: &ChainId,
        _looper: u64,
        buckets: usize,
    ) -> Option<LoopEditReading> {
        let len = *self.len_frames.borrow();
        if len == 0 {
            return None;
        }
        Some(LoopEditReading {
            peaks: vec![0.5; buckets],
            len_frames: len,
            length_label: "0:01".into(),
            playing: self.playing,
            can_undo: self.can_undo,
            can_redo: false,
        })
    }

    fn chain_loopers(&self, _chain: &ChainId) -> Option<Result<(Vec<LooperStatus>, u32), String>> {
        Some(Ok((
            vec![LooperStatus {
                uid: 1,
                state: if self.playing {
                    LooperState::Playing
                } else {
                    LooperState::Stopped
                },
                position_frames: self.position,
                len_frames: *self.len_frames.borrow(),
                layers: 1,
                content_rev: 0,
            }],
            48_000,
        )))
    }
}

fn chain() -> Chain {
    Chain {
        id: ChainId(CHAIN.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![LooperConfig::new(1)],
    }
}

struct Wired {
    window: AppWindow,
    spy: Rc<SpyDispatcher>,
}

impl Wired {
    fn editor(&self) -> LooperEditor<'_> {
        self.window.global::<LooperEditor>()
    }

    fn commands(&self) -> Vec<Command> {
        self.spy.seen.borrow().clone()
    }
}

fn wire_with(live: FakeLive) -> Wired {
    wire_with_store(live, false)
}

/// `store_applies` ⇒ an edit really shortens the loop, so the callback's
/// re-read sees a different length (the Applied case).
fn wire_with_store(live: FakeLive, store_applies: bool) -> Wired {
    i_slint_backend_testing::init_no_event_loop();
    let window = AppWindow::new().expect("window");
    let spy = Rc::new(SpyDispatcher::default());
    let session = ProjectSession::with_dispatcher(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![chain()],
            midi: None,
        },
        Rc::clone(&spy) as Rc<dyn CommandDispatcher>,
        None,
        None,
        std::path::PathBuf::from("./presets"),
    );
    let live = Rc::new(live);
    if store_applies {
        *spy.applies.borrow_mut() = Some(Rc::clone(&live));
    }
    let dirty = EditorDirtyCtx {
        window: window.as_weak(),
        saved_project_snapshot: Rc::new(RefCell::new(None)),
        project_dirty: Rc::new(RefCell::new(false)),
        auto_save: false,
    };
    wire_looper_editor_callbacks(
        &window,
        &Rc::new(RefCell::new(Some(session))),
        &(Rc::clone(&live) as Rc<dyn LiveSource>),
        &dirty,
    );
    Wired { window, spy }
}

#[test]
fn opening_the_editor_loads_the_loop_it_was_asked_for() {
    let w = wire_with(FakeLive::default());
    w.window.invoke_looper_edit(0, 1);

    let editor = w.editor();
    assert!(editor.get_open(), "the overlay opens");
    assert_eq!(editor.get_uid(), 1);
    assert_eq!(editor.get_chain_index(), 0);
    assert_eq!(editor.get_length_label(), "0:01");
    assert!(
        editor.get_can_undo(),
        "the history depth comes from the read"
    );
    assert_eq!(
        (editor.get_sel_start(), editor.get_sel_end()),
        (0.0, 1.0),
        "a freshly opened editor selects the whole take"
    );
    assert!(w.commands().is_empty(), "opening dispatches nothing");
}

#[test]
fn an_empty_loop_never_opens_the_editor() {
    let w = wire_with(FakeLive {
        len_frames: RefCell::new(0),
        ..FakeLive::default()
    });
    w.window.invoke_looper_edit(0, 1);
    assert!(
        !w.editor().get_open(),
        "an editor over a loop with no material would draw a flat line and \
         refuse every button"
    );
}

#[test]
fn an_edit_travels_as_the_command_with_the_frames_the_ratios_mean() {
    let w = wire_with(FakeLive::default());
    w.window.invoke_looper_edit(0, 1);
    w.window
        .invoke_looper_edit_apply(0, 1, LoopEditKind_slint::Crop, 0.25, 0.75);

    match w.commands().last() {
        Some(Command::Looper(LooperCommand::EditChainLooperAudio {
            chain,
            looper,
            edit,
        })) => {
            assert_eq!(chain.0, CHAIN);
            assert_eq!(*looper, 1);
            // The ratios are resolved against the length the read reports —
            // 48 000 frames — not against anything the frontend remembered.
            assert_eq!(
                *edit,
                LoopEdit::Crop {
                    start: 12_000,
                    end: 36_000
                }
            );
        }
        other => panic!("the edit must travel the bus as a command; got {other:?}"),
    }
    assert_eq!(
        (w.editor().get_sel_start(), w.editor().get_sel_end()),
        (0.0, 1.0),
        "the selection resets: its ratios described a loop that no longer exists"
    );
}

#[test]
fn an_edit_that_changed_nothing_says_so() {
    let w = wire_with(FakeLive::default());
    w.window.invoke_looper_edit(0, 1);
    // The fake's length never moves, so the loop comes back the same size.
    w.window
        .invoke_looper_edit_apply(0, 1, LoopEditKind_slint::Fit, 0.0, 1.0);
    assert_ne!(
        w.editor().get_status_code(),
        0,
        "a silent button is indistinguishable from a broken one — an edit that \
         changed nothing has to report it"
    );
}

#[test]
fn an_edit_that_shortened_the_loop_reports_applied() {
    let w = wire_with_store(FakeLive::default(), true);
    w.window.invoke_looper_edit(0, 1);
    w.window
        .invoke_looper_edit_apply(0, 1, LoopEditKind_slint::Trim, 0.0, 0.5);
    assert_eq!(
        w.editor().get_status_code(),
        0,
        "an applied edit reports nothing to complain about — the waveform says it"
    );
}

#[test]
fn the_editors_transport_is_the_same_command_the_row_dispatches() {
    let w = wire_with(FakeLive::default());
    w.window.invoke_looper_edit(0, 1);
    w.window.invoke_looper_edit_play_stop(0, 1);

    assert!(
        matches!(
            w.commands().last(),
            Some(Command::Looper(LooperCommand::SetChainLooperTransport {
                action: LooperAction::PlayStop,
                ..
            }))
        ),
        "one button, one meaning: the editor must not invent its own transport"
    );
}

#[test]
fn undo_and_redo_step_the_edit_history_over_the_bus() {
    let w = wire_with(FakeLive::default());
    w.window.invoke_looper_edit(0, 1);

    w.window.invoke_looper_edit_undo(0, 1);
    assert!(matches!(
        w.commands().last(),
        Some(Command::Looper(LooperCommand::UndoChainLooperEdit { .. }))
    ));

    w.window.invoke_looper_edit_redo(0, 1);
    assert!(matches!(
        w.commands().last(),
        Some(Command::Looper(LooperCommand::RedoChainLooperEdit { .. }))
    ));
}

#[test]
fn a_chain_index_that_names_no_chain_is_a_no_op() {
    let w = wire_with(FakeLive::default());
    w.window.invoke_looper_edit(7, 1);
    w.window
        .invoke_looper_edit_apply(7, 1, LoopEditKind_slint::Cut, 0.1, 0.2);
    w.window.invoke_looper_edit_play_stop(7, 1);
    w.window.invoke_looper_edit_undo(7, 1);

    assert!(!w.editor().get_open());
    assert!(
        w.commands().is_empty(),
        "a stale index must never reshape whatever chain happens to be at it"
    );
}
