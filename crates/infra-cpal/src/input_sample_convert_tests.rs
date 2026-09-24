//! Invariant #8 for the integer input arms (#978): the callback must not
//! allocate. Int32 is the format most ASIO drivers deliver, so on Windows this
//! is the normal path, not an edge case.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use super::{i16_to_f32, i32_to_f32, u16_to_f32, InputSampleBuffer};

// Guard and count are both per thread: parallel tests can neither hide nor
// inherit each other's allocations.
thread_local! {
    static ALLOC_GUARD: Cell<bool> = const { Cell::new(false) };
    static ALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
}

fn count_if_guarded() {
    if ALLOC_GUARD.with(|g| g.get()) {
        ALLOC_COUNT.with(|c| c.set(c.get() + 1));
    }
}

/// Counts `alloc`/`realloc` on the thread that set the guard; a plain
/// pass-through everywhere else, so the rest of the suite is unaffected.
struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_if_guarded();
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_if_guarded();
        System.alloc_zeroed(layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count_if_guarded();
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn allocs_during(f: impl FnOnce()) -> usize {
    ALLOC_COUNT.with(|c| c.set(0));
    ALLOC_GUARD.with(|g| g.set(true));
    f();
    ALLOC_GUARD.with(|g| g.set(false));
    ALLOC_COUNT.with(|c| c.get())
}

const FRAMES: usize = 256;
const CHANNELS: usize = 2;

#[test]
fn first_callback_of_a_stream_does_not_allocate() {
    let mut buffer = InputSampleBuffer::with_capacity(FRAMES * CHANNELS);
    let data = vec![i32::MAX / 2; FRAMES * CHANNELS];
    let allocs = allocs_during(|| {
        buffer.convert(&data, i32_to_f32);
    });
    assert_eq!(
        allocs, 0,
        "the conversion buffer must be sized at build time"
    );
}

#[test]
fn shorter_then_full_callbacks_do_not_allocate() {
    // WASAPI shared mode can hand over fewer frames than the period.
    let mut buffer = InputSampleBuffer::with_capacity(FRAMES * CHANNELS);
    let short = vec![0i16; FRAMES];
    let full = vec![0i16; FRAMES * CHANNELS];
    let allocs = allocs_during(|| {
        buffer.convert(&short, i16_to_f32);
        buffer.convert(&full, i16_to_f32);
    });
    assert_eq!(allocs, 0);
}

#[test]
fn converted_samples_keep_the_existing_scaling() {
    // The formulas the stream builder used inline, unchanged.
    let mut buffer = InputSampleBuffer::with_capacity(4);
    assert_eq!(
        buffer.convert(&[i32::MAX, 0, i32::MAX / 2], i32_to_f32),
        &[1.0, 0.0, (i32::MAX / 2) as f32 / i32::MAX as f32]
    );
    assert_eq!(
        buffer.convert(&[i16::MAX, 0, -i16::MAX], i16_to_f32),
        &[1.0, 0.0, -1.0]
    );
    assert_eq!(buffer.convert(&[u16::MAX, 0], u16_to_f32), &[1.0, -1.0]);
}

#[test]
fn converted_length_follows_the_callback() {
    let mut buffer = InputSampleBuffer::with_capacity(8);
    assert_eq!(buffer.convert(&[0i32; 6], i32_to_f32).len(), 6);
    assert_eq!(buffer.convert(&[0i32; 2], i32_to_f32).len(), 2);
}

#[test]
fn the_counter_sees_an_allocation_on_its_own_thread() {
    // Self-check: a counter that never counts would make every test above pass.
    let allocs = allocs_during(|| {
        std::hint::black_box(Vec::<u8>::with_capacity(64));
    });
    assert_eq!(allocs, 1);
}
