//! #328 — a compact row inside a split's path acts on ITS block. The row
//! index is not a position in `chain.blocks` once the paths are listed, so a
//! handler reading `chain.blocks[row]` hit the wrong block or none at all.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Global, Timer, VecModel};

use application::live_source::NoLiveSource;
use project::block::AudioBlockKind;

use crate::chain_graph_fixtures_tests::{mix_chain, session_with};
use crate::compact_chain_callbacks::{self, CompactChainCallbacksCtx};
use crate::state::ProjectSession;
use crate::{AppWindow, CompactChainViewWindow};

type Compact = Rc<RefCell<Option<(usize, slint::Weak<CompactChainViewWindow>)>>>;

fn open_compact(
    session: &Rc<RefCell<Option<ProjectSession>>>,
) -> (AppWindow, CompactChainViewWindow) {
    i_slint_backend_testing::init_no_event_loop();
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    let app = AppWindow::new().unwrap();
    let open: Compact = Rc::new(RefCell::new(None));
    compact_chain_callbacks::wire(
        &app,
        CompactChainCallbacksCtx {
            project_session: session.clone(),
            block_stream_reads: Rc::new(NoLiveSource),
            audio_taps: Rc::new(application::audio_taps::NoAudioTaps),
            project_chains: Rc::new(VecModel::default()),
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(Vec::new())),
            saved_project_snapshot: Rc::new(RefCell::new(None)),
            project_dirty: Rc::new(RefCell::new(false)),
            toast_timer: Rc::new(Timer::default()),
            open_compact_window: open.clone(),
            block_editor_draft: Rc::new(RefCell::new(None)),
            fullscreen: false,
        },
    );
    app.invoke_open_compact_chain_view(0);
    let compact = open
        .borrow()
        .as_ref()
        .and_then(|(_, weak)| weak.upgrade())
        .expect("the compact view opens");
    (app, compact)
}

/// Ids of path A and path B of the split `sp`, and of the chain itself.
fn layout(
    session: &Rc<RefCell<Option<ProjectSession>>>,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let borrow = session.borrow();
    let project = borrow.as_ref().unwrap().project.borrow();
    let chain = &project.chains[0];
    let ids = |blocks: &[project::block::AudioBlock]| {
        blocks.iter().map(|b| b.id.0.clone()).collect::<Vec<_>>()
    };
    let split = chain
        .blocks
        .iter()
        .find_map(|b| match &b.kind {
            AudioBlockKind::Split(split) => Some(split),
            _ => None,
        })
        .expect("the chain keeps its split");
    (
        ids(&chain.blocks),
        ids(&split.paths[0]),
        ids(&split.paths[1]),
    )
}

fn enabled(session: &Rc<RefCell<Option<ProjectSession>>>, id: &str) -> bool {
    let borrow = session.borrow();
    let project = borrow.as_ref().unwrap().project.borrow();
    project
        .find_block(&domain::ids::BlockId(id.into()))
        .expect("the block exists")
        .enabled
}

// Rows of `mix_chain()`: 0 pre, 1 sp, 2 a1, 3 a2, 4 b1, 5 post.

#[test]
fn the_footswitch_of_a_path_row_bypasses_that_block() {
    let session = session_with(vec![mix_chain()]);
    let (_app, compact) = open_compact(&session);
    compact.invoke_toggle_block_enabled(0, 4);
    assert!(!enabled(&session, "b1"), "row 4 is b1, on path B");
    assert!(
        enabled(&session, "post"),
        "the chain's third block is untouched"
    );
}

#[test]
fn the_row_after_the_paths_bypasses_its_own_block() {
    let session = session_with(vec![mix_chain()]);
    let (_app, compact) = open_compact(&session);
    compact.invoke_toggle_block_enabled(0, 5);
    assert!(!enabled(&session, "post"), "row 5 is post");
}

