//! #716 RED: a chain referencing TWO bindings (SCARLET + TEYUN) must resolve its
//! I/O GROUPED BY BINDING, so each input pairs only with its own binding's output
//! (TEYUN in → TEYUN out, SCARLET in → SCARLET out). The flat `resolve_chain_io`
//! drops the binding association, which is what cross-routes TEYUN out the SCARLET.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;

fn binding(id: &str, dev: &str) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.into(),
        inputs: vec![IoEndpoint {
            name: "In 1".into(),
            device_id: DeviceId(format!("{dev}-in")),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Out 1".into(),
            device_id: DeviceId(format!("{dev}-out")),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }
}

fn two_binding_chain() -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["scarlet".into(), "teyun".into()],
        blocks: Vec::new(),
    }
}

#[test]
fn resolves_io_grouped_per_binding_so_inputs_pair_with_their_own_output() {
    let registry = vec![binding("scarlet", "scarlett"), binding("teyun", "teyun")];
    let groups = engine::runtime_endpoints::resolve_chain_io_by_binding(&two_binding_chain(), &registry);

    assert_eq!(groups.len(), 2, "one group per referenced binding");

    let teyun = groups
        .iter()
        .find(|g| g.inputs.iter().any(|i| i.device_id.0 == "teyun-in"))
        .expect("a group carrying the TEYUN input");
    assert!(
        teyun.outputs.iter().all(|o| o.device_id.0 == "teyun-out"),
        "TEYUN input pairs only with the TEYUN output — never cross to SCARLET"
    );

    let scarlet = groups
        .iter()
        .find(|g| g.inputs.iter().any(|i| i.device_id.0 == "scarlett-in"))
        .expect("a group carrying the SCARLET input");
    assert!(
        scarlet.outputs.iter().all(|o| o.device_id.0 == "scarlett-out"),
        "SCARLET input pairs only with the SCARLET output"
    );
}
