use std::cell::RefCell;
use std::rc::Rc;

use application::event::{Event, PluginLibraryEvent};
use project::project::Project;
use slint::{ComponentHandle, Global, Model};

use super::PluginEditorCtx;
use crate::plugin_editor_draft::EditorMode;
use crate::state::ProjectSession;
use crate::tone3000_session::SessionCell;
use crate::{PluginEditorBridge, PluginEditorWindow};

struct Harness {
    window: PluginEditorWindow,
    ctx: PluginEditorCtx,
    shown: Rc<RefCell<usize>>,
    hidden: Rc<RefCell<usize>>,
}

fn open_session() -> SessionCell {
    Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-plugin-editor-ctx-tests"),
    ))))
}

fn harness() -> Harness {
    i_slint_backend_testing::init_no_event_loop();
    let window = PluginEditorWindow::new().unwrap();
    let shown = Rc::new(RefCell::new(0));
    let hidden = Rc::new(RefCell::new(0));
    let (s, h) = (shown.clone(), hidden.clone());
    let ctx = PluginEditorCtx::new(
        open_session(),
        window.as_weak(),
        Rc::new(|_| {}),
        Rc::new(move || *s.borrow_mut() += 1),
        Rc::new(move || *h.borrow_mut() += 1),
    );
    Harness {
        window,
        ctx,
        shown,
        hidden,
    }
}

fn bridge(h: &Harness) -> PluginEditorBridge<'_> {
    PluginEditorBridge::get(&h.window)
}

#[test]
fn create_opens_an_empty_draft() {
    let h = harness();
    h.ctx.open_create();
    assert_eq!(h.ctx.draft().unwrap().mode, EditorMode::Create);
    assert_eq!(bridge(&h).get_mode(), 1);
    assert_eq!(*h.shown.borrow(), 1);
}

#[test]
fn a_typed_value_does_not_redraw_but_a_new_column_does() {
    let h = harness();
    h.ctx.open_create();
    h.ctx
        .edit(true, |d| d.add_files(vec!["a/clean.nam".into()]));
    h.ctx.edit(false, |d| d.set_cell(0, 0, "crunch"));
    let drawn = bridge(&h).get_rows().row_data(0).unwrap();
    assert_eq!(drawn.cells.row_data(0).unwrap(), "clean");
    assert_eq!(h.ctx.draft().unwrap().grid.rows[0].cells[0], "crunch");
    h.ctx.edit(true, |d| d.add_column());
    assert_eq!(bridge(&h).get_columns().row_count(), 2);
}

#[test]
fn a_refused_create_stays_open_with_the_reason() {
    let h = harness();
    h.ctx.open_create();
    h.ctx.save();
    assert!(h.ctx.draft().is_some());
    assert!(!h.ctx.is_busy());
    assert!(bridge(&h).get_error().contains("name"));
    assert_eq!(*h.hidden.borrow(), 0);
}

#[test]
fn cancel_closes_the_window() {
    let h = harness();
    h.ctx.open_create();
    h.ctx.cancel();
    assert!(h.ctx.draft().is_none());
    assert_eq!(*h.hidden.borrow(), 1);
}

#[test]
fn a_plugin_outside_the_catalog_does_not_open() {
    let h = harness();
    assert!(h.ctx.open_edit("no_such_plugin").is_err());
    assert!(h.ctx.draft().is_none());
    assert_eq!(*h.shown.borrow(), 0);
}

#[test]
fn a_finished_create_closes_the_window() {
    let h = harness();
    h.ctx.open_create();
    h.ctx.busy.set(true);
    h.ctx
        .on_events(&[Event::PluginLibrary(PluginLibraryEvent::Created {
            plugin_id: "my_amp".into(),
        })]);
    assert!(h.ctx.draft().is_none());
    assert_eq!(*h.hidden.borrow(), 1);
}

#[test]
fn a_failed_create_shows_its_message() {
    let h = harness();
    h.ctx.open_create();
    h.ctx.busy.set(true);
    h.ctx
        .on_events(&[Event::PluginLibrary(PluginLibraryEvent::CreateFailed {
            message: "disk full".into(),
        })]);
    assert!(!h.ctx.is_busy());
    assert_eq!(bridge(&h).get_error(), "disk full");
    assert!(h.ctx.draft().is_some());
}
