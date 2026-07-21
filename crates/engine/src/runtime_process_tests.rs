//! Engine runtime tests (issue #792 split from runtime_tests.rs).
//! Grouped by responsibility; shared fixtures live in `runtime_tests.rs`.
#![allow(unused_imports)]
use super::*;
use super::tests::*;


#[test]
fn process_input_limits_buffered_output_frames() {
    let chain = tuner_track("chain:0", Vec::new());
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[])
            .expect("runtime state should build"),
    );
    let total_frames = DEFAULT_ELASTIC_TARGET * 2 + 64;
    let input = vec![0.25f32; total_frames];

    process_input_f32(&runtime, 0, &input, 1);

    let routes = runtime.output_routes.load();
    assert!(routes[0].buffer.len() <= DEFAULT_ELASTIC_TARGET * 2);
}


#[test]
#[ignore] // requires asset_paths initialization
fn process_output_drains_buffered_frames() {
    let chain = tuner_track("chain:0", Vec::new());
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[])
            .expect("runtime state should build"),
    );

    process_input_f32(&runtime, 0, &[0.25, 0.5, 0.75, 1.0], 1);

    let mut out = vec![0.0f32; 4];
    process_output_f32(&runtime, 0, &mut out, 1);

    assert_eq!(out, vec![0.25, 0.5, 0.75, 1.0]);
    let routes = runtime.output_routes.load();
    assert_eq!(routes[0].buffer.len(), 0);
}


#[test]
#[ignore] // requires asset_paths initialization
fn update_chain_runtime_state_preserves_select_instance_when_switching_active_option() {
    let mut chain = select_delay_chain("chain:select", "delay_a");
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[])
            .expect("runtime state should build"),
    );
    let original_serial = {
        let locked = runtime.processing.lock().expect("runtime poisoned");
        locked.input_states[0].blocks[0].instance_serial
    };

    if let AudioBlockKind::Select(select) = &mut chain.blocks[0].kind {
        select.selected_block_id = BlockId("chain:select:block:0::delay_b".into());
    }

    update_chain_runtime_state(
        &runtime,
        &chain,
        48_000.0,
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &[],
    )
    .expect("runtime update should succeed when switching select option");

    let updated_serial = {
        let locked = runtime.processing.lock().expect("runtime poisoned");
        locked.input_states[0].blocks[0].instance_serial
    };

    assert_eq!(updated_serial, original_serial);
}


#[test]
fn panicking_processor_does_not_crash_the_caller() {
    let mut block = panicking_block_node();
    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    // Must not panic
    apply_block_processor(&mut block, &mut frames, &error_queue);
}


#[test]
fn panicking_processor_zeroes_output_frames() {
    let mut block = panicking_block_node();
    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    apply_block_processor(&mut block, &mut frames, &error_queue);

    for frame in &frames {
        match frame {
            AudioFrame::Stereo([l, r]) => {
                assert_eq!(*l, 0.0, "left channel should be silent after panic");
                assert_eq!(*r, 0.0, "right channel should be silent after panic");
            }
            AudioFrame::Mono(s) => assert_eq!(*s, 0.0, "mono channel should be silent after panic"),
        }
    }
}


#[test]
fn panicking_processor_posts_error_to_queue() {
    let mut block = panicking_block_node();
    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    apply_block_processor(&mut block, &mut frames, &error_queue);

    assert_eq!(error_queue.len(), 1, "exactly one error should be posted");
    let err = error_queue.pop().expect("error should be available");
    assert_eq!(err.block_id.0, "test:panicking");
    assert!(
        err.message.contains("simulated plugin crash"),
        "error message should contain panic message"
    );
}


#[test]
fn second_call_after_panic_does_not_process_or_post_error() {
    let mut block = panicking_block_node();
    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    // First call: panics, marks faulted, posts error
    apply_block_processor(&mut block, &mut frames, &error_queue);
    assert_eq!(error_queue.len(), 1);

    // Second call: faulted — must not post another error
    apply_block_processor(&mut block, &mut frames, &error_queue);
    assert_eq!(
        error_queue.len(),
        1,
        "no additional error should be posted for an already-faulted block"
    );
}


