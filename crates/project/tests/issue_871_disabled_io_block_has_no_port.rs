//! #871 — a DISABLED mid `Input`/`Output` block contributes no I/O port.
//!
//! A block toggled off is not part of the signal path, so it must not open a
//! stream nor claim a device route. The owner's report: an `Input` block bound
//! to the Scarlett's channel 1 (physical input 2) and an `Output` block bound
//! to the TEYUN — BOTH disabled — still fed audio, and the Scarlett input came
//! out of the TEYUN output (a stream-isolation violation on top of the leak).

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock};
use project::chain::Chain;
use project::param::ParameterSet;

const SCARLETT: &str = "coreaudio:Focusrite:Scarlett 2i2";
const TEYUN: &str = "coreaudio:TEYUN Q26";

fn ep(name: &str, device: &str, ch: usize) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    }
}

fn stereo_out(name: &str, device: &str) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Stereo,
        channels: vec![0, 1],
    }
}

fn effect(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn input_block(id: &str, enabled: bool, io: &str, endpoint: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: io.into(),
            endpoint: endpoint.into(),
        }),
    }
}

fn output_block(id: &str, enabled: bool, io: &str, endpoint: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: io.into(),
            endpoint: endpoint.into(),
        }),
    }
}

fn chain_with(io_binding_ids: Vec<String>, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("guitarra-tones".into()),
        description: Some("GUITARRA - TONES".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids,
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

/// The owner's registry: the head bindings the chain selects (Scarlett in 1,
/// TEYUN in 1) plus the two the disabled blocks reference.
fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "io-4-d73c".into(),
            name: "SCARLET - 1".into(),
            inputs: vec![ep("In 1", SCARLETT, 0)],
            outputs: vec![stereo_out("Out 1", SCARLETT)],
        },
        IoBinding {
            id: "io-5-79a0".into(),
            name: "TEYUN - 1".into(),
            inputs: vec![ep("In 1", TEYUN, 0)],
            outputs: vec![stereo_out("Out 1", TEYUN)],
        },
        IoBinding {
            id: "io-3-47e6".into(),
            name: "SCARLET - 2".into(),
            inputs: vec![ep("In 1", SCARLETT, 1)],
            outputs: vec![stereo_out("Out 1", SCARLETT)],
        },
        IoBinding {
            id: "io-2-fb90".into(),
            name: "TEYUN - ALL".into(),
            inputs: vec![ep("In 1", TEYUN, 0), ep("In 2", TEYUN, 1)],
            outputs: vec![stereo_out("Out 1", TEYUN)],
        },
    ]
}

#[test]
fn disabled_mid_input_block_resolves_no_port() {
    let chain = chain_with(
        vec![],
        vec![
            effect("A"),
            input_block("mid:in", false, "io-3-47e6", "In 1"),
        ],
    );
    let ports = resolve_chain_ports(&chain, &registry());
    assert!(
        ports.is_empty(),
        "a disabled Input block is out of the signal path — no port, no stream; got {:?}",
        ports
            .iter()
            .map(|p| (p.direction, p.binding_id.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn disabled_mid_output_block_resolves_no_port() {
    let chain = chain_with(
        vec![],
        vec![
            effect("A"),
            output_block("mid:out", false, "io-2-fb90", "Out 1"),
        ],
    );
    let ports = resolve_chain_ports(&chain, &registry());
    assert!(
        ports.is_empty(),
        "a disabled Output block claims no device route; got {:?}",
        ports
            .iter()
            .map(|p| (p.direction, p.binding_id.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn enabled_mid_blocks_still_resolve() {
    // The guard above must not silence the working case (#85 / #716).
    let chain = chain_with(
        vec![],
        vec![
            input_block("mid:in", true, "io-3-47e6", "In 1"),
            output_block("mid:out", true, "io-2-fb90", "Out 1"),
        ],
    );
    let ports = resolve_chain_ports(&chain, &registry());
    assert_eq!(ports.len(), 2, "enabled mid blocks keep their ports");
}

#[test]
fn owners_rig_does_not_open_the_scarletts_second_input() {
    // GUITARRA - TONES: head = Scarlett in 1 + TEYUN in 1. The disabled mid
    // Input (Scarlett channel 1 = physical input 2) and the disabled mid
    // Output (TEYUN) must contribute nothing.
    let chain = chain_with(
        vec!["io-4-d73c".into(), "io-5-79a0".into()],
        vec![
            effect("comp"),
            input_block("mid:in", false, "io-3-47e6", "In 1"),
            effect("preamp"),
            output_block("mid:out", false, "io-2-fb90", "Out 1"),
            effect("reverb"),
        ],
    );
    let ports = resolve_chain_ports(&chain, &registry());

    let inputs: Vec<_> = ports
        .iter()
        .filter(|p| p.direction == PortDirection::Input)
        .collect();
    assert_eq!(
        inputs.len(),
        2,
        "only the two head inputs (Scarlett in 1, TEYUN in 1) are live"
    );
    assert!(
        !inputs
            .iter()
            .any(|p| p.endpoint.device_id.0 == SCARLETT && p.endpoint.channels == vec![1]),
        "the Scarlett's channel 1 (physical input 2) belongs to a DISABLED block \
         and must not be opened"
    );
    assert!(
        !ports.iter().any(|p| p.from_block),
        "no port comes from a disabled block"
    );
}
