//! Issue #980 — memory allocated while OpenRig runs (a rebuilt chain, a
//! plugin enabled live) must become resident too, not only what existed when
//! the audio started.

use std::time::{Duration, Instant};

use super::keep_resident;
use crate::memory_wiring::memory_wiring_tests::{touched_buffer, user_wired_count};

#[test]
fn the_keeper_wires_memory_allocated_after_it_started() {
    keep_resident();
    // Past the keeper's first pass: only a later one can wire this buffer.
    std::thread::sleep(Duration::from_secs(1));
    let buffer = touched_buffer();
    let deadline = Instant::now() + Duration::from_secs(15);
    while user_wired_count(buffer.as_ptr() as *const u8) == 0 {
        assert!(
            Instant::now() < deadline,
            "a buffer allocated after the keeper started (a rebuilt chain, a \
             new plugin) must be wired by its next pass"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
