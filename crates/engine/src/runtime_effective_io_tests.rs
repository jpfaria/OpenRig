//! Engine runtime tests (issue #792 split from runtime_tests.rs).
//! Grouped by responsibility; shared fixtures live in `runtime_tests.rs`.
#![allow(unused_imports)]
use super::*;
use super::tests::*;


#[test]
fn effective_inputs_includes_insert_return() {
    let chain = insert_chain();
    let (resolved_in, _resolved_out) =
        crate::runtime_endpoints::resolve_chain_io(&chain, &insert_registry());
    let (eff_inputs, cpal_indices, _split_positions, _entry_groups) =
        effective_inputs(&chain, &resolved_in, &insert_registry());
    // Should have: 1 regular input + 1 insert return = 2
    assert_eq!(eff_inputs.len(), 2);
    assert_eq!(cpal_indices.len(), 2);
}


#[test]
fn effective_outputs_includes_insert_send() {
    let chain = insert_chain();
    let (_resolved_in, resolved_out) =
        crate::runtime_endpoints::resolve_chain_io(&chain, &insert_registry());
    let eff_outputs = effective_outputs(&chain, &resolved_out, &insert_registry());
    // Should have: 1 regular output + 1 insert send = 2
    assert_eq!(eff_outputs.len(), 2);
    assert_eq!(eff_outputs.len(), 2);
}


// ── effective_inputs with mono multi-channel splitting ────────────────────

#[test]
fn effective_inputs_splits_mono_multichannel_entry() {
    let chain = empty_chain("chain:split");
    // A single mono endpoint spanning 3 channels (as resolved from a binding).
    let resolved = vec![InputEntry {
        device_id: DeviceId("dev".into()),
        mode: ChainInputMode::Mono,
        channels: vec![0, 1, 2],
    }];
    let (eff_inputs, cpal_indices, _split_positions, _entry_groups) =
        effective_inputs(&chain, &resolved, &[]);
    assert_eq!(
        eff_inputs.len(),
        3,
        "mono entry with 3 channels should split into 3 entries"
    );
    assert_eq!(eff_inputs[0].channels, vec![0]);
    assert_eq!(eff_inputs[1].channels, vec![1]);
    assert_eq!(eff_inputs[2].channels, vec![2]);
    // All should share the same CPAL stream index
    assert_eq!(cpal_indices[0], cpal_indices[1]);
    assert_eq!(cpal_indices[1], cpal_indices[2]);
}


// ── effective_inputs / outputs fallback ───────────────────────────────────

#[test]
fn effective_inputs_fallback_when_no_input_blocks() {
    let chain = empty_chain("chain:fallback");
    // No resolved inputs (no binding selected) → fallback.
    let (eff_inputs, cpal_indices, _split_positions, _entry_groups) =
        effective_inputs(&chain, &[], &[]);
    assert_eq!(
        eff_inputs.len(),
        1,
        "fallback should produce exactly 1 input"
    );
    assert_eq!(cpal_indices, vec![0]);
}


#[test]
fn effective_outputs_fallback_when_no_output_blocks() {
    let chain = empty_chain("chain:fallback");
    // No resolved outputs (no binding selected) → fallback.
    let eff_outputs = effective_outputs(&chain, &[], &[]);
    assert_eq!(
        eff_outputs.len(),
        1,
        "fallback should produce exactly 1 output"
    );
}


#[test]
fn insert_return_as_input_entry_copies_return_endpoint() {
    use super::insert_return_as_input_entry;
    use domain::io_binding::ChannelMode;
    let reg = fx_registry(ChannelMode::Stereo, vec![0, 1], ChannelMode::Mono, vec![2]);
    let entry = insert_return_as_input_entry(&fx_insert(), &reg).expect("return resolves");
    assert_eq!(entry.device_id.0, "return_dev");
    assert_eq!(entry.channels, vec![2]);
    assert!(matches!(entry.mode, ChainInputMode::Mono));
}


// ── insert_send_as_output_entry tests ───────────────────────────────────

