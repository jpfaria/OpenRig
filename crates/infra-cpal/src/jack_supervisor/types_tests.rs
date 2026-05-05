use super::*;


#[test]
fn server_name_roundtrips_through_string_and_ref() {
    let a = ServerName::from("card1");
    let b = ServerName::new(String::from("card1"));
    let c: ServerName = String::from("card1").into();
    assert_eq!(a, b);
    assert_eq!(a, c);
    assert_eq!(a.as_str(), "card1");
    assert_eq!(format!("{}", a), "card1");
}

#[test]
fn jack_server_state_ready_and_terminal_match_variant() {
    let meta = JackMeta {
        sample_rate: 48_000,
        buffer_size: 128,
        capture_port_count: 2,
        playback_port_count: 2,
        hw_name: "hw".into(),
    };
    let ready = JackServerState::Ready {
        meta,
        launched_config: JackConfig::test_default(),
        ready_at: Instant::now(),
    };
    assert!(ready.is_ready());
    assert!(!ready.is_terminal());
    assert!(ready.launched_config().is_some());

    let not_started = JackServerState::NotStarted;
    assert!(!not_started.is_ready());
    assert!(not_started.is_terminal());
    assert!(not_started.launched_config().is_none());

    let failed = JackServerState::Failed {
        last_error: "boom".into(),
        attempts: 3,
    };
    assert!(!failed.is_ready());
    assert!(failed.is_terminal());
}

#[test]
fn restart_reason_config_mismatch_carries_both_configs() {
    let old = JackConfig::test_default();
    let new = JackConfig {
        buffer_size: 256,
        ..JackConfig::test_default()
    };
    let reason = RestartReason::ConfigMismatch {
        old: old.clone(),
        new: new.clone(),
    };
    match reason {
        RestartReason::ConfigMismatch { old: o, new: n } => {
            assert_eq!(o.buffer_size, 128);
            assert_eq!(n.buffer_size, 256);
        }
        other => panic!("unexpected variant: {:?}", other),
    }
}
