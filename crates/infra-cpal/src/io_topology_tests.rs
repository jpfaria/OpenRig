//! Tests for the chain's stream-topology signature (#743, #881).

use domain::ids::DeviceId;

// ── #881: an insert is part of the chain's stream topology ──────────────────

/// Adding (or binding) an insert on a RUNNING chain changes how many streams
/// the chain needs: the send is an output, the return an input. The signature
/// the live-edit path compares against must say so, otherwise the edit takes
/// the DSP-only rebuild, the streams stay as they were, and the segment after
/// the insert waits on a return stream nobody opened — silence.
#[test]
fn a_bound_insert_adds_its_send_and_return_to_the_signature() {
    use domain::ids::{BlockId, ChainId};
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
    use project::chain::Chain;

    const DEV: &str = "coreaudio:hd8";
    let ep = |name: &str, mode: ChannelMode, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(DEV.into()),
        mode,
        channels,
    };
    let registry = vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![ep("In 1", ChannelMode::Mono, vec![0])],
            outputs: vec![ep("Out 1", ChannelMode::Stereo, vec![0, 1])],
        },
        IoBinding {
            id: "fx".into(),
            name: "SYNERGY".into(),
            inputs: vec![ep("ret", ChannelMode::Mono, vec![3])],
            outputs: vec![ep("snd", ChannelMode::Mono, vec![4])],
        },
    ];
    let mut chain = Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    };

    let (plain_in, plain_out) = super::bound_io_signature(&chain, &registry);

    chain.blocks.push(AudioBlock {
        id: BlockId("insert".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "external_loop".into(),
            io: "fx".into(),
        }),
    });
    let (with_in, with_out) = super::bound_io_signature(&chain, &registry);

    assert!(
        super::io_topology_changed(&plain_in, &with_in, &plain_out, &with_out),
        "#881: adding a bound insert must read as an I/O topology change — \
         before {plain_in:?}/{plain_out:?}, after {with_in:?}/{with_out:?}"
    );
    assert!(
        with_in.contains(&(DeviceId(DEV.into()), vec![3])),
        "the RETURN must be in the input signature: {with_in:?}"
    );
    assert!(
        with_out.contains(&(DeviceId(DEV.into()), vec![4])),
        "the SEND must be in the output signature: {with_out:?}"
    );
}
