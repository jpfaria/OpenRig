//! #827, red-first: the editor tells the user which of the save outcomes
//! happened — a silent Save button is indistinguishable from a broken one.

use super::*;
use application::looper_take_library::TakeSaveError;

fn refused(err: TakeSaveError) -> anyhow::Result<Vec<Event>> {
    Err(anyhow::Error::new(err))
}

#[test]
fn a_saved_take_reports_saved() {
    let ok: anyhow::Result<Vec<Event>> = Ok(vec![]);
    assert_eq!(take_status_code(&ok), TAKE_SAVED);
}

#[test]
fn each_refusal_reports_its_own_reason() {
    assert_eq!(
        take_status_code(&refused(TakeSaveError::NameTaken("x.wav".into()))),
        TAKE_NAME_TAKEN
    );
    assert_eq!(
        take_status_code(&refused(TakeSaveError::NothingRecorded)),
        TAKE_NOTHING_RECORDED
    );
    assert_eq!(
        take_status_code(&refused(TakeSaveError::Io("disk full".into()))),
        TAKE_FAILED
    );
    assert_eq!(
        take_status_code(&refused(TakeSaveError::EmptyName)),
        TAKE_NEEDS_NAME
    );
}

#[test]
fn an_untyped_failure_is_a_failed_save_not_a_success() {
    assert_eq!(
        take_status_code(&Err(anyhow::anyhow!("chain not found"))),
        TAKE_FAILED
    );
}

#[test]
fn the_codes_are_the_ones_the_editor_reads() {
    // looper_panel_globals.slint: 1 saved, 2 name taken, 3 nothing recorded,
    // 4 could not write, 5 the name has nothing usable.
    assert_eq!(
        (
            TAKE_SAVED,
            TAKE_NAME_TAKEN,
            TAKE_NOTHING_RECORDED,
            TAKE_FAILED,
            TAKE_NEEDS_NAME
        ),
        (1, 2, 3, 4, 5)
    );
}

// ── the wiring, driven on a REAL `AppWindow` ─────────────────────────────

mod wiring {
    use std::cell::RefCell;
    use std::rc::Rc;

    use application::command::{Command, LooperCommand};
    use application::dispatcher::CommandDispatcher;
    use application::event::Event;
    use application::looper_take_library::TakeSaveError;
    use domain::ids::ChainId;
    use project::chain::{Chain, LooperConfig};
    use project::project::Project;
    use slint::ComponentHandle;

    use super::super::wire_looper_take_callbacks;
    use crate::state::ProjectSession;
    use crate::{AppWindow, LooperEditor};

    /// Answers every save with `refusal` (or success) and remembers it.
    #[derive(Default)]
    struct SpyDispatcher {
        seen: RefCell<Vec<Command>>,
        refusal: RefCell<Option<TakeSaveError>>,
        selection: std::sync::Arc<std::sync::RwLock<application::SelectionState>>,
    }

    impl CommandDispatcher for SpyDispatcher {
        fn dispatch(&self, cmd: Command) -> anyhow::Result<Vec<Event>> {
            self.seen.borrow_mut().push(cmd);
            match self.refusal.borrow().clone() {
                Some(err) => Err(anyhow::Error::new(err)),
                None => Ok(vec![]),
            }
        }

        fn selection_state(
            &self,
        ) -> std::sync::Arc<std::sync::RwLock<application::SelectionState>> {
            std::sync::Arc::clone(&self.selection)
        }
    }

    fn wired(refusal: Option<TakeSaveError>) -> (AppWindow, Rc<SpyDispatcher>) {
        i_slint_backend_testing::init_no_event_loop();
        let window = AppWindow::new().expect("window");
        let spy = Rc::new(SpyDispatcher::default());
        *spy.refusal.borrow_mut() = refusal;
        let session = ProjectSession::with_dispatcher(
            Project {
                name: None,
                device_settings: vec![],
                chains: vec![Chain {
                    id: ChainId("rig:in".into()),
                    description: None,
                    instrument: "electric_guitar".into(),
                    enabled: false,
                    volume: 100.0,
                    io_binding_ids: vec![],
                    blocks: vec![],
                    di_output: None,
                    loopers: vec![LooperConfig::new(1)],
                }],
                midi: None,
            },
            Rc::clone(&spy) as Rc<dyn CommandDispatcher>,
            None,
            None,
            std::path::PathBuf::from("./presets"),
        );
        wire_looper_take_callbacks(&window, &Rc::new(RefCell::new(Some(session))));
        (window, spy)
    }

    #[test]
    fn save_dispatches_the_take_for_that_loop_and_clears_the_name() {
        let (window, spy) = wired(None);
        let editor = window.global::<LooperEditor>();
        editor.set_take_name("verse".into());

        editor.invoke_save_take(0, 1, "verse".into());

        let seen = spy.seen.borrow();
        assert!(matches!(
            seen.as_slice(),
            [Command::Looper(LooperCommand::SaveChainLooperTake { chain, looper: 1, name })]
                if chain.0 == "rig:in" && name == "verse"
        ));
        assert_eq!(editor.get_take_status(), super::TAKE_SAVED);
        assert_eq!(editor.get_take_name(), "", "a saved take clears the field");
    }

    #[test]
    fn a_refused_save_keeps_the_name_and_says_why() {
        let (window, _spy) = wired(Some(TakeSaveError::NameTaken("verse.wav".into())));
        let editor = window.global::<LooperEditor>();
        editor.set_take_name("verse".into());

        editor.invoke_save_take(0, 1, "verse".into());

        assert_eq!(editor.get_take_status(), super::TAKE_NAME_TAKEN);
        assert_eq!(
            editor.get_take_name(),
            "verse",
            "the name is kept so it can be changed, not retyped"
        );
    }

    #[test]
    fn a_row_that_no_longer_exists_dispatches_nothing() {
        let (window, spy) = wired(None);
        window
            .global::<LooperEditor>()
            .invoke_save_take(7, 1, "x".into());
        assert!(spy.seen.borrow().is_empty());
    }

    #[test]
    fn a_save_with_no_project_open_is_dropped() {
        i_slint_backend_testing::init_no_event_loop();
        let window = AppWindow::new().expect("window");
        wire_looper_take_callbacks(&window, &Rc::new(RefCell::new(None)));
        let editor = window.global::<LooperEditor>();

        editor.invoke_save_take(0, 1, "verse".into());

        assert_eq!(editor.get_take_status(), 0, "nothing was attempted");
    }
}
