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

fn registry() -> Vec<IoBinding> {
    let out = |name: &str, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels,
    };
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![],
        outputs: vec![out("Main Out", vec![0, 1]), out("FX Out", vec![2, 3])],
    }]
}

#[test]
fn options_list_the_chains_bound_output_endpoints_in_flat_order() {
    let options = build_di_output_options(&chain(None), &registry());
    assert_eq!(options.len(), 2);
    assert_eq!(options[0].label, "Main Out");
    assert_eq!(options[0].di_ref.binding_id, "io");
    assert_eq!(options[0].di_ref.endpoint, "Main Out");
    assert_eq!(options[1].label, "FX Out");
    assert_eq!(options[1].di_ref.endpoint, "FX Out");
}

#[test]
fn no_choice_selects_the_main_output() {
    let c = chain(None);
    let options = build_di_output_options(&c, &registry());
    assert_eq!(di_output_selected_index(&c, &options), 0);
}

#[test]
fn persisted_choice_selects_its_index() {
    let c = chain(Some(DiOutputRef {
        binding_id: "io".into(),
        endpoint: "FX Out".into(),
    }));
    let options = build_di_output_options(&c, &registry());
    assert_eq!(di_output_selected_index(&c, &options), 1);
}

#[test]
fn stale_choice_falls_back_to_the_main_output() {
    let c = chain(Some(DiOutputRef {
        binding_id: "gone".into(),
        endpoint: "x".into(),
    }));
    let options = build_di_output_options(&c, &registry());
    assert_eq!(di_output_selected_index(&c, &options), 0);
}

#[test]
fn unbound_chain_yields_no_options() {
    let mut c = chain(None);
    c.io_binding_ids.clear();
    let options = build_di_output_options(&c, &registry());
    assert!(options.is_empty());
    assert_eq!(di_output_selected_index(&c, &options), -1);
}

/// Two bindings of the chain both carry MAIN (same device + channels): the
/// select lists it once, and a choice saved through either copy selects it.
fn overlapping_registry() -> Vec<IoBinding> {
    let out = |name: &str, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels,
    };
    vec![
        IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![],
            outputs: vec![out("MAIN", vec![0, 1]), out("FRFR", vec![14, 15])],
        },
        IoBinding {
            id: "main".into(),
            name: "Main".into(),
            inputs: vec![],
            outputs: vec![out("MAIN", vec![0, 1])],
        },
    ]
}

#[test]
fn an_output_shared_by_two_bindings_is_one_option() {
    let mut c = chain(None);
    c.io_binding_ids = vec!["io".into(), "main".into()];
    let options = build_di_output_options(&c, &overlapping_registry());
    let labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(labels, vec!["MAIN", "FRFR"]);
}

#[test]
fn a_choice_saved_through_the_other_copy_selects_the_shared_option() {
    let mut c = chain(Some(DiOutputRef {
        binding_id: "main".into(),
        endpoint: "MAIN".into(),
    }));
    c.io_binding_ids = vec!["io".into(), "main".into()];
    let options = build_di_output_options(&c, &overlapping_registry());
    assert_eq!(di_output_selected_index(&c, &options), 0);
}
