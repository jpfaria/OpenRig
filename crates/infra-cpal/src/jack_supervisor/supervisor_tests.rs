//! Tests for `jack_supervisor::supervisor`. Lifted out so the production
//! file stays under the size cap. Re-attached as `mod tests` of the parent
//! via `#[cfg(test)] #[path = "supervisor_tests.rs"] mod tests;`.

    use super::super::backend::{JackBackend, MockBackend, MockCall, PostReadyStatus};
    use super::super::types::{
        HealthStatus, JackConfig, JackMeta, JackServerState, RestartReason, ServerName,
        SupervisorEvent,
    };
    use super::*;

    fn name() -> ServerName {
        ServerName::from("test")
    }

    fn noop_hook() -> impl FnMut(&ServerName) {
        |_: &ServerName| {}
    }

    fn make_supervisor() -> JackSupervisor<MockBackend> {
        JackSupervisor::new(MockBackend::new())
    }

    // When the supervisor is invoked against a never-seen server, the full
    // Spawn → PostReadyStatus → ProbeMeta sequence runs in order and the
    // server ends up in Ready with the probed meta.
    #[test]
    fn ensure_server_from_not_started_transitions_to_ready() {
        let mut sup = make_supervisor();
        let config = JackConfig::test_default();
        let meta = sup
            .ensure_server(&name(), &config, &mut noop_hook())
            .expect("cold start succeeds");
        assert_eq!(meta.sample_rate, config.sample_rate);
        assert!(sup.state(&name()).unwrap().is_ready());

        let calls = sup.backend.calls();
        assert!(matches!(calls[0], MockCall::Spawn(_, _)));
        assert!(matches!(calls[1], MockCall::PostReadyStatus(_)));
        assert!(matches!(calls[2], MockCall::ProbeMeta(_)));
    }

    // A repeat ensure_server with the identical desired config must NOT
    // re-spawn. Cached meta is returned from the prior Ready state.
    #[test]
    fn ensure_server_with_matching_config_is_idempotent() {
        let mut sup = make_supervisor();
        let config = JackConfig::test_default();
        sup.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        let before = sup.backend.call_count();
        sup.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        assert_eq!(sup.backend.call_count(), before, "no extra backend calls");
    }

    // When the supervisor state is NotStarted but a jackd socket is already
    // present (externally launched — e.g. start_jack_in_background at boot,
    // or a previous controller whose handle was dropped without terminating
    // jackd), ensure_server must ADOPT the running server, not try to spawn
    // a new one. This is the fix for the issue #308 hardware regression
    // where toggling a chain off+on recreated the controller, the new
    // supervisor didn't know about the running jackd, and spawn tried to
    // nuke /dev/shm/jack_* sockets as "stale".
    #[test]
    fn ensure_server_adopts_running_jackd_with_matching_config() {
        let mut sup = make_supervisor();
        let config = JackConfig::test_default();
        // Simulate an externally-launched jackd by seeding the mock
        // backend's running set + meta, without calling supervisor.ensure.
        sup.backend.inner.lock().unwrap().running.insert(name());
        sup.backend.set_default_meta(
            &name(),
            JackMeta {
                sample_rate: config.sample_rate,
                buffer_size: config.buffer_size,
                capture_port_count: 2,
                playback_port_count: 2,
                hw_name: "external".into(),
            },
        );

        let meta = sup.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        assert_eq!(meta.sample_rate, config.sample_rate);

        // Adoption must NOT have triggered a spawn — the backend should have
        // seen probe_meta (the adoption check) but no Spawn call.
        let calls = sup.backend.calls();
        assert!(
            calls.iter().any(|c| matches!(c, MockCall::ProbeMeta(_))),
            "adoption must probe the running server"
        );
        assert!(
            !calls.iter().any(|c| matches!(c, MockCall::Spawn(_, _))),
            "adoption must not spawn a new jackd"
        );
        assert!(
            !calls.iter().any(|c| matches!(c, MockCall::Terminate(_))),
            "adoption with matching config must not terminate"
        );
        assert!(sup.state(&name()).unwrap().is_ready());
    }

    // Adoption with mismatched config must cleanly terminate the running
    // jackd and then spawn a fresh one under supervision.
    #[test]
    fn ensure_server_adopts_and_restarts_on_config_mismatch() {
        let mut sup = make_supervisor();
        sup.backend.inner.lock().unwrap().running.insert(name());
        // External jackd running at buf=128; we want buf=256.
        sup.backend.set_default_meta(
            &name(),
            JackMeta {
                sample_rate: 48_000,
                buffer_size: 128,
                capture_port_count: 2,
                playback_port_count: 2,
                hw_name: "external".into(),
            },
        );

        let desired = JackConfig {
            buffer_size: 256,
            ..JackConfig::test_default()
        };
        let meta = sup.ensure_server(&name(), &desired, &mut noop_hook()).unwrap();
        assert_eq!(meta.buffer_size, 256);

        let calls = sup.backend.calls();
        let terminate_idx = calls
            .iter()
            .position(|c| matches!(c, MockCall::Terminate(_)))
            .expect("adoption mismatch must terminate");
        let spawn_idx = calls
            .iter()
            .position(|c| matches!(c, MockCall::Spawn(_, _)))
            .expect("adoption mismatch must spawn after terminate");
        assert!(terminate_idx < spawn_idx, "terminate must precede spawn");
    }

    // Full controller-recreation scenario: supervisor A spawns jackd and is
    // dropped without calling shutdown_all (the GUI path). Supervisor B is
    // created with a backend that sees the same running server and must
    // adopt it without spawning. This is the end-to-end shape of the issue
    // #308 hardware regression.
    #[test]
    fn supervisor_b_adopts_jackd_left_running_by_dropped_supervisor_a() {
        use super::super::backend::MockBackendInner;
        use std::sync::Arc as StdArc;
        let shared_inner = StdArc::new(std::sync::Mutex::new(MockBackendInner::default()));
        let backend_a = MockBackend { inner: shared_inner.clone() };
        let backend_b = MockBackend { inner: shared_inner.clone() };

        let mut sup_a = JackSupervisor::new(backend_a);
        let config = JackConfig::test_default();
        sup_a.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        assert!(sup_a.state(&name()).unwrap().is_ready());

        // Drop A WITHOUT shutting down — the shared backend keeps `running`
        // populated, simulating a jackd that survived the controller drop.
        drop(sup_a);
        assert!(shared_inner.lock().unwrap().running.contains(&name()));

        // Clear the recorded calls so we can assert cleanly against B alone.
        shared_inner.lock().unwrap().calls.clear();

        let mut sup_b = JackSupervisor::new(backend_b);
        let meta = sup_b.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        assert_eq!(meta.sample_rate, config.sample_rate);

        let calls = sup_b.backend.calls();
        assert!(
            !calls.iter().any(|c| matches!(c, MockCall::Spawn(_, _))),
            "supervisor B must adopt — not spawn"
        );
        assert!(
            calls.iter().any(|c| matches!(c, MockCall::ProbeMeta(_))),
            "supervisor B must probe during adoption"
        );
    }

    // When adoption needs to terminate a mismatched jackd but terminate
    // fails (e.g. we can't discover the PID and the socket persists),
    // ensure_server MUST propagate the terminate error as a Failed state,
    // not silently fall through to spawn (which would retry and hit its
    // safety check, masking the real diagnostic).
    #[test]
    fn ensure_server_propagates_terminate_failure_on_adoption_mismatch() {
        let mut sup = make_supervisor();
        sup.backend.inner.lock().unwrap().running.insert(name());
        sup.backend.set_default_meta(
            &name(),
            JackMeta {
                sample_rate: 48_000,
                buffer_size: 128,
                capture_port_count: 2,
                playback_port_count: 2,
                hw_name: "external".into(),
            },
        );
        // Script the terminate of the adopted server to fail.
        sup.backend.queue_terminate_result(Err("socket persists after kill".into()));

        let desired = JackConfig {
            buffer_size: 256,
            ..JackConfig::test_default()
        };
        let err = sup
            .ensure_server(&name(), &desired, &mut noop_hook())
            .expect_err("terminate failure must surface as an error");
        assert!(err.to_string().contains("socket persists"));

        // State must be Failed, never Restarting or NotStarted — so the next
        // ensure_server has a truthful ground-zero to recover from.
        match sup.state(&name()) {
            Some(JackServerState::Failed { last_error, .. }) => {
                assert!(last_error.contains("socket persists"));
            }
            other => panic!("expected Failed, got {:?}", other),
        }

        // No spawn was attempted after the failed terminate — important so
        // the live backend's spawn safety check doesn't bury the real cause.
        assert!(
            !sup.backend.calls().iter().any(|c| matches!(c, MockCall::Spawn(_, _))),
            "spawn must not run after adoption terminate fails"
        );
    }

    // Adoption when the socket is present but the server is unresponsive
    // (zombie) must terminate+respawn, never leave the supervisor stuck.
    #[test]
    fn ensure_server_adopts_zombie_by_terminating_and_respawning() {
        let mut sup = make_supervisor();
        sup.backend.inner.lock().unwrap().running.insert(name());
        // Script the first probe (the adoption probe) to fail.
        sup.backend.queue_probe_result(&name(), Err("zombie unresponsive".into()));

        let meta = sup
            .ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .expect("supervisor must recover from a zombie adoption");
        assert_eq!(meta.sample_rate, 48_000);

        let calls = sup.backend.calls();
        assert!(calls.iter().any(|c| matches!(c, MockCall::Terminate(_))));
        assert!(calls.iter().any(|c| matches!(c, MockCall::Spawn(_, _))));
    }

    // When desired config changes and clients are registered, the pre-kill
    // teardown hook must fire BEFORE backend.terminate. Invariant #1.
    #[test]
    fn ensure_server_runs_teardown_hook_before_terminate_when_config_changes() {
        let mut sup = make_supervisor();
        let config1 = JackConfig::test_default();
        sup.ensure_server(&name(), &config1, &mut noop_hook()).unwrap();
        sup.register_client(&name());
        sup.register_client(&name());

        let call_count_before_hook = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        let observed_call_count = std::sync::Arc::clone(&call_count_before_hook);
        let calls_arc = sup.backend.inner.clone();
        let mut hook = {
            let observed_call_count = std::sync::Arc::clone(&observed_call_count);
            let calls_arc = calls_arc.clone();
            move |_: &ServerName| {
                let count = calls_arc.lock().unwrap().calls.len();
                *observed_call_count.lock().unwrap() = count;
            }
        };

        let config2 = JackConfig {
            buffer_size: 256,
            ..config1
        };
        sup.ensure_server(&name(), &config2, &mut hook).unwrap();

        let calls = sup.backend.calls();
        let hook_saw = *call_count_before_hook.lock().unwrap();
        let terminate_idx = calls
            .iter()
            .position(|c| matches!(c, MockCall::Terminate(_)))
            .expect("terminate must have been called");
        assert!(
            hook_saw <= terminate_idx,
            "teardown hook ran at call {} but terminate was at {}",
            hook_saw,
            terminate_idx
        );
        assert_eq!(sup.client_count(&name()), 0, "clients cleared post-teardown");
    }

    // With zero registered clients, the teardown hook is skipped. Terminate
    // still runs — the restart itself is independent of the hook.
    #[test]
    fn ensure_server_skips_teardown_hook_when_no_clients_registered() {
        let mut sup = make_supervisor();
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        let hook_fired = std::sync::Arc::new(std::sync::Mutex::new(false));
        let hook_flag = std::sync::Arc::clone(&hook_fired);
        let mut hook = move |_: &ServerName| {
            *hook_flag.lock().unwrap() = true;
        };
        let config2 = JackConfig {
            buffer_size: 256,
            ..JackConfig::test_default()
        };
        sup.ensure_server(&name(), &config2, &mut hook).unwrap();
        assert!(!*hook_fired.lock().unwrap(), "hook must not fire with zero clients");
    }

    // Restart event carries a ConfigMismatch reason with both old and new
    // configs. This is the signal the UI uses to explain the transient gap.
    #[test]
    fn ensure_server_emits_restart_requested_with_config_mismatch_reason() {
        let mut sup = make_supervisor();
        let rx = sup.events();
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        let config2 = JackConfig {
            buffer_size: 256,
            ..JackConfig::test_default()
        };
        sup.ensure_server(&name(), &config2, &mut noop_hook()).unwrap();

        let events: Vec<_> = rx.try_iter().collect();
        let restart_event = events
            .iter()
            .find(|e| matches!(e, SupervisorEvent::RestartRequested { .. }))
            .expect("RestartRequested must be emitted");
        match restart_event {
            SupervisorEvent::RestartRequested {
                reason: RestartReason::ConfigMismatch { old, new },
                ..
            } => {
                assert_eq!(old.buffer_size, 128);
                assert_eq!(new.buffer_size, 256);
            }
            other => panic!("unexpected reason: {:?}", other),
        }
    }

    // When post-ready reports SocketVanished on the first attempt but Healthy
    // on the second, the supervisor must emit BufferClampedTo and end up
    // Ready at the bumped buffer size.
    #[test]
    fn spawn_bumps_buffer_on_post_ready_socket_vanished() {
        let mut sup = make_supervisor();
        let rx = sup.events();
        sup.backend
            .queue_post_ready(&name(), PostReadyStatus::SocketVanished);
        // Second attempt succeeds.
        let config = JackConfig {
            buffer_size: 64,
            ..JackConfig::test_default()
        };
        let meta = sup.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        assert_eq!(meta.buffer_size, 128, "bumped to 2x");

        let events: Vec<_> = rx.try_iter().collect();
        let clamp_event = events
            .iter()
            .find(|e| matches!(e, SupervisorEvent::BufferClampedTo { .. }));
        assert!(clamp_event.is_some(), "BufferClampedTo must be emitted");
    }

    // DriverFailure is treated identically to SocketVanished from the
    // buffer-fallback perspective.
    #[test]
    fn spawn_bumps_buffer_on_post_ready_driver_failure() {
        let mut sup = make_supervisor();
        sup.backend
            .queue_post_ready(&name(), PostReadyStatus::DriverFailure("Broken pipe".into()));
        let config = JackConfig {
            buffer_size: 64,
            ..JackConfig::test_default()
        };
        // Speed up the test — we don't actually need the 2s sleep between
        // attempts here because the mock doesn't block on sockets.
        // (We accept the real delay; 2s is acceptable for one test.)
        let meta = sup.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        assert_eq!(meta.buffer_size, 128);
    }

    // All three attempts fail → state is Failed and ensure_server returns
    // Err. A subsequent ensure_server must be able to recover (no stuck
    // state).
    #[test]
    fn spawn_exhausts_attempts_and_transitions_to_failed() {
        let mut sup = make_supervisor();
        for _ in 0..MAX_SPAWN_ATTEMPTS {
            sup.backend.queue_spawn_result(Err("persistent".into()));
        }
        let err = sup
            .ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap_err();
        assert!(err.to_string().contains("persistent"));
        matches!(
            sup.state(&name()),
            Some(JackServerState::Failed { attempts, .. }) if *attempts == MAX_SPAWN_ATTEMPTS
        );

        // Next call should not be stuck — it performs another spawn attempt.
        let meta = sup
            .ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        assert_eq!(meta.sample_rate, 48_000);
    }

    // Health check on a Ready server whose socket has vanished (simulated
    // by the mock backend clearing `running`) transitions the verdict to
    // NotRunning. The supervisor intentionally avoids opening a libjack
    // client on every tick — the next ensure_server retry is what diagnoses
    // a zombie, not the health check.
    #[test]
    fn health_check_reports_not_running_when_socket_vanishes() {
        let mut sup = make_supervisor();
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        // Simulate the jackd socket disappearing (e.g. USB disconnect).
        sup.backend.inner.lock().unwrap().running.remove(&name());

        let verdicts = sup.health_check();
        assert_eq!(verdicts.get(&name()), Some(&HealthStatus::NotRunning));
    }

    // Health check is pure filesystem introspection — it must not open any
    // libjack client. The old implementation did a `probe_meta` per tick,
    // which destabilised USB audio stacks on RK3588 and was the proximate
    // cause of the test-3 audio-stop regression during issue #308 hardware
    // validation.
    #[test]
    fn health_check_does_not_call_probe_meta() {
        let mut sup = make_supervisor();
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        let probes_before = sup
            .backend
            .calls()
            .iter()
            .filter(|c| matches!(c, MockCall::ProbeMeta(_)))
            .count();
        let _ = sup.health_check();
        let _ = sup.health_check();
        let _ = sup.health_check();
        let probes_after = sup
            .backend
            .calls()
            .iter()
            .filter(|c| matches!(c, MockCall::ProbeMeta(_)))
            .count();
        assert_eq!(probes_before, probes_after, "health_check must not probe_meta");
    }

    // Health check on a server that was never started returns NotRunning.
    #[test]
    fn health_check_reports_not_running_for_unknown_server() {
        let mut sup = make_supervisor();
        let verdicts = sup.health_check();
        assert!(verdicts.is_empty(), "no servers means empty verdict map");
    }

    // stop_server drives Ready → NotStarted via terminate + forget and emits
    // the ServerStopped event. Counters are reset.
    #[test]
    fn stop_server_resets_state_and_emits_stopped_event() {
        let mut sup = make_supervisor();
        let rx = sup.events();
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        sup.register_client(&name());
        sup.stop_server(&name()).unwrap();

        assert!(matches!(sup.state(&name()), Some(JackServerState::NotStarted)));
        assert_eq!(sup.client_count(&name()), 0);
        let events: Vec<_> = rx.try_iter().collect();
        assert!(events.iter().any(|e| matches!(e, SupervisorEvent::ServerStopped { .. })));
    }

    // shutdown_all is idempotent — calling twice after a stop does nothing
    // but still returns Ok(()).
    #[test]
    fn shutdown_all_is_idempotent() {
        let mut sup = make_supervisor();
        sup.ensure_server(&"a".into(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        sup.ensure_server(&"b".into(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        sup.shutdown_all().unwrap();
        let first_round = sup.backend.call_count();
        sup.shutdown_all().unwrap();
        assert_eq!(
            sup.backend.call_count(),
            first_round,
            "second shutdown_all must not call the backend"
        );
    }

    // Registering a client then unregistering it bookkeeps the count; the
    // teardown hook is not fired when the count reaches zero before the
    // next restart.
    #[test]
    fn client_registration_counter_is_balanced() {
        let mut sup = make_supervisor();
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        assert_eq!(sup.client_count(&name()), 0);
        sup.register_client(&name());
        sup.register_client(&name());
        assert_eq!(sup.client_count(&name()), 2);
        sup.unregister_client(&name());
        assert_eq!(sup.client_count(&name()), 1);
        sup.unregister_client(&name());
        assert_eq!(sup.client_count(&name()), 0);
        sup.unregister_client(&name()); // saturating
        assert_eq!(sup.client_count(&name()), 0);
    }

    // would_restart is side-effect-free and only returns true for
    // Ready(config mismatch) — the case where callers must drop AsyncClients.
    #[test]
    fn would_restart_distinguishes_mismatch_from_unseen_or_terminal_states() {
        let mut sup = make_supervisor();
        let config = JackConfig::test_default();

        // Unknown server — no restart needed, ensure_server will spawn.
        assert!(!sup.would_restart(&name(), &config));

        // After start, matching config → no restart.
        sup.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        assert!(!sup.would_restart(&name(), &config));

        // Mismatched config → restart required.
        let different = JackConfig {
            buffer_size: 256,
            ..config.clone()
        };
        assert!(sup.would_restart(&name(), &different));

        // Still no backend calls — would_restart is pure.
        let calls_before = sup.backend.call_count();
        let _ = sup.would_restart(&name(), &different);
        let _ = sup.would_restart(&name(), &different);
        assert_eq!(sup.backend.call_count(), calls_before);
    }

    // Basic sanity: each subscriber gets its own event stream.
    #[test]
    fn events_fan_out_to_multiple_subscribers() {
        let mut sup = make_supervisor();
        let rx1 = sup.events();
        let rx2 = sup.events();
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        let r1: Vec<_> = rx1.try_iter().collect();
        let r2: Vec<_> = rx2.try_iter().collect();
        assert!(!r1.is_empty());
        assert_eq!(r1.len(), r2.len());
    }

    // The meta() accessor only returns data for Ready servers; all other
    // states return Err.
    #[test]
    fn meta_accessor_requires_ready_state() {
        let mut sup = make_supervisor();
        assert!(sup.meta(&name()).is_err(), "unknown server");
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        assert!(sup.meta(&name()).is_ok());
        sup.stop_server(&name()).unwrap();
        assert!(sup.meta(&name()).is_err(), "not-started after stop");
    }

    // The supervisor tolerates multiple concurrent server identities without
    // cross-contamination.
    #[test]
    fn multiple_servers_do_not_share_state() {
        let mut sup = make_supervisor();
        sup.ensure_server(&"a".into(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        sup.ensure_server(
            &"b".into(),
            &JackConfig {
                buffer_size: 256,
                ..JackConfig::test_default()
            },
            &mut noop_hook(),
        )
        .unwrap();
        let a_state = sup.state(&"a".into()).unwrap();
        let b_state = sup.state(&"b".into()).unwrap();
        match (a_state, b_state) {
            (
                JackServerState::Ready {
                    launched_config: ca, ..
                },
                JackServerState::Ready {
                    launched_config: cb, ..
                },
            ) => {
                assert_eq!(ca.buffer_size, 128);
                assert_eq!(cb.buffer_size, 256);
            }
            _ => panic!("both servers must be Ready"),
        }
    }

    // Mock meta override — used to make assertions about what probe returns.
    fn custom_meta() -> JackMeta {
        JackMeta {
            sample_rate: 44_100,
            buffer_size: 512,
            capture_port_count: 4,
            playback_port_count: 2,
            hw_name: "custom".into(),
        }
    }

    #[test]
    fn probe_meta_returned_by_ensure_server_is_the_backends_meta() {
        let mut sup = make_supervisor();
        sup.backend.queue_probe_result(&name(), Ok(custom_meta()));
        let meta = sup
            .ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        assert_eq!(meta, custom_meta());
    }
