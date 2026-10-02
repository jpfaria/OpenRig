//! Responsibility: narrows a drum picker list to the rows a search matches.

use crate::drums_view::DrumPick;

/// The rows of `picks` that match `query`, ignoring case. A genre header
/// stays while any groove under it matches; a query naming the genre keeps
/// the whole genre.
pub(crate) fn filter_picks(picks: &[DrumPick], query: &str) -> Vec<DrumPick> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return picks.to_vec();
    }
    let matches = |pick: &DrumPick| pick.label.to_lowercase().contains(&needle);
    let mut kept = Vec::new();
    let mut header: Option<&DrumPick> = None;
    let mut whole_genre = false;
    for pick in picks {
        if pick.header {
            header = Some(pick);
            whole_genre = matches(pick);
            continue;
        }
        if whole_genre || matches(pick) {
            if let Some(h) = header.take() {
                kept.push(h.clone());
            }
            kept.push(pick.clone());
        }
    }
    kept
}

#[cfg(test)]
#[path = "drums_picker_filter_tests.rs"]
mod tests;
