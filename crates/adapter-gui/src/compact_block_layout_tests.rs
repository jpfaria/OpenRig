//! #787 — the compact row's parameter strip wraps into lines and the row grows
//! to fit them. Pure geometry, so it is unit-tested without any window.

use super::*;
use crate::BlockParameterItem;

fn knob(path: &str) -> BlockParameterItem {
    BlockParameterItem {
        path: path.into(),
        widget_kind: "slider".into(),
        tab_slot: 0,
        strip_line: -1,
        ..Default::default()
    }
}

fn boolean(path: &str) -> BlockParameterItem {
    BlockParameterItem {
        path: path.into(),
        widget_kind: "bool".into(),
        tab_slot: 0,
        strip_line: -1,
        ..Default::default()
    }
}

fn enum_with(path: &str, options: usize) -> BlockParameterItem {
    let labels: Vec<slint::SharedString> =
        (0..options).map(|i| format!("opt{i}").into()).collect();
    BlockParameterItem {
        path: path.into(),
        widget_kind: "enum".into(),
        option_labels: slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(labels))),
        tab_slot: 0,
        strip_line: -1,
        ..Default::default()
    }
}

/// Hidden by the tab filter (#780 keeps the model full and marks it -1).
fn hidden(mut it: BlockParameterItem) -> BlockParameterItem {
    it.tab_slot = -1;
    it
}

#[test]
fn a_strip_that_fits_the_budget_stays_on_one_line() {
    let mut items: Vec<_> = (0..8).map(|i| knob(&format!("p{i}"))).collect();

    let lines = assign_strip_lines(&mut items);

    assert_eq!(lines, 1, "8 knobs fit the strip budget");
    assert!(
        items.iter().all(|it| it.strip_line == 0),
        "every knob lands on line 0, got {:?}",
        items.iter().map(|it| it.strip_line).collect::<Vec<_>>()
    );
}

#[test]
fn a_strip_wider_than_the_budget_wraps_to_the_next_line() {
    // A knob cell is 62px + 4px spacing = 66px; the budget is 720px, so 10
    // knobs fill a line and the 11th starts a new one.
    let mut items: Vec<_> = (0..11).map(|i| knob(&format!("p{i}"))).collect();

    let lines = assign_strip_lines(&mut items);

    assert_eq!(lines, 2, "11 knobs need a second line");
    assert_eq!(items[9].strip_line, 0, "the 10th knob still fits line 0");
    assert_eq!(items[10].strip_line, 1, "the 11th knob wraps");
}

#[test]
fn enum_and_bool_cells_use_their_own_widths() {
    // 5 dropdown enums (140px + 4) = 720px exactly, so the 6th wraps.
    let mut items: Vec<_> = (0..6).map(|i| enum_with(&format!("e{i}"), 8)).collect();
    let lines = assign_strip_lines(&mut items);
    assert_eq!(lines, 2, "6 dropdown enums do not fit one line");
    assert_eq!(items[5].strip_line, 1);

    // Bools are the narrowest cell (48px + 4), so 13 of them still fit.
    let mut items: Vec<_> = (0..13).map(|i| boolean(&format!("b{i}"))).collect();
    assert_eq!(assign_strip_lines(&mut items), 1, "13 bools fit one line");
}

#[test]
fn parameters_hidden_by_the_tab_filter_take_no_space_and_no_line() {
    let mut items = vec![
        knob("a"),
        hidden(knob("b")),
        hidden(knob("c")),
        knob("d"),
    ];

    let lines = assign_strip_lines(&mut items);

    assert_eq!(lines, 1);
    assert_eq!(items[0].strip_line, 0);
    assert_eq!(items[1].strip_line, -1, "hidden params get no line");
    assert_eq!(items[2].strip_line, -1);
    assert_eq!(items[3].strip_line, 0, "the visible params share line 0");
}

#[test]
fn an_empty_strip_has_no_lines() {
    let mut items: Vec<BlockParameterItem> = Vec::new();
    assert_eq!(assign_strip_lines(&mut items), 0);
}

#[test]
fn a_block_that_fits_keeps_the_current_row_height() {
    assert_eq!(row_height_px(1, false), BASE_ROW_HEIGHT_PX);
    assert_eq!(row_height_px(0, false), BASE_ROW_HEIGHT_PX);
}

#[test]
fn each_extra_line_grows_the_row() {
    assert_eq!(row_height_px(2, false), 10.0 + 2.0 * LINE_HEIGHT_PX);
    assert_eq!(row_height_px(3, false), 10.0 + 3.0 * LINE_HEIGHT_PX);
}

#[test]
fn a_tab_bar_adds_its_own_height() {
    assert_eq!(
        row_height_px(1, true),
        10.0 + TAB_BAR_HEIGHT_PX + LINE_HEIGHT_PX
    );
    assert_eq!(
        row_height_px(2, true),
        10.0 + TAB_BAR_HEIGHT_PX + 2.0 * LINE_HEIGHT_PX
    );
}

#[test]
fn rows_stack_with_the_gap_between_them() {
    let heights = [100.0_f32, 190.0, 128.0];

    let ys = row_y_offsets(&heights);

    assert_eq!(ys[0], ROW_GAP_PX, "the first row sits below the insert slot");
    assert_eq!(ys[1], ROW_GAP_PX + 100.0 + ROW_GAP_PX);
    assert_eq!(ys[2], ROW_GAP_PX + 100.0 + ROW_GAP_PX + 190.0 + ROW_GAP_PX);
}

#[test]
fn the_viewport_covers_every_row_plus_the_trailing_slot() {
    let heights = [100.0_f32, 190.0];

    assert_eq!(
        viewport_height_px(&heights),
        ROW_GAP_PX + 100.0 + ROW_GAP_PX + 190.0 + ROW_GAP_PX
    );
    assert_eq!(viewport_height_px(&[]), ROW_GAP_PX);
}
