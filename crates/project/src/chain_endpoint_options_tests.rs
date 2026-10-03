//! A chain lists each physical input and output once, however many of the
//! chain's I/O bindings carry it.

use super::*;
use crate::chain::{Chain, EndpointRef};
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};

fn ep(name: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("hd8".into()),
        mode: if channels.len() == 2 {
            ChannelMode::Stereo
        } else {
            ChannelMode::Mono
        },
        channels,
    }
}

fn binding(id: &str, inputs: Vec<IoEndpoint>, outputs: Vec<IoEndpoint>) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.to_uppercase(),
        inputs,
        outputs,
    }
}

/// The owner's ANALOGICO chain: two bindings share the guitar input; one
/// carries MAIN + FRFR, the other the SYN-5050 output; a third repeats MAIN.
fn registry() -> Vec<IoBinding> {
    vec![
        binding(
            "g2",
            vec![ep("GUITARRA 2", vec![1])],
            vec![ep("MAIN", vec![0, 1]), ep("FRFR", vec![14, 15])],
        ),
        binding(
            "g2-syn",
            vec![ep("GUITARRA 2", vec![1])],
            vec![ep("SYN-5050", vec![4, 5])],
        ),
        binding("main", vec![], vec![ep("MAIN", vec![0, 1])]),
    ]
}

fn chain(ids: &[&str]) -> Chain {
    Chain {
        id: ChainId("c".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: ids.iter().map(|s| s.to_string()).collect(),
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

fn r(binding: &str, endpoint: &str) -> EndpointRef {
    EndpointRef {
        binding_id: binding.into(),
        endpoint: endpoint.into(),
    }
}

#[test]
fn an_input_shared_by_two_bindings_is_listed_once() {
    let (inputs, _) = chain_endpoint_labels(&chain(&["g2", "g2-syn"]), &registry());
    assert_eq!(inputs, vec!["GUITARRA 2".to_string()]);
}

#[test]
fn an_output_shared_by_two_bindings_is_listed_once() {
    let (_, outputs) = chain_endpoint_labels(&chain(&["g2", "g2-syn", "main"]), &registry());
    assert_eq!(outputs, vec!["MAIN", "FRFR", "SYN-5050"]);
}

#[test]
fn two_different_endpoints_with_the_same_name_are_both_listed_by_binding() {
    let reg = vec![
        binding("a", vec![ep("In", vec![0])], vec![]),
        binding("b", vec![ep("In", vec![1])], vec![]),
    ];
    let (inputs, _) = chain_endpoint_labels(&chain(&["a", "b"]), &reg);
    assert_eq!(inputs, vec!["A · In", "B · In"]);
}

#[test]
fn an_option_picks_the_endpoint_it_shows() {
    let c = chain(&["g2", "g2-syn", "main"]);
    let reg = registry();
    assert_eq!(input_option_ref(&c, &reg, 0), Some(r("g2", "GUITARRA 2")));
    assert_eq!(
        output_option_ref(&c, &reg, 2),
        Some(r("g2-syn", "SYN-5050"))
    );
    assert_eq!(output_option_ref(&c, &reg, 3), None);
}

#[test]
fn a_loop_on_a_repeated_endpoint_shows_that_endpoint_selected() {
    let c = chain(&["g2", "g2-syn", "main"]);
    let reg = registry();
    // MAIN reached through the third binding is the same MAIN option (0).
    assert_eq!(output_option_index(&c, &reg, Some(&r("main", "MAIN"))), 0);
    assert_eq!(
        output_option_index(&c, &reg, Some(&r("g2-syn", "SYN-5050"))),
        2
    );
    assert_eq!(
        input_option_index(&c, &reg, Some(&r("g2-syn", "GUITARRA 2"))),
        0
    );
    assert_eq!(input_option_index(&c, &reg, None), 0);
}
