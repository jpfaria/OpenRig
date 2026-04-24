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
mod tests {
    use super::*;

    #[test]
    fn new_buffer_starts_empty() {
        let buf = LookaheadBuffer::new(8);
        assert_eq!(buf.len(), 8);
        assert_eq!(buf.peak(), 0.0);
    }

    #[test]
    fn push_delays_by_len_samples() {
        let mut buf = LookaheadBuffer::new(4);
        // First 4 pushes return the initial zeros
        for _ in 0..4 {
            assert_eq!(buf.push(1.0), 0.0);
        }
        // Subsequent pushes return the 1.0 we pushed earlier
        assert_eq!(buf.push(2.0), 1.0);
        assert_eq!(buf.push(2.0), 1.0);
    }

    #[test]
    fn peak_reports_max_abs_over_window() {
        let mut buf = LookaheadBuffer::new(3);
        buf.push(0.2);
        assert_eq!(buf.peak(), 0.2);
        buf.push(-0.8);
        assert_eq!(buf.peak(), 0.8);
        buf.push(0.5);
        assert_eq!(buf.peak(), 0.8);
        // 0.2 falls out of the window; 0.8 still in
        buf.push(0.1);
        assert_eq!(buf.peak(), 0.8);
        // 0.8 falls out; remaining are 0.5, 0.1, and new value
        buf.push(0.3);
        assert_eq!(buf.peak(), 0.5);
    }

    #[test]
    fn clear_resets_state() {
        let mut buf = LookaheadBuffer::new(4);
        buf.push(1.0);
        buf.push(0.5);
        buf.clear();
        assert_eq!(buf.peak(), 0.0);
        assert_eq!(buf.push(0.1), 0.0);
    }

    #[test]
    fn zero_len_is_coerced_to_one() {
        let mut buf = LookaheadBuffer::new(0);
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.push(0.7), 0.0);
        assert_eq!(buf.push(0.2), 0.7);
    }
}