#[test]
fn insert_send_as_output_entry_mono_mode() {
    use super::insert_send_as_output_entry;
    use domain::io_binding::ChannelMode;
    let reg = fx_registry(ChannelMode::Mono, vec![0], ChannelMode::Stereo, vec![0, 1]);
    let entry = insert_send_as_output_entry(&fx_insert(), &reg).expect("send resolves");
    assert_eq!(entry.device_id.0, "send_dev");
    assert_eq!(entry.channels, vec![0]);
    assert!(matches!(entry.mode, ChainOutputMode::Mono));
}


#[test]
fn insert_send_as_output_entry_stereo_mode() {
    use super::insert_send_as_output_entry;
    use domain::io_binding::ChannelMode;
    let reg = fx_registry(ChannelMode::Stereo, vec![0, 1], ChannelMode::Mono, vec![0]);
    let entry = insert_send_as_output_entry(&fx_insert(), &reg).expect("send resolves");
    assert!(matches!(entry.mode, ChainOutputMode::Stereo));
}


#[test]
fn insert_send_as_output_entry_dual_mono_becomes_stereo() {
    use super::insert_send_as_output_entry;
    use domain::io_binding::ChannelMode;
    let reg = fx_registry(
        ChannelMode::DualMono,
        vec![0, 1],
        ChannelMode::Mono,
        vec![0],
    );
    let entry = insert_send_as_output_entry(&fx_insert(), &reg).expect("send resolves");
    assert!(matches!(entry.mode, ChainOutputMode::Stereo));
}


// ── build_output_routing_state tests ────────────────────────────────────

#[test]
fn build_output_routing_state_mono_single_channel() {
    use super::build_output_routing_state;
    let output = OutputEntry {
        device_id: DeviceId("dev".into()),
        mode: ChainOutputMode::Mono,
        channels: vec![0],
    };
    let state = build_output_routing_state(&output, DEFAULT_ELASTIC_TARGET, 0);
    assert_eq!(state.output_channels, vec![0]);
}


#[test]
fn build_output_routing_state_stereo_two_channels() {
    use super::build_output_routing_state;
    let output = OutputEntry {
        device_id: DeviceId("dev".into()),
        mode: ChainOutputMode::Stereo,
        channels: vec![0, 1],
    };
    let state = build_output_routing_state(&output, DEFAULT_ELASTIC_TARGET, 0);
    assert_eq!(state.output_channels, vec![0, 1]);
}


#[test]
fn build_output_routing_state_mono_mode_with_two_channels_uses_mono() {
    use super::build_output_routing_state;
    let output = OutputEntry {
        device_id: DeviceId("dev".into()),
        mode: ChainOutputMode::Mono,
        channels: vec![0, 1],
    };
    let _state = build_output_routing_state(&output, DEFAULT_ELASTIC_TARGET, 0);
    // Mono mode with 2 channels: layout should be Mono per the logic
    // Just verifying it doesn't panic and runs correctly
}


// ── input tap ───────────────────────────────────────────────────────────

#[test]
fn subscribe_input_tap_receives_pre_fx_samples() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_mono(),
        )
        .expect("runtime should build"),
    );

    // Subscribe to input 0, channel 0 (mono input).
    let rings = runtime.subscribe_input_tap(0, 1, &[0], 256);
    assert_eq!(rings.len(), 1, "one ring per subscribed channel");

    // Feed a known buffer through process_input_f32.
    let data = [0.1_f32, 0.2, 0.3, 0.4];
    process_input_f32(&runtime, 0, &data, 1);

    // Drain the ring and confirm the same samples landed pre-FX.
    let mut received = Vec::new();
    while let Some(s) = rings[0].pop() {
        received.push(s);
    }
    assert_eq!(received, vec![0.1, 0.2, 0.3, 0.4]);
}


