use super::*;

fn server(name: &str) -> ServerName {
    ServerName::from(name)
}

#[test]
fn log_path_is_scoped_per_server_name() {
    let a = stderr_log_path(&server("a"));
    let b = stderr_log_path(&server("b"));
    assert_ne!(a, b);
    assert!(a.to_string_lossy().contains("/tmp/jackd-a-"));
}

#[test]
fn recognises_a_broken_pipe() {
    let log = "xrun\nALSA: could not start playback (Broken pipe)\n";
    assert_eq!(driver_failure_in(log).as_deref(), Some("Broken pipe"));
}

#[test]
fn recognises_a_driver_that_would_not_start() {
    assert_eq!(
        driver_failure_in("Cannot start driver\n").as_deref(),
        Some("Cannot start driver")
    );
}

#[test]
fn recognises_a_server_that_would_not_start() {
    assert_eq!(
        driver_failure_in("Failed to start server\n").as_deref(),
        Some("Failed to start server")
    );
}

#[test]
fn benign_chatter_is_not_a_driver_failure() {
    // jackd is noisy on a healthy start; only the known markers may promote a
    // log to "the driver refused", otherwise every boot looks like a failure.
    let log = "JackMessageBuffer:: nothing wrong here\njackdmp comming from...\n";
    assert!(driver_failure_in(log).is_none());
}

#[test]
fn an_empty_log_is_not_a_driver_failure() {
    assert!(driver_failure_in("").is_none());
}

#[test]
fn a_missing_log_reads_as_empty_rather_than_failing() {
    // The log is absent on the very first spawn; that must not be reported as
    // a driver failure.
    let missing = std::env::temp_dir().join("openrig-jack-no-such-log-873.log");
    let _ = std::fs::remove_file(&missing);
    assert_eq!(read_stderr_snippet(&missing), "");
    assert!(stderr_has_driver_failure(&missing).is_none());
}
