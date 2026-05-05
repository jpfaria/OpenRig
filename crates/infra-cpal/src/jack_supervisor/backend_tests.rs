use super::*;


fn name() -> ServerName {
    ServerName::from("test")
}

#[test]
fn mock_backend_default_spawn_makes_server_probeable() {
    let mut backend = MockBackend::new();
    backend.spawn(&name(), &JackConfig::test_default()).unwrap();
    assert!(backend.is_socket_present(&name()));
    let meta = backend.probe_meta(&name()).unwrap();
    assert_eq!(meta.sample_rate, 48_000);
    assert_eq!(meta.buffer_size, 128);
}

#[test]
fn mock_backend_spawn_error_leaves_server_not_running() {
    let mut backend = MockBackend::new();
    backend.queue_spawn_result(Err("simulated".into()));
    let err = backend
        .spawn(&name(), &JackConfig::test_default())
        .unwrap_err();
    assert!(err.to_string().contains("simulated"));
    assert!(!backend.is_socket_present(&name()));
}

#[test]
fn mock_backend_post_ready_failure_clears_running() {
    let mut backend = MockBackend::new();
    backend.queue_post_ready(
        &name(),
        PostReadyStatus::DriverFailure("Broken pipe".into()),
    );
    backend.spawn(&name(), &JackConfig::test_default()).unwrap();
    assert!(backend.is_socket_present(&name()));
    let status = backend.post_ready_status(&name());
    assert!(matches!(status, PostReadyStatus::DriverFailure(_)));
    assert!(!backend.is_socket_present(&name()));
}

#[test]
fn mock_backend_records_call_order() {
    let mut backend = MockBackend::new();
    backend.spawn(&name(), &JackConfig::test_default()).unwrap();
    backend.post_ready_status(&name());
    let _ = backend.probe_meta(&name());
    backend.terminate(&name()).unwrap();
    backend.forget(&name());
    let calls = backend.calls();
    assert!(matches!(calls[0], MockCall::Spawn(_, _)));
    assert!(matches!(calls[1], MockCall::PostReadyStatus(_)));
    assert!(matches!(calls[2], MockCall::ProbeMeta(_)));
    assert!(matches!(calls[3], MockCall::Terminate(_)));
    assert!(matches!(calls[4], MockCall::Forget(_)));
}

#[test]
fn mock_backend_forget_clears_running_and_scripts() {
    let mut backend = MockBackend::new();
    backend.spawn(&name(), &JackConfig::test_default()).unwrap();
    backend.queue_post_ready(&name(), PostReadyStatus::SocketVanished);
    backend.forget(&name());
    assert!(!backend.is_socket_present(&name()));
    assert!(backend
        .inner
        .lock()
        .unwrap()
        .post_ready_script
        .get(&name())
        .is_none());
}

#[test]
fn mock_backend_probe_script_overrides_default_meta() {
    let mut backend = MockBackend::new();
    backend.spawn(&name(), &JackConfig::test_default()).unwrap();
    backend.queue_probe_result(&name(), Err("simulated probe failure".into()));
    let err = backend.probe_meta(&name()).unwrap_err();
    assert!(err.to_string().contains("simulated"));
}