// ── process_output_f32 edge cases ────────────────────────────────────────

#[test]
fn process_output_fills_silence_for_invalid_output_index() {
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

    let mut out = vec![1.0f32; 8];
    process_output_f32(&runtime, 999, &mut out, 1);

    assert!(
        out.iter().all(|&v| v == 0.0),
        "invalid output index should fill with silence"
    );
}


#[test]
fn process_output_underrun_returns_silence_not_last_frame() {
    // Issue #496: was `..._repeats_last_frame` and pinned the
    // "brief sustain on underrun" form, which produced broadband
    // noise on every chain. Standard DAW behavior is silence on
    // underrun (a tiny gap is musically inaudible, repeated samples
    // are not).
    let chain = io_passthrough_chain("chain:underrun");
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_mono(),
        )
        .expect("runtime should build"),
    );
    let warmup = vec![0.0f32; FADE_IN_FRAMES + 16];
    process_input_f32(&runtime, 0, &warmup, 1);
    let mut drain = vec![0.0f32; warmup.len()];
    process_output_f32(&runtime, 0, &mut drain, 1);

    process_input_f32(&runtime, 0, &[0.5, 0.7], 1);
    let mut out = vec![0.0f32; 4];
    process_output_f32(&runtime, 0, &mut out, 1);

    assert!(
        (out[0] - 0.5).abs() < 1e-6,
        "frame 0 should be 0.5, got {}",
        out[0]
    );
    assert!(
        (out[1] - 0.7).abs() < 1e-6,
        "frame 1 should be 0.7, got {}",
        out[1]
    );
    assert!(
        out[2].abs() < 1e-6,
        "frame 2 should be silence (underrun), got {}",
        out[2]
    );
    assert!(
        out[3].abs() < 1e-6,
        "frame 3 should be silence (underrun), got {}",
        out[3]
    );
}


// ── ChainRuntimeState method tests ───────────────────────────────────────

#[test]
fn measured_latency_ms_returns_zero_initially() {
    let chain = tuner_track("chain:0", Vec::new());
    let runtime =
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[]).unwrap();
    assert!((runtime.measured_latency_ms() - 0.0).abs() < 1e-6);
}


// ── downcast_panic_message tests ─────────────────────────────────────────

#[test]
fn downcast_panic_str_message() {
    use super::downcast_panic_message;
    let payload: Box<dyn std::any::Any + Send> = Box::new("static string");
    assert_eq!(downcast_panic_message(payload), "static string");
}


#[test]
fn downcast_panic_string_message() {
    use super::downcast_panic_message;
    let payload: Box<dyn std::any::Any + Send> = Box::new(String::from("owned string"));
    assert_eq!(downcast_panic_message(payload), "owned string");
}


#[test]
fn downcast_panic_unknown_type_message() {
    use super::downcast_panic_message;
    let payload: Box<dyn std::any::Any + Send> = Box::new(42i32);
    assert_eq!(downcast_panic_message(payload), "unknown panic");
}


// ── process_input_f32 with stereo I/O ────────────────────────────────────

#[test]
fn process_input_stereo_output_preserves_channels() {
    let chain = Chain {
        id: ChainId("chain:stereo-io".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![IO_BINDING_ID.into()],
        blocks: vec![],
        di_output: None,
    };
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &io_registry_stereo(),
        )
        .expect("runtime should build"),
    );

    // Push enough frames to get past fade-in (interleaved stereo)
    let warmup = vec![0.0f32; (FADE_IN_FRAMES + 16) * 2];
    process_input_f32(&runtime, 0, &warmup, 2);
    let mut drain = vec![0.0f32; warmup.len()];
    process_output_f32(&runtime, 0, &mut drain, 2);

    // Interleaved stereo: L=0.3, R=0.7, L=0.5, R=0.9
    let input = [0.3f32, 0.7, 0.5, 0.9];
    process_input_f32(&runtime, 0, &input, 2);

    let mut out = vec![0.0f32; 4];
    process_output_f32(&runtime, 0, &mut out, 2);

    assert!(
        (out[0] - 0.3).abs() < 1e-6,
        "left ch frame 0: got {}",
        out[0]
    );
    assert!(
        (out[1] - 0.7).abs() < 1e-6,
        "right ch frame 0: got {}",
        out[1]
    );
    assert!(
        (out[2] - 0.5).abs() < 1e-6,
        "left ch frame 1: got {}",
        out[2]
    );
    assert!(
        (out[3] - 0.9).abs() < 1e-6,
        "right ch frame 1: got {}",
        out[3]
    );
}


