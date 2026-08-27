//! Responsibility: clears the latency badges whose display window ended.
//!
//! Split out of `latency_probe` (#913). The sweep runs from a timer, but what
//! it decides is time arithmetic over the open display windows — pass it the
//! instant and it is fully determined.

use std::time::Instant;

use slint::{Model, VecModel};

use crate::latency_probe::ProbeWindows;
use crate::ProjectChainItem;

/// Clear every badge whose window closed at or before `now`, and forget those
/// windows. Returns the row indexes cleared.
///
/// A row already reading zero is left untouched: rewriting an unchanged row
/// would make Slint re-render the whole chain list on every 500 ms tick.
pub(crate) fn clear_expired_badges(
    project_chains: &VecModel<ProjectChainItem>,
    probe_windows: &ProbeWindows,
    now: Instant,
) -> Vec<usize> {
    let expired: Vec<usize> = probe_windows
        .borrow()
        .iter()
        .filter(|(_, expiry)| now >= **expiry)
        .map(|(index, _)| *index)
        .collect();
    if expired.is_empty() {
        return expired;
    }
    let mut cleared = Vec::new();
    for index in &expired {
        if let Some(mut item) = project_chains.row_data(*index) {
            if item.latency_ms != 0.0 {
                item.latency_ms = 0.0;
                project_chains.set_row_data(*index, item);
                cleared.push(*index);
            }
        }
    }
    let mut windows = probe_windows.borrow_mut();
    for index in expired {
        windows.remove(&index);
    }
    cleared
}

#[cfg(test)]
#[path = "latency_badge_expiry_tests.rs"]
mod tests;
