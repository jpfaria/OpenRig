//! Task 6 — migrate legacy chain I/O entries into the io_bindings registry.
//!
//! Contract:
//! - `migrate_legacy_io(project, io_bindings)` scans all chains for input/output
//!   blocks that still have `entries` (legacy, io/endpoint empty).
//! - For each such chain: collects all input + output entries, converts them to
//!   `IoEndpoint`s, and creates (or reuses) ONE `IoBinding` covering all endpoints.
//! - The binding id is deterministic (hash of sorted endpoints for dedup).
//! - Each input/output block gets `io = <binding_id>` and `endpoint = <entry_name>`,
//!   and its `entries` vec is drained.
//! - Running twice is a no-op (idempotent).

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::migrate_io_binding::migrate_legacy_io;
use project::project::Project;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_project(chains: Vec<Chain>) -> Project {
    Project {
        name: Some("Test".into()),
        device_settings: vec![],
        chains,
        midi: None,
    }
}

fn make_chain(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks,
    }
}

fn legacy_input(id: &str, device: &str, channels: Vec<usize>, mode: ChainInputMode) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: vec![InputEntry {
                device_id: DeviceId(device.into()),
                mode,
                channels,
            }],
        }),
    }
}

fn legacy_output(
    id: &str,
    device: &str,
    channels: Vec<usize>,
    mode: ChainOutputMode,
) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: vec![OutputEntry {
                device_id: DeviceId(device.into()),
                mode,
                channels,
            }],
        }),
    }
}

fn get_input_block(block: &AudioBlock) -> &InputBlock {
    match &block.kind {
        AudioBlockKind::Input(ib) => ib,
        other => panic!("expected InputBlock, got {:?}", other),
    }
}

