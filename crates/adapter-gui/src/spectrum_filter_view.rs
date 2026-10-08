//! Responsibility: applies the spectrum filter to the rows the window renders.
//!
//! The session's row model keeps every row (the readings every transport
//! reports come from it); the window renders a `FilterModel` over it that drops
//! the rows of the unchecked outputs. Each new session's model goes through
//! [`SpectrumFilterView::show`], and the user's picks survive the rebuild.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{FilterModel, Model, ModelRc, VecModel};

use crate::spectrum_filter::SpectrumFilter;
use crate::{ChannelOptionItem, SpectrumRow};

type RowFilter = Box<dyn Fn(&SpectrumRow) -> bool>;

#[derive(Default)]
pub struct SpectrumFilterView {
    filter: Rc<RefCell<SpectrumFilter>>,
    outputs: RefCell<Vec<String>>,
    shown: RefCell<Option<Rc<FilterModel<ModelRc<SpectrumRow>, RowFilter>>>>,
    items: Rc<VecModel<ChannelOptionItem>>,
}

impl SpectrumFilterView {
    /// The rows of `rows` whose output is checked — what the window renders.
    pub fn show(&self, rows: ModelRc<SpectrumRow>) -> ModelRc<SpectrumRow> {
        *self.outputs.borrow_mut() = distinct_outputs(&rows);
        let filter = Rc::clone(&self.filter);
        let keep: RowFilter = Box::new(move |row| filter.borrow().shows(&row.output));
        let shown = Rc::new(FilterModel::new(rows, keep));
        *self.shown.borrow_mut() = Some(Rc::clone(&shown));
        self.refresh_items();
        ModelRc::from(shown)
    }

    /// Check or uncheck the output at `index` of [`Self::items`].
    pub fn toggle(&self, index: usize, shown: bool) {
        let Some(output) = self.outputs.borrow().get(index).cloned() else {
            return;
        };
        self.filter.borrow_mut().set_shown(&output, shown);
        if let Some(rows) = self.shown.borrow().as_ref() {
            rows.reset();
        }
        self.refresh_items();
    }

    /// One checkbox per output of the current rows.
    pub fn items(&self) -> ModelRc<ChannelOptionItem> {
        ModelRc::from(Rc::clone(&self.items))
    }

    /// How many outputs are checked.
    pub fn shown_count(&self) -> i32 {
        self.items.iter().filter(|item| item.selected).count() as i32
    }

    fn refresh_items(&self) {
        let items = self.filter.borrow().items(&self.outputs.borrow());
        self.items.set_vec(items);
    }
}

fn distinct_outputs(rows: &ModelRc<SpectrumRow>) -> Vec<String> {
    let mut outputs: Vec<String> = Vec::new();
    for row in rows.iter() {
        if !outputs
            .iter()
            .any(|seen| seen.as_str() == row.output.as_str())
        {
            outputs.push(row.output.to_string());
        }
    }
    outputs
}

#[cfg(test)]
#[path = "spectrum_filter_view_tests.rs"]
mod tests;
