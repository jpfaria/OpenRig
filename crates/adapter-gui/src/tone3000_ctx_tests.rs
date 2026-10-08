use std::cell::RefCell;
use std::rc::Rc;

use application::command::Tone3000Command;
use application::tone3000::Tone3000Architecture;
use project::project::Project;
use slint::{ComponentHandle, Global, Model};

use super::Tone3000Ctx;
use crate::state::ProjectSession;
use crate::tone3000_session::SessionCell;
use crate::{Tone3000Bridge, Tone3000Window};

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
        std::env::temp_dir().join("openrig-tone3000-ctx-tests"),
    ))))
}

fn ctx(session: SessionCell) -> (Tone3000Window, Tone3000Ctx, Rc<RefCell<usize>>) {
    i_slint_backend_testing::init_no_event_loop();
    let w = Tone3000Window::new().unwrap();
    let forwarded = Rc::new(RefCell::new(0));
    let f = forwarded.clone();
    let ctx = Tone3000Ctx::new(
        session,
        w.as_weak(),
        Rc::new(move |events| *f.borrow_mut() += events.len()),
    );
    (w, ctx, forwarded)
}

fn set_key(ctx: &Tone3000Ctx, key: &str) {
    ctx.dispatch(Tone3000Command::SetTone3000ApiKey { key: key.into() });
}

fn search() -> Tone3000Command {
    Tone3000Command::SearchTone3000 {
        query: "plexi".into(),
        page: 1,
        format: None,
        gear: None,
        sort: None,
    }
}

#[test]
fn with_no_project_nothing_is_drawn() {
    let (w, ctx, _) = ctx(Rc::new(RefCell::new(None)));
    Tone3000Bridge::get(&w).set_key_configured(true);
    ctx.render();
    ctx.tick();
    assert!(Tone3000Bridge::get(&w).get_key_configured());
}

#[test]
fn the_key_status_reaches_the_window() {
    let (w, ctx, _) = ctx(open_session());
    set_key(&ctx, "t3k_cs_test");
    assert!(Tone3000Bridge::get(&w).get_key_configured());
    set_key(&ctx, "");
    assert!(!Tone3000Bridge::get(&w).get_key_configured());
}

#[test]
fn a_refused_search_shows_why_until_a_command_is_accepted() {
    let (w, ctx, _) = ctx(open_session());
    set_key(&ctx, "");
    ctx.dispatch(search());
    let error = Tone3000Bridge::get(&w).get_error();
    assert!(error.contains("Secret Key"), "{error}");
    set_key(&ctx, "");
    assert_eq!(Tone3000Bridge::get(&w).get_error(), "");
}

#[test]
fn a_pick_is_remembered_per_tone() {
    let (_w, ctx, _) = ctx(open_session());
    ctx.pick(12, Tone3000Architecture::A1);
    assert_eq!(ctx.arch_of(12), Some(Tone3000Architecture::A1));
    assert_eq!(ctx.arch_of(13), None);
}

#[test]
fn a_state_changed_elsewhere_is_redrawn_on_the_next_tick() {
    let session = open_session();
    let (w, ctx, forwarded) = ctx(session.clone());
    set_key(&ctx, "");
    session
        .borrow()
        .as_ref()
        .unwrap()
        .dispatcher
        .dispatch(application::command::Command::Tone3000(
            Tone3000Command::SetTone3000ApiKey {
                key: "t3k_cs_test".into(),
            },
        ))
        .unwrap();
    assert!(!Tone3000Bridge::get(&w).get_key_configured());
    ctx.tick();
    assert!(Tone3000Bridge::get(&w).get_key_configured());
    // Nothing was in flight, so nothing was drained.
    assert_eq!(*forwarded.borrow(), 0);
    assert_eq!(Tone3000Bridge::get(&w).get_results().row_count(), 0);
}

#[test]
fn the_installed_filter_is_kept_until_changed() {
    let (_w, ctx, _) = ctx(open_session());
    assert_eq!(ctx.installed_query(), "");
    ctx.filter_installed("dumble");
    assert_eq!(ctx.installed_query(), "dumble");
}