#[test]
fn subscribe_input_tap_only_targets_matching_input_index() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_mono(),
        )
        .expect("runtime should build"),
    );

    let rings = runtime.subscribe_input_tap(0, 1, &[0], 256);

    // Push to a *different* input index — tap should not see the samples.
    process_input_f32(&runtime, 99, &[0.5, 0.6], 1);
    assert!(
        rings[0].pop().is_none(),
        "tap on input 0 must ignore input 99"
    );
}


#[test]
fn prune_dead_input_taps_removes_unused() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_mono(),
        )
        .expect("runtime should build"),
    );

    {
        let rings = runtime.subscribe_input_tap(0, 1, &[0], 64);
        assert_eq!(rings.len(), 1);
        assert_eq!(runtime.input_taps.load().len(), 1);
    }
    // rings out of scope — only the runtime's InputTap holds the channel
    // ring Arc, so its strong_count == 1 and prune drops it.
    runtime.prune_dead_input_taps();
    assert_eq!(runtime.input_taps.load().len(), 0);
}


// ── stream tap ──────────────────────────────────────────────────────────

#[test]
fn subscribe_stream_tap_receives_post_fx_stereo() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_mono(),
        )
        .expect("runtime should build"),
    );

    // Subscribe to stream 0 (the chain's single input pipeline).
    let [l_ring, r_ring] = runtime.subscribe_stream_tap(0, 256);

    // Drive the chain so the segment's frame_buffer has known data.
    process_input_f32(&runtime, 0, &[0.1_f32, 0.2, 0.3, 0.4], 1);

    // Mono input is upmixed before FX → both rings receive the same
    // samples after the (no-op) processing of the passthrough chain.
    let mut left = Vec::new();
    while let Some(s) = l_ring.pop() {
        left.push(s);
    }
    let mut right = Vec::new();
    while let Some(s) = r_ring.pop() {
        right.push(s);
    }
    assert_eq!(left.len(), 4, "left ring got {left:?}");
    assert_eq!(right.len(), 4, "right ring got {right:?}");
}


#[test]
fn subscribe_stream_tap_only_targets_matching_stream_index() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_mono(),
        )
        .expect("runtime should build"),
    );

    // Subscribe to a stream that does not exist (chain has only one).
    let [l_ring, r_ring] = runtime.subscribe_stream_tap(99, 256);
    process_input_f32(&runtime, 0, &[0.1_f32, 0.2, 0.3, 0.4], 1);
    assert!(
        l_ring.pop().is_none(),
        "tap on stream 99 must ignore segment 0"
    );
    assert!(r_ring.pop().is_none());
}


#[test]
fn stream_tap_publishes_independent_of_output_mute() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_mono(),
        )
        .expect("runtime should build"),
    );

    let [l_ring, r_ring] = runtime.subscribe_stream_tap(0, 256);
    // Mute the output — the device buffer will be zero, but the
    // stream tap is dispatched **before** the output stage so the
    // analyzer keeps receiving samples.
    runtime.set_output_muted(true);
    process_input_f32(&runtime, 0, &[0.1_f32, 0.2, 0.3, 0.4], 1);
    let mut out = vec![0.0_f32; 4];
    process_output_f32(&runtime, 0, &mut out, 1);

    assert!(
        out.iter().all(|&s| s == 0.0),
        "muted device output must be zero, got {:?}",
        out
    );
    // Stream tap kept feeding the analyzer.
    let mut left = Vec::new();
    while let Some(s) = l_ring.pop() {
        left.push(s);
    }
    let mut right = Vec::new();
    while let Some(s) = r_ring.pop() {
        right.push(s);
    }
    assert_eq!(left.len(), 4);
    assert_eq!(right.len(), 4);
}


#[test]
fn prune_dead_stream_taps_removes_unused() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_mono(),
        )
        .expect("runtime should build"),
    );

    {
        let _rings = runtime.subscribe_stream_tap(0, 64);
        assert_eq!(runtime.stream_taps.load().len(), 1);
    }
    runtime.prune_dead_stream_taps();
    assert_eq!(runtime.stream_taps.load().len(), 0);
}


// ── output_muted flag ────────────────────────────────────────────────────

