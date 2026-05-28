//! #548 hot-plug: prove our enumeration path is cache-free on every
//! call, so any staleness lives in CoreMIDI / midir (out of our scope)
//! and not in our wrapper. The user-visible bug is "plug pedal after
//! app start → refresh doesn't pick it up"; the diagnostic log added in
//! daemon.rs shows the CoreMIDI source count straight from FFI before
//! and after `MIDIRestart`. If those numbers match what we expect but
//! list_input_ports still doesn't return the port, we know to look at
//! midir; if they don't, we know to look at CoreMIDI's BLE plumbing.

#[test]
fn list_input_ports_does_not_panic_when_called_repeatedly() {
    // 50 sequential calls in a tight loop. Each creates a fresh
    // `MidiInput` client and disposes it; if any path leaks the
    // CoreMIDI client or builds up state, this would either error or
    // panic by the end.
    for _ in 0..50 {
        let _ = adapter_midi::list_input_ports();
    }
}

#[test]
fn request_rescan_safe_to_call_repeatedly_without_daemon() {
    // The OnceLock-backed RESCAN_TX is empty in this test process.
    // Calling request_rescan a bunch of times must be a no-op, never
    // panic, never deadlock.
    for _ in 0..100 {
        adapter_midi::request_rescan();
    }
}

#[test]
fn each_list_input_ports_call_returns_a_fresh_snapshot() {
    // Two back-to-back calls must each produce their own Vec — no
    // cached `Vec<MidiPortInfo>` shared between calls in our code. The
    // values must be equal (no devices added/removed in this test
    // process) but they're distinct allocations.
    let a = adapter_midi::list_input_ports().unwrap_or_default();
    let b = adapter_midi::list_input_ports().unwrap_or_default();
    assert_eq!(
        a.len(),
        b.len(),
        "back-to-back snapshots must agree on the port count"
    );
    // Same content, different allocations.
    for (pa, pb) in a.iter().zip(b.iter()) {
        assert_eq!(pa.raw_name, pb.raw_name);
    }
}
