//! The spectrum window shows only the rows of the outputs left checked: one
//! unchecked output takes both its L and R scopes away, and a rebuilt session
//! keeps what the user unchecked.

use super::SpectrumFilterView;
use crate::SpectrumRow;
use slint::{Model, ModelRc, VecModel};
use std::rc::Rc;

fn rows(outputs: &[&str]) -> ModelRc<SpectrumRow> {
    let rows: Vec<SpectrumRow> = outputs
        .iter()
        .flat_map(|output| {
            ["L", "R"].map(|side| SpectrumRow {
                label: format!("{output}  ·  {side}").into(),
                output: (*output).into(),
                ..Default::default()
            })
        })
        .collect();
    ModelRc::from(Rc::new(VecModel::from(rows)))
}

fn labels(model: &ModelRc<SpectrumRow>) -> Vec<String> {
    model.iter().map(|row| row.label.to_string()).collect()
}

#[test]
fn every_row_shows_until_an_output_is_unchecked() {
    let view = SpectrumFilterView::default();

    let shown = view.show(rows(&["A", "B"]));

    assert_eq!(shown.row_count(), 4);
    assert_eq!(view.items().row_count(), 2, "one checkbox per output");
    assert_eq!(view.shown_count(), 2);
}

#[test]
fn unchecking_an_output_hides_both_of_its_rows() {
    let view = SpectrumFilterView::default();
    let shown = view.show(rows(&["A", "B"]));

    view.toggle(0, false);

    assert_eq!(labels(&shown), vec!["B  ·  L", "B  ·  R"]);
    assert!(!view.items().row_data(0).unwrap().selected);
    assert_eq!(view.shown_count(), 1);
}

#[test]
fn checking_it_again_brings_both_rows_back() {
    let view = SpectrumFilterView::default();
    let shown = view.show(rows(&["A", "B"]));
    view.toggle(0, false);

    view.toggle(0, true);

    assert_eq!(shown.row_count(), 4);
}

#[test]
fn a_rebuilt_session_keeps_the_outputs_the_user_unchecked() {
    let view = SpectrumFilterView::default();
    view.show(rows(&["A", "B"]));
    view.toggle(0, false);

    let shown = view.show(rows(&["A", "B", "C"]));

    assert_eq!(
        labels(&shown),
        vec!["B  ·  L", "B  ·  R", "C  ·  L", "C  ·  R"]
    );
    assert_eq!(view.shown_count(), 2);
}
