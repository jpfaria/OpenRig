//! #328 — the split/mixer editor overlay, driven by real pointer events, and
//! the wiring end to end (open → rows → edit → project).

use std::cell::RefCell;
use std::rc::Rc;

use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Global, LogicalPosition, Model, ModelRc, Timer, VecModel};

use project::block::split_param_keys::mix_pan;
use project::block::split_params::MIX_MASTER_SUM;
use project::block::{AudioBlockKind, SplitBlock, SplitEnd};

use crate::chain_graph_fixtures_tests::{chain_in, mix_chain, rows, session_with, y_chain};
use crate::split_editor_grid::split_editor_grid;
use crate::split_editor_items::SplitEditorKind;
use crate::split_editor_wiring::{wire, SplitEditorWiringCtx};
use crate::{ChainGraphOverlayState, SplitEditorHarness};

fn click(w: &impl ComponentHandle, el: &i_slint_backend_testing::ElementHandle) {
    let (pos, size) = (el.absolute_position(), el.size());
    let p = LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: p });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: p,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerReleased {
        position: p,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerExited);
}

fn open(h: &SplitEditorHarness, kind: SplitEditorKind) {
    open_paths(h, kind, 2);
}

fn open_paths(h: &SplitEditorHarness, kind: SplitEditorKind, path_count: usize) {
    let split = SplitBlock::with_paths(SplitEnd::Mix, vec![vec![]; path_count]);
    let state = ChainGraphOverlayState::get(h);
    let grid = split_editor_grid(&split, kind);
    state.set_split_editor_cols(grid.cols as i32);
    state.set_split_editor_rows(grid.rows as i32);
    state.set_split_editor_items(ModelRc::new(VecModel::from(grid.items)));
    state.set_split_editor_chain_index(0);
    state.set_split_editor_split_id("sp".into());
    state.set_split_editor_title("Mixer".into());
    state.set_split_editor_kind(if kind == SplitEditorKind::Split { 0 } else { 1 });
    state.set_split_editor_open(true);
}

fn wired_app(
    chain: project::chain::Chain,
) -> (
    crate::AppWindow,
    Rc<RefCell<Option<crate::state::ProjectSession>>>,
) {
    let app = crate::AppWindow::new().unwrap();
    let session = session_with(vec![chain]);
    wire(
        &app,
        SplitEditorWiringCtx {
            project_session: session.clone(),
            project_chains: rows(),
            input_chain_devices: Rc::new(RefCell::new(Vec::new())),
            output_chain_devices: Rc::new(RefCell::new(Vec::new())),
            toast_timer: Rc::new(Timer::default()),
        },
    );
    (app, session)
}

fn end_of(session: &Rc<RefCell<Option<crate::state::ProjectSession>>>) -> SplitEnd {
    let AudioBlockKind::Split(split) = &chain_in(session, 0).blocks[1].kind else {
        panic!("block 1 is the split")
    };
    split.end
}

#[test]
fn toggling_master_sum_reports_the_split_and_the_knob() {
    i_slint_backend_testing::init_no_event_loop();
    let h = SplitEditorHarness::new().unwrap();
    open(&h, SplitEditorKind::Mixer);
    let got: Rc<RefCell<Option<(i32, String, String, bool)>>> = Rc::new(RefCell::new(None));
    let seen = got.clone();
    ChainGraphOverlayState::get(&h).on_split_editor_bool(move |ci, id, path, on| {
        *seen.borrow_mut() = Some((ci, id.to_string(), path.to_string(), on));
    });
    h.show().unwrap();
    let switch =
        i_slint_backend_testing::ElementHandle::find_by_element_type_name(&h, "ToggleSwitch")
            .next()
            .expect("the master-sum switch is drawn");
    click(&h, &switch);
    assert_eq!(
        *got.borrow(),
        Some((0, "sp".to_string(), MIX_MASTER_SUM.to_string(), true))
    );
}

#[test]
fn the_close_button_closes_the_editor() {
    i_slint_backend_testing::init_no_event_loop();
    let h = SplitEditorHarness::new().unwrap();
    open(&h, SplitEditorKind::Mixer);
    h.show().unwrap();
    let close = i_slint_backend_testing::ElementHandle::find_by_element_id(
        &h,
        "SplitEditorOverlay::close-ta",
    )
    .next()
    .expect("close button");
    click(&h, &close);
    assert!(!ChainGraphOverlayState::get(&h).get_split_editor_open());
}

/// Orchestrator decision 9: the split editor carries a Mix / Y switch.
#[test]
fn the_y_segment_asks_for_a_y_end() {
    i_slint_backend_testing::init_no_event_loop();
    let h = SplitEditorHarness::new().unwrap();
    open(&h, SplitEditorKind::Split);
    let got: Rc<RefCell<Option<(i32, String, bool)>>> = Rc::new(RefCell::new(None));
    let seen = got.clone();
    ChainGraphOverlayState::get(&h).on_split_editor_set_end(move |ci, id, y| {
        *seen.borrow_mut() = Some((ci, id.to_string(), y));
    });
    h.show().unwrap();
    let y = i_slint_backend_testing::ElementHandle::find_by_element_id(
        &h,
        "SplitEditorOverlay::end-y-ta",
    )
    .next()
    .expect("the Y segment is drawn in the split editor");
    click(&h, &y);
    assert_eq!(*got.borrow(), Some((0, "sp".to_string(), true)));
}

#[test]
fn the_mixer_editor_has_no_end_switch() {
    i_slint_backend_testing::init_no_event_loop();
    let h = SplitEditorHarness::new().unwrap();
    open(&h, SplitEditorKind::Mixer);
    h.show().unwrap();
    assert!(i_slint_backend_testing::ElementHandle::find_by_element_id(
        &h,
        "SplitEditorOverlay::end-y-ta"
    )
    .next()
    .is_none());
}

