use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, DrumsCommand};
use application::live_source::LiveSource;
use feature_dsp::drums::DrumPosition;
use project::project::Project;
use slint::{ComponentHandle, Global};

use super::DrumsCtx;
use crate::drums_session::SessionCell;
use crate::state::ProjectSession;
use crate::{DrumsBridge, DrumsWindow};

struct Live(Option<DrumPosition>);

impl LiveSource for Live {
    fn drums(&self) -> Option<DrumPosition> {
        self.0
    }
}

fn open() -> SessionCell {
    Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-drums-ctx-tests"),
    ))))
}

/// A context over a real `DrumsWindow`; the `AppWindow` side stays empty,
/// since no test builds one.
fn ctx(session: &SessionCell, live: Option<DrumPosition>) -> (DrumsWindow, DrumsCtx) {
    i_slint_backend_testing::init_no_event_loop();
    let w = DrumsWindow::new().unwrap();
    let c = DrumsCtx::new(
        session.clone(),
        Rc::new(Live(live)),
        w.as_weak(),
        slint::Weak::default(),
    );
    (w, c)
}

#[test]
fn a_command_redraws_the_panel_with_the_new_state() {
    let session = open();
    let (w, c) = ctx(&session, None);
    c.dispatch(DrumsCommand::SetDrumsBpm { bpm: 133.0 });
    assert_eq!(DrumsBridge::get(&w).get_bpm(), 133.0);
}

#[test]
fn with_no_project_the_panel_is_left_alone() {
    let session: SessionCell = Rc::new(RefCell::new(None));
    let (w, c) = ctx(&session, None);
    DrumsBridge::get(&w).set_bpm(77.0);
    c.dispatch(DrumsCommand::SetDrumsBpm { bpm: 133.0 });
    c.render();
    c.tick();
    assert_eq!(DrumsBridge::get(&w).get_bpm(), 77.0);
}

#[test]
fn the_poll_redraws_a_change_made_behind_the_panels_back() {
    let session = open();
    let (w, c) = ctx(&session, None);
    c.render();
    session
        .borrow()
        .as_ref()
        .unwrap()
        .dispatcher
        .dispatch(Command::Drums(DrumsCommand::SetDrumsBpm { bpm: 150.0 }))
        .unwrap();
    c.tick();
    assert_eq!(DrumsBridge::get(&w).get_bpm(), 150.0);
}

#[test]
fn the_poll_does_not_redraw_an_unchanged_state() {
    let session = open();
    let (w, c) = ctx(&session, None);
    c.render();
    DrumsBridge::get(&w).set_bpm(77.0);
    c.tick();
    assert_eq!(DrumsBridge::get(&w).get_bpm(), 77.0);
}

#[test]
fn the_poll_moves_the_lamps_with_the_groove() {
    let session = open();
    let position = DrumPosition {
        playing: true,
        bar: 1,
        beat: 2,
        in_fill: false,
    };
    let (w, c) = ctx(&session, Some(position));
    c.tick();
    let bridge = DrumsBridge::get(&w);
    assert_eq!(
        (bridge.get_current_bar(), bridge.get_current_beat()),
        (1, 2)
    );
}

#[test]
fn closing_drops_the_open_picker() {
    let session = open();
    let (w, c) = ctx(&session, None);
    DrumsBridge::get(&w).set_picker(2);
    c.close();
    assert_eq!(DrumsBridge::get(&w).get_picker(), 0);
}
