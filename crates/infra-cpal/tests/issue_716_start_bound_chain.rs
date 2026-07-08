//! #716 RED: starting the project runtime for a binding-bound chain must NOT
//! fail with "chain '...' has no input blocks configured". The controller's
//! `start()` schedules the cold activation while its I/O binding registry is
//! still empty (the owner installs it only AFTER start returns), so the bound
//! chain resolves zero inputs and bails. Reproduce the exact error headlessly.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "Iface".into(),
        inputs: vec![IoEndpoint {
            name: "In 1".into(),
            device_id: DeviceId("test-input-device".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Out 1".into(),
            device_id: DeviceId("test-output-device".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn bound_project() -> Project {
    Project {
        name: Some("p".to_string()),
        device_settings: Vec::new(),
        midi: None,
        chains: vec![Chain {
            id: ChainId("rig:input-1".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["io".into()],
            blocks: vec![AudioBlock {
                id: BlockId("gain".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "gain".into(),
                    model: "volume".into(),
                    params: ParameterSet::default(),
                }),
            }],
            di_output: None,
        }],
    }
}

#[test]
fn start_does_not_bail_no_input_blocks_for_a_binding_bound_chain() {
    let project = bound_project();
    let result = infra_cpal::ProjectRuntimeController::start_with_io_bindings(&project, registry());
    let msg = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        !msg.contains("no input blocks"),
        "start() must resolve a binding-bound chain's input from the registry \
         (the registry must be installed before the cold activation is scheduled); got: {msg}"
    );
}
