//! Lookahead delay line with O(1) amortized sliding-window peak tracking.
//!
//! The buffer delays the input signal by `len` samples. In parallel, a
//! monotonically-decreasing deque tracks the maximum absolute value over the
//! same window, so gain reduction can be applied *before* the peak exits the
//! delay line. This is what makes the limiter actually "brick wall" — without
//! it, the first samples of a transient pass through above the ceiling.

use std::collections::VecDeque;

#[derive(Debug)]
pub struct LookaheadBuffer {
    buffer: Vec<f32>,
    write: usize,
    len: usize,
    peak_deque: VecDeque<(usize, f32)>,
    index: usize,
}

impl LookaheadBuffer {
    pub fn new(len: usize) -> Self {
        let len = len.max(1);
        Self {
            buffer: vec![0.0; len],
            write: 0,
            len,
            peak_deque: VecDeque::with_capacity(len),
            index: 0,
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[cfg(test)]
    pub fn clear(&mut self) {
        self.buffer.iter_mut().for_each(|s| *s = 0.0);
        self.peak_deque.clear();
        self.write = 0;
        self.index = 0;
    }

    /// Push `input`, return the delayed sample that falls out of the window.
    pub fn push(&mut self, input: f32) -> f32 {
        let read = self.write;
        let delayed = self.buffer[read];
        self.buffer[read] = input;
        self.write = (self.write + 1) % self.len;

        let abs = input.abs();
        // Expire entries that have aged out of the window. Skip while the
        // counter hasn't reached `len` yet — underflow would otherwise pop
        // still-valid entries on the first few pushes.
        if self.index >= self.len {
            let expire_index = self.index - self.len;
            while let Some(&(idx, _)) = self.peak_deque.front() {
                if idx <= expire_index {
                    self.peak_deque.pop_front();
                } else {
                    break;
                }
            }
        }
        while let Some(&(_, val)) = self.peak_deque.back() {
            if val <= abs {
                self.peak_deque.pop_back();
            } else {
                break;
            }
        }
        self.peak_deque.push_back((self.index, abs));
        self.index = self.index.wrapping_add(1);

        delayed
    }

    /// Peak magnitude over the current window.
    pub fn peak(&self) -> f32 {
        self.peak_deque.front().map(|&(_, v)| v).unwrap_or(0.0)
    }
}

#[cfg(test)]
#[path = "lookahead_tests.rs"]
mod tests;
