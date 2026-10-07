use std::cell::RefCell;
use std::rc::Rc;

use application::event::{Event, PluginLibraryEvent};
use project::project::Project;
use slint::{ComponentHandle, Global};

use super::PluginsCtx;
use crate::plugins_view::{OriginFilter, PluginsFilter};
use crate::state::ProjectSession;
use crate::tone3000_session::SessionCell;
use crate::{PluginsBridge, PluginsWindow};

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
        std::env::temp_dir().join("openrig-plugins-ctx-tests"),
    ))))
}

fn ctx(session: SessionCell) -> (PluginsWindow, PluginsCtx, Rc<RefCell<usize>>) {
    i_slint_backend_testing::init_no_event_loop();
    let w = PluginsWindow::new().unwrap();
    let forwarded = Rc::new(RefCell::new(0));
    let f = forwarded.clone();
    let ctx = PluginsCtx::new(
        session,
        w.as_weak(),
        Rc::new(move |events| *f.borrow_mut() += events.len()),
    );
    (w, ctx, forwarded)
}

#[test]
fn with_no_project_nothing_is_drawn() {
    let (w, ctx, _) = ctx(Rc::new(RefCell::new(None)));
    PluginsBridge::get(&w).set_total(9);
    ctx.render();
    ctx.redo("plexi");
    assert_eq!(PluginsBridge::get(&w).get_total(), 9);
}

#[test]
fn an_open_project_draws_the_type_choices() {
    let (w, ctx, _) = ctx(open_session());
    ctx.render();
    // "Every type" is always the first choice.
    assert!(slint::Model::row_count(&PluginsBridge::get(&w).get_types()) >= 1);
}

#[test]
fn a_refused_redo_shows_why_and_frees_the_row() {
    let (w, ctx, forwarded) = ctx(open_session());
    ctx.redo("no_such_plugin");
    assert!(!ctx.is_busy("no_such_plugin"));
    assert_ne!(PluginsBridge::get(&w).get_error(), "");
    assert_eq!(*forwarded.borrow(), 0);
}

#[test]
fn a_finished_redo_frees_the_row() {
    let (_w, ctx, _) = ctx(open_session());
    ctx.busy.borrow_mut().insert("plexi".into());
    ctx.on_events(&[Event::PluginLibrary(PluginLibraryEvent::Redone {
        plugin_id: "plexi".into(),
        version: 3,
    })]);
    assert!(!ctx.is_busy("plexi"));
}

#[test]
fn a_failed_redo_shows_its_message() {
    let (w, ctx, _) = ctx(open_session());
    ctx.busy.borrow_mut().insert("plexi".into());
    ctx.on_events(&[Event::PluginLibrary(PluginLibraryEvent::RedoFailed {
        plugin_id: "plexi".into(),
        message: "TONE3000 is offline".into(),
    })]);
    assert!(!ctx.is_busy("plexi"));
    assert_eq!(PluginsBridge::get(&w).get_error(), "TONE3000 is offline");
}

#[test]
fn the_filter_is_kept_until_changed() {
    let (_w, ctx, _) = ctx(open_session());
    let filter = PluginsFilter {
        origin: OriginFilter::Tone3000,
        effect_type: "amp".into(),
        query: "plexi".into(),
    };
    ctx.set_filter(filter.clone());
    assert_eq!(ctx.filter(), filter);
}