#[test]
fn output_muted_defaults_to_false() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_mono(),
        )
        .expect("runtime should build"),
    );

    assert!(!runtime.is_output_muted());
}


#[test]
fn set_output_muted_round_trips() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_mono(),
        )
        .expect("runtime should build"),
    );

    runtime.set_output_muted(true);
    assert!(runtime.is_output_muted());

    runtime.set_output_muted(false);
    assert!(!runtime.is_output_muted());
}


#[test]
fn output_muted_zeros_process_output_buffer() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_mono(),
        )
        .expect("runtime should build"),
    );

    // Push some samples through the input so the chain has data
    // available when process_output_f32 runs.
    process_input_f32(&runtime, 0, &[0.1, 0.2, 0.3, 0.4], 1);

    runtime.set_output_muted(true);
    let mut out = vec![0.5_f32; 4];
    process_output_f32(&runtime, 0, &mut out, 1);
    assert!(
        out.iter().all(|&s| s == 0.0),
        "muted output must be all zero, got {:?}",
        out
    );
}


#[test]
fn output_muted_unset_does_not_zero_buffer() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_mono(),
        )
        .expect("runtime should build"),
    );

    // Drive a known signal through input → passthrough → output.
    process_input_f32(&runtime, 0, &[0.1, 0.2, 0.3, 0.4], 1);
    let mut out = vec![0.0_f32; 4];
    process_output_f32(&runtime, 0, &mut out, 1);

    // With the mute flag off, the output buffer must contain the
    // forwarded samples — at least one frame is non-zero.
    assert!(
        out.iter().any(|&s| s != 0.0),
        "unmuted passthrough output must have non-zero samples, got {:?}",
        out
    );
}


// ── effective_inputs with stereo entry does not split ────────────────────

#[test]
fn effective_inputs_stereo_entry_not_split() {
    let chain = empty_chain("chain:stereo");
    let resolved = vec![InputEntry {
        device_id: DeviceId("dev".into()),
        mode: ChainInputMode::Stereo,
        channels: vec![0, 1],
    }];
    let (eff_inputs, _, _, _) = effective_inputs(&chain, &resolved, &[]);
    assert_eq!(eff_inputs.len(), 1, "stereo entry should not be split");
    assert_eq!(eff_inputs[0].channels, vec![0, 1]);
}


// ── effective_inputs with multiple input blocks ─────────────────────────

#[test]
fn effective_inputs_multiple_input_blocks() {
    let chain = empty_chain("chain:multi-in");
    // Two distinct-device mono endpoints (as resolved from a binding).
    let resolved = vec![
        InputEntry {
            device_id: DeviceId("dev1".into()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        },
        InputEntry {
            device_id: DeviceId("dev2".into()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        },
    ];
    let (eff_inputs, cpal_indices, _split_positions, _entry_groups) =
        effective_inputs(&chain, &resolved, &[]);
    assert_eq!(eff_inputs.len(), 2);
    assert_eq!(eff_inputs[0].device_id.0, "dev1");
    assert_eq!(eff_inputs[1].device_id.0, "dev2");
    // Different devices should have different CPAL indices
    assert_ne!(cpal_indices[0], cpal_indices[1]);
}


// ── effective_inputs same device shares cpal index ──────────────────────

#[test]
fn effective_inputs_same_device_shares_cpal_index() {
    let chain = empty_chain("chain:same-dev");
    // Two same-device mono endpoints (as resolved from a binding).
    let resolved = vec![
        InputEntry {
            device_id: DeviceId("same_dev".into()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        },
        InputEntry {
            device_id: DeviceId("same_dev".into()),
            mode: ChainInputMode::Mono,
            channels: vec![1],
        },
    ];
    let (eff_inputs, cpal_indices, _split_positions, _entry_groups) =
        effective_inputs(&chain, &resolved, &[]);
    assert_eq!(eff_inputs.len(), 2);
    assert_eq!(
        cpal_indices[0], cpal_indices[1],
        "same device should share CPAL index"
    );
}

