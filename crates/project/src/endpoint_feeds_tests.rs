//! #328 — the endpoint checklist of a chain's input and output nodes: which
//! node uses which endpoint of the chain's own E/S.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};

use super::{
    checklist_silences, head_input_enabled, inputs_all_unchecked, outputs_all_unchecked, tail_feed,
    TailFeed,
};
use crate::block::split_params::default_split_params;
use crate::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock, SplitBlock, SplitEnd};
use crate::chain::Chain;
use crate::endpoint_disables::{EndpointDisables, EndpointRef};

fn ep(name: &str, ch: usize) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    }
}

/// One E/S with two inputs and two outputs.
fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![ep("in-1", 0), ep("in-2", 1)],
        outputs: vec![ep("out-1", 0), ep("out-2", 1)],
    }]
}

fn r(endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: "main".into(),
        endpoint: endpoint.into(),
    }
}

fn disables(
    inputs: &[&str],
    outputs: &[&str],
    path_a: &[&str],
    path_b: &[&str],
) -> EndpointDisables {
    let refs = |names: &[&str]| names.iter().map(|name| r(name)).collect::<Vec<_>>();
    EndpointDisables {
        inputs: refs(inputs),
        outputs: refs(outputs),
        path_a_outputs: refs(path_a),
        path_b_outputs: refs(path_b),
    }
}

fn split(end: SplitEnd) -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end,
            params: default_split_params(),
            a: vec![],
            b: vec![],
        }),
    }
}

fn mid_input() -> AudioBlock {
    AudioBlock {
        id: BlockId("mid-in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "aux-in".into(),
        }),
    }
}

fn mid_output() -> AudioBlock {
    AudioBlock {
        id: BlockId("mid-out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "aux-out".into(),
        }),
    }
}

fn chain(blocks: Vec<AudioBlock>, disabled_endpoints: EndpointDisables) -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks,
        di_output: None,
        loopers: vec![],
        disabled_endpoints,
    }
}

#[test]
fn an_unchecked_input_is_off_for_the_input_node_only() {
    let chain = chain(vec![], disables(&["in-1"], &[], &[], &[]));
    assert!(
        !head_input_enabled(&chain, "main", "in-1"),
        "#328: in-1 is unchecked on the input node"
    );
    assert!(
        head_input_enabled(&chain, "main", "in-2"),
        "in-2 stays checked"
    );
    assert_eq!(
        tail_feed(&chain, "main", "out-1"),
        TailFeed::Chain,
        "the output node is untouched"
    );
}

#[test]
fn a_linear_chain_sends_to_what_its_output_node_checks() {
    let chain = chain(vec![], disables(&[], &["out-2"], &["out-1"], &["out-1"]));
    assert_eq!(
        tail_feed(&chain, "main", "out-2"),
        TailFeed::Off,
        "#328: out-2 is unchecked on the chain output node"
    );
    assert_eq!(
        tail_feed(&chain, "main", "out-1"),
        TailFeed::Chain,
        "#328: the path lists mean nothing without a Y split"
    );
}

#[test]
fn a_mix_split_sends_through_the_chain_output_node() {
    let chain = chain(
        vec![split(SplitEnd::Mix)],
        disables(&[], &["out-1"], &[], &[]),
    );
    assert_eq!(tail_feed(&chain, "main", "out-1"), TailFeed::Off);
    assert_eq!(
        tail_feed(&chain, "main", "out-2"),
        TailFeed::Chain,
        "#328: a Split → Mix ends in the chain output node"
    );
}

#[test]
fn a_y_split_sends_each_output_from_the_paths_that_check_it() {
    let chain = chain(
        vec![split(SplitEnd::Y)],
        disables(&[], &["out-1", "out-2"], &["out-2"], &["out-1"]),
    );
    assert_eq!(
        tail_feed(&chain, "main", "out-1"),
        TailFeed::Paths { a: true, b: false },
        "#328: path A has out-1 checked, path B does not"
    );
    assert_eq!(
        tail_feed(&chain, "main", "out-2"),
        TailFeed::Paths { a: false, b: true },
        "#328: the chain output list means nothing on a Y chain"
    );
}

#[test]
fn a_y_output_both_paths_uncheck_is_off() {
    let chain = chain(
        vec![split(SplitEnd::Y)],
        disables(&[], &[], &["out-1"], &["out-1"]),
    );
    assert_eq!(
        tail_feed(&chain, "main", "out-1"),
        TailFeed::Off,
        "#328: no path sends to out-1"
    );
    assert_eq!(
        tail_feed(&chain, "main", "out-2"),
        TailFeed::Paths { a: true, b: true },
        "#328: checked on both paths by default"
    );
}

#[test]
fn a_reference_the_bindings_no_longer_have_is_ignored() {
    let chain = chain(vec![], disables(&["gone"], &["gone"], &[], &[]));
    assert!(head_input_enabled(&chain, "main", "in-1"));
    assert!(
        !inputs_all_unchecked(&chain, &registry()),
        "#328: a stale ref must not count as an unchecked input"
    );
    assert!(!outputs_all_unchecked(&chain, &registry()));
}

#[test]
fn unchecking_every_input_silences_the_chain_unless_a_mid_input_feeds_it() {
    let silenced = chain(vec![], disables(&["in-1", "in-2"], &[], &[], &[]));
    assert!(
        inputs_all_unchecked(&silenced, &registry()),
        "#328: no head input is left"
    );
    assert!(checklist_silences(&silenced, &registry()));
    let fed = chain(
        vec![mid_input()],
        disables(&["in-1", "in-2"], &[], &[], &[]),
    );
    assert!(
        !inputs_all_unchecked(&fed, &registry()),
        "#328: a mid Input is its own node and still feeds the chain"
    );
}

#[test]
fn unchecking_every_output_silences_the_chain_unless_a_mid_output_takes_it() {
    let linear = chain(vec![], disables(&[], &["out-1", "out-2"], &[], &[]));
    assert!(outputs_all_unchecked(&linear, &registry()));
    assert!(checklist_silences(&linear, &registry()));
    let tapped = chain(
        vec![mid_output()],
        disables(&[], &["out-1", "out-2"], &[], &[]),
    );
    assert!(
        !outputs_all_unchecked(&tapped, &registry()),
        "#328: a mid Output still plays the chain"
    );
    let y_silent = chain(
        vec![split(SplitEnd::Y)],
        disables(&[], &[], &["out-1", "out-2"], &["out-1", "out-2"]),
    );
    assert!(
        outputs_all_unchecked(&y_silent, &registry()),
        "#328: no path sends anywhere"
    );
    let y_one = chain(
        vec![split(SplitEnd::Y)],
        disables(&[], &[], &["out-1", "out-2"], &["out-1"]),
    );
    assert!(
        !outputs_all_unchecked(&y_one, &registry()),
        "#328: path B still sends to out-2"
    );
}

#[test]
fn a_chain_without_bindings_is_never_silenced_by_the_checklist() {
    let mut legacy = chain(
        vec![],
        disables(&["in-1", "in-2"], &["out-1", "out-2"], &[], &[]),
    );
    legacy.io_binding_ids.clear();
    assert!(
        !checklist_silences(&legacy, &registry()),
        "#328: nothing to uncheck — the legacy fallback endpoints stay"
    );
}