#[test]
fn opening_the_mixer_lists_its_knobs_and_an_edit_reaches_the_split() {
    i_slint_backend_testing::init_no_event_loop();
    let (app, session) = wired_app(mix_chain());
    let state = ChainGraphOverlayState::get(&app);
    state.invoke_open_split_editor(0, "sp".into(), 1);
    assert!(state.get_split_editor_open());
    assert_eq!(
        state.get_split_editor_items().row_count(),
        8,
        "level, pan and polarity per path, then master and master sum"
    );
    assert_eq!(state.get_split_editor_split_id().as_str(), "sp");

    let mix_pan_a = mix_pan(0);
    state.invoke_split_editor_number(0, "sp".into(), mix_pan_a.as_str().into(), -50.0);

    let AudioBlockKind::Split(split) = &chain_in(&session, 0).blocks[1].kind else {
        panic!("block 1 is the split")
    };
    assert_eq!(
        split.params.get(&mix_pan_a).and_then(|v| v.as_f32()),
        Some(-50.0)
    );
    let row = state
        .get_split_editor_items()
        .iter()
        .find(|r| r.path.as_str() == mix_pan_a)
        .unwrap();
    assert_eq!(
        row.numeric_value, -50.0,
        "the row follows the edit in place"
    );
}

#[test]
fn switching_a_y_to_mix_reaches_the_project() {
    i_slint_backend_testing::init_no_event_loop();
    let (app, session) = wired_app(y_chain());
    let state = ChainGraphOverlayState::get(&app);
    state.invoke_open_split_editor(0, "sp".into(), 0);
    assert!(state.get_split_editor_end_y(), "a Y opens with the Y lit");
    state.invoke_split_editor_set_end(0, "sp".into(), false);
    assert_eq!(end_of(&session), SplitEnd::Mix);
    assert!(!state.get_split_editor_end_y());
}

#[test]
fn a_refused_switch_shows_the_error() {
    i_slint_backend_testing::init_no_event_loop();
    let (app, session) = wired_app(mix_chain());
    let state = ChainGraphOverlayState::get(&app);
    state.invoke_open_split_editor(0, "sp".into(), 0);
    state.invoke_split_editor_set_end(0, "sp".into(), true);
    assert_eq!(end_of(&session), SplitEnd::Mix);
    assert!(!state.get_split_editor_end_y());
    assert!(
        !app.get_status_message().is_empty(),
        "the refusal is shown as a toast"
    );
}

/// #328 §11: no limit on the number of Mix splits, so the Y at the end of
/// the chain may become a second Mix.
#[test]
fn mix_then_y_the_y_at_the_end_switches_to_a_second_mix() {
    i_slint_backend_testing::init_no_event_loop();
    let (app, session) = wired_app(crate::chain_graph_fixtures_tests::mix_then_y_chain());
    let state = ChainGraphOverlayState::get(&app);
    state.set_split_editor_chain_index(0);
    state.set_split_editor_kind(0);
    state.set_split_editor_split_id("y".into());
    state.set_split_editor_end_y(true);
    state.set_split_editor_open(true);

    state.invoke_split_editor_set_end(0, "y".into(), false);

    let AudioBlockKind::Split(y) = &chain_in(&session, 0).blocks[3].kind else {
        panic!("block 3 is the split")
    };
    assert_eq!(y.end, SplitEnd::Mix);
    assert!(!state.get_split_editor_end_y(), "Mix is lit");
}

fn param_cells(h: &SplitEditorHarness) -> Vec<(f32, f32)> {
    i_slint_backend_testing::ElementHandle::find_by_element_type_name(h, "BlockPanelParameterItem")
        .map(|el| (el.absolute_position().x, el.absolute_position().y))
        .collect()
}

/// #328: with A, B and C the mixer is one row per path — Level B sits under
/// Level A, not beside Pan A.
#[test]
fn the_mixer_draws_one_row_per_path() {
    i_slint_backend_testing::init_no_event_loop();
    let h = SplitEditorHarness::new().unwrap();
    open_paths(&h, SplitEditorKind::Mixer, 3);
    h.show().unwrap();
    let cells = param_cells(&h);
    assert_eq!(cells.len(), 11, "three knobs per path, master, master sum");
    let (level_a, level_b, level_c) = (cells[0], cells[3], cells[6]);
    assert_eq!(level_b.0, level_a.0, "Level B is under Level A");
    assert_eq!(level_c.0, level_a.0, "Level C is under Level B");
    assert!(level_a.1 < level_b.1 && level_b.1 < level_c.1);
}

/// #328: the mode is the whole split's knob — alone on the top row.
#[test]
fn the_split_draws_the_mode_alone_on_top() {
    i_slint_backend_testing::init_no_event_loop();
    let h = SplitEditorHarness::new().unwrap();
    open_paths(&h, SplitEditorKind::Split, 3);
    h.show().unwrap();
    let cells = param_cells(&h);
    let (mode, level_to_a, balance_a) = (cells[0], cells[1], cells[2]);
    assert_eq!(level_to_a.0, mode.0, "path A starts a row");
    assert!(level_to_a.1 > mode.1);
    assert_eq!(balance_a.1, level_to_a.1, "path A on one row");
}

#[test]
fn opening_the_mixer_sizes_the_grid_one_row_per_path() {
    i_slint_backend_testing::init_no_event_loop();
    let (app, _session) = wired_app(mix_chain());
    let state = ChainGraphOverlayState::get(&app);
    state.invoke_open_split_editor(0, "sp".into(), 1);
    assert_eq!(
        (state.get_split_editor_cols(), state.get_split_editor_rows()),
        (3, 3),
        "A, B, then the master"
    );
}
