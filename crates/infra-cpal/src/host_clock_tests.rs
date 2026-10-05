//! The clock every loop shares runs forward in real nanoseconds.

use super::*;

#[test]
fn the_host_clock_runs_forward_in_nanoseconds() {
    let a = now_ns();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let b = now_ns();
    assert!(a > 0, "the clock counts from boot, never from zero");
    assert!(b >= a + 15_000_000, "20 ms of sleep reads as {} ns", b - a);
    assert!(b < a + 2_000_000_000, "and not as seconds");
}
