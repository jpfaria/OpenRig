//! Engine runtime tests (issue #792 split from runtime_tests.rs).
//! Grouped by responsibility; shared fixtures live in `runtime_tests.rs`.
#![allow(unused_imports)]
use super::*;
use super::tests::*;


#[test]
#[ignore] // requires asset_paths initialization
fn runtime_graph_rejects_chain_when_runtime_sample_rate_does_not_match_ir() {
    let (model, params) = any_ir_cab_defaults();

    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("chain:0".into()),
            description: Some("Cab test".into()),
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![AudioBlock {
                id: BlockId("chain:0:block:0".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "cab".to_string(),
                    model,
                    params,
                }),
            }],
            di_output: None,
        }],
        midi: None,
    };

    let error = match build_runtime_graph(
        &project,
        &HashMap::from([(ChainId("chain:0".into()), 44_100.0)]),
        &HashMap::new(),
        &[],
    ) {
        Ok(_) => panic!("runtime graph should reject mismatched IR sample rate"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("sample_rate"));
}


#[test]
#[ignore] // requires asset_paths initialization
fn dual_mono_chain_does_not_leak_left_into_right() {
    let chain = Chain {
        id: ChainId("chain:stereo".into()),
        description: Some("Stereo isolation".into()),
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![IO_BINDING_ID.into()],
        blocks: vec![
            compressor_block("chain:stereo:block:0"),
            preamp_block("chain:stereo:block:1"),
            native_cab_block("chain:stereo:block:2"),
            reverb_block("chain:stereo:block:3"),
        ],
        di_output: None,
    };
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_split_dual(),
        )
        .expect("runtime state should build"),
    );

    let mut input = vec![0.0f32; 256 * 2];
    for frame in input.chunks_mut(2) {
        frame[0] = 0.25;
        frame[1] = 0.0;
    }
    process_input_f32(&runtime, 0, &input, 2);

    let mut output = vec![0.0f32; input.len()];
    process_output_f32(&runtime, 0, &mut output, 2);

    let right_peak = output
        .chunks_exact(2)
        .map(|frame| frame[1].abs())
        .fold(0.0f32, f32::max);
    assert!(
        right_peak <= 1.0e-6,
        "dual-mono chain leaked signal into right channel: peak={right_peak}"
    );
}


#[test]
#[ignore] // requires asset_paths initialization
fn asset_backed_dual_mono_chain_does_not_leak_left_into_right() {
    let chain = Chain {
        id: ChainId("chain:asset-backed".into()),
        description: Some("Stereo isolation asset-backed".into()),
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![IO_BINDING_ID.into()],
        blocks: vec![
            marshall_preamp_block("chain:asset-backed:block:0"),
            ir_cab_block("chain:asset-backed:block:1"),
            reverb_block("chain:asset-backed:block:2"),
        ],
        di_output: None,
    };
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_split_dual(),
        )
        .expect("runtime state should build"),
    );

    let mut input = vec![0.0f32; 256 * 2];
    for frame in input.chunks_mut(2) {
        frame[0] = 0.25;
        frame[1] = 0.0;
    }
    process_input_f32(&runtime, 0, &input, 2);

    let mut output = vec![0.0f32; input.len()];
    process_output_f32(&runtime, 0, &mut output, 2);

    let right_peak = output
        .chunks_exact(2)
        .map(|frame| frame[1].abs())
        .fold(0.0f32, f32::max);
    assert!(
        right_peak <= 1.0e-6,
        "asset-backed dual-mono chain leaked signal into right channel: peak={right_peak}"
    );
}


#[test]
fn build_runtime_graph_errors_on_missing_sample_rate() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("chain:0".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
        }],
        midi: None,
    };

    let result = build_runtime_graph(&project, &HashMap::new(), &HashMap::new(), &[]);
    assert!(
        result.is_err(),
        "should error when chain has no sample rate"
    );
}


// ── process passthrough chain round-trip ────────────────────────────────

