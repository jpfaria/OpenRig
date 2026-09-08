//! #928 — every meter row names its E/S. The rows are the chain's streams
//! (`chain_stream_count`), so the labels come from the same segment map: one
//! (input E/S, output E/S) pair per stream, in the same order.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime_graph::chain_stream_count;
use engine::stream_io_labels::chain_stream_io_labels;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InsertBlock};
use project::chain::Chain;
use project::param::ParameterSet;

fn ep(name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("coreaudio:quantum".into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn binding(id: &str, name: &str, input: &[usize], out: &[usize]) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: name.into(),
        inputs: vec![ep("in", ChannelMode::Mono, input)],
        outputs: vec![ep("out", ChannelMode::Stereo, out)],
    }
}

fn effect(id: &str) -> AudioBlock {
    AudioBlock {
        id: domain::ids::BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn insert(id: &str, io: &str, enabled: bool) -> AudioBlock {
    AudioBlock {
        id: domain::ids::BlockId(id.into()),
        enabled,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".into(),
            io: io.into(),
        }),
    }
}

fn chain(bindings: &[&str], blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("rig:input-2".into()),
        description: Some("GUITARRA 1".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: bindings.iter().map(|b| b.to_string()).collect(),
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

/// The owner's registry: two E/S reading the same input (ch 0) into different
/// outputs, plus the SYN-2 loop.
fn registry() -> Vec<IoBinding> {
    vec![
        binding("guitarra-1", "GUITARRA 1 - MAIN", &[0], &[0, 1]),
        binding("guitarra-1-5050", "GUITARRA 1 - SYN5050", &[0], &[16, 17]),
        binding("syn2-main", "SYN-2 (Re-amp 1 -> MAIN OUT)", &[17, 18], &[10]),
    ]
}

fn pairs(labels: &[engine::stream_io_labels::StreamIoLabels]) -> Vec<(String, String)> {
    labels.iter().map(|l| (l.input.clone(), l.output.clone())).collect()
}

#[test]
fn two_bindings_on_one_input_name_their_own_row() {
    let registry = registry();
    let chain = chain(&["guitarra-1", "guitarra-1-5050"], vec![effect("gate"), insert("syn2", "syn2-main", false)]);

    let labels = chain_stream_io_labels(&chain, &registry);

    assert_eq!(labels.len(), chain_stream_count(&chain, &registry), "one label per stream");
    assert_eq!(
        pairs(&labels),
        vec![
            ("GUITARRA 1 - MAIN".to_string(), "GUITARRA 1 - MAIN".to_string()),
            ("GUITARRA 1 - SYN5050".to_string(), "GUITARRA 1 - SYN5050".to_string()),
        ],
        "#928: both rows read ch 0 — the E/S the row belongs to is the one whose output it feeds"
    );
}

#[test]
fn an_enabled_insert_names_the_loop_on_the_rows_it_splits() {
    let registry = registry();
    let chain = chain(&["guitarra-1", "guitarra-1-5050"], vec![effect("gate"), insert("syn2", "syn2-main", true), effect("delay")]);

    let labels = chain_stream_io_labels(&chain, &registry);

    assert_eq!(labels.len(), chain_stream_count(&chain, &registry), "one label per stream");
    assert_eq!(
        pairs(&labels),
        vec![
            ("GUITARRA 1 - MAIN".to_string(), "SYN-2 (Re-amp 1 -> MAIN OUT)".to_string()),
            ("GUITARRA 1 - SYN5050".to_string(), "SYN-2 (Re-amp 1 -> MAIN OUT)".to_string()),
            (
                "SYN-2 (Re-amp 1 -> MAIN OUT)".to_string(),
                "GUITARRA 1 - MAIN + GUITARRA 1 - SYN5050".to_string()
            ),
        ],
        "#928: the head rows send into the loop; the return row comes back from it and feeds both tails"
    );
}

#[test]
fn a_single_binding_chain_names_it_on_both_sides() {
    let registry = registry();
    let chain = chain(&["guitarra-1"], vec![effect("gate")]);

    assert_eq!(
        pairs(&chain_stream_io_labels(&chain, &registry)),
        vec![("GUITARRA 1 - MAIN".to_string(), "GUITARRA 1 - MAIN".to_string())]
    );
}
