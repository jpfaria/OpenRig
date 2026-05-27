//! Red-first (issue #548 rescan): the daemon must rescan MIDI inputs
//! only when the GUI's "refresh" button asks for it — not on a timer.
//! `request_rescan()` is the public signal entry point the refresh
//! button call site uses.

#[test]
fn request_rescan_callable_when_daemon_not_started() {
    // No daemon → no Sender registered → no panic, no error.
    adapter_midi::request_rescan();
}

#[test]
fn request_rescan_signature_exists() {
    let _f: fn() = adapter_midi::request_rescan;
}
