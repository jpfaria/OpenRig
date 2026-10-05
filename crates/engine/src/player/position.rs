//! Responsibility: maps frames the output has consumed back to the track second being heard.
//!
//! The worker renders ahead of the listener by whatever sits in the ring. It
//! marks each block it pushes with the track position at the block's end; the
//! callback's consumed-frame counter then says which mark is playing now.

use std::collections::VecDeque;

/// Marks a worker keeps before the oldest is dropped; far more than a ring's
/// worth of render blocks.
const MAX_MARKS: usize = 256;

#[derive(Debug)]
pub struct HeardPosition {
    marks: VecDeque<(u64, f64)>,
    pushed: u64,
    heard: f64,
}

impl Default for HeardPosition {
    fn default() -> Self {
        Self {
            marks: VecDeque::with_capacity(MAX_MARKS),
            pushed: 0,
            heard: 0.0,
        }
    }
}

impl HeardPosition {
    /// Starts over at `seconds` once the ring is empty and `consumed` frames
    /// have left it in total.
    pub fn reset(&mut self, seconds: f64, consumed: u64) {
        self.marks.clear();
        self.pushed = consumed;
        self.heard = seconds;
    }

    /// Records a pushed block of `frames` that ends at `seconds` of the track.
    pub fn pushed(&mut self, frames: usize, seconds: f64) {
        self.pushed += frames as u64;
        if self.marks.len() == MAX_MARKS {
            self.marks.pop_front();
        }
        self.marks.push_back((self.pushed, seconds));
    }

    /// The track second being heard once `consumed` frames have left the ring.
    pub fn heard(&mut self, consumed: u64) -> f64 {
        while let Some(&(total, seconds)) = self.marks.front() {
            if total > consumed {
                break;
            }
            self.heard = seconds;
            self.marks.pop_front();
        }
        self.heard
    }

    /// Frames pushed that the output has not taken yet.
    pub fn queued(&self, consumed: u64) -> u64 {
        self.pushed.saturating_sub(consumed)
    }
}

#[cfg(test)]
#[path = "position_tests.rs"]
mod tests;
