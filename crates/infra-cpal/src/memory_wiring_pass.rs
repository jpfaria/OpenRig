//! Responsibility: runs one memory-wiring pass over a snapshot of the process's regions.

/// A region larger than this is a reservation, not a working set.
pub(crate) const MAX_REGION: u64 = 256 << 20;

/// One region of the process as the kernel reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Region {
    pub(crate) start: u64,
    pub(crate) size: u64,
    pub(crate) writable: bool,
    pub(crate) private: bool,
    pub(crate) touched: bool,
    pub(crate) wired: bool,
}

/// How many regions, and how many bytes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tally {
    pub(crate) regions: usize,
    pub(crate) bytes: u64,
}

/// What one pass did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Report {
    /// Wired by this pass.
    pub(crate) wired: Tally,
    /// Bytes of the process's own memory wired after the pass.
    pub(crate) resident: u64,
    /// Left unwired: the budget was spent.
    pub(crate) over_budget: Tally,
    /// Left unwired: larger than [`MAX_REGION`].
    pub(crate) too_large: Tally,
    /// Left unwired: the kernel refused the wire.
    pub(crate) refused: Tally,
}

impl Tally {
    fn add(&mut self, region: &Region) {
        self.regions += 1;
        self.bytes += region.size;
    }
}

impl Region {
    /// The process's own memory that something already touched.
    fn eligible(&self) -> bool {
        self.writable && self.private && self.touched
    }
}

/// Wires, in address order, every region of `regions` that is writable,
/// private and touched and not wired yet, within `budget` bytes wired in
/// total — counting every region already wired, wherever it sits (#980
/// review: a running total in address order let a pass wire past the cap).
/// `still_unwired` is asked right before each wire: a region another pass
/// wired since the snapshot must not get a second wire (`mlock` stacks one
/// per call). `wire` returns whether the kernel took it. Whatever the pass
/// leaves unwired is reported, never dropped silently.
pub(crate) fn run_pass(
    regions: &[Region],
    budget: u64,
    mut still_unwired: impl FnMut(&Region) -> bool,
    mut wire: impl FnMut(&Region) -> bool,
) -> Report {
    let mut report = Report {
        resident: regions
            .iter()
            .filter(|region| region.eligible() && region.wired)
            .map(|region| region.size)
            .sum(),
        ..Report::default()
    };
    for region in regions.iter().filter(|r| r.eligible() && !r.wired) {
        if region.size > MAX_REGION {
            report.too_large.add(region);
        } else if report.resident + region.size > budget {
            report.over_budget.add(region);
        } else if !still_unwired(region) {
            report.resident += region.size;
        } else if wire(region) {
            report.wired.add(region);
            report.resident += region.size;
        } else {
            report.refused.add(region);
        }
    }
    report
}

#[cfg(test)]
#[path = "memory_wiring_pass_tests.rs"]
mod memory_wiring_pass_tests;
