//! #913 — writing one parameter row back by its path.
//!
//! Every setter addresses a row by `path`, so what must hold is: the row named
//! is the row changed and no sibling moves, a path that is not in the model
//! leaves everything alone, and the numeric setter quantizes against the row's
//! own spec before formatting the text the user reads — an integer knob must
//! never show "3.00".

use super::{
    set_block_parameter_bool, set_block_parameter_number, set_block_parameter_option,
    set_block_parameter_text,
};
use crate::BlockParameterItem;
use slint::{Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;

fn row(path: &str) -> BlockParameterItem {
    BlockParameterItem {
        path: path.into(),
        ..Default::default()
    }
}

fn numeric(path: &str, min: f32, max: f32, step: f32, integer: bool) -> BlockParameterItem {
    BlockParameterItem {
        path: path.into(),
        numeric_min: min,
        numeric_max: max,
        numeric_step: step,
        numeric_integer: integer,
        ..Default::default()
    }
}

fn model(rows: Vec<BlockParameterItem>) -> Rc<VecModel<BlockParameterItem>> {
    Rc::new(VecModel::from(rows))
}

fn find(model: &Rc<VecModel<BlockParameterItem>>, path: &str) -> BlockParameterItem {
    (0..model.row_count())
        .filter_map(|i| model.row_data(i))
        .find(|r| r.path.as_str() == path)
        .unwrap_or_else(|| panic!("row '{path}' missing"))
}

#[test]
fn a_text_setter_writes_only_the_row_it_names() {
    let m = model(vec![row("gain.level"), row("gain.tone")]);
    set_block_parameter_text(&m, "gain.tone", "warm");
    assert_eq!(find(&m, "gain.tone").value_text.as_str(), "warm");
    assert_eq!(
        find(&m, "gain.level").value_text.as_str(),
        "",
        "the sibling row must not move"
    );
}

#[test]
fn a_bool_setter_flips_only_its_own_row() {
    let m = model(vec![row("gate.enabled"), row("gate.hard")]);
    set_block_parameter_bool(&m, "gate.enabled", true);
    assert!(find(&m, "gate.enabled").bool_value);
    assert!(!find(&m, "gate.hard").bool_value);
}

#[test]
fn a_path_that_is_not_in_the_model_changes_nothing() {
    let m = model(vec![row("gain.level")]);
    set_block_parameter_text(&m, "no.such.path", "x");
    set_block_parameter_bool(&m, "no.such.path", true);
    set_block_parameter_number(&m, "no.such.path", 9.0);
    set_block_parameter_option(&m, "no.such.path", 3);
    let untouched = find(&m, "gain.level");
    assert_eq!(untouched.value_text.as_str(), "");
    assert!(!untouched.bool_value);
    assert_eq!(untouched.numeric_value, 0.0);
    assert_eq!(untouched.selected_option_index, 0);
}

#[test]
fn a_number_above_the_maximum_is_clamped_before_it_is_shown() {
    let m = model(vec![numeric("amp.gain", 0.0, 10.0, 0.1, false)]);
    set_block_parameter_number(&m, "amp.gain", 42.0);
    let r = find(&m, "amp.gain");
    assert_eq!(r.numeric_value, 10.0);
    assert_eq!(r.value_text.as_str(), "10.00");
}

#[test]
fn a_number_below_the_minimum_is_clamped_before_it_is_shown() {
    let m = model(vec![numeric("amp.gain", 2.0, 10.0, 0.1, false)]);
    set_block_parameter_number(&m, "amp.gain", -5.0);
    assert_eq!(find(&m, "amp.gain").numeric_value, 2.0);
}

#[test]
fn an_integer_row_is_never_shown_with_decimals() {
    let m = model(vec![numeric("delay.taps", 1.0, 8.0, 1.0, true)]);
    set_block_parameter_number(&m, "delay.taps", 3.4);
    let r = find(&m, "delay.taps");
    assert_eq!(
        r.value_text.as_str(),
        "3",
        "an integer knob showing '3.40' is the bug this formatting prevents"
    );
    assert_eq!(r.numeric_value.fract(), 0.0);
}

#[test]
fn selecting_an_option_copies_that_options_value_into_the_text() {
    let mut r = row("cab.mic");
    r.option_values = ModelRc::from(Rc::new(VecModel::from(vec![
        SharedString::from("sm57"),
        SharedString::from("r121"),
    ])));
    let m = model(vec![r]);
    set_block_parameter_option(&m, "cab.mic", 1);
    let after = find(&m, "cab.mic");
    assert_eq!(after.selected_option_index, 1);
    assert_eq!(after.value_text.as_str(), "r121");
}

#[test]
fn clearing_the_selection_records_the_index_without_reading_a_value() {
    let mut r = row("cab.mic");
    r.value_text = "sm57".into();
    r.option_values = ModelRc::from(Rc::new(VecModel::from(vec![SharedString::from("sm57")])));
    let m = model(vec![r]);
    set_block_parameter_option(&m, "cab.mic", -1);
    let after = find(&m, "cab.mic");
    assert_eq!(after.selected_option_index, -1);
    assert_eq!(
        after.value_text.as_str(),
        "sm57",
        "index -1 must not index the option list"
    );
}

#[test]
fn an_index_past_the_end_leaves_the_text_alone() {
    let mut r = row("cab.mic");
    r.value_text = "sm57".into();
    r.option_values = ModelRc::from(Rc::new(VecModel::from(vec![SharedString::from("sm57")])));
    let m = model(vec![r]);
    set_block_parameter_option(&m, "cab.mic", 7);
    assert_eq!(find(&m, "cab.mic").value_text.as_str(), "sm57");
}
