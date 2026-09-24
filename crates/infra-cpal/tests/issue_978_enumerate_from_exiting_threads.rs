//! Issue #978: on Windows, every infra-cpal test binary that touched WASAPI
//! twice died with STATUS_ACCESS_VIOLATION. In each case the first test to
//! enumerate passed and the next one crashed, serially too.
//!
//! cpal creates its `IMMDeviceEnumerator` once, in a single-threaded COM
//! apartment on whichever thread first asks, and caches it for the process.
//! When that thread exits, its COM usage ends; if it was the last one, COM
//! shuts down in the process and can unload the enumerator's DLL, so the next
//! enumeration calls into freed code. The app does this too: device lists
//! refresh on short-lived `device-enum-*` threads, and the console and render
//! binaries have no long-lived COM thread at all.
#![cfg(target_os = "windows")]

#[test]
fn device_lists_survive_the_threads_that_asked_for_them_exiting() {
    for round in 0..3 {
        std::thread::spawn(|| {
            let _ = infra_cpal::list_devices();
        })
        .join()
        .unwrap_or_else(|_| panic!("round {round}: the enumeration thread panicked"));
    }
}