#[test]
fn the_trash_of_a_path_row_removes_that_block() {
    let session = session_with(vec![mix_chain()]);
    let (_app, compact) = open_compact(&session);
    compact.invoke_remove_block(0, 3);
    crate::OverlayBridge::get(&compact).invoke_confirm_delete_block();
    let (chain, a, b) = layout(&session);
    assert_eq!(a, vec!["a1"], "row 3 is a2");
    assert_eq!(b, vec!["b1"]);
    assert_eq!(chain, vec!["pre", "sp", "post"]);
}

#[test]
fn dragging_a_path_row_reorders_that_path() {
    let session = session_with(vec![mix_chain()]);
    let (_app, compact) = open_compact(&session);
    // a2 (row 3) dropped on the slot above a1 (row 2).
    compact.invoke_reorder_block(0, 3, 2);
    let (chain, a, b) = layout(&session);
    assert_eq!(a, vec!["a2", "a1"]);
    assert_eq!(b, vec!["b1"]);
    assert_eq!(chain, vec!["pre", "sp", "post"]);
}

#[test]
fn dragging_the_last_row_up_moves_it_on_the_chain() {
    let session = session_with(vec![mix_chain()]);
    let (_app, compact) = open_compact(&session);
    // post (row 5) dropped on the slot above pre (row 0).
    compact.invoke_reorder_block(0, 5, 0);
    let (chain, a, _) = layout(&session);
    assert_eq!(chain, vec!["post", "pre", "sp"]);
    assert_eq!(a, vec!["a1", "a2"]);
}

/// Opens the compact view with the insert flow wired, as the app does.
fn open_compact_with_insert(
    session: &Rc<RefCell<Option<ProjectSession>>>,
) -> (
    AppWindow,
    CompactChainViewWindow,
    Rc<RefCell<Option<crate::state::BlockEditorDraft>>>,
) {
    let (app, compact) = open_compact(session);
    let draft = Rc::new(RefCell::new(None));
    crate::block_insert_callbacks::wire(
        &app,
        crate::block_insert_callbacks::BlockInsertCallbacksCtx {
            inline_tab_state: Rc::new(RefCell::new(Default::default())),
            selected_block: Rc::new(RefCell::new(None)),
            block_editor_draft: draft.clone(),
            block_type_options: Rc::new(VecModel::default()),
            block_model_options: Rc::new(VecModel::default()),
            filtered_block_model_options: Rc::new(VecModel::default()),
            block_model_option_labels: Rc::new(VecModel::default()),
            block_parameter_items: Rc::new(VecModel::default()),
            multi_slider_points: Rc::new(VecModel::default()),
            curve_editor_points: Rc::new(VecModel::default()),
            eq_band_curves: Rc::new(VecModel::default()),
            project_session: session.clone(),
            project_chains: Rc::new(VecModel::default()),
            saved_project_snapshot: Rc::new(RefCell::new(None)),
            project_dirty: Rc::new(RefCell::new(false)),
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(Vec::new())),
            block_editor_persist_timer: Rc::new(Timer::default()),
        },
    );
    (app, compact, draft)
}

#[test]
fn the_plus_under_a_path_row_adds_to_that_path() {
    let session = session_with(vec![mix_chain()]);
    let (_app, compact, draft) = open_compact_with_insert(&session);
    // The slot under a1 (row 2) is above row 3.
    compact.invoke_choose_block_type(0, 3, 0);
    let draft = draft.borrow();
    let draft = draft.as_ref().expect("the insert flow seeds a draft");
    assert_eq!(
        draft.path,
        Some(project::block::PathRef {
            split: domain::ids::BlockId("sp".into()),
            path: 0,
        }),
        "the new block joins path A"
    );
    assert_eq!(draft.before_index, 1, "right after a1");
}

#[test]
fn the_plus_under_the_last_row_adds_to_the_chain() {
    let session = session_with(vec![mix_chain()]);
    let (_app, compact, draft) = open_compact_with_insert(&session);
    compact.invoke_choose_block_type(0, 6, 0);
    let draft = draft.borrow();
    let draft = draft.as_ref().expect("the insert flow seeds a draft");
    assert_eq!(draft.path, None);
    assert_eq!(draft.before_index, 3, "after post, the chain's third block");
}

