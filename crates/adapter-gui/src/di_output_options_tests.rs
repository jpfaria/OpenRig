use super::*;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoEndpoint};

fn chain(di_output: Option<DiOutputRef>) -> Chain {
    Chain {
        id: ChainId("di771opts".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

fn out(name: &str, device: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Stereo,
        channels,
    }
}

/// The chain's binding `io` plus another binding the chain does not use.
fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![],
            outputs: vec![
                out("Main Out", "dev", vec![0, 1]),
                out("FX Out", "dev", vec![2, 3]),
            ],
        },
        IoBinding {
            id: "other".into(),
            name: "OTHER".into(),
            inputs: vec![],
            outputs: vec![out("FRFR", "dev", vec![24, 25])],
        },
    ]
}

fn devices() -> Vec<AudioDeviceDescriptor> {
    vec![AudioDeviceDescriptor {
        id: "dev".into(),
        name: "Quantum HD 8".into(),
        channels: 32,
    }]
}

#[test]
fn options_list_every_output_of_the_project_by_device_and_channels() {
    let (labels, _) = output_labels_and_index(&chain(None), &registry(), &devices());
    assert_eq!(
        labels,
        vec![
            "Quantum HD 8 · Out 1/2",
            "Quantum HD 8 · Out 3/4",
            "Quantum HD 8 · Out 25/26",
        ],
        "the same list the player shows, outputs outside the chain included"
    );
}

#[test]
fn a_picked_row_persists_the_endpoint_it_names() {
    let outputs = output_endpoints(&registry(), &[]);
    let picked = di_output_ref(&outputs, 2).expect("row exists");
    assert_eq!(picked.binding_id, "other");
    assert_eq!(picked.endpoint, "FRFR");
    assert!(di_output_ref(&outputs, 3).is_none());
}

#[test]
fn no_choice_selects_the_main_output() {
    let (_, index) = output_labels_and_index(&chain(None), &registry(), &[]);
    assert_eq!(index, 0);
}

#[test]
fn persisted_choice_selects_its_row_even_outside_the_chain() {
    let c = chain(Some(DiOutputRef {
        binding_id: "other".into(),
        endpoint: "FRFR".into(),
    }));
    let (_, index) = output_labels_and_index(&c, &registry(), &[]);
    assert_eq!(index, 2);
}

#[test]
fn stale_choice_falls_back_to_the_main_output() {
    let c = chain(Some(DiOutputRef {
        binding_id: "gone".into(),
        endpoint: "x".into(),
    }));
    let (_, index) = output_labels_and_index(&c, &registry(), &[]);
    assert_eq!(index, 0);
}

#[test]
fn unbound_chain_selects_no_row() {
    let mut c = chain(None);
    c.io_binding_ids.clear();
    let (_, index) = output_labels_and_index(&c, &registry(), &[]);
    assert_eq!(index, -1);
}

#[test]
fn bindings_sharing_device_channels_list_them_once() {
    // #771 had two interfaces each exposing an "Out 1"; the list now names
    // outputs by device and channels, so only a real duplicate collapses.
    let mut registry = registry();
    registry.push(IoBinding {
        id: "dup".into(),
        name: "DUP".into(),
        inputs: vec![],
        outputs: vec![out("Main Out", "dev", vec![0, 1])],
    });
    let (labels, _) = output_labels_and_index(&chain(None), &registry, &devices());
    assert_eq!(labels.len(), 3);
}

/// #808 (owner): "open the project without ever enabling the chain, open the
/// DI — the output select does not appear; only after I enable the chain the
/// first time." The select's options are built inside `replace_project_chains`
/// from its `io_bindings` arg, which every caller passes EMPTY, so the row
/// opens with no outputs. The refresh that keeps it fresh must populate it
/// from the real bindings even while the chain is disabled (the options are
/// offline — invariant #4).
#[test]
fn di_output_select_populates_before_the_chain_is_ever_enabled() {
    let mut disabled = chain(None);
    disabled.enabled = false;
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![disabled],
        midi: None,
    };
    let model: Rc<VecModel<ProjectChainItem>> = Rc::new(VecModel::default());
    // Reproduce the open flow: rows built with an EMPTY binding registry.
    crate::project_view::replace_project_chains(&model, &project, &[], &[], &[]);
    let before = model.row_data(0).unwrap().di_loop_outputs.iter().count();
    assert_eq!(
        before, 0,
        "precondition: the open flow leaves the DI select empty"
    );

    // The refresh must fill the select from the real bindings — disabled or not.
    apply_di_outputs_to_rows(&model, &project, &registry(), &devices());

    let after = model.row_data(0).unwrap().di_loop_outputs.iter().count();
    assert_eq!(
        after, 3,
        "#808: the DI output select must list the project's outputs even though \
         the chain was never enabled — it stayed empty until the first enable."
    );
}
