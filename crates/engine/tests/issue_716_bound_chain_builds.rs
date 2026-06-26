//! #716 RED: a chain bound purely via `io_binding_ids` (NO Input/Output blocks
//! in `chain.blocks`) MUST build a runnable runtime when the registry holds the
//! referenced binding — the head input / tail output come from the binding.
//! Regression guard for "chain 'rig:input-1' has no input blocks configured".

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::build_per_input_runtime_states;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "Scarlett".into(),
        inputs: vec![IoEndpoint {
            name: "In 1".into(),
            device_id: DeviceId("scarlett".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Out 1".into(),
            device_id: DeviceId("scarlett".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

/// A bound chain: only an effect block + `io_binding_ids` — no Input/Output
/// blocks (exactly what the rig projects for a binding-bound input).
fn bound_chain() -> Chain {
    Chain {
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
    }
}

#[test]
fn bound_chain_builds_a_runnable_runtime_from_the_binding() {
    let runtimes = build_per_input_runtime_states(&bound_chain(), 48_000.0, &[1024], &registry())
        .expect("a binding-bound chain must build (head input from the binding)");
    assert_eq!(
        runtimes.len(),
        1,
        "one input endpoint in the binding → one isolated input runtime"
    );
}