/// What the main window was asked to open.
#[derive(Debug, PartialEq)]
enum Opened {
    ChainRow(i32),
    PathBlock(String, i32, i32),
    SplitEditor(String, i32),
}

fn open_detail(row: i32) -> Option<Opened> {
    let session = session_with(vec![mix_chain()]);
    let (app, compact) = open_compact(&session);
    let seen = Rc::new(RefCell::new(None));
    let s = seen.clone();
    app.on_select_chain_block(move |_, row| *s.borrow_mut() = Some(Opened::ChainRow(row)));
    let s = seen.clone();
    crate::ChainGraphBridge::get(&app).on_open_path_block(move |_, split, path, index| {
        *s.borrow_mut() = Some(Opened::PathBlock(split.to_string(), path, index))
    });
    let s = seen.clone();
    crate::ChainGraphOverlayState::get(&app).on_open_split_editor(move |_, split, kind| {
        *s.borrow_mut() = Some(Opened::SplitEditor(split.to_string(), kind))
    });
    compact.invoke_open_block_detail(0, row);
    seen.take()
}

#[test]
fn opening_a_path_a_row_opens_that_path_block() {
    assert_eq!(open_detail(3), Some(Opened::PathBlock("sp".into(), 0, 1)));
}

#[test]
fn opening_a_path_b_row_opens_that_path_block() {
    assert_eq!(open_detail(4), Some(Opened::PathBlock("sp".into(), 1, 0)));
}

#[test]
fn opening_the_row_after_the_paths_opens_its_chain_block() {
    assert_eq!(open_detail(5), Some(Opened::ChainRow(2)));
}

#[test]
fn opening_the_split_row_opens_the_split_editor() {
    assert_eq!(
        open_detail(1),
        Some(Opened::SplitEditor("sp".into(), 0)),
        "the split editor, as a click on the graph's split node"
    );
}

#[test]
fn mix_then_y_opening_the_y_row_opens_the_editor_on_the_y() {
    let session = session_with(vec![crate::chain_graph_fixtures_tests::mix_then_y_chain()]);
    let (app, compact) = open_compact(&session);
    crate::split_editor_wiring::wire(
        &app,
        crate::split_editor_wiring::SplitEditorWiringCtx {
            project_session: session.clone(),
            project_chains: Rc::new(VecModel::default()),
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(Vec::new())),
            toast_timer: Rc::new(Timer::default()),
        },
    );

    // Rows: pre, mx, ma, mb, mid, y, ya, yb.
    compact.invoke_open_block_detail(0, 5);

    let state = crate::ChainGraphOverlayState::get(&app);
    assert!(state.get_split_editor_open());
    assert_eq!(state.get_split_editor_split_id().as_str(), "y");
}

fn split_param(
    session: &Rc<RefCell<Option<ProjectSession>>>,
    path: &str,
) -> Option<domain::value_objects::ParameterValue> {
    let borrow = session.borrow();
    let project = borrow.as_ref().unwrap().project.borrow();
    match &project.chains[0].blocks[1].kind {
        AudioBlockKind::Split(split) => split.params.get(path).cloned(),
        _ => None,
    }
}

#[test]
fn turning_a_knob_on_the_split_row_sets_the_split() {
    let session = session_with(vec![mix_chain()]);
    let (_app, compact) = open_compact(&session);
    compact.invoke_update_block_parameter_number(0, 1, "level_to_0".into(), 40.0);
    assert_eq!(
        split_param(&session, "level_to_0"),
        Some(domain::value_objects::ParameterValue::Float(40.0))
    );
}

#[test]
fn picking_the_split_mode_on_the_split_row_sets_the_split() {
    let session = session_with(vec![mix_chain()]);
    let (_app, compact) = open_compact(&session);
    compact.invoke_select_block_parameter_option(0, 1, "split_mode".into(), 1);
    assert_eq!(
        split_param(&session, "split_mode"),
        Some(domain::value_objects::ParameterValue::String(
            "dual_mono".into()
        ))
    );
}
