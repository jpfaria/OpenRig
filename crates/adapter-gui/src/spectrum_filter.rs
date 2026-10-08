//! Responsibility: decides which outputs the spectrum window shows.
//!
//! A view filter: it changes what the window draws, never what the analyzer
//! reads, so it is not a `Command` and `openrig://spectrum` keeps every row.
//! It remembers the outputs the user unchecked, by label, so a session rebuilt
//! after an I/O change keeps them hidden; an output never seen before shows.

use std::collections::BTreeSet;

use crate::ChannelOptionItem;

#[derive(Default)]
pub struct SpectrumFilter {
    hidden: BTreeSet<String>,
}

impl SpectrumFilter {
    pub fn shows(&self, output: &str) -> bool {
        !self.hidden.contains(output)
    }

    pub fn set_shown(&mut self, output: &str, shown: bool) {
        if shown {
            self.hidden.remove(output);
        } else {
            self.hidden.insert(output.to_string());
        }
    }

    /// One checkbox per output, in the order the rows read them.
    pub fn items(&self, outputs: &[String]) -> Vec<ChannelOptionItem> {
        outputs
            .iter()
            .enumerate()
            .map(|(index, output)| ChannelOptionItem {
                index: index as i32,
                label: output.as_str().into(),
                selected: self.shows(output),
                available: true,
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "spectrum_filter_tests.rs"]
mod tests;
