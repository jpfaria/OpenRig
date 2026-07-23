//! #736 per-binding rate tests (issue #792 split from runtime_graph.rs).
//! Each runtime is clocked at its OWN device's rate (stream isolation).

    use super::build_per_input_runtimes;
    use domain::ids::{ChainId, DeviceId};
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use project::chain::Chain;
    use std::collections::HashMap;

    // Two mono inputs on two devices, paired with two outputs on the same two
    // devices — two bindings, the #736 "Scarlett + TEYUN" shape.
    fn two_binding_registry() -> Vec<IoBinding> {
        vec![
            IoBinding {
                id: "a".into(),
                name: "A".into(),
                inputs: vec![IoEndpoint {
                    name: "in".into(),
                    device_id: DeviceId("devA".into()),
                    mode: ChannelMode::Mono,
                    channels: vec![0],
                }],
                outputs: vec![IoEndpoint {
                    name: "out".into(),
                    device_id: DeviceId("devA".into()),
                    mode: ChannelMode::Stereo,
                    channels: vec![0, 1],
                }],
            },
            IoBinding {
                id: "b".into(),
                name: "B".into(),
                inputs: vec![IoEndpoint {
                    name: "in".into(),
                    device_id: DeviceId("devB".into()),
                    mode: ChannelMode::Mono,
                    channels: vec![0],
                }],
                outputs: vec![IoEndpoint {
                    name: "out".into(),
                    device_id: DeviceId("devB".into()),
                    mode: ChannelMode::Stereo,
                    channels: vec![0, 1],
                }],
            },
        ]
    }

    // Mirrored from crates/engine/tests/issue_716_per_binding_routing.rs —
    // use direct struct construction since Chain has no new_for_test helper.
    fn two_binding_chain() -> Chain {
        Chain {
            id: ChainId("rig:input-1".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["a".into(), "b".into()],
            blocks: Vec::new(),
            di_output: None,
            loopers: vec![],
        }
    }

    use super::RuntimeGraph;

    #[test]
    fn upsert_full_rebuild_preserves_per_device_rates() {
        let chain = two_binding_chain();
        let registry = two_binding_registry();
        let mut rates = HashMap::new();
        rates.insert(DeviceId("devA".into()), 44_100.0_f32);
        rates.insert(DeviceId("devB".into()), 48_000.0_f32);

        let mut graph = RuntimeGraph {
            chains: std::collections::HashMap::new(),
        };
        graph
            .upsert_chain(&chain, 48_000.0, &rates, false, &[], &registry)
            .expect("initial upsert");
        let mut seen: Vec<f32> = graph
            .runtimes_for(&chain.id)
            .iter()
            .map(|r| r.sample_rate())
            .collect();
        seen.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(seen, vec![44_100.0, 48_000.0]);
    }

    #[test]
    fn each_runtime_built_at_its_own_device_rate() {
        let chain = two_binding_chain();
        let registry = two_binding_registry();
        let mut rates = HashMap::new();
        rates.insert(DeviceId("devA".into()), 44_100.0_f32);
        rates.insert(DeviceId("devB".into()), 48_000.0_f32);

        let runtimes = build_per_input_runtimes(&chain, 48_000.0, &rates, &[], &registry)
            .expect("two-binding build must succeed");
        assert_eq!(runtimes.len(), 2, "two devices → two isolated runtimes");

        let mut seen: Vec<f32> = runtimes.iter().map(|(_, s)| s.sample_rate()).collect();
        seen.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(
            seen,
            vec![44_100.0, 48_000.0],
            "each runtime clocked at its own device rate"
        );
    }

    #[test]
    fn empty_rate_map_falls_back_to_scalar_bit_exact() {
        let chain = two_binding_chain();
        let registry = two_binding_registry();
        let empty: HashMap<DeviceId, f32> = HashMap::new();
        let runtimes = build_per_input_runtimes(&chain, 48_000.0, &empty, &[], &registry).unwrap();
        for (_, state) in &runtimes {
            assert_eq!(
                state.sample_rate(),
                48_000.0,
                "no override → scalar rate (legacy)"
            );
        }
    }
