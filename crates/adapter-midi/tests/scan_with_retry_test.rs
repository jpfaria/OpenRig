//! Red-first: the daemon's rescan function must keep retrying after
//! MIDIRestart until either (a) it sees a new port or (b) the retry
//! budget runs out. CoreMIDI surfaces BLE-MIDI ports asynchronously
//! after a restart; a single sleep + enumerate is not enough.
//!
//! The actual hot-plug behaviour can't be unit-tested (no real device),
//! but the function signature contract can: takes a previous port list
//! and the retry budget, returns the union of every snapshot it saw.

use adapter_midi::daemon::scan_with_retry;
use std::time::Duration;

#[test]
fn scan_with_retry_signature_exists() {
    let _f: fn(&[String], usize, Duration) -> Vec<String> = scan_with_retry;
}

#[test]
fn scan_with_retry_zero_budget_returns_single_snapshot() {
    // 0 retries → one enumerate pass, no MIDIRestart, no sleep. Used
    // by the initial boot scan that doesn't need the BLE workaround.
    let result = scan_with_retry(&[], 0, Duration::from_millis(0));
    // We can't assert content (depends on host), only that it doesn't
    // panic and returns *something*.
    let _ = result;
}
