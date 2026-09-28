//! Issue #980 (review of #981) — what one wiring pass wires, and what it must
//! say it left unwired.
//!
//! The first pass capped the wired memory against a running total taken in
//! address order: a new region below regions an earlier pass wired saw only
//! the wired bytes beneath it, so a pass could wire past the quarter-of-RAM
//! cap. And whatever it left unwired — over the budget, larger than 256 MB,
//! or refused by `mlock` — it left silently, so on a smaller Mac the kernel
//! could compress a new chain's memory again with nothing in the log.

use super::{run_pass, Region, Report, Tally, MAX_REGION};

const MB: u64 = 1 << 20;

fn eligible(start: u64, size: u64, wired: bool) -> Region {
    Region {
        start,
        size,
        writable: true,
        private: true,
        touched: true,
        wired,
    }
}

/// Runs a pass where every region is still unwired at wire time and every
/// wire succeeds; returns the report and the regions it wired.
fn pass(regions: &[Region], budget: u64) -> (Report, Vec<u64>) {
    let mut wired = Vec::new();
    let report = run_pass(
        regions,
        budget,
        |_| true,
        |region| {
            wired.push(region.start);
            true
        },
    );
    (report, wired)
}

#[test]
fn the_budget_counts_wired_memory_above_a_new_region() {
    // A rebuild freed low memory; the next chain's buffer landed below the
    // 100 MB an earlier pass wired.
    let regions = [
        eligible(0x1000_0000, 8 * MB, false),
        eligible(0x9000_0000, 100 * MB, true),
    ];
    let (report, wired) = pass(&regions, 104 * MB);
    assert_eq!(
        (wired.len(), report.over_budget, report.resident),
        (
            0,
            Tally {
                regions: 1,
                bytes: 8 * MB
            },
            100 * MB
        ),
        "100 MB wired + 8 MB new > the 104 MB budget: the new region must stay \
         unwired and be reported, wherever it sits in the address space"
    );
}

#[test]
fn what_a_pass_leaves_unwired_is_reported() {
    let regions = [
        eligible(0x1000_0000, MAX_REGION + MB, false),
        eligible(0x3000_0000, 8 * MB, false),
    ];
    let (report, wired) = pass(&regions, 4 * MB);
    assert_eq!(
        (wired.len(), report.too_large, report.over_budget),
        (
            0,
            Tally {
                regions: 1,
                bytes: MAX_REGION + MB
            },
            Tally {
                regions: 1,
                bytes: 8 * MB
            }
        ),
        "a region over 256 MB and one over the budget are left unwired — and \
         the pass must say so"
    );
}

#[test]
fn a_refused_wire_is_reported_and_not_counted_as_resident() {
    let regions = [eligible(0x1000_0000, 8 * MB, false)];
    let report = run_pass(&regions, 1 << 40, |_| true, |_| false);
    assert_eq!(
        (report.wired, report.refused, report.resident),
        (
            Tally::default(),
            Tally {
                regions: 1,
                bytes: 8 * MB
            },
            0
        ),
        "mlock failing (EAGAIN at the kernel's wire limit) must be reported, \
         and the region is not resident"
    );
}

#[test]
fn a_region_wired_since_the_snapshot_is_not_wired_again() {
    // Another pass wired it between this pass's snapshot and its wire:
    // `mlock` stacks a user wire per call.
    let regions = [eligible(0x1000_0000, 8 * MB, false)];
    let mut wires = 0;
    let report = run_pass(
        &regions,
        1 << 40,
        |_| false,
        |_| {
            wires += 1;
            true
        },
    );
    assert_eq!(
        (wires, report.wired, report.resident),
        (0, Tally::default(), 8 * MB),
        "a region already wired when its turn comes must not get a second wire"
    );
}

#[test]
fn memory_that_is_not_ours_or_untouched_is_left_alone() {
    let shared = Region {
        private: false,
        ..eligible(0x1000_0000, 8 * MB, false)
    };
    let read_only = Region {
        writable: false,
        ..eligible(0x2000_0000, 8 * MB, false)
    };
    let untouched = Region {
        touched: false,
        ..eligible(0x3000_0000, 64 * MB, false)
    };
    let (report, wired) = pass(&[shared, read_only, untouched], 1 << 40);
    assert_eq!(
        (wired.len(), report),
        (0, Report::default()),
        "shared, read-only and untouched memory is neither wired nor reported"
    );
}
