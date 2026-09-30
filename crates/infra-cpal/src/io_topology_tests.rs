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
        disabled_endpoints: Default::default(),
        mix: Default::default(),
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
        disabled_endpoints: Default::default(),
        mix: Default::default(),
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
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    };

    let on = super::chain_structure_signature(&chain, &[]);
    chain.blocks[0].enabled = false;
    let off = super::chain_structure_signature(&chain, &[]);

    assert_eq!(
        on, off,
        "#967: toggling an insert must not read as a structural change"
    );
}

/// #967: the switch is only a DSP rebuild while it keeps the chain's runtime
/// grouping. Two E/S with a loop: off, two isolated runtimes; on, one pipeline
/// across both heads. The streams those runtimes are bound to differ, so for
/// such a chain the switch MUST read as a structural change (new streams) —
/// otherwise the rebuilt runtimes land in slots that do not match them.
#[test]
fn a_switch_that_regroups_the_chains_runtimes_is_a_structural_change() {
    use domain::ids::{BlockId, ChainId};
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
    use project::chain::Chain;

    let ep = |name: &str, dev: &str, ch: usize| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(dev.into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    };
    let registry = vec![
        IoBinding {
            id: "scarlett".into(),
            name: "SCARLETT".into(),
            inputs: vec![ep("in", "scarlett", 0)],
            outputs: vec![ep("out", "scarlett", 0)],
        },
        IoBinding {
            id: "teyun".into(),
            name: "TEYUN".into(),
            inputs: vec![ep("in", "teyun", 0)],
            outputs: vec![ep("out", "teyun", 0)],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![ep("ret", "scarlett", 3)],
            outputs: vec![ep("snd", "scarlett", 3)],
        },
    ];
    let chain = |enabled| Chain {
        id: ChainId("rig:input-2".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["scarlett".into(), "teyun".into()],
        blocks: vec![AudioBlock {
            id: BlockId("insert".into()),
            enabled,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "fx".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    };

    assert_ne!(
        super::chain_structure_signature(&chain(true), &registry),
        super::chain_structure_signature(&chain(false), &registry),
        "#967: this switch regroups the runtimes — it needs new streams"
    );
}

// ── #871/#881: a mid port's enable flag IS part of the chain structure ──────

/// A mid `Input` / `Output` is a port: a disabled one owns no stream
/// (`resolve_chain_ports`, #871), so switching it changes the streams the
/// chain needs and the structure signature must say so — otherwise the
/// live-edit path swaps only the DSP and the port's stream is never opened
/// (or never closed). The insert beside them is the #967 exception: its flag
/// only moves the DSP cut. The registry is empty on purpose: nothing
/// resolves, so the runtime-grouping row is identical in every case and a
/// difference can only come from the port's own row.
#[test]
fn a_mid_ports_enable_flag_is_part_of_the_chain_structure() {
    use domain::ids::{BlockId, ChainId};
    use project::block::{AudioBlock, AudioBlockKind, InputBlock, InsertBlock, OutputBlock};
    use project::chain::Chain;

    let block = |id: &str, kind: AudioBlockKind| AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind,
    };
    let chain = Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![
            block(
                "mid-in",
                AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    io: "tap".into(),
                    endpoint: "in".into(),
                }),
            ),
            block(
                "mid-out",
                AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    io: "tap".into(),
                    endpoint: "out".into(),
                }),
            ),
            block(
                "insert",
                AudioBlockKind::Insert(InsertBlock {
                    model: "external_loop".into(),
                    io: "fx".into(),
                }),
            ),
        ],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    };
    let on = super::chain_structure_signature(&chain, &[]);

    for (idx, name) in [(0, "mid Input"), (1, "mid Output")] {
        let mut toggled = chain.clone();
        toggled.blocks[idx].enabled = false;
        let off = super::chain_structure_signature(&toggled, &[]);

        assert_eq!(
            on.last(),
            off.last(),
            "precondition: with nothing resolved the runtime grouping must not \
             move, so only the {name}'s own row can tell the two apart"
        );
        assert_ne!(
            on, off,
            "#881: switching a {name} off changes the streams the chain owns — \
             it must read as a structural change"
        );
    }
}

