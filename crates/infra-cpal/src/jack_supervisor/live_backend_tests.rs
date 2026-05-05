use super::*;


// These tests exercise only the pure helpers — no real jackd is launched.
// Integration tests that actually spawn `jackd` are marked `#[ignore]`
// and run under `cargo test -- --ignored`.

#[test]
fn stderr_log_path_is_scoped_per_server_name() {
    let a = LiveJackBackend::stderr_log_path(&ServerName::from("a"));
    let b = LiveJackBackend::stderr_log_path(&ServerName::from("b"));
    assert_ne!(a, b);
    assert!(a.to_string_lossy().contains("/tmp/jackd-a-"));
}

#[test]
fn stderr_driver_failure_detected_for_known_markers() {
    let tmp = std::env::temp_dir().join("openrig-jack-test-failure.log");
    std::fs::write(&tmp, "xrun\nALSA: could not start playback (Broken pipe)\n").unwrap();
    let marker = LiveJackBackend::stderr_has_driver_failure(&tmp);
    assert_eq!(marker.as_deref(), Some("Broken pipe"));
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn stderr_driver_failure_absent_for_benign_content() {
    let tmp = std::env::temp_dir().join("openrig-jack-test-benign.log");
    std::fs::write(&tmp, "JackMessageBuffer:: nothing wrong here\n").unwrap();
    let marker = LiveJackBackend::stderr_has_driver_failure(&tmp);
    assert!(marker.is_none());
    let _ = std::fs::remove_file(&tmp);
}

// Integration test — starts a real jackd if one is available on the
// system. Skipped by default. Run with:
//   cargo test -p infra-cpal --features jack -- --ignored live_backend
#[test]
#[ignore]
fn live_backend_cold_start_and_shutdown_against_real_jackd() {
    let mut backend = LiveJackBackend::new();
    let name = ServerName::from("openrig-test");
    // Use a harmless card id 99 — this test assumes no such card exists
    // so jackd will fail. The point is to exercise the error path fully.
    let config = JackConfig {
        sample_rate: 48_000,
        buffer_size: 128,
        nperiods: 3,
        realtime: false,
        rt_priority: 70,
        card_num: 99,
        capture_channels: 2,
        playback_channels: 2,
    };
    let err = backend.spawn(&name, &config).unwrap_err();
    assert!(err.to_string().contains(name.as_str()));
    backend.forget(&name);
}
