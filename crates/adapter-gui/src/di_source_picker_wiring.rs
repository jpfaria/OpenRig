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
mod tests {
    use super::filter_di_sources;

    #[test]
    fn filter_di_sources_empty_query_returns_all() {
        let sources = vec![
            "Clean DI".to_string(),
            "Crunch DI".to_string(),
            "Lead DI".to_string(),
        ];
        let got: Vec<&String> = filter_di_sources(&sources, "");
        assert_eq!(got, vec![&sources[0], &sources[1], &sources[2]]);

        // Whitespace-only query is treated as empty.
        let got_ws: Vec<&String> = filter_di_sources(&sources, "   ");
        assert_eq!(got_ws, vec![&sources[0], &sources[1], &sources[2]]);
    }

    #[test]
    fn filter_di_sources_matches_case_insensitive_substring() {
        let sources = vec![
            "Clean DI".to_string(),
            "Crunch DI".to_string(),
            "Lead Tone".to_string(),
        ];
        // "di" matches the two "DI" sources, in original order.
        let got: Vec<&String> = filter_di_sources(&sources, "di");
        assert_eq!(got, vec![&sources[0], &sources[1]]);

        // Uppercase query still matches.
        let got_upper: Vec<&String> = filter_di_sources(&sources, "CRUNCH");
        assert_eq!(got_upper, vec![&sources[1]]);
    }

    #[test]
    fn filter_di_sources_no_match_returns_empty() {
        let sources = vec!["Clean DI".to_string(), "Crunch DI".to_string()];
        let got: Vec<&String> = filter_di_sources(&sources, "zzz");
        assert!(got.is_empty());
    }
}
