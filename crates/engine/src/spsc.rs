//! Single-producer / single-consumer lock-free ring buffer for audio frames.
//!
//! Used by the elastic buffer between the input DSP thread (producer) and the
//! output callback thread (consumer). Both sides operate with `&self` via
//! atomics and `UnsafeCell`, avoiding the priority-inversion risk of a
//! `Mutex` in the real-time audio path.
//!
//! Capacity is a power of two to allow cheap `head & mask` addressing. Only
//! one producer and one consumer thread may call `push` / `pop` respectively;
//! calling from other combinations is undefined behaviour.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct SpscRing<T: Copy> {
    slots: Box<[UnsafeCell<T>]>,
    mask: usize,
    capacity: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}

unsafe impl<T: Copy + Send> Sync for SpscRing<T> {}

impl<T: Copy> SpscRing<T> {
    pub fn new(capacity: usize, init: T) -> Self {
        let capacity = capacity.next_power_of_two().max(2);
        let slots: Box<[UnsafeCell<T>]> = (0..capacity).map(|_| UnsafeCell::new(init)).collect();
        Self {
            slots,
            mask: capacity - 1,
            capacity,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    #[allow(dead_code)]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        head.wrapping_sub(tail)
    }

    /// Push a value. Returns `false` if the ring was full and the value was
    /// dropped. Must only be called from the single producer thread.
    pub fn push(&self, value: T) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= self.capacity {
            return false;
        }
        unsafe {
            *self.slots[head & self.mask].get() = value;
        }
        self.head.store(head.wrapping_add(1), Ordering::Release);
        true
    }

    /// Pop a value. Returns `None` if the ring was empty. Must only be called
    /// from the single consumer thread.
    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail == head {
            return None;
        }
        let value = unsafe { *self.slots[tail & self.mask].get() };
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Some(value)
    }
}

#[cfg(test)]
#[path = "spsc_tests.rs"]
mod tests;
