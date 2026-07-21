//! #749: search-as-you-type for the chain DI loop source dropdown (the shared
//! `Select` component). Mirrors `chain_rig_nav_wiring::wire_preset_picker_search`
//! one-to-one, but DI sources are plain strings where `key == label ==` the
//! source string, so there is no stable slot to preserve — the filter just
//! keeps matching strings in their original order.
//!
//! Only one dropdown popup is open at a time, so a single cached source list
//! backs whichever picker is open — no per-chain-row filtered model (which
//! would hit the in-place mutation dance of #537). `open` caches the picker's
//! full source list and publishes every row; each keystroke republishes the
//! filtered view. Filtering lives here because Slint has no string `contains`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Global, Model, ModelRc, SharedString, VecModel};

use crate::{DiSourcePicker, SelectOption};

/// Wires the `DiSourcePicker` global's `open`/`query-changed` callbacks to the
/// substring filter. Holds the full source list cached on `open` so each
/// keystroke can republish the filtered `options`.
pub(crate) fn wire_di_source_picker_search<W>(window: &W)
where
    W: ComponentHandle + 'static,
    for<'a> DiSourcePicker<'a>: slint::Global<'a, W>,
{
    let full_sources: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let weak = window.as_weak();
        let full_sources = full_sources.clone();
        DiSourcePicker::get(window).on_open(move |sources| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let sources: Vec<String> = sources.iter().map(|s| s.to_string()).collect();
            publish_di_options(&window, &sources, "");
            *full_sources.borrow_mut() = sources;
        });
    }
    {
        let weak = window.as_weak();
        let full_sources = full_sources.clone();
        DiSourcePicker::get(window).on_query_changed(move |query| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            publish_di_options(&window, &full_sources.borrow(), query.as_str());
        });
    }
}

/// Publish the filtered DI source rows onto the `DiSourcePicker` global. Each
/// row carries the source string in both `key` and `label`.
fn publish_di_options<W>(window: &W, sources: &[String], query: &str)
where
    W: ComponentHandle,
    for<'a> DiSourcePicker<'a>: slint::Global<'a, W>,
{
    let options: Vec<SelectOption> = filter_di_sources(sources, query)
        .into_iter()
        .map(|s| SelectOption {
            key: SharedString::from(s),
            label: SharedString::from(s),
        })
        .collect();
    DiSourcePicker::get(window).set_options(ModelRc::new(VecModel::from(options)));
}

/// Filter `sources` by `query` with a case-insensitive substring match,
/// preserving original order. An empty (trimmed) query returns every source.
/// Public for testing.
pub fn filter_di_sources<'a>(sources: &'a [String], query: &str) -> Vec<&'a String> {
    let query_lower = query.trim().to_lowercase();
    sources
        .iter()
        .filter(|s| query_lower.is_empty() || s.to_lowercase().contains(&query_lower))
        .collect()
}

#[cfg(test)]
#[path = "di_source_picker_wiring_tests.rs"]
mod tests;
