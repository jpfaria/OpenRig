//! Responsibility: finds which wired memory the audio no longer holds.
//!
//! A wire outlives the free: pages the audio let go stay wired, and the
//! allocator hands them to the rest of the app, so the kernel can never take
//! them back. Each pass therefore compares what it wired before with the
//! audio's spans now; whatever falls outside them is unwired.

/// The wired ranges as `(start, size)`, split against the audio's spans.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Split {
    /// Still under the audio's memory: stays wired.
    pub(crate) held: Vec<(u64, u64)>,
    /// No longer the audio's: to unwire.
    pub(crate) released: Vec<(u64, u64)>,
}

/// Splits `wired` against `spans` (both `(start, size)`, any order); both
/// halves come out in address order.
pub(crate) fn split_wired(wired: &[(u64, u64)], spans: &[(u64, u64)]) -> Split {
    let spans = merged(spans);
    let mut wired = wired.to_vec();
    wired.sort_unstable();
    let mut split = Split::default();
    for (start, size) in wired {
        let end = start + size;
        let mut at = start;
        for &(span_start, span_end) in &spans {
            if span_end <= at {
                continue;
            }
            if span_start >= end {
                break;
            }
            push(&mut split.released, at, span_start.max(at));
            let held_end = span_end.min(end);
            push(&mut split.held, span_start.max(at), held_end);
            at = held_end;
        }
        push(&mut split.released, at, end);
    }
    split
}

/// `spans` as sorted, non-overlapping `[start, end)` intervals.
fn merged(spans: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let mut intervals: Vec<(u64, u64)> = spans
        .iter()
        .map(|&(start, size)| (start, start + size))
        .collect();
    intervals.sort_unstable();
    let mut out: Vec<(u64, u64)> = Vec::with_capacity(intervals.len());
    for (start, end) in intervals {
        match out.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => out.push((start, end)),
        }
    }
    out
}

/// Appends `[from, to)` as `(start, size)` when it is not empty.
fn push(ranges: &mut Vec<(u64, u64)>, from: u64, to: u64) {
    if to > from {
        ranges.push((from, to - from));
    }
}

#[cfg(test)]
#[path = "memory_wiring_release_tests.rs"]
mod memory_wiring_release_tests;
