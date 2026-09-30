//! #328 — Review Focus 1: an index inside a split path is never read against
//! the top level. The editor's draft carries the path; every flow resolves the
//! block through it.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::{AudioBlock, AudioBlockKind, PathRef, PathSide};
use slint::{Global, Timer, VecModel};

use crate::block_delete::delete_drafted_block;
use crate::block_param_apply::{apply_block_parameter, ParamValue};
use crate::chain_graph_fixtures_tests::{chain_in, mix_chain, rows, session_with};
use crate::state::BlockEditorDraft;

fn on(side: PathSide) -> Option<PathRef> {
    Some(PathRef {
        split: BlockId("sp".into()),
        side,
    })
}

fn draft(
    block_index: Option<usize>,
    before_index: usize,
    path: Option<PathRef>,
) -> BlockEditorDraft {
    BlockEditorDraft {
        chain_index: 0,
        block_index,
        before_index,
        instrument: "electric_guitar".into(),
        effect_type: "gain".into(),
        model_id: "volume".into(),
        enabled: true,
        is_select: false,
        path,
    }
}

fn volume(block: &AudioBlock) -> Option<f32> {
    match &block.kind {
        AudioBlockKind::Core(core) => core.params.get("volume").and_then(ParameterValue::as_f32),
        _ => None,
    }
}

fn lane(
    session: &Rc<RefCell<Option<crate::state::ProjectSession>>>,
    side: PathSide,
) -> Vec<AudioBlock> {
    let chain = chain_in(session, 0);
    let AudioBlockKind::Split(split) = &chain.blocks[1].kind else {
        panic!("block 1 is the split")
    };
    match side {
        PathSide::A => split.a.clone(),
        PathSide::B => split.b.clone(),
    }
}

#[test]
fn a_knob_edit_on_a_path_block_reaches_that_block_not_the_top_level_one() {
    let session = session_with(vec![mix_chain()]);
    let d = Rc::new(RefCell::new(Some(draft(Some(0), 0, on(PathSide::A)))));
    apply_block_parameter(
        &session,
        &d,
        "volume",
        ParamValue::Number(42.0),
        &rows(),
        &[],
        &[],
    )
    .expect("the path block exists");
    assert_eq!(
        volume(&lane(&session, PathSide::A)[0]),
        Some(42.0),
        "a1 got the edit"
    );
    assert_eq!(
        volume(&chain_in(&session, 0).blocks[0]),
        None,
        "pre was not touched"
    );
}

#[test]
fn deleting_a_path_block_removes_it_from_its_path_only() {
    let session = session_with(vec![mix_chain()]);
    delete_drafted_block(
        &session,
        &draft(Some(0), 0, on(PathSide::B)),
        &rows(),
        &[],
        &[],
    )
    .expect("delete");
    assert!(lane(&session, PathSide::B).is_empty(), "b1 removed");
    let top: Vec<String> = chain_in(&session, 0)
        .blocks
        .iter()
        .map(|b| b.id.0.clone())
        .collect();
    assert_eq!(top, vec!["pre", "sp", "post"], "the top level is intact");
}

#[test]
fn inserting_into_path_a_puts_the_block_in_path_a() {
    i_slint_backend_testing::init_no_event_loop();
    let window = crate::AppWindow::new().unwrap();
    let session = session_with(vec![mix_chain()]);
    let items = Rc::new(VecModel::from(
        crate::block_editor::block_parameter_items_for_model("gain", "volume", &Default::default()),
    ));
    crate::block_editor::persist_block_editor_draft(
        &window,
        &draft(None, 1, on(PathSide::A)),
        &items,
        &session,
        &rows(),
        &Rc::new(RefCell::new(None)),
        &Rc::new(RefCell::new(false)),
        &[],
        &[],
        false,
    )
    .expect("insert");
    let a: Vec<String> = lane(&session, PathSide::A)
        .iter()
        .map(|b| b.id.0.clone())
        .collect();
    assert_eq!(a.len(), 3, "path A grew: {a:?}");
    assert_eq!(
        (a[0].as_str(), a[2].as_str()),
        ("a1", "a2"),
        "the new block sits at 1"
    );
    assert_eq!(
        chain_in(&session, 0).blocks.len(),
        3,
        "the top level did not grow"
    );
}

#[test]
fn the_graph_opens_the_editor_of_a_path_block() {
    let opened = open_path_block_through_the_bridge(on(PathSide::B).unwrap(), 0);
    let d = opened.borrow().clone().expect("a draft was opened");
    assert_eq!((d.block_index, d.path), (Some(0), on(PathSide::B)));
}

/// Wires `select_chain_block_callback` the way `issue_85_click_port_opens_editor_tests.rs`
/// does, then fires the bridge callback the graph fires for a path card.
fn open_path_block_through_the_bridge(
    path: PathRef,
    index: i32,
) -> Rc<RefCell<Option<BlockEditorDraft>>> {
    use crate::select_chain_block_callback::{wire, SelectChainBlockCallbackCtx};
    i_slint_backend_testing::init_no_event_loop();
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    let window = crate::AppWindow::new().unwrap();
    let insert_window = crate::ChainInsertWindow::new().unwrap();
    let port_window = crate::ChainPortWindow::new().unwrap();
    let draft: Rc<RefCell<Option<BlockEditorDraft>>> = Rc::new(RefCell::new(None));
    let session = session_with(vec![mix_chain()]);
    wire(
        &window,
        &insert_window,
        &port_window,
        SelectChainBlockCallbackCtx {
            open_compact_window: Rc::new(RefCell::new(None)),
            inline_tab_state: Rc::new(RefCell::new(Default::default())),
            selected_block: Rc::new(RefCell::new(None)),
            block_editor_draft: draft.clone(),
            insert_draft: Rc::new(RefCell::new(None)),
            block_type_options: Rc::new(VecModel::default()),
            block_model_options: Rc::new(VecModel::default()),
            filtered_block_model_options: Rc::new(VecModel::default()),
            block_model_option_labels: Rc::new(VecModel::default()),
            block_parameter_items: Rc::new(VecModel::default()),
            multi_slider_points: Rc::new(VecModel::default()),
            curve_editor_points: Rc::new(VecModel::default()),
            eq_band_curves: Rc::new(VecModel::default()),
            project_session: session,
            project_chains: Rc::new(VecModel::default()),
            saved_project_snapshot: Rc::new(RefCell::new(None)),
            project_dirty: Rc::new(RefCell::new(false)),
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(Vec::new())),
            open_block_windows: Rc::new(RefCell::new(Vec::new())),
            inline_stream_timer: Rc::new(RefCell::new(None)),
            toast_timer: Rc::new(Timer::default()),
            plugin_info_window: Rc::new(RefCell::new(None)),
            block_stream_reads: Rc::new(application::live_source::NoLiveSource),
            port_draft: Rc::new(RefCell::new(None)),
        },
    );
    crate::ChainGraphBridge::get(&window).invoke_open_path_block(
        0,
        path.split.0.as_str().into(),
        crate::chain_block_lists::side_index(&path.side),
        index,
    );
    draft
}
