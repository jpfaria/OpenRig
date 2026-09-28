//! #947: an output stream mixes ONLY the runtimes that write ITS output. Two
//! guitars on two bindings whose outputs sit on one interface are two runtimes
//! on one device — but guitar 2's runtime writes nothing to guitar 1's output,
//! so guitar 1's output stream must not hold guitar 2's runtime at all.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::build_per_input_runtime_states;
use project::chain::Chain;

use super::*;

fn binding(id: &str, out_channels: Vec<usize>) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.into(),
        inputs: vec![IoEndpoint {
            name: "In".into(),
            device_id: DeviceId(format!("{id}-in")),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Out".into(),
            device_id: DeviceId("shared-out".into()),
            mode: ChannelMode::Stereo,
            channels: out_channels,
        }],
    }
}

#[test]
fn each_guitar_output_stream_holds_only_its_own_guitar_runtime() {
    let registry = vec![
        binding("guitar-1", vec![0, 1]),
        binding("guitar-2", vec![2, 3]),
    ];
    let chain = Chain {
        id: ChainId("rig:input-4".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitar-1".into(), "guitar-2".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    };
    let runtimes =
        build_per_input_runtime_states(&chain, 48_000.0, &HashMap::new(), &[], &registry)
            .expect("two-binding chain builds");
    let slots = build_chain_slots(&runtimes);
    let map: Vec<Vec<String>> = runtimes.iter().map(|_| vec!["shared-out".into()]).collect();

    for (output_index, (_, own)) in runtimes.iter().enumerate() {
        let mixed = slots_for_output_stream(&slots, &map, "shared-out", output_index);
        assert_eq!(
            mixed.len(),
            1,
            "output {output_index} must hold ONLY the guitar that writes it, not the other guitar"
        );
        assert!(
            Arc::ptr_eq(&mixed[0].load(), own),
            "output {output_index} holds the wrong guitar's runtime"
        );
    }
}

/// #967: a bound insert's send stream is opened with the chain even while the
/// insert is OFF — and it must hold the chain's runtime then too. Its route is
/// unwritten while the loop is off (the stream plays silence), but the switch
/// ON is a DSP rebuild published into the SAME slot: a send stream built with no
/// slot would stay silent after it, the gear would get nothing and everything
/// after the insert would go quiet.
#[test]
fn a_disabled_inserts_send_stream_holds_the_chain_runtime() {
    use project::block::{AudioBlock, AudioBlockKind, InsertBlock};

    let ep = |name: &str, dev: &str, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(dev.into()),
        mode: ChannelMode::Mono,
        channels,
    };
    let registry = vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![ep("in", "hd8", vec![0])],
            outputs: vec![ep("out", "hd8", vec![0])],
        },
        IoBinding {
            id: "fx".into(),
            name: "SYN-2".into(),
            inputs: vec![ep("ret", "hd8", vec![17])],
            outputs: vec![ep("snd", "hd8", vec![10])],
        },
    ];
    let chain = Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![AudioBlock {
            id: domain::ids::BlockId("syn2".into()),
            enabled: false,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "fx".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    };
    let runtimes =
        build_per_input_runtime_states(&chain, 48_000.0, &HashMap::new(), &[], &registry)
            .expect("the chain builds");
    let slots = build_chain_slots(&runtimes);
    let map: Vec<Vec<String>> = runtimes.iter().map(|_| vec!["hd8".into()]).collect();

    let send_route = 1;
    assert_eq!(
        slots_for_output_stream(&slots, &map, "hd8", send_route).len(),
        1,
        "#967: the switched-off loop's send stream must hold the chain's runtime"
    );
}

fn insert_block(enabled: bool) -> project::block::AudioBlock {
    project::block::AudioBlock {
        id: domain::ids::BlockId("loop".into()),
        enabled,
        kind: project::block::AudioBlockKind::Insert(project::block::InsertBlock {
            model: "standard".into(),
            io: "fx".into(),
        }),
    }
}

fn mono(name: &str, dev: &str, ch: usize) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(dev.into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    }
}

