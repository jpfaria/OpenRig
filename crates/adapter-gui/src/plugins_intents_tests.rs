use std::cell::RefCell;
use std::rc::Rc;

use project::project::Project;
use slint::{ComponentHandle, Global};

use super::wire_plugins_intents;
use crate::plugins_ctx::PluginsCtx;
use crate::plugins_view::{OriginFilter, PluginsFilter};
use crate::state::ProjectSession;
use crate::{PluginsBridge, PluginsWindow};

type Calls = Rc<RefCell<Vec<String>>>;

fn wired() -> (PluginsWindow, PluginsCtx, Calls) {
    i_slint_backend_testing::init_no_event_loop();
    let w = PluginsWindow::new().unwrap();
    let session = Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-plugins-intents-tests"),
    ))));
    let ctx = PluginsCtx::new(session, w.as_weak(), Rc::new(|_| {}));
    let calls: Calls = Rc::new(RefCell::new(vec![]));
    let (info, edit) = (calls.clone(), calls.clone());
    wire_plugins_intents(
        &w,
        &ctx,
        Rc::new(move |id, effect_type| info.borrow_mut().push(format!("info {id} {effect_type}"))),
        Rc::new(move |id| {
            edit.borrow_mut()
                .push(format!("edit {}", id.unwrap_or("new")));
            match id {
                Some("locked") => Err("cannot edit locked".into()),
                _ => Ok(()),
            }
        }),
    );
    (w, ctx, calls)
}

#[test]
fn the_filters_reach_the_catalog() {
    let (w, ctx, _) = wired();
    let bridge = PluginsBridge::get(&w);
    bridge.set_origin(2);
    bridge.set_type_key("amp".into());
    bridge.set_query("plexi".into());
    bridge.invoke_filter();
    assert_eq!(
        ctx.filter(),
        PluginsFilter {
            origin: OriginFilter::Tone3000,
            effect_type: "amp".into(),
            query: "plexi".into(),
        }
    );
}

#[test]
fn info_edit_and_create_open_their_windows() {
    let (w, _ctx, calls) = wired();
    let bridge = PluginsBridge::get(&w);
    bridge.invoke_info("plexi".into(), "amp".into());
    bridge.invoke_edit("plexi".into());
    bridge.invoke_create();
    assert_eq!(
        *calls.borrow(),
        vec!["info plexi amp", "edit plexi", "edit new"]
    );
}

#[test]
fn a_plugin_the_editor_refuses_shows_why() {
    let (w, _ctx, _) = wired();
    PluginsBridge::get(&w).invoke_edit("locked".into());
    assert_eq!(PluginsBridge::get(&w).get_error(), "cannot edit locked");
}