fn get_output_block(block: &AudioBlock) -> &OutputBlock {
    match &block.kind {
        AudioBlockKind::Output(ob) => ob,
        other => panic!("expected OutputBlock, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// single_in_out: one input + one output entry → one binding, both blocks
// reference it by id+endpoint name; entries drained.
// ---------------------------------------------------------------------------

#[test]
fn single_in_out() {
    let mut project = make_project(vec![make_chain(
        "chain:0",
        vec![
            legacy_input("in:0", "coreaudio:in", vec![0], ChainInputMode::Mono),
            legacy_output("out:0", "coreaudio:out", vec![0, 1], ChainOutputMode::Stereo),
        ],
    )]);
    let mut bindings: Vec<IoBinding> = vec![];

    migrate_legacy_io(&mut project, &mut bindings);

    // Exactly one binding created.
    assert_eq!(bindings.len(), 1, "expected exactly one IoBinding");
    let binding = &bindings[0];
    assert!(!binding.id.is_empty(), "binding id must not be empty");
    assert_eq!(binding.inputs.len(), 1, "one input endpoint");
    assert_eq!(binding.outputs.len(), 1, "one output endpoint");

    // Both blocks now reference the binding.
    let chain = &project.chains[0];
    let ib = get_input_block(&chain.blocks[0]);
    assert_eq!(ib.io, binding.id, "input block io must match binding id");
    assert!(
        !ib.endpoint.is_empty(),
        "input block endpoint must be non-empty"
    );
    assert!(
        ib.entries.is_empty(),
        "input block entries must be drained after migration"
    );

    let ob = get_output_block(&chain.blocks[1]);
    assert_eq!(ob.io, binding.id, "output block io must match binding id");
    assert!(
        !ob.endpoint.is_empty(),
        "output block endpoint must be non-empty"
    );
    assert!(
        ob.entries.is_empty(),
        "output block entries must be drained after migration"
    );

    // Endpoint names match what the blocks reference.
    let in_ep_name = &ib.endpoint;
    let out_ep_name = &ob.endpoint;
    assert!(
        binding.inputs.iter().any(|ep| &ep.name == in_ep_name),
        "input endpoint '{}' must exist in binding; binding has: {:?}",
        in_ep_name,
        binding.inputs.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert!(
        binding.outputs.iter().any(|ep| &ep.name == out_ep_name),
        "output endpoint '{}' must exist in binding; binding has: {:?}",
        out_ep_name,
        binding.outputs.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// multi_in_out_all_to_all: chain with 2 inputs + 2 outputs → ONE binding
// holding all 4 endpoints (preserves today's all-to-all routing).
// ---------------------------------------------------------------------------

#[test]
fn multi_in_out_all_to_all() {
    let mut project = make_project(vec![make_chain(
        "chain:0",
        vec![
            legacy_input("in:0", "coreaudio:in", vec![0], ChainInputMode::Mono),
            legacy_input("in:1", "coreaudio:in", vec![1], ChainInputMode::Mono),
            legacy_output("out:0", "coreaudio:out", vec![0, 1], ChainOutputMode::Stereo),
            legacy_output("out:1", "monitors:out", vec![0, 1], ChainOutputMode::Stereo),
        ],
    )]);
    let mut bindings: Vec<IoBinding> = vec![];

    migrate_legacy_io(&mut project, &mut bindings);

    // All 4 endpoints in exactly ONE binding.
    assert_eq!(
        bindings.len(),
        1,
        "all-to-all routing must map to ONE binding, not {}: {:?}",
        bindings.len(),
        bindings.iter().map(|b| &b.id).collect::<Vec<_>>()
    );
    let binding = &bindings[0];
    assert_eq!(binding.inputs.len(), 2, "two input endpoints");
    assert_eq!(binding.outputs.len(), 2, "two output endpoints");

    // All 4 blocks reference the same binding id.
    let chain = &project.chains[0];
    for block in &chain.blocks {
        match &block.kind {
            AudioBlockKind::Input(ib) => {
                assert_eq!(ib.io, binding.id);
                assert!(ib.entries.is_empty(), "entries drained on input block");
            }
            AudioBlockKind::Output(ob) => {
                assert_eq!(ob.io, binding.id);
                assert!(ob.entries.is_empty(), "entries drained on output block");
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// dedup_across_chains: two chains with identical endpoint sets → one binding.
// ---------------------------------------------------------------------------

#[test]
fn dedup_across_chains() {
    let chain_a = make_chain(
        "chain:a",
        vec![
            legacy_input("in:a", "coreaudio:in", vec![0], ChainInputMode::Mono),
            legacy_output("out:a", "coreaudio:out", vec![0, 1], ChainOutputMode::Stereo),
        ],
    );
    // Identical endpoints, different block ids.
    let chain_b = make_chain(
        "chain:b",
        vec![
            legacy_input("in:b", "coreaudio:in", vec![0], ChainInputMode::Mono),
            legacy_output("out:b", "coreaudio:out", vec![0, 1], ChainOutputMode::Stereo),
        ],
    );

    let mut project = make_project(vec![chain_a, chain_b]);
    let mut bindings: Vec<IoBinding> = vec![];

    migrate_legacy_io(&mut project, &mut bindings);

    assert_eq!(
        bindings.len(),
        1,
        "identical endpoint sets across two chains must dedup to one binding; \
         got {} bindings: {:?}",
        bindings.len(),
        bindings.iter().map(|b| &b.id).collect::<Vec<_>>()
    );

    let binding_id = &bindings[0].id;

    // Both chains reference the same binding id.
    for chain in &project.chains {
        for block in &chain.blocks {
            match &block.kind {
                AudioBlockKind::Input(ib) => {
                    assert_eq!(
                        &ib.io, binding_id,
                        "chain '{}' input block must reference shared binding",
                        chain.id.0
                    );
                }
                AudioBlockKind::Output(ob) => {
                    assert_eq!(
                        &ob.io, binding_id,
                        "chain '{}' output block must reference shared binding",
                        chain.id.0
                    );
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// idempotent: running migrate twice → identical result, no duplicate bindings,
// entries stay drained.
// ---------------------------------------------------------------------------

#[test]
fn idempotent() {
    let mut project = make_project(vec![make_chain(
        "chain:0",
        vec![
            legacy_input("in:0", "coreaudio:in", vec![0], ChainInputMode::Mono),
            legacy_output("out:0", "coreaudio:out", vec![0, 1], ChainOutputMode::Stereo),
        ],
    )]);
    let mut bindings: Vec<IoBinding> = vec![];

    // First run.
    migrate_legacy_io(&mut project, &mut bindings);
    let after_first_count = bindings.len();
    let after_first_id = bindings[0].id.clone();

    // Snapshot block state after first run.
    let chain = &project.chains[0];
    let ib_io_after_first = get_input_block(&chain.blocks[0]).io.clone();
    let ob_io_after_first = get_output_block(&chain.blocks[1]).io.clone();

    // Second run.
    migrate_legacy_io(&mut project, &mut bindings);

    assert_eq!(
        bindings.len(),
        after_first_count,
        "second run must not add duplicate bindings"
    );
    assert_eq!(
        bindings[0].id, after_first_id,
        "binding id must be stable across runs"
    );

    let chain = &project.chains[0];
    assert_eq!(
        get_input_block(&chain.blocks[0]).io,
        ib_io_after_first,
        "input block io must be unchanged after second run"
    );
    assert_eq!(
        get_output_block(&chain.blocks[1]).io,
        ob_io_after_first,
        "output block io must be unchanged after second run"
    );
    assert!(
        get_input_block(&chain.blocks[0]).entries.is_empty(),
        "entries must still be empty after second run"
    );
    assert!(
        get_output_block(&chain.blocks[1]).entries.is_empty(),
        "entries must still be empty after second run"
    );
}

// ---------------------------------------------------------------------------
// mode_conversion: ChainInputMode/ChainOutputMode map to ChannelMode correctly.
// ---------------------------------------------------------------------------

#[test]
fn mode_conversion_mono_maps_to_channel_mode_mono() {
    let mut project = make_project(vec![make_chain(
        "chain:0",
        vec![
            legacy_input("in:0", "dev", vec![0], ChainInputMode::Mono),
            legacy_output("out:0", "dev", vec![0], ChainOutputMode::Mono),
        ],
    )]);
    let mut bindings: Vec<IoBinding> = vec![];

    migrate_legacy_io(&mut project, &mut bindings);

    let binding = &bindings[0];
    assert_eq!(
        binding.inputs[0].mode,
        ChannelMode::Mono,
        "ChainInputMode::Mono must convert to ChannelMode::Mono"
    );
    assert_eq!(
        binding.outputs[0].mode,
        ChannelMode::Mono,
        "ChainOutputMode::Mono must convert to ChannelMode::Mono"
    );
}

#[test]
fn mode_conversion_stereo_maps_to_channel_mode_stereo() {
    let mut project = make_project(vec![make_chain(
        "chain:0",
        vec![
            legacy_input("in:0", "dev", vec![0, 1], ChainInputMode::Stereo),
            legacy_output("out:0", "dev", vec![0, 1], ChainOutputMode::Stereo),
        ],
    )]);
    let mut bindings: Vec<IoBinding> = vec![];

    migrate_legacy_io(&mut project, &mut bindings);

    let binding = &bindings[0];
    assert_eq!(
        binding.inputs[0].mode,
        ChannelMode::Stereo,
        "ChainInputMode::Stereo must convert to ChannelMode::Stereo"
    );
    assert_eq!(
        binding.outputs[0].mode,
        ChannelMode::Stereo,
        "ChainOutputMode::Stereo must convert to ChannelMode::Stereo"
    );
}

#[test]
fn mode_conversion_dual_mono_input_maps_to_channel_mode_dual_mono() {
    // ChainOutputMode has no DualMono variant (outputs are always Mono or Stereo).
    // Only the input side has DualMono.
    let mut project = make_project(vec![make_chain(
        "chain:0",
        vec![
            legacy_input("in:0", "dev", vec![0, 1], ChainInputMode::DualMono),
            legacy_output("out:0", "dev", vec![0, 1], ChainOutputMode::Stereo),
        ],
    )]);
    let mut bindings: Vec<IoBinding> = vec![];

    migrate_legacy_io(&mut project, &mut bindings);

    let binding = &bindings[0];
    assert_eq!(
        binding.inputs[0].mode,
        ChannelMode::DualMono,
        "ChainInputMode::DualMono must convert to ChannelMode::DualMono"
    );
}

// ---------------------------------------------------------------------------
// skip_already_migrated: blocks with non-empty `io` are skipped (no double-
// migration, entries stay as-is).
// ---------------------------------------------------------------------------

#[test]
fn skip_already_migrated_blocks() {
    // A chain where the input block already has `io` set (already migrated).
    let already_migrated_input = AudioBlock {
        id: BlockId("in:0".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: "existing-binding".into(),
            endpoint: "Guitar In".into(),
            entries: vec![],
        }),
    };
    let already_migrated_output = AudioBlock {
        id: BlockId("out:0".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "existing-binding".into(),
            endpoint: "Monitor Out".into(),
            entries: vec![],
        }),
    };

    let mut project = make_project(vec![make_chain(
        "chain:0",
        vec![already_migrated_input, already_migrated_output],
    )]);
    let existing_binding = IoBinding {
        id: "existing-binding".into(),
        name: "Existing".into(),
        inputs: vec![IoEndpoint {
            name: "Guitar In".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Monitor Out".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    };
    let mut bindings = vec![existing_binding];

    migrate_legacy_io(&mut project, &mut bindings);

    // No new binding added.
    assert_eq!(
        bindings.len(),
        1,
        "already-migrated chain must not add new bindings"
    );

    // Blocks unchanged.
    let chain = &project.chains[0];
    let ib = get_input_block(&chain.blocks[0]);
    assert_eq!(ib.io, "existing-binding");
    let ob = get_output_block(&chain.blocks[1]);
    assert_eq!(ob.io, "existing-binding");
}

// ---------------------------------------------------------------------------
// empty_project: no crash, no bindings added.
// ---------------------------------------------------------------------------

#[test]
fn empty_project_is_noop() {
    let mut project = make_project(vec![]);
    let mut bindings: Vec<IoBinding> = vec![];

    migrate_legacy_io(&mut project, &mut bindings);

    assert!(bindings.is_empty(), "empty project must yield no bindings");
}
