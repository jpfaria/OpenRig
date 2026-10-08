use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use application::plugin_library::{CaptureBackend, ColumnKind};
use application::tone3000::Tone3000BlockType;
use project::project::Project;
use slint::{ComponentHandle, Global, Model};

use super::wire_plugin_editor_intents;
use crate::plugin_editor_ctx::PluginEditorCtx;
use crate::state::ProjectSession;
use crate::{PluginEditorBridge, PluginEditorWindow};

fn wired() -> (
    PluginEditorWindow,
    PluginEditorCtx,
    Rc<RefCell<Vec<CaptureBackend>>>,
) {
    i_slint_backend_testing::init_no_event_loop();
    let w = PluginEditorWindow::new().unwrap();
    let session = Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-plugin-editor-intents-tests"),
    ))));
    let ctx = PluginEditorCtx::new(
        session,
        w.as_weak(),
        Rc::new(|_| {}),
        Rc::new(|| {}),
        Rc::new(|| {}),
    );
    let asked = Rc::new(RefCell::new(vec![]));
    let a = asked.clone();
    wire_plugin_editor_intents(
        &w,
        &ctx,
        Rc::new(move |backend| {
            a.borrow_mut().push(backend);
            vec![
                PathBuf::from("caps/clean.nam"),
                PathBuf::from("caps/lead.nam"),
            ]
        }),
    );
    ctx.open_create();
    (w, ctx, asked)
}

#[test]
fn added_files_become_rows_of_the_picked_backend() {
    let (w, ctx, asked) = wired();
    let bridge = PluginEditorBridge::get(&w);
    bridge.invoke_set_backend(1);
    bridge.invoke_add_files();
    assert_eq!(*asked.borrow(), vec![CaptureBackend::Ir]);
    assert_eq!(ctx.draft().unwrap().grid.rows.len(), 2);
    assert_eq!(bridge.get_rows().row_count(), 2);
}

#[test]
fn grid_controls_change_the_draft() {
    let (w, ctx, _) = wired();
    let bridge = PluginEditorBridge::get(&w);
    bridge.invoke_add_files();
    bridge.invoke_set_cell(1, 0, "solo".into());
    bridge.invoke_add_column();
    bridge.invoke_rename_column(1, " gain ".into());
    bridge.invoke_set_kind(1, 0);
    bridge.invoke_remove_row(0);
    bridge.invoke_set_block_type(3);
    let draft = ctx.draft().unwrap();
    assert_eq!(draft.grid.rows.len(), 1);
    assert_eq!(draft.grid.rows[0].cells[0], "solo");
    assert_eq!(draft.grid.columns[1].name, "gain");
    assert_eq!(draft.grid.columns[1].kind, ColumnKind::Knob);
    assert_eq!(draft.create.block_type, Tone3000BlockType::Cab);
    bridge.invoke_remove_column(1);
    assert_eq!(ctx.draft().unwrap().grid.columns.len(), 1);
}

#[test]
fn a_redraw_keeps_the_typed_name_and_brand() {
    let (w, ctx, _) = wired();
    let bridge = PluginEditorBridge::get(&w);
    bridge.set_display_name("My Amp".into());
    bridge.set_brand("Me".into());
    bridge.invoke_add_files();
    assert_eq!(bridge.get_display_name(), "My Amp");
    let draft = ctx.draft().unwrap();
    assert_eq!(
        (
            draft.create.display_name.as_str(),
            draft.create.brand.as_str()
        ),
        ("My Amp", "Me")
    );
}

#[test]
fn cancel_drops_the_draft() {
    let (w, ctx, _) = wired();
    PluginEditorBridge::get(&w).invoke_cancel();
    assert!(ctx.draft().is_none());
}