// ── update_chain_runtime_state with reset_output_queue ───────────────────

#[test]
fn update_chain_runtime_state_with_reset_output_queue() {
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

    // Push some data
    process_input_f32(&runtime, 0, &[0.5, 0.7], 1);

    // Update with reset_output_queue=true should clear the buffer
    update_chain_runtime_state(
        &runtime,
        &chain,
        48_000.0,
        true,
        &[DEFAULT_ELASTIC_TARGET],
        &io_registry_mono(),
    )
    .expect("update should succeed");

    let mut out = vec![0.0f32; 2];
    process_output_f32(&runtime, 0, &mut out, 1);
    // After reset, output should be silence (no frames in queue)
    assert!(
        out.iter().all(|&v| v.abs() < 1e-6),
        "after reset_output_queue, output should be silent"
    );
}


// ── process_input_f32 edge cases ────────────────────────────────────────

#[test]
fn process_input_with_empty_data_does_not_panic() {
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
    process_input_f32(&runtime, 0, &[], 1);
}


#[test]
fn process_input_with_invalid_index_does_not_panic() {
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
    process_input_f32(&runtime, 999, &[0.5, 0.7], 1);
}


// ── ChainRuntimeState measured_latency_ms with stored nanos ─────────────

#[test]
fn measured_latency_ms_converts_nanos_correctly() {
    let chain = tuner_track("chain:lat", Vec::new());
    let runtime =
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[]).unwrap();
    // Store 5ms worth of nanos
    runtime
        .measured_latency_nanos
        .store(5_000_000, std::sync::atomic::Ordering::Relaxed);
    let ms = runtime.measured_latency_ms();
    assert!((ms - 5.0).abs() < 1e-3, "expected ~5.0ms, got {ms}");
}


// ── Regression: clear_draining re-arms callback after teardown (#316) ────

#[test]
fn chain_runtime_state_clear_draining_resets_flag() {
    // The draining flag is set by infra-cpal::teardown_active_chain_for_rebuild
    // before dropping the old streams. The Arc<ChainRuntimeState> is then
    // reused by the rebuild — without clear_draining the new streams' first
    // callbacks observe is_draining()==true and silence audio for every
    // segment (including sibling InputEntries on the same chain), until the
    // chain is fully removed and re-added. See issue #316.
    let chain = tuner_track("chain:drain", Vec::new());
    let runtime = build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[])
        .expect("runtime should build");
    runtime.set_draining();
    assert!(runtime.is_draining(), "set_draining should arm the flag");
    runtime.clear_draining();
    assert!(
        !runtime.is_draining(),
        "clear_draining must reset the flag so a runtime reused across a teardown+rebuild can resume audio (#316)"
    );
}


// ── Empty chain builds successfully ─────────────────────────────────────

#[test]
fn build_chain_runtime_state_empty_chain_succeeds() {
    let chain = Chain {
        id: ChainId("chain:empty".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
    };
    let runtime = build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[]);
    assert!(runtime.is_ok(), "empty chain should build successfully");
}


// ── RuntimeGraph remove non-existent chain ──────────────────────────────

#[test]
fn runtime_graph_remove_nonexistent_chain_no_panic() {
    use super::RuntimeGraph;
    let mut graph = RuntimeGraph {
        chains: HashMap::new(),
    };
    graph.remove_chain(&ChainId("does_not_exist".into()));
    assert!(graph.chains.is_empty());
}

