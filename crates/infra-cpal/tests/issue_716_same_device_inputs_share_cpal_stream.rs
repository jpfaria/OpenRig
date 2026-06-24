//! #716 RED — two input endpoints on the SAME device must share ONE cpal
//! input stream (per docs/audio-config.md "Two entries on ONE device", #703:
//! Core Audio cannot open two streams on one device, so one stream fans out to
//! every per-entry runtime bound to that cpal index).
//!
//! The bug: a binding with two inputs on one device builds two per-input
//! runtimes with DIFFERENT input_cpal_index → the engine expects two device
//! streams, only one exists, so the second runtime is never fed → the device
//! starves (480k underruns on hardware). The two same-device runtimes must
//! carry the SAME input_cpal_index.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_cpal::{build_chain_runtime, BuildRequest};
use project::chain::Chain;

/// One binding, two input endpoints on the SAME device (ch0, ch1) + a stereo
/// output on that device — exactly the user's io-1-50c4 (SCARLET In1+In2).
fn binding_two_inputs_one_device() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "One device".into(),
        inputs: vec![
            IoEndpoint {
                name: "in1".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            },
            IoEndpoint {
                name: "in2".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Mono,
                channels: vec![1],
            },
        ],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn bound_chain() -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
    }
}

#[test]
fn two_inputs_on_one_device_share_one_cpal_stream() {
    let req = BuildRequest {
        chain: bound_chain(),
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024, 1024],
        io_bindings: binding_two_inputs_one_device(),
    };
    let runtimes = build_chain_runtime(&req).expect("bound chain must build");

    assert_eq!(
        runtimes.len(),
        2,
        "two input endpoints → two isolated per-input runtimes"
    );

    let indices: Vec<Option<usize>> = runtimes
        .iter()
        .map(|(_, rt)| rt.input_cpal_index())
        .collect();

    // Same physical device → ONE cpal stream → both runtimes bound to the same
    // cpal index, so the single device callback fans out to both.
    assert_eq!(
        indices[0], indices[1],
        "two inputs on the SAME device must share ONE cpal stream (#703): both \
         per-input runtimes must carry the same input_cpal_index; got {indices:?} — \
         distinct indices make the engine expect two device streams (impossible on \
         Core Audio) so one runtime is never fed and the device starves"
    );
    assert!(
        indices[0].is_some(),
        "the shared device stream index must be assigned (not None → group fallback)"
    );
}
