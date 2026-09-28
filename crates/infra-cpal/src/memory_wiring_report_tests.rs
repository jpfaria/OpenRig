//! Issue #980 (review of #981) — a wiring pass that leaves memory unwired
//! must say so, once, and a pass that changes nothing stays quiet (the
//! keeper runs every 5 s for as long as OpenRig runs).

use super::report_lines;
use crate::memory_wiring_pass::{Report, Tally};

const MB: u64 = 1 << 20;

#[test]
fn memory_left_unwired_is_logged_once_as_a_warning() {
    let now = Report {
        resident: 100 * MB,
        over_budget: Tally {
            regions: 1,
            bytes: 8 * MB,
        },
        ..Report::default()
    };
    let first = report_lines(&now, None);
    let again = report_lines(&now, Some(&now));
    assert!(
        first.len() == 1
            && first[0].0 == log::Level::Warn
            && first[0].1.contains("8 MB")
            && again.is_empty(),
        "8 MB left over the budget must be a warning the first time and not \
         every 5 s after: first {first:?}, again {again:?}"
    );
}

#[test]
fn every_reason_to_leave_memory_unwired_is_named() {
    let now = Report {
        too_large: Tally {
            regions: 1,
            bytes: 300 * MB,
        },
        refused: Tally {
            regions: 2,
            bytes: 16 * MB,
        },
        ..Report::default()
    };
    let lines = report_lines(&now, None);
    let text = lines
        .iter()
        .map(|(_, line)| line.as_str())
        .collect::<String>();
    assert!(
        text.contains("300 MB") && text.contains("16 MB"),
        "too large and refused by the kernel must both be in the log: {lines:?}"
    );
}

#[test]
fn newly_wired_memory_is_logged() {
    let now = Report {
        wired: Tally {
            regions: 2,
            bytes: 30 * MB,
        },
        resident: 100 * MB,
        ..Report::default()
    };
    let lines = report_lines(&now, Some(&Report::default()));
    assert!(
        lines.len() == 1 && lines[0].0 == log::Level::Info && lines[0].1.contains("30 MB"),
        "a pass that wired memory says how much: {lines:?}"
    );
}

#[test]
fn a_pass_that_changes_nothing_is_silent() {
    let steady = Report {
        resident: 100 * MB,
        ..Report::default()
    };
    assert!(report_lines(&steady, Some(&steady)).is_empty());
}
