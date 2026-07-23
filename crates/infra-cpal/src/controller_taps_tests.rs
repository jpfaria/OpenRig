//! Unit tests for controller_taps DI-loop routing (issue #792 split).

#[cfg(test)]
mod di_loop_multirate_output_tests {
    use super::super::arm_di_loop_per_output_stream;
    use domain::ids::{ChainId, DeviceId};
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use engine::runtime::build_chain_runtime_state;
    use engine::DiPcm;
    use project::chain::Chain;
    use std::sync::Arc;

    /// One per-input runtime clocked at `rate`, mono in / stereo out (#716: the
    /// endpoints live in the binding registry).
    fn runtime_at(rate: f32) -> Arc<engine::runtime::ChainRuntimeState> {
        let chain = Chain {
            id: ChainId("mr".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["io".into()],
            blocks: vec![],
            di_output: None,
        };
        let registry = vec![IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId("d".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![IoEndpoint {
                name: "out0".into(),
                device_id: DeviceId("d".into()),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        }];
        Arc::new(build_chain_runtime_state(&chain, rate, &[256], &registry).unwrap())
    }

    /// #749 slow-mo — the armed loop must be resampled to the RUNTIME's rate,
    /// not to a single global `engine_sr`. A 48 kHz-built loop armed on a
    /// 44.1 kHz runtime keeps its 48 kHz frame count, so the audio thread
    /// (one loop frame per 44.1 kHz output frame) stretches it: the owner's
    /// "está lento" on the Scarlett @44.1 while engine_sr was 48 kHz. The
    /// arming path owns each runtime's rate, so it — not the loader — must
    /// build the loop at that rate.
    #[test]
    fn di_loop_is_resampled_to_each_runtime_rate() {
        // One decoded 48 kHz source, armed on outputs at two different rates.
        let src: Vec<f32> = (0..4800).map(|i| ((i as f32) * 0.1).sin()).collect();
        let pcm = Arc::new(DiPcm::new(src, 48_000, 1));

        let rt_48 = runtime_at(48_000.0);
        let rt_441 = runtime_at(44_100.0);
        arm_di_loop_per_output_stream(std::slice::from_ref(&rt_48), Some(pcm.clone()));
        arm_di_loop_per_output_stream(std::slice::from_ref(&rt_441), Some(pcm));

        let len48 = rt_48.di_loop_len().expect("48 kHz armed");
        let len441 = rt_441.di_loop_len().expect("44.1 kHz armed");

        // The 44.1 kHz output plays one loop frame per output frame, so its loop
        // must hold FEWER frames than the 48 kHz one, scaled by the rate ratio.
        // Before #749 both were the single engine_sr buffer → equal → slow-mo on
        // the mismatched output.
        assert!(
            len48 > len441,
            "slow-mo #749: the 44.1 kHz loop ({len441}) must have fewer frames \
             than the 48 kHz loop ({len48}) — a single engine_sr buffer makes \
             them equal and stretches on the mismatched output"
        );
        let ratio = len441 as f32 / len48 as f32;
        assert!(
            (ratio - 44_100.0 / 48_000.0).abs() < 0.02,
            "slow-mo #749: the 44.1 kHz loop length must scale by the rate ratio \
             (got {ratio:.4}, want {:.4})",
            44_100.0 / 48_000.0
        );
    }

    /// #749 — on a multi-rate chain the DI loop must reach EVERY output stream.
    /// #736 mixes only same-rate runtimes into an output, so arming a single
    /// entry leaves the other-rate output silent while the icon shows blue
    /// ("dead always"). Arm the first runtime of each distinct rate so both the
    /// 44.1 kHz and the 48 kHz outputs play one copy.
    #[test]
    fn di_loop_reaches_every_output_rate_on_a_multirate_chain() {
        let e0 = runtime_at(44_100.0); // Scarlett input entry
        let e1 = runtime_at(48_000.0); // TEYUN input entry
        let di = Arc::new(DiPcm::new(vec![0.5; 256], 48_000, 1));

        arm_di_loop_per_output_stream(&[e0.clone(), e1.clone()], Some(di));

        assert!(
            e0.has_di_loop(),
            "REGRESSION #749: the 44.1 kHz output stream must receive the loop"
        );
        assert!(
            e1.has_di_loop(),
            "REGRESSION #749: the 48 kHz output stream must receive the loop — \
             armed only on the 44.1 kHz entry, the 48 kHz output (#736) stays \
             silent while the icon is blue"
        );
    }

    /// Two entries at the SAME rate feed the SAME output stream and the backend
    /// sums them — only ONE may carry the loop or it doubles (#715). The per-rate
    /// arming must pick exactly one runtime per distinct rate.
    #[test]
    fn di_loop_does_not_double_same_rate_entries() {
        let a = runtime_at(48_000.0);
        let b = runtime_at(48_000.0); // sibling on the same 48 kHz output
        let di = Arc::new(DiPcm::new(vec![0.5; 256], 48_000, 1));

        arm_di_loop_per_output_stream(&[a.clone(), b.clone()], Some(di));

        assert!(a.has_di_loop(), "the first 48 kHz runtime carries the loop");
        assert!(
            !b.has_di_loop(),
            "the same-rate sibling must NOT also carry it (#715 doubling)"
        );
    }
}

#[cfg(test)]
mod di_loop_doubling_tests {
    use super::super::arm_di_loop_per_output_stream;
    use crate::{build_chain_runtime, BuildRequest};
    use domain::ids::{ChainId, DeviceId};
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use engine::DiPcm;
    use project::chain::Chain;
    use std::sync::Arc;

    /// A chain whose input binding has TWO mono entries on the same device
    /// (ch0 + ch1) — the "two inputs, one interface" shape. #703 builds one
    /// runtime per entry. Model A (#716): the endpoints live in the registry.
    fn two_entry_chain() -> Chain {
        Chain {
            id: ChainId("dbl".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["io".into()],
            blocks: vec![],
            di_output: None,
        }
    }

    /// Registry mirroring `two_entry_chain`: two same-device mono inputs +
    /// one stereo output.
    fn two_entry_registry() -> Vec<IoBinding> {
        vec![IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![
                IoEndpoint {
                    name: "in0".into(),
                    device_id: DeviceId("dev".into()),
                    mode: ChannelMode::Mono,
                    channels: vec![0],
                },
                IoEndpoint {
                    name: "in1".into(),
                    device_id: DeviceId("dev".into()),
                    mode: ChannelMode::Mono,
                    channels: vec![1],
                },
            ],
            outputs: vec![IoEndpoint {
                name: "out0".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        }]
    }

    #[test]
    fn two_entry_chain_builds_two_runtimes() {
        // Pins the doubling PREMISE: a 2-entry chain is two isolated runtimes.
        let req = BuildRequest {
            chain: two_entry_chain(),
            sample_rate: 48_000.0,
            device_sample_rates: std::collections::HashMap::new(),
            buffer_sizes: vec![64],
            io_bindings: two_entry_registry(),
        };
        let runtimes = build_chain_runtime(&req).expect("build 2-entry chain");
        assert_eq!(runtimes.len(), 2, "#703: one runtime per input entry");
    }

    #[test]
    fn di_loop_is_armed_on_the_first_runtime_only() {
        let req = BuildRequest {
            chain: two_entry_chain(),
            sample_rate: 48_000.0,
            device_sample_rates: std::collections::HashMap::new(),
            buffer_sizes: vec![64],
            io_bindings: two_entry_registry(),
        };
        let built = build_chain_runtime(&req).expect("build 2-entry chain");
        let runtimes: Vec<_> = built.into_iter().map(|(_, rt)| rt).collect();
        assert_eq!(runtimes.len(), 2);

        let di = Arc::new(DiPcm::new(vec![0.1, 0.2, 0.3, 0.4], 48_000, 1));
        arm_di_loop_per_output_stream(&runtimes, Some(di));

        assert!(
            runtimes[0].has_di_loop(),
            "the loop plays on the first runtime"
        );
        assert!(
            !runtimes[1].has_di_loop(),
            "the loop must NOT also play on the second entry's runtime — that is \
             the doubling: two runtimes sum at the output device (#715)"
        );
    }

    #[test]
    fn clearing_disarms_every_runtime() {
        let req = BuildRequest {
            chain: two_entry_chain(),
            sample_rate: 48_000.0,
            device_sample_rates: std::collections::HashMap::new(),
            buffer_sizes: vec![64],
            io_bindings: two_entry_registry(),
        };
        let built = build_chain_runtime(&req).expect("build");
        let runtimes: Vec<_> = built.into_iter().map(|(_, rt)| rt).collect();
        let di = Arc::new(DiPcm::new(vec![0.1, 0.2], 48_000, 1));
        arm_di_loop_per_output_stream(&runtimes, Some(di));
        arm_di_loop_per_output_stream(&runtimes, None);
        assert!(!runtimes[0].has_di_loop() && !runtimes[1].has_di_loop());
    }
}