#[test]
fn passthrough_chain_round_trip_preserves_signal() {
    let chain = io_passthrough_chain("chain:rt");
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_mono(),
        )
        .expect("runtime should build"),
    );

    // Warm up past fade-in
    let warmup = vec![0.0f32; FADE_IN_FRAMES + 64];
    process_input_f32(&runtime, 0, &warmup, 1);
    let mut drain = vec![0.0f32; warmup.len()];
    process_output_f32(&runtime, 0, &mut drain, 1);

    // Now test actual signal
    let input = [0.1f32, 0.2, 0.3, 0.4];
    process_input_f32(&runtime, 0, &input, 1);
    let mut out = vec![0.0f32; 4];
    process_output_f32(&runtime, 0, &mut out, 1);

    for (i, (&expected, &actual)) in input.iter().zip(out.iter()).enumerate() {
        assert!(
            (expected - actual).abs() < 1e-6,
            "frame {i}: expected {expected}, got {actual}"
        );
    }
}


// ── processor_scratch tests ─────────────────────────────────────────────

#[test]
fn processor_scratch_mono_creates_mono_scratch() {
    use super::processor_scratch;
    struct NoopMono;
    impl block_core::MonoProcessor for NoopMono {
        fn process_sample(&mut self, s: f32) -> f32 {
            s
        }
    }
    let proc = AudioProcessor::Mono(Box::new(NoopMono));
    let scratch = processor_scratch(&proc);
    assert!(matches!(scratch, ProcessorScratch::Mono(_)));
}


#[test]
fn processor_scratch_stereo_creates_stereo_scratch() {
    use super::processor_scratch;
    struct NoopStereo;
    impl block_core::StereoProcessor for NoopStereo {
        fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
            input
        }
    }
    let proc = AudioProcessor::Stereo(Box::new(NoopStereo));
    let scratch = processor_scratch(&proc);
    assert!(matches!(scratch, ProcessorScratch::Stereo(_)));
}


#[test]
fn processor_scratch_dual_mono_creates_dual_mono_scratch() {
    use super::processor_scratch;
    struct NoopMono;
    impl block_core::MonoProcessor for NoopMono {
        fn process_sample(&mut self, s: f32) -> f32 {
            s
        }
    }
    let proc = AudioProcessor::DualMono {
        left: Box::new(NoopMono),
        right: Box::new(NoopMono),
    };
    let scratch = processor_scratch(&proc);
    assert!(matches!(scratch, ProcessorScratch::DualMono { .. }));
}


// ── #723: no hardcoded sample rate in the latency-probe beep ──────────────

#[test]
fn chain_runtime_state_reports_its_build_sample_rate() {
    // The live probe beep (process_input_f32) must synthesize at the real
    // device rate, so the runtime has to remember the rate it was built at —
    // never assume 48000 (issue #723).
    let chain = tuner_track("chain:0", Vec::new());
    let rt = build_chain_runtime_state(&chain, 44_100.0, &[DEFAULT_ELASTIC_TARGET], &[])
        .expect("runtime state should build");
    assert_eq!(rt.sample_rate(), 44_100.0);
}


#[test]
fn write_probe_beep_depends_on_the_passed_sample_rate() {
    // The probe beep is a 1 kHz sine. Its sample at frame f is
    // sin(2π·freq·f/sr)·0.95·envelope, so feeding 44100 vs 48000 must produce
    // different audio — proving the rate is honored, not hardcoded.
    use crate::runtime_probe::{write_probe_beep, PROBE_BEEP_FREQ};
    let frames = 64usize;
    let channels = 2usize;
    let mut at_44100 = vec![0.0f32; frames * channels];
    let mut at_48000 = vec![0.0f32; frames * channels];
    write_probe_beep(&mut at_44100, channels, 44_100.0, frames);
    write_probe_beep(&mut at_48000, channels, 48_000.0, frames);
    assert_ne!(
        at_44100, at_48000,
        "beep must depend on the passed sample rate"
    );

    let f = 20usize;
    let sr = 44_100.0f32;
    let env = (std::f32::consts::PI * f as f32 / frames as f32).sin();
    let expected =
        (2.0 * std::f32::consts::PI * PROBE_BEEP_FREQ * (f as f32 / sr)).sin() * 0.95 * env;
    assert!((at_44100[f * channels] - expected).abs() < 1e-5);
    // Both channels carry the same mono beep.
    assert_eq!(at_44100[f * channels], at_44100[f * channels + 1]);
}

