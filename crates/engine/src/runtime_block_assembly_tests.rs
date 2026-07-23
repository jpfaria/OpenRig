//! Engine runtime tests (issue #792 split from runtime_tests.rs).
//! Grouped by responsibility; shared fixtures live in `runtime_tests.rs`.
#![allow(unused_imports)]
use super::*;
use super::tests::*;

#[test]
#[ignore] // IR cabs migrated to disk packages (issue #287); needs registry lookup
fn runtime_graph_builds_for_chain_with_cab_block() {
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
            loopers: vec![],
        }],
        midi: None,
    };

    let runtime = build_runtime_graph(
        &project,
        &HashMap::from([(ChainId("chain:0".into()), 48_000.0)]),
        &HashMap::new(),
        &[],
    )
    .expect("runtime graph should build");
    assert_eq!(runtime.chains.len(), 1);
}


#[test]
#[ignore] // requires asset_paths initialization
fn update_chain_runtime_state_preserves_unchanged_block_instances() {
    let mut chain = tuner_track(
        "chain:0",
        vec![
            tuner_block("chain:0:block:a", 440.0),
            tuner_block("chain:0:block:b", 445.0),
        ],
    );

    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[])
            .expect("runtime state should build"),
    );
    let original_serials = {
        let locked = runtime.processing.lock().expect("runtime poisoned");
        locked.input_states[0]
            .blocks
            .iter()
            .map(|block| block.instance_serial)
            .collect::<Vec<_>>()
    };

    if let AudioBlockKind::Core(core) = &mut chain.blocks[1].kind {
        core.params
            .insert("reference_hz", ParameterValue::Float(432.0));
    }

    update_chain_runtime_state(
        &runtime,
        &chain,
        48_000.0,
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &[],
    )
    .expect("runtime update should succeed");

    let updated_serials = {
        let locked = runtime.processing.lock().expect("runtime poisoned");
        locked.input_states[0]
            .blocks
            .iter()
            .map(|block| block.instance_serial)
            .collect::<Vec<_>>()
    };

    assert_eq!(updated_serials[0], original_serials[0]);
    assert_ne!(updated_serials[1], original_serials[1]);
}


#[test]
#[ignore] // requires asset_paths initialization
fn update_chain_runtime_state_preserves_block_identity_when_reordered() {
    let mut chain = tuner_track(
        "chain:0",
        vec![
            tuner_block("chain:0:block:a", 440.0),
            tuner_block("chain:0:block:b", 445.0),
        ],
    );

    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[])
            .expect("runtime state should build"),
    );
    let original_by_block_id = {
        let locked = runtime.processing.lock().expect("runtime poisoned");
        locked.input_states[0]
            .blocks
            .iter()
            .map(|block| (block.block_id.clone(), block.instance_serial))
            .collect::<HashMap<_, _>>()
    };

    chain.blocks.swap(0, 1);

    update_chain_runtime_state(
        &runtime,
        &chain,
        48_000.0,
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &[],
    )
    .expect("runtime update should succeed");

    let reordered = runtime.processing.lock().expect("runtime poisoned");
    assert_eq!(reordered.input_states[0].blocks.len(), 2);
    for block in &reordered.input_states[0].blocks {
        assert_eq!(
            Some(&block.instance_serial),
            original_by_block_id.get(&block.block_id)
        );
    }
}


#[test]
#[ignore] // requires asset_paths initialization
fn select_block_builds_for_generic_delay_options() {
    let chain = select_delay_chain("chain:select", "delay_a");

    let runtime = build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[])
        .expect("select delay chain should build");

    let locked = runtime.processing.lock().expect("runtime poisoned");
    assert_eq!(locked.input_states[0].blocks.len(), 1);
}


#[test]
fn panicking_processor_marks_block_as_faulted() {
    let mut block = panicking_block_node();
    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    apply_block_processor(&mut block, &mut frames, &error_queue);

    assert!(
        block.faulted,
        "block should be marked faulted after a panic"
    );
}


#[test]
fn faulted_block_is_permanently_bypassed() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.faulted = true; // pre-fault the block

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    apply_block_processor(&mut block, &mut frames, &error_queue);

    assert_eq!(
        counter.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "process_sample should never be called on a faulted block"
    );
}


#[test]
fn process_audio_block_bypassed_state_skips_processing() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.fade_state = FadeState::Bypassed;

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    process_audio_block(&mut block, &mut frames, &error_queue);

    assert_eq!(
        counter.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "bypassed block should not call process_sample"
    );
}


#[test]
fn process_audio_block_fading_in_applies_processing() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.fade_state = FadeState::FadingIn {
        frames_remaining: FADE_IN_FRAMES,
    };

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    process_audio_block(&mut block, &mut frames, &error_queue);

    assert!(
        counter.load(std::sync::atomic::Ordering::SeqCst) > 0,
        "fading-in block should call process_sample"
    );
}


#[test]
fn poll_stream_returns_none_for_unknown_block() {
    let chain = tuner_track("chain:0", Vec::new());
    let runtime =
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[]).unwrap();
    assert!(runtime
        .poll_stream(&BlockId("nonexistent".into()))
        .is_none());
}


// ── next_block_instance_serial tests ────────────────────────────────────

#[test]
fn next_block_instance_serial_increments() {
    use super::next_block_instance_serial;
    let a = next_block_instance_serial();
    let b = next_block_instance_serial();
    assert!(b > a, "serial should increment monotonically");
}


// ── build_chain_runtime_state with only effects (no I/O blocks) ─────────

#[test]
fn build_chain_runtime_state_no_io_blocks_uses_fallback() {
    let chain = Chain {
        id: ChainId("chain:no-io".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![tuner_block("b:0", 440.0)],
        di_output: None,
        loopers: vec![],
    };
    let runtime = build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[]);
    assert!(runtime.is_ok(), "should build with fallback I/O");
}


// ── bypass_runtime_node tests ───────────────────────────────────────────

#[test]
fn bypass_runtime_node_has_bypass_processor() {
    use super::bypass_runtime_node;
    let block = AudioBlock {
        id: BlockId("test:bypass".into()),
        enabled: false,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    };
    let node = bypass_runtime_node(&block, AudioChannelLayout::Mono, true);
    assert!(matches!(node.processor, RuntimeProcessor::Bypass));
    assert_eq!(node.block_id.0, "test:bypass");
    assert_eq!(node.input_layout, AudioChannelLayout::Mono);
    assert_eq!(node.output_layout, AudioChannelLayout::Mono);
}


// ── SelectRuntimeState selected_node_mut ────────────────────────────────

#[test]
fn select_runtime_state_finds_selected_option() {
    use super::SelectRuntimeState;
    let mut state = SelectRuntimeState {
        selected_block_id: BlockId("opt:b".into()),
        options: vec![
            counting_block_node(std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0))),
            {
                let mut node = counting_block_node(std::sync::Arc::new(
                    std::sync::atomic::AtomicUsize::new(0),
                ));
                node.block_id = BlockId("opt:b".into());
                node
            },
        ],
    };
    let found = state.selected_node_mut();
    assert!(found.is_some());
    assert_eq!(found.unwrap().block_id.0, "opt:b");
}


#[test]
fn select_runtime_state_returns_none_when_no_match() {
    use super::SelectRuntimeState;
    let mut state = SelectRuntimeState {
        selected_block_id: BlockId("nonexistent".into()),
        options: vec![counting_block_node(std::sync::Arc::new(
            std::sync::atomic::AtomicUsize::new(0),
        ))],
    };
    assert!(state.selected_node_mut().is_none());
}