/// #967: two E/S on two interfaces with the loop OFF are two isolated
/// runtimes. The loop's send stream (on the Scarlett) must hold neither of
/// them — switching the loop ON regroups the chain into one pipeline and gets
/// new streams anyway, so binding them now only puts the TEYUN runtime on a
/// Scarlett callback.
#[test]
fn a_multi_runtime_chains_loop_send_holds_no_runtime_while_the_loop_is_off() {
    let registry = vec![
        IoBinding {
            id: "scarlett".into(),
            name: "SCARLETT".into(),
            inputs: vec![mono("in", "scarlett", 0)],
            outputs: vec![mono("out", "scarlett", 0)],
        },
        IoBinding {
            id: "teyun".into(),
            name: "TEYUN".into(),
            inputs: vec![mono("in", "teyun", 0)],
            outputs: vec![mono("out", "teyun", 0)],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![mono("ret", "scarlett", 3)],
            outputs: vec![mono("snd", "scarlett", 3)],
        },
    ];
    let chain = Chain {
        id: ChainId("rig:input-2".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["scarlett".into(), "teyun".into()],
        blocks: vec![insert_block(false)],
        di_output: None,
        loopers: vec![],
    };
    let runtimes =
        build_per_input_runtime_states(&chain, 48_000.0, &HashMap::new(), &[], &registry)
            .expect("the chain builds");
    assert_eq!(runtimes.len(), 2, "loop off: one runtime per E/S");
    let slots = build_chain_slots(&runtimes);
    let heads = engine::runtime_endpoints::resolve_chain_io(&chain, &registry).0;
    let map = crate::chain_resolve_io_map::output_devices_by_input_cpal(&chain, &registry, &heads);
    assert_eq!(
        slots_for_output_stream(&slots, &map, "scarlett", 2).len(),
        0,
        "#967: the switched-off loop's send must not hold another E/S's runtime"
    );
    // Each E/S's own output holds ONLY its own runtime (#716/#947), even with
    // every tail device listed for the chain's inputs.
    for (route, device, owner) in [(0, "scarlett", 0), (1, "teyun", 1)] {
        let held = slots_for_output_stream(&slots, &map, device, route);
        assert_eq!(held.len(), 1, "output {route} holds exactly one runtime");
        assert!(
            Arc::ptr_eq(&held[0].load(), &runtimes[owner].1),
            "output {route} ({device}) holds another E/S's runtime"
        );
    }
}

/// #967: an output-only E/S B next to A (with the guitar) and a loop: with the
/// loop OFF only A's head plays and it pairs with A's outputs, so nothing
/// writes B's output; with the loop ON the return feeds every tail, B's
/// included. The switch keeps the single runtime (a DSP rebuild), so B's output
/// stream must hold it in BOTH states — on another interface too, which the
/// guitar's own E/S never lists. The device map is the real one.
#[test]
fn a_tail_only_the_cut_writes_is_bound_on_any_interface() {
    let registry = vec![
        IoBinding {
            id: "a".into(),
            name: "A".into(),
            inputs: vec![mono("in", "scarlett", 0)],
            outputs: vec![mono("out", "scarlett", 0)],
        },
        IoBinding {
            id: "b".into(),
            name: "B".into(),
            inputs: vec![],
            outputs: vec![mono("out", "teyun", 0)],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![mono("ret", "scarlett", 3)],
            outputs: vec![mono("snd", "scarlett", 3)],
        },
    ];
    for loop_on in [false, true] {
        let chain = Chain {
            id: ChainId("rig:input-1".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["a".into(), "b".into()],
            blocks: vec![insert_block(loop_on)],
            di_output: None,
            loopers: vec![],
        };
        let runtimes =
            build_per_input_runtime_states(&chain, 48_000.0, &HashMap::new(), &[], &registry)
                .expect("the chain builds");
        let slots = build_chain_slots(&runtimes);
        let heads = engine::runtime_endpoints::resolve_chain_io(&chain, &registry).0;
        let map =
            crate::chain_resolve_io_map::output_devices_by_input_cpal(&chain, &registry, &heads);
        let b_out = 1;
        assert_eq!(
            slots_for_output_stream(&slots, &map, "teyun", b_out).len(),
            1,
            "#967: B's output (TEYUN) must hold the runtime the loop's cut writes it \
             from — loop {}",
            if loop_on { "on" } else { "off" }
        );
    }
}