// ── #328: an endpoint unchecked on the chain graph is a re-bind ─────────────

/// Unchecking an input on a RUNNING chain must read as an I/O change, or the
/// live-edit path keeps the stream it no longer wants open.
#[test]
fn unchecking_an_input_endpoint_changes_the_bound_io_signature() {
    use domain::ids::ChainId;
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use project::chain::Chain;
    use project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef};

    let ep = |name: &str, ch: usize| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("scarlett".into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    };
    let registry = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![ep("in 1", 0), ep("in 2", 1)],
        outputs: vec![ep("out", 0)],
    }];
    let chain = |disabled_endpoints: EndpointDisables| Chain {
        mix: Default::default(),
        id: ChainId("rig:g".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints,
    };
    let mut unchecked = EndpointDisables::default();
    unchecked.set_enabled(
        EndpointNode::Input,
        EndpointRef {
            io: "io".into(),
            endpoint: "in 2".into(),
        },
        false,
    );

    let (all_inputs, _) = super::bound_io_signature(&chain(EndpointDisables::default()), &registry);
    let (kept_inputs, _) = super::bound_io_signature(&chain(unchecked), &registry);
    assert_eq!(all_inputs.len(), 2);
    assert_eq!(
        kept_inputs,
        vec![(DeviceId("scarlett".into()), vec![0])],
        "#328: the unchecked input opens no stream, so a running chain must re-bind"
    );
}

// ── #328: which split paths feed each output is part of the structure ─────

/// A Y → A/B chain whose two outputs both stay open: checking path B on the
/// output path A already feeds opens no new device stream (the I/O signature
/// is unchanged), yet that output's pipeline now runs both paths. Only the
/// structure signature can see it; without the row `schedule_chain_activation`
/// takes the edit for a knob turn instead of giving the chain brand-new
/// streams (#881, spec §4.2).
#[test]
fn a_path_set_change_is_a_structural_change() {
    use domain::ids::{BlockId, ChainId};
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use project::block::split_params::default_split_params;
    use project::block::{AudioBlock, AudioBlockKind, SplitBlock, SplitEnd};
    use project::chain::Chain;
    use project::endpoint_disables::{EndpointDisables, EndpointRef};

    let ep = |name: &str, ch: usize| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    };
    let registry = vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![ep("in", 0)],
        outputs: vec![ep("out-a", 0), ep("out-b", 1)],
    }];
    let off = |name: &str| EndpointRef {
        io: "main".into(),
        endpoint: name.into(),
    };
    let chain = |path_b_outputs: Vec<EndpointRef>| Chain {
        mix: Default::default(),
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![AudioBlock {
            id: BlockId("split".into()),
            enabled: true,
            kind: AudioBlockKind::Split(SplitBlock {
                end: SplitEnd::Y,
                params: default_split_params(),
                a: vec![],
                b: vec![],
            }),
        }],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: EndpointDisables {
            inputs: vec![],
            outputs: vec![],
            path_a_outputs: vec![off("out-b")],
            path_b_outputs,
        },
    };
    // Path A → out-a. Path B → out-b only, then → out-a too.
    let before = chain(vec![off("out-a")]);
    let after = chain(vec![]);

    assert_eq!(
        super::bound_io_signature(&before, &registry),
        super::bound_io_signature(&after, &registry),
        "fixture: both outputs stay open, so the I/O signature cannot tell"
    );
    assert_ne!(
        super::chain_structure_signature(&before, &registry),
        super::chain_structure_signature(&after, &registry),
        "#328: out-a now runs path A AND path B — its pipeline changed, so the chain needs new streams (#881)"
    );
}
