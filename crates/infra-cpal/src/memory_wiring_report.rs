//! Responsibility: words what a memory-wiring pass did for the log.
//!
//! #980 (review of #981): memory a pass leaves unwired is memory the kernel
//! may compress under pressure — the stall the wiring exists to prevent — so
//! it is a warning, said when it changes and not every pass (the keeper runs
//! every 5 s for as long as OpenRig runs).

use crate::memory_wiring_pass::{Report, Tally};

/// The log lines for `now`, given the pass `before` it.
pub(crate) fn report_lines(now: &Report, before: Option<&Report>) -> Vec<(log::Level, String)> {
    let mut lines = Vec::new();
    if now.wired.regions > 0 {
        lines.push((
            log::Level::Debug,
            format!(
                "memory residency: wired {} regions ({} MB), {} MB resident for good",
                now.wired.regions,
                mb(now.wired.bytes),
                mb(now.resident)
            ),
        ));
    }
    let left = [now.over_budget, now.too_large, now.refused];
    let left_before = before.map(|b| [b.over_budget, b.too_large, b.refused]);
    if left_before != Some(left) && left.iter().any(|tally| tally.regions > 0) {
        lines.push((
            log::Level::Warn,
            format!(
                "memory residency: left unwired — over the quarter-of-RAM budget {}, \
                 larger than 256 MB {}, refused by the kernel {}; the kernel may \
                 compress it under memory pressure and the audio may underrun",
                tally(&now.over_budget),
                tally(&now.too_large),
                tally(&now.refused)
            ),
        ));
    }
    lines
}

fn mb(bytes: u64) -> u64 {
    bytes >> 20
}

fn tally(tally: &Tally) -> String {
    format!("{} regions ({} MB)", tally.regions, mb(tally.bytes))
}

#[cfg(test)]
#[path = "memory_wiring_report_tests.rs"]
mod memory_wiring_report_tests;
