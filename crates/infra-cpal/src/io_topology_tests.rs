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

// ── #967: an insert's enable flag is NOT part of the stream topology ────────

/// Measured on the owner's rig: toggling the insert of a running chain took
/// 2.1 s (off) and 3.0 s (on) before audio flowed again, and the callback
/// counters of the chain's OTHER routes reset too — every stream the chain
/// owns was torn down and reopened for a one-bit flip.
///
/// The signature only counted an insert's send/return while the block was
/// `enabled`, so disabling it read as a re-bind: `chain_io_changed` → the
/// synchronous `upsert_chain` → all streams reopened. A bound insert occupies
/// its send and return whether or not it is enabled; switching it off means
/// the loop is bypassed in the DSP, not unplugged from the interface.
#[test]
fn disabling_a_bound_insert_is_not_a_topology_change() {
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
        blocks: vec![AudioBlock {
            id: BlockId("insert".into()),
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "external_loop".into(),
                io: "fx".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    };

    let (on_in, on_out) = super::bound_io_signature(&chain, &registry);
    chain.blocks[0].enabled = false;
    let (off_in, off_out) = super::bound_io_signature(&chain, &registry);

    assert!(
        !super::io_topology_changed(&on_in, &off_in, &on_out, &off_out),
        "#967: switching a bound insert off must not read as a re-bind — \
         on {on_in:?}/{on_out:?}, off {off_in:?}/{off_out:?}"
    );
}

/// #967, the other half: the structure signature decides whether the chain
/// gets brand-new streams (`schedule_chain_activation`). An insert's enable
/// flag must be out of it for the same reason it is out of the I/O signature —
/// the loop is bypassed in the DSP, the streams are untouched.
#[test]
fn an_inserts_enable_flag_is_not_part_of_the_chain_structure() {
    use domain::ids::{BlockId, ChainId};
    use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
    use project::chain::Chain;

    let mut chain = Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![AudioBlock {
            id: BlockId("insert".into()),
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "external_loop".into(),
                io: "fx".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    };

    let on = super::chain_structure_signature(&chain);
    chain.blocks[0].enabled = false;
    let off = super::chain_structure_signature(&chain);

    assert_eq!(
        on, off,
        "#967: toggling an insert must not read as a structural change"
    );
}
